//! Mapbox Vector Tile (protobuf) decoder: layers, features, properties and geometry in tile
//! coordinates (0..extent). Small and dependency-free; only what the chart needs.

use std::sync::Arc;

#[derive(Clone, Debug, PartialEq)]
pub enum Val {
    Str(Arc<str>),
    Num(f64),
    Bool(bool),
}

impl Val {
    pub fn as_str(&self) -> &str {
        match self {
            Val::Str(s) => s,
            _ => "",
        }
    }
    pub fn as_f64(&self) -> f64 {
        match self {
            Val::Num(n) => *n,
            Val::Bool(b) => f64::from(u8::from(*b)),
            Val::Str(s) => s.parse().unwrap_or(0.0),
        }
    }
    pub fn to_text(&self) -> String {
        match self {
            Val::Str(s) => s.to_string(),
            Val::Num(n) if n.fract() == 0.0 => format!("{}", *n as i64),
            Val::Num(n) => n.to_string(),
            Val::Bool(b) => b.to_string(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeomType {
    Unknown,
    Point,
    Line,
    Polygon,
}

#[derive(Clone, Debug)]
pub struct Feature {
    pub kind: GeomType,
    /// points: one part per point; lines: one part per line; polygons: one part per ring
    pub parts: Vec<Vec<[f32; 2]>>,
    pub props: Vec<(Arc<str>, Val)>,
}

impl Feature {
    pub fn get(&self, key: &str) -> Option<&Val> {
        self.props.iter().find(|(k, _)| &**k == key).map(|(_, v)| v)
    }
    pub fn str(&self, key: &str) -> &str {
        self.get(key).map(|v| v.as_str()).unwrap_or("")
    }
    pub fn num(&self, key: &str) -> f64 {
        self.get(key).map(|v| v.as_f64()).unwrap_or(0.0)
    }
    pub fn text(&self, key: &str) -> String {
        self.get(key).map(|v| v.to_text()).unwrap_or_default()
    }
}

#[derive(Clone, Debug)]
pub struct Layer {
    pub name: String,
    pub extent: f32,
    pub features: Vec<Feature>,
}

#[derive(Clone, Debug, Default)]
pub struct Tile {
    pub layers: Vec<Layer>,
}

impl Tile {
    pub fn layer(&self, name: &str) -> Option<&Layer> {
        self.layers.iter().find(|l| l.name == name)
    }
}

struct Reader<'a> {
    b: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn varint(&mut self) -> Option<u64> {
        let mut v = 0u64;
        let mut shift = 0;
        loop {
            let byte = *self.b.get(self.pos)?;
            self.pos += 1;
            v |= ((byte & 0x7f) as u64) << shift;
            if byte & 0x80 == 0 {
                return Some(v);
            }
            shift += 7;
            if shift > 63 {
                return None;
            }
        }
    }
    fn key(&mut self) -> Option<(u32, u8)> {
        let k = self.varint()?;
        Some(((k >> 3) as u32, (k & 7) as u8))
    }
    fn bytes(&mut self) -> Option<&'a [u8]> {
        let n = self.varint()? as usize;
        let s = self.b.get(self.pos..self.pos + n)?;
        self.pos += n;
        Some(s)
    }
    fn skip(&mut self, wire: u8) -> Option<()> {
        match wire {
            0 => {
                self.varint()?;
            }
            1 => self.pos += 8,
            2 => {
                self.bytes()?;
            }
            5 => self.pos += 4,
            _ => return None,
        }
        Some(())
    }
    fn done(&self) -> bool {
        self.pos >= self.b.len()
    }
}

fn packed(b: &[u8]) -> Vec<u32> {
    let mut r = Reader { b, pos: 0 };
    let mut out = Vec::with_capacity(b.len());
    while !r.done() {
        match r.varint() {
            Some(v) => out.push(v as u32),
            None => break,
        }
    }
    out
}

fn zigzag(v: u32) -> i32 {
    ((v >> 1) as i32) ^ -((v & 1) as i32)
}

fn value(b: &[u8]) -> Val {
    let mut r = Reader { b, pos: 0 };
    let mut out = Val::Str(Arc::from(""));
    while !r.done() {
        let Some((field, wire)) = r.key() else { break };
        match (field, wire) {
            (1, 2) => out = Val::Str(Arc::from(r.bytes().map(|s| String::from_utf8_lossy(s).into_owned()).unwrap_or_default())),
            (2, 5) => {
                if let Some(x) = r.b.get(r.pos..r.pos + 4) {
                    out = Val::Num(f32::from_le_bytes(x.try_into().unwrap()) as f64);
                }
                r.pos += 4;
            }
            (3, 1) => {
                if let Some(x) = r.b.get(r.pos..r.pos + 8) {
                    out = Val::Num(f64::from_le_bytes(x.try_into().unwrap()));
                }
                r.pos += 8;
            }
            (4, 0) => out = Val::Num(r.varint().unwrap_or(0) as i64 as f64),
            (5, 0) => out = Val::Num(r.varint().unwrap_or(0) as f64),
            (6, 0) => {
                let v = r.varint().unwrap_or(0);
                out = Val::Num((((v >> 1) as i64) ^ -((v & 1) as i64)) as f64);
            }
            (7, 0) => out = Val::Bool(r.varint().unwrap_or(0) != 0),
            (_, w) => {
                if r.skip(w).is_none() {
                    break;
                }
            }
        }
    }
    out
}

fn geometry(_kind: GeomType, cmds: &[u32]) -> Vec<Vec<[f32; 2]>> {
    let mut parts: Vec<Vec<[f32; 2]>> = Vec::new();
    let (mut x, mut y) = (0i32, 0i32);
    let mut i = 0;
    while i < cmds.len() {
        let id = cmds[i] & 7;
        let count = (cmds[i] >> 3) as usize;
        i += 1;
        match id {
            1 | 2 => {
                for _ in 0..count {
                    let (Some(dx), Some(dy)) = (cmds.get(i), cmds.get(i + 1)) else { return parts };
                    x += zigzag(*dx);
                    y += zigzag(*dy);
                    i += 2;
                    let p = [x as f32, y as f32];
                    // MoveTo starts a part (every point of a multipoint is its own part)
                    if id == 1 {
                        parts.push(vec![p]);
                    } else if let Some(last) = parts.last_mut() {
                        last.push(p);
                    }
                }
            }
            7 => {
                if let Some(last) = parts.last_mut() {
                    if let Some(first) = last.first().copied() {
                        last.push(first);
                    }
                }
            }
            _ => return parts,
        }
    }
    parts
}

fn feature(b: &[u8], keys: &[Arc<str>], values: &[Val]) -> Option<Feature> {
    let mut r = Reader { b, pos: 0 };
    let mut tags = Vec::new();
    let mut kind = GeomType::Unknown;
    let mut geom = Vec::new();
    while !r.done() {
        let (field, wire) = r.key()?;
        match (field, wire) {
            (2, 2) => tags = packed(r.bytes()?),
            (3, 0) => {
                kind = match r.varint()? {
                    1 => GeomType::Point,
                    2 => GeomType::Line,
                    3 => GeomType::Polygon,
                    _ => GeomType::Unknown,
                }
            }
            (4, 2) => geom = packed(r.bytes()?),
            (_, w) => r.skip(w)?,
        }
    }
    let props = tags.chunks_exact(2).filter_map(|kv| Some((keys.get(kv[0] as usize)?.clone(), values.get(kv[1] as usize)?.clone()))).collect();
    Some(Feature { kind, parts: geometry(kind, &geom), props })
}

fn layer(b: &[u8]) -> Option<Layer> {
    let mut r = Reader { b, pos: 0 };
    let mut name = String::new();
    let mut extent = 4096.0;
    let mut keys: Vec<Arc<str>> = Vec::new();
    let mut values: Vec<Val> = Vec::new();
    let mut raw_features: Vec<&[u8]> = Vec::new();
    while !r.done() {
        let (field, wire) = r.key()?;
        match (field, wire) {
            (1, 2) => name = String::from_utf8_lossy(r.bytes()?).into_owned(),
            (2, 2) => raw_features.push(r.bytes()?),
            (3, 2) => keys.push(Arc::from(String::from_utf8_lossy(r.bytes()?).into_owned())),
            (4, 2) => values.push(value(r.bytes()?)),
            (5, 0) => extent = r.varint()? as f32,
            (_, w) => r.skip(w)?,
        }
    }
    let features = raw_features.into_iter().filter_map(|f| feature(f, &keys, &values)).collect();
    Some(Layer { name, extent, features })
}

/// Decode an (already decompressed) vector tile.
pub fn decode(b: &[u8]) -> Tile {
    let mut r = Reader { b, pos: 0 };
    let mut layers = Vec::new();
    while !r.done() {
        let Some((field, wire)) = r.key() else { break };
        if field == 3 && wire == 2 {
            match r.bytes().and_then(layer) {
                Some(l) => layers.push(l),
                None => break,
            }
        } else if r.skip(wire).is_none() {
            break;
        }
    }
    Tile { layers }
}
