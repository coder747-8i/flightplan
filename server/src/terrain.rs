//! Terrain elevation from Terrarium-encoded DEM tiles (AWS open data, ~30-90 m source),
//! cached on disk and in memory. Used for the vertical profile and terrain alerts.

use crate::geo;
use futures::{stream, StreamExt};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;

const Z: u32 = 11;
const SIZE: usize = 256;
const MEM_TILES: usize = 600;
const M_TO_FT: f64 = 3.28084;

pub struct Terrain {
    dir: PathBuf,
    http: reqwest::Client,
    mem: Mutex<(HashMap<(u32, u32), Arc<Vec<i16>>>, VecDeque<(u32, u32)>)>,
}

#[derive(Deserialize)]
pub struct ProfileReq {
    /// route as [lon, lat] pairs
    pub path: Vec<[f64; 2]>,
    /// sample spacing in NM (default 0.25)
    pub step: Option<f64>,
    /// half-width of the corridor searched for the highest terrain, NM (default 1)
    pub corridor: Option<f64>,
}

#[derive(Serialize)]
pub struct Sample {
    pub d: f64,
    pub lat: f64,
    pub lon: f64,
    /// terrain on the centerline, ft MSL
    pub elev: f64,
    /// highest terrain within the corridor, ft MSL
    pub max: f64,
    pub leg: usize,
}

fn tile_xy(lat: f64, lon: f64) -> (f64, f64) {
    let n = (1u32 << Z) as f64;
    let x = (lon + 180.0) / 360.0 * n;
    let lr = lat.to_radians();
    let y = (1.0 - (lr.tan() + 1.0 / lr.cos()).ln() / std::f64::consts::PI) / 2.0 * n;
    (x, y)
}

impl Terrain {
    pub fn new(dir: PathBuf, http: reqwest::Client) -> Self {
        let _ = std::fs::create_dir_all(&dir);
        Self { dir, http, mem: Mutex::new((HashMap::new(), VecDeque::new())) }
    }

    async fn tile(&self, x: u32, y: u32) -> anyhow::Result<Arc<Vec<i16>>> {
        if let Some(t) = self.mem.lock().0.get(&(x, y)) {
            return Ok(t.clone());
        }
        let path = self.dir.join(format!("{Z}-{x}-{y}.png"));
        let bytes = match tokio::fs::read(&path).await {
            Ok(b) => b,
            Err(_) => {
                let url = format!("https://s3.amazonaws.com/elevation-tiles-prod/terrarium/{Z}/{x}/{y}.png");
                let b = self.http.get(&url).send().await?.error_for_status()?.bytes().await?.to_vec();
                let _ = tokio::fs::write(&path, &b).await;
                b
            }
        };
        let decoded = tokio::task::spawn_blocking(move || decode(&bytes)).await??;
        let t = Arc::new(decoded);
        let mut m = self.mem.lock();
        m.0.insert((x, y), t.clone());
        m.1.push_back((x, y));
        while m.1.len() > MEM_TILES {
            if let Some(old) = m.1.pop_front() {
                m.0.remove(&old);
            }
        }
        Ok(t)
    }

    /// Make sure every tile touched by these points is loaded (parallel fetch).
    async fn preload(&self, pts: &[(f64, f64)]) -> anyhow::Result<HashMap<(u32, u32), Arc<Vec<i16>>>> {
        let mut keys: Vec<(u32, u32)> = pts
            .iter()
            .map(|&(la, lo)| {
                let (x, y) = tile_xy(la, lo);
                (x as u32, y as u32)
            })
            .collect();
        keys.sort_unstable();
        keys.dedup();
        let futs: Vec<_> = keys.into_iter().map(|(x, y)| async move { (x, y, self.tile(x, y).await) }).collect();
        let results: Vec<_> = stream::iter(futs)
            .buffer_unordered(16)
            .collect()
            .await;
        let mut out = HashMap::new();
        for (x, y, t) in results {
            out.insert((x, y), t?);
        }
        Ok(out)
    }

    pub async fn elevation_ft(&self, lat: f64, lon: f64) -> anyhow::Result<f64> {
        let tiles = self.preload(&[(lat, lon)]).await?;
        Ok(sample(&tiles, lat, lon))
    }

