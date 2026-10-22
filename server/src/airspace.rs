//! Route vs. airspace: which volumes the route passes through, and where (along-track NM).

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Deserialize)]
struct Volume {
    k: String,
    n: String,
    lo: i64,
    hi: i64,
    agl: i64,
    #[serde(default)]
    t: String,
    b: [f64; 4],
    /// polygons -> rings -> [lon, lat]
    p: Vec<Vec<Vec<[f64; 2]>>>,
}

pub struct Airspace {
    vols: Vec<Volume>,
}

#[derive(Serialize)]
pub struct Crossing {
    pub kind: String,
    pub name: String,
    pub lo: i64,
    pub hi: i64,
    pub lo_agl: bool,
    pub times: String,
    /// along-track NM where the route enters / leaves
    pub from: f64,
    pub to: f64,
}

fn in_ring(x: f64, y: f64, ring: &[[f64; 2]]) -> bool {
    let mut inside = false;
    let mut j = ring.len().wrapping_sub(1);
    for i in 0..ring.len() {
        let (xi, yi) = (ring[i][0], ring[i][1]);
        let (xj, yj) = (ring[j][0], ring[j][1]);
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

impl Volume {
    fn contains(&self, lon: f64, lat: f64) -> bool {
        if lon < self.b[0] || lat < self.b[1] || lon > self.b[2] || lat > self.b[3] {
            return false;
        }
        self.p.iter().any(|rings| !rings.is_empty() && in_ring(lon, lat, &rings[0]) && !rings[1..].iter().any(|h| in_ring(lon, lat, h)))
    }
}

impl Airspace {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let vols: Vec<Volume> = serde_json::from_slice(&std::fs::read(path)?)?;
        tracing::info!("airspace index: {} volumes", vols.len());
        Ok(Self { vols })
    }

    /// `samples` are (along-track NM, lat, lon) in route order.
    pub fn crossings(&self, samples: &[(f64, f64, f64)]) -> Vec<Crossing> {
        if samples.is_empty() {
            return vec![];
        }
        let (mut x0, mut y0, mut x1, mut y1) = (180.0f64, 90.0f64, -180.0f64, -90.0f64);
        for &(_, la, lo) in samples {
            x0 = x0.min(lo);
            x1 = x1.max(lo);
            y0 = y0.min(la);
            y1 = y1.max(la);
        }
        let mut out = Vec::new();
        for v in self.vols.iter().filter(|v| v.b[0] <= x1 && v.b[2] >= x0 && v.b[1] <= y1 && v.b[3] >= y0) {
            let mut start: Option<f64> = None;
            let mut last_d = 0.0;
            for &(d, la, lo) in samples {
                let inside = v.contains(lo, la);
                match (inside, start) {
                    (true, None) => start = Some(d),
                    (false, Some(s)) => {
                        out.push(self.crossing(v, s, last_d));
                        start = None;
                    }
                    _ => {}
                }
                last_d = d;
            }
            if let Some(s) = start {
                out.push(self.crossing(v, s, last_d));
            }
        }
        out.sort_by(|a, b| a.from.total_cmp(&b.from));
        out
    }

    fn crossing(&self, v: &Volume, from: f64, to: f64) -> Crossing {
        Crossing { kind: v.k.clone(), name: v.n.clone(), lo: v.lo, hi: v.hi, lo_agl: v.agl == 1, times: v.t.clone(), from, to }
    }
}
