//! Reader for local PMTiles v3 archives (the chart data pack's aero / airspace / obstacle tiles).
//! Directories are cached; tile bytes are read with positioned reads and returned gunzipped.

use anyhow::{bail, Context};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;

#[derive(Clone, Copy, Debug)]
struct Entry {
    tile_id: u64,
    offset: u64,
    length: u32,
    run_length: u32,
}

pub struct PmTiles {
    file: Mutex<File>,
    root: Vec<Entry>,
    leaf_offset: u64,
    data_offset: u64,
    internal_gzip: bool,
    tile_gzip: bool,
    pub min_zoom: u8,
    pub max_zoom: u8,
    leaves: Mutex<HashMap<(u64, u64), std::sync::Arc<Vec<Entry>>>>,
}

fn read_at(f: &mut File, offset: u64, len: usize) -> std::io::Result<Vec<u8>> {
    use std::io::{Seek, SeekFrom};
    f.seek(SeekFrom::Start(offset))?;
    let mut buf = vec![0u8; len];
    f.read_exact(&mut buf)?;
    Ok(buf)
}

fn gunzip(b: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut out = Vec::with_capacity(b.len() * 3);
    flate2::read::GzDecoder::new(b).read_to_end(&mut out)?;
    Ok(out)
}

fn varint(b: &[u8], pos: &mut usize) -> anyhow::Result<u64> {
    let mut v = 0u64;
    let mut shift = 0;
    loop {
        let byte = *b.get(*pos).context("truncated directory")?;
        *pos += 1;
        v |= ((byte & 0x7f) as u64) << shift;
        if byte & 0x80 == 0 {
            return Ok(v);
        }
        shift += 7;
        if shift > 63 {
            bail!("bad varint");
        }
    }
}

fn parse_dir(b: &[u8]) -> anyhow::Result<Vec<Entry>> {
    let mut pos = 0;
    let n = varint(b, &mut pos)? as usize;
    let mut e = vec![Entry { tile_id: 0, offset: 0, length: 0, run_length: 0 }; n];
    let mut last = 0u64;
    for x in e.iter_mut() {
        last += varint(b, &mut pos)?;
        x.tile_id = last;
    }
    for x in e.iter_mut() {
        x.run_length = varint(b, &mut pos)? as u32;
    }
    for x in e.iter_mut() {
        x.length = varint(b, &mut pos)? as u32;
    }
    for i in 0..n {
        let v = varint(b, &mut pos)?;
        e[i].offset = if v == 0 && i > 0 { e[i - 1].offset + e[i - 1].length as u64 } else { v.saturating_sub(1) };
    }
    Ok(e)
}

/// Hilbert-curve tile id (PMTiles v3).
pub fn tile_id(z: u8, x: u32, y: u32) -> u64 {
    let mut acc: u64 = 0;
    for i in 0..z {
        acc += 1u64 << (2 * i as u64);
    }
    let (mut tx, mut ty) = (x as u64, y as u64);
    let mut d: u64 = 0;
    let mut s: u64 = (1u64 << z) / 2;
    while s > 0 {
        let rx = u64::from(tx & s > 0);
        let ry = u64::from(ty & s > 0);
        d += s * s * ((3 * rx) ^ ry);
        // only the bits below s matter from here on
        tx &= s - 1;
        ty &= s - 1;
        if ry == 0 {
            if rx == 1 {
                tx = s - 1 - tx;
                ty = s - 1 - ty;
            }
            std::mem::swap(&mut tx, &mut ty);
        }
        s /= 2;
    }
    acc + d
}

fn find(entries: &[Entry], id: u64) -> Option<Entry> {
    let i = entries.partition_point(|e| e.tile_id <= id);
    if i == 0 {
        return None;
    }
    let e = entries[i - 1];
    if e.run_length == 0 || id < e.tile_id + e.run_length as u64 { Some(e) } else { None }
}