    pub async fn profile(&self, req: &ProfileReq) -> anyhow::Result<Vec<Sample>> {
        let step = req.step.unwrap_or(0.25).clamp(0.05, 5.0);
        let corridor = req.corridor.unwrap_or(1.0).clamp(0.0, 5.0);
        let pts: Vec<(f64, f64)> = req.path.iter().map(|p| (p[1], p[0])).collect();
        if pts.len() < 2 {
            return Ok(vec![]);
        }
        let total: f64 = pts.windows(2).map(|w| geo::distance_nm(w[0].0, w[0].1, w[1].0, w[1].1)).sum();
        let step = step.max(total / 3000.0); // cap the sample count on long routes
        // centerline + lateral offsets
        let mut centre = Vec::new();
        let mut probes = Vec::new();
        let mut d0 = 0.0;
        for (leg, w) in pts.windows(2).enumerate() {
            let (a, b) = (w[0], w[1]);
            let len = geo::distance_nm(a.0, a.1, b.0, b.1);
            let n = (len / step).ceil().max(1.0) as usize;
            let brg = geo::bearing(a.0, a.1, b.0, b.1);
            for i in 0..=n {
                if leg > 0 && i == 0 {
                    continue;
                }
                let f = i as f64 / n as f64;
                let (la, lo) = geo::interpolate(a.0, a.1, b.0, b.1, f);
                centre.push((d0 + len * f, la, lo, leg));
                let mut side = vec![(la, lo)];
                if corridor > 0.0 {
                    for k in [-1.0, -0.5, 0.5, 1.0] {
                        side.push(geo::offset(la, lo, brg + 90.0, corridor * k));
                    }
                }
                probes.push(side);
            }
            d0 += len;
        }
        let all: Vec<(f64, f64)> = probes.iter().flatten().copied().collect();
        let tiles = self.preload(&all).await?;
        Ok(centre
            .into_iter()
            .zip(probes)
            .map(|((d, la, lo, leg), side)| {
                let elev = sample(&tiles, la, lo);
                let max = side.iter().map(|&(a, b)| sample(&tiles, a, b)).fold(elev, f64::max);
                Sample { d: (d * 100.0).round() / 100.0, lat: la, lon: lo, elev: elev.round(), max: max.round(), leg }
            })
            .collect())
    }
}

fn sample(tiles: &HashMap<(u32, u32), Arc<Vec<i16>>>, lat: f64, lon: f64) -> f64 {
    let (x, y) = tile_xy(lat, lon);
    let (tx, ty) = (x as u32, y as u32);
    let Some(t) = tiles.get(&(tx, ty)) else { return 0.0 };
    let px = ((x - tx as f64) * SIZE as f64 - 0.5).clamp(0.0, (SIZE - 1) as f64);
    let py = ((y - ty as f64) * SIZE as f64 - 0.5).clamp(0.0, (SIZE - 1) as f64);
    let (x0, y0) = (px.floor() as usize, py.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(SIZE - 1), (y0 + 1).min(SIZE - 1));
    let (fx, fy) = (px - x0 as f64, py - y0 as f64);
    let v = |x: usize, y: usize| t[y * SIZE + x] as f64;
    let top = v(x0, y0) * (1.0 - fx) + v(x1, y0) * fx;
    let bot = v(x0, y1) * (1.0 - fx) + v(x1, y1) * fx;
    ((top * (1.0 - fy) + bot * fy) * M_TO_FT).max(0.0)
}

fn decode(bytes: &[u8]) -> anyhow::Result<Vec<i16>> {
    let mut dec = png::Decoder::new(std::io::Cursor::new(bytes));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = dec.read_info()?;
    let mut buf = vec![0; reader.output_buffer_size().unwrap_or(SIZE * SIZE * 4)];
    let info = reader.next_frame(&mut buf)?;
    let ch = info.color_type.samples();
    let mut out = Vec::with_capacity(SIZE * SIZE);
    for px in buf[..info.buffer_size()].chunks_exact(ch) {
        let (r, g, b) = (px[0] as f64, px[1] as f64, px[2] as f64);
        out.push((r * 256.0 + g + b / 256.0 - 32768.0).round().clamp(-500.0, 9000.0) as i16);
    }
    Ok(out)
}
