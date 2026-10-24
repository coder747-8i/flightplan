//! Point features pulled out of vector tiles for live drawing: airports, navaids, fixes,
//! obstacles, airspace labels, airway labels and place names. Positions are in world units
//! (web-mercator, 0..1).

use super::mvt::{Feature, GeomType, Tile};
use super::tiles::Source;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct Airport {
    pub id: Arc<str>,
    pub icao: Arc<str>,
    pub name: Arc<str>,
    pub ty: Arc<str>,
    pub pvt: bool,
    pub twr: bool,
    pub mil: bool,
    pub elev: f64,
    pub rwy: f64,
    pub hard: bool,
    pub lit: bool,
    pub hdg: f64,
    pub fuel: bool,
    pub ctaf: Arc<str>,
    pub rank: f64,
}

impl Airport {
    pub fn ident(&self) -> &str {
        if self.icao.is_empty() { &self.id } else { &self.icao }
    }
}

#[derive(Clone, Debug)]
pub enum Kind {
    Airport(Airport),
    Navaid { id: Arc<str>, name: Arc<str>, ty: Arc<str>, freq: Arc<str>, var: f64 },
    Fix { id: Arc<str>, vfr: bool },
    Obstacle { agl: f64, msl: f64, lit: bool, ty: Arc<str> },
    AirspaceLabel { kind: Arc<str>, label: Arc<str>, range: Arc<str>, name: Arc<str>, lo_agl: bool, lo: f64, hi: f64 },
    Airway { id: Arc<str>, high: bool, angle: f32 },
    Place { city: bool, name: Arc<str> },
    Water { name: Arc<str> },
}

#[derive(Clone, Debug)]
pub struct Sym {
    pub world: [f64; 2],
    pub kind: Kind,
}

fn s(f: &Feature, k: &str) -> Arc<str> {
    match f.get(k) {
        Some(super::mvt::Val::Str(v)) => v.clone(),
        Some(v) => Arc::from(v.to_text()),
        None => Arc::from(""),
    }
}

/// Symbols in one tile. Features in the tile buffer (outside 0..extent) are skipped so
/// neighbours don't duplicate them.
pub fn extract(src: Source, z: u8, x: u32, y: u32, tile: &Tile) -> Vec<Sym> {
    let mut out = Vec::new();
    let n = (1u64 << z) as f64;
    for layer in &tile.layers {
        let ext = layer.extent;
        let world = |p: [f32; 2]| [(x as f64 + p[0] as f64 / ext as f64) / n, (y as f64 + p[1] as f64 / ext as f64) / n];
        let inside = |p: [f32; 2]| p[0] >= 0.0 && p[1] >= 0.0 && p[0] < ext && p[1] < ext;
        for f in &layer.features {
            let point = || f.parts.first().and_then(|p| p.first()).copied().filter(|p| inside(*p));
            let kind = match (src, layer.name.as_str()) {
                (Source::Aero, "airports") => Kind::Airport(Airport {
                    id: s(f, "id"),
                    icao: s(f, "icao"),
                    name: s(f, "name"),
                    ty: s(f, "type"),
                    pvt: f.num("pvt") == 1.0,
                    twr: f.num("twr") == 1.0,
                    mil: f.num("mil") == 1.0,
                    elev: f.num("elev"),
                    rwy: f.num("rwy"),
                    hard: f.num("hard") == 1.0,
                    lit: f.num("lit") == 1.0,
                    hdg: f.get("hdg").map(|v| v.as_f64()).unwrap_or(-1.0),
                    fuel: f.num("fuel") == 1.0,
                    ctaf: s(f, "ctaf"),
                    rank: f.num("rank"),
                }),
                (Source::Aero, "navaids") => Kind::Navaid { id: s(f, "id"), name: s(f, "name"), ty: s(f, "type"), freq: s(f, "freq"), var: f.num("var") },
                (Source::Aero, "fixes") => Kind::Fix { id: s(f, "id"), vfr: f.num("vfr") == 1.0 },
                (Source::Aero, "airways") => {
                    // label at the middle of the longest part, along the line
                    let Some(part) = f.parts.iter().max_by_key(|p| p.len()) else { continue };
                    if part.len() < 2 {
                        continue;
                    }
                    let mid = part.len() / 2;
                    let (a, b) = (part[mid.saturating_sub(1)], part[mid]);
                    let m = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
                    if !inside(m) {
                        continue;
                    }
                    let angle = (b[1] - a[1]).atan2(b[0] - a[0]);
                    out.push(Sym { world: world(m), kind: Kind::Airway { id: s(f, "id"), high: f.str("level") == "high", angle } });
                    continue;
                }
                (Source::Obstacles, "obstacles") => Kind::Obstacle { agl: f.num("agl"), msl: f.num("msl"), lit: f.num("lit") == 1.0, ty: s(f, "type") },
                (Source::Airspace, "airspace_labels") => Kind::AirspaceLabel {
                    kind: s(f, "kind"),
                    label: s(f, "label"),
                    range: s(f, "range"),
                    name: s(f, "name"),
                    lo_agl: f.num("lo_agl") == 1.0,
                    lo: f.num("lo"),
                    hi: f.num("hi"),
                },
                (Source::Base, "place") => {
                    let class = f.str("class");
                    let city = class == "city";
                    if !city && class != "town" && class != "village" {
                        continue;
                    }
                    let mut name = s(f, "name:en");
                    if name.is_empty() {
                        name = s(f, "name");
                    }
                    Kind::Place { city, name }
                }
                (Source::Base, "water_name") => Kind::Water { name: s(f, "name:en") },
                _ => continue,
            };
            if f.kind != GeomType::Point {
                continue;
            }
            if let Some(p) = point() {
                out.push(Sym { world: world(p), kind });
            }
        }
    }
    out
}