impl PmTiles {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let mut f = File::open(path).with_context(|| format!("open {}", path.display()))?;
        let h = read_at(&mut f, 0, 127)?;
        if &h[0..7] != b"PMTiles" || h[7] != 3 {
            bail!("{} is not a PMTiles v3 archive", path.display());
        }
        let u = |o: usize| u64::from_le_bytes(h[o..o + 8].try_into().unwrap());
        let internal_gzip = h[97] == 2;
        let tile_gzip = h[98] == 2;
        let root_raw = read_at(&mut f, u(8), u(16) as usize)?;
        let root = parse_dir(&if internal_gzip { gunzip(&root_raw)? } else { root_raw })?;
        Ok(PmTiles {
            file: Mutex::new(f),
            root,
            leaf_offset: u(40),
            data_offset: u(56),
            internal_gzip,
            tile_gzip,
            min_zoom: h[100],
            max_zoom: h[101],
            leaves: Mutex::new(HashMap::new()),
        })
    }

    /// Uncompressed tile bytes (MVT), or None when the archive has no tile there.
    pub fn get(&self, z: u8, x: u32, y: u32) -> anyhow::Result<Option<Vec<u8>>> {
        let id = tile_id(z, x, y);
        let mut dir = std::sync::Arc::new(self.root.clone());
        for _ in 0..4 {
            let Some(e) = find(&dir, id) else { return Ok(None) };
            if e.run_length > 0 {
                let raw = read_at(&mut self.file.lock(), self.data_offset + e.offset, e.length as usize)?;
                return Ok(Some(if self.tile_gzip || raw.starts_with(&[0x1f, 0x8b]) { gunzip(&raw)? } else { raw }));
            }
            let key = (e.offset, e.length as u64);
            let cached = self.leaves.lock().get(&key).cloned();
            dir = match cached {
                Some(d) => d,
                None => {
                    let raw = read_at(&mut self.file.lock(), self.leaf_offset + e.offset, e.length as usize)?;
                    let d = std::sync::Arc::new(parse_dir(&if self.internal_gzip { gunzip(&raw)? } else { raw })?);
                    let mut l = self.leaves.lock();
                    if l.len() > 256 {
                        l.clear();
                    }
                    l.insert(key, d.clone());
                    d
                }
            };
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::tile_id;

    #[test]
    fn ids_match_the_spec() {
        // values from the PMTiles reference implementation
        assert_eq!(tile_id(0, 0, 0), 0);
        assert_eq!(tile_id(1, 0, 0), 1);
        assert_eq!(tile_id(1, 0, 1), 2);
        assert_eq!(tile_id(1, 1, 1), 3);
        assert_eq!(tile_id(1, 1, 0), 4);
        assert_eq!(tile_id(2, 0, 0), 5);
        assert_eq!(tile_id(3, 7, 7), 63);
        assert_eq!(tile_id(20, 0, 0), 366503875925);
    }
}

#[cfg(test)]
mod data_tests {
    /// Reads the repository's own data build when present (skipped otherwise).
    #[test]
    fn reads_real_archives() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../tiles");
        let Ok(pm) = super::PmTiles::open(&root.join("aero.pmtiles")) else { return };
        // Phoenix area at z9
        let (x, y) = (96, 205);
        let bytes = pm.get(9, x, y).unwrap().expect("tile");
        let t = crate::map::mvt::decode(&bytes);
        let names: Vec<&str> = t.layers.iter().map(|l| l.name.as_str()).collect();
        assert!(names.contains(&"airports"), "{names:?}");
        let apt = t.layer("airports").unwrap();
        assert!(apt.features.iter().any(|f| f.str("id") == "PHX" || f.str("icao") == "KPHX"), "no KPHX");
        let a = super::PmTiles::open(&root.join("airspace.pmtiles")).unwrap();
        let t = crate::map::mvt::decode(&a.get(9, x, y).unwrap().expect("airspace tile"));
        let l = t.layer("airspace").unwrap();
        assert!(l.features.iter().any(|f| f.str("kind") == "B"));
        assert!(l.features[0].parts[0].len() > 3);
    }
}

#[cfg(test)]
mod dump {
    #[test]
    #[ignore]
    fn dump_props() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../tiles");
        for (file, z, x, y) in [("airspace.pmtiles", 9u8, 98u32, 206u32)] {
            let pm = super::PmTiles::open(&root.join(file)).unwrap();
            eprintln!("{file} zoom {}..{}", pm.min_zoom, pm.max_zoom);
            let t = crate::map::mvt::decode(&pm.get(z, x, y).unwrap().unwrap());
            for l in &t.layers {
                eprintln!("  layer {} ({} features)", l.name, l.features.len());
                for f in l.features.iter().take(80) {
                    eprintln!("    {:?} {:?}", f.kind, f.props.iter().map(|(k, v)| format!("{k}={}", v.to_text())).collect::<Vec<_>>());
                }
            }
        }
    }
}
