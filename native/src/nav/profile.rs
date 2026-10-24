//! Route profile data from the backend: terrain samples, obstacles near the route and the
//! airspace volumes it passes through, plus helpers that relate them to the planned altitude.

use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct Sample {
    /// along-track NM
    pub d: f64,
    pub lat: f64,
    pub lon: f64,
    /// terrain on the centerline, ft MSL
    pub elev: f64,
    /// highest terrain within the corridor, ft MSL
    pub max: f64,
    #[serde(default)]
    pub leg: usize,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Obstacle {
    pub d: f64,
    /// cross-track NM (negative = left of course)
    pub xt: f64,
    pub msl: f64,
    pub agl: f64,
    pub lit: bool,
    #[serde(rename = "type")]
    pub kind: String,
    pub lat: f64,
    pub lon: f64,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Crossing {
    pub kind: String,
    pub name: String,
    pub lo: f64,
    pub hi: f64,
    pub lo_agl: bool,
    #[serde(default)]
    pub times: String,
    pub from: f64,
    pub to: f64,
    /// simplified outline (polygons -> rings -> [lon, lat]) for highlighting on the map
    #[serde(default)]
    pub geom: Vec<Vec<Vec<[f64; 2]>>>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Profile {
    #[serde(default)]
    pub samples: Vec<Sample>,
    #[serde(default)]
    pub obstacles: Vec<Obstacle>,
    #[serde(default)]
    pub airspace: Vec<Crossing>,
}

/// Planned altitude at along-track distance `d` on an [d, ft] path.
pub fn alt_at(path: &[[f64; 2]], d: f64) -> f64 {
    for i in 1..path.len() {
        if d <= path[i][0] {
            let f = (d - path[i - 1][0]) / (path[i][0] - path[i - 1][0]).max(1e-6);
            return path[i - 1][1] + (path[i][1] - path[i - 1][1]) * f;
        }
    }
    path.last().map(|p| p[1]).unwrap_or(0.0)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Level {
    Warning,
    Caution,
    Info,
}

/// A stretch where the planned altitude is inside a volume.
#[derive(Clone, Debug)]
pub struct Incursion {
    pub crossing: usize,
    pub from: f64,
    pub to: f64,
    pub level: Level,
}

/// Stretch with under 1,000 (warn) / 500 (bad) ft clearance over the corridor high point.
#[derive(Clone, Debug)]
pub struct TerrainConflict {
    pub from: f64,
    pub to: f64,
    pub bad: bool,
}

fn incursion_level(kind: &str) -> Option<Level> {
    Some(match kind {
        "P" | "R" => Level::Warning,
        "B" | "C" | "MOA" => Level::Caution,
        "D" | "W" | "A" => Level::Info,
        _ => return None,
    })
}

pub fn terrain_conflicts(p: &Profile, path: &[[f64; 2]]) -> Vec<TerrainConflict> {
    let mut out: Vec<TerrainConflict> = Vec::new();
    if path.is_empty() {
        return out;
    }
    let total = p.samples.last().map(|s| s.d).unwrap_or(0.0);
    for s in &p.samples {
        if s.d < 5.0 || s.d > total - 5.0 {
            continue; // departure climb and pattern entry
        }
        let clr = alt_at(path, s.d) - s.max;
        let bad = clr < 500.0;
        if clr >= 1000.0 {
            continue;
        }
        match out.last_mut() {
            Some(last) if last.bad == bad && s.d - last.to < 0.6 => last.to = s.d,
            _ => out.push(TerrainConflict { from: s.d, to: s.d, bad }),
        }
    }
    out
}

pub fn incursions(p: &Profile, path: &[[f64; 2]]) -> Vec<Incursion> {
    let mut out = Vec::new();
    for (ci, c) in p.airspace.iter().enumerate() {
        let Some(level) = incursion_level(&c.kind) else { continue };
        let mut cur: Option<Incursion> = None;
        for s in &p.samples {
            if s.d < c.from || s.d > c.to {
                continue;
            }
            let alt = alt_at(path, s.d);
            let floor = if c.lo_agl { s.elev + c.lo } else { c.lo };
            let inside = alt >= floor && alt <= c.hi;
            match (&mut cur, inside) {
                (None, true) => cur = Some(Incursion { crossing: ci, from: s.d, to: s.d, level }),
                (Some(x), true) => x.to = s.d,
                (Some(_), false) => out.push(cur.take().unwrap()),
                (None, false) => {}
            }
        }
        if let Some(x) = cur {
            out.push(x);
        }
    }
    out.sort_by(|a, b| a.from.total_cmp(&b.from));
    out
}
