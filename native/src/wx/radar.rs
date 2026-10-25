//! Radar for any time in the next five days. Now: the observed NEXRAD mosaic. Up to ~48 h: HRRR
//! simulated reflectivity (Iowa Environmental Mesonet tiles; 15-minute steps from the latest
//! run to 18 h, hourly from the latest extended run as far as IEM serves it, usually 36-48 h).
//! Beyond that: model precipitation
//! (Open-Meteo) on a lattice, drawn with the same reflectivity colours so it reads like radar.

use crate::backend::{Backend, Task};
use chrono::{DateTime, TimeZone, Utc};
use egui::Color32;
use serde_json::Value;
use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Observed,
    Hrrr,
    Model,
}

/// What the radar layer shows for a requested time.
#[derive(Clone, Debug)]
pub struct Frame {
    pub kind: Kind,
    /// IEM tile layer ("nexrad-n0q-900913", "hrrr::REFD-F0360-0"); None for the model lattice
    pub layer: Option<String>,
    /// time the picture is valid for
    pub valid: DateTime<Utc>,
    /// model run, for forecasts
    pub run: Option<DateTime<Utc>>,
}

impl Frame {
    pub fn source(&self) -> &'static str {
        match self.kind {
            Kind::Observed => "Radar (NEXRAD)",
            Kind::Hrrr => "Forecast radar (HRRR)",
            Kind::Model => "Model precipitation",
        }
    }

    /// "Forecast radar (HRRR) · valid Sun 14:00 · run 03Z"
    pub fn label(&self) -> String {
        let local = self.valid.with_timezone(&chrono::Local);
        match self.kind {
            Kind::Observed => format!("{} · live", self.source()),
            _ => format!("{} · {}{}", self.source(), local.format("%a %H:%M"), self.run.map(|r| format!(" · {}Z run", r.format("%H"))).unwrap_or_default()),
        }
    }

    pub fn tile_url(&self, z: u8, x: u32, y: u32) -> Option<String> {
        self.layer.as_ref().map(|l| format!("https://mesonet.agron.iastate.edu/cache/tile.py/1.0.0/{l}/{z}/{x}/{y}.png"))
    }
}

/// Init times of the HRRR runs IEM serves: the latest (to 18 h) and the latest extended (to 48 h).
#[derive(Default)]
pub struct Runs {
    pub short: Option<DateTime<Utc>>,
    pub extended: Option<DateTime<Utc>>,
    /// last forecast minute of the extended run IEM actually serves tiles for
    pub extended_last: i64,
    at: Option<Instant>,
    task: Task<(Option<i64>, Option<i64>, i64)>,
}

impl Runs {
    pub fn update(&mut self, be: &Backend) {
        if self.at.is_none_or(|t| t.elapsed() > Duration::from_secs(600)) && !self.task.pending() {
            self.at = Some(Instant::now());
            let st = be.server.clone();
            self.task = be.spawn(async move {
                let init = |v: Value| v["model_init_utc"].as_str().and_then(|s| DateTime::parse_from_rfc3339(s).ok()).map(|d| d.timestamp());
                let base = "https://mesonet.agron.iastate.edu/data/gis/images/4326/hrrr";
                let a = st.cache.get_json(&format!("{base}/refd_0000.json"), Duration::from_secs(300)).await.ok().and_then(init);
                let b = st.cache.get_json(&format!("{base}/refd_1140.json"), Duration::from_secs(300)).await.ok().and_then(init);
                // the extended run's tiles stop short of 48 h at times: find the last hour served
                // (binary search on a small CONUS tile)
                let ok = |m: i64| {
                    let http = st.cache.http().clone();
                    async move {
                        let url = format!("https://mesonet.agron.iastate.edu/cache/tile.py/1.0.0/hrrr::REFD-F{m:04}-0/4/3/5.png");
                        matches!(http.get(url).send().await, Ok(r) if r.status().is_success())
                    }
                };
                let mut last = 1080;
                if b.is_some() {
                    let (mut lo, mut hi) = (19i64, 47i64);
                    if ok(lo * 60).await {
                        while lo < hi {
                            let mid = (lo + hi + 1) / 2;
                            if ok(mid * 60).await {
                                lo = mid;
                            } else {
                                hi = mid - 1;
                            }
                        }
                        last = lo * 60;
                    }
                }
                (a, b, last)
            });
        }
        if let Some((a, b, last)) = self.task.poll() {
            self.short = a.and_then(|s| Utc.timestamp_opt(s, 0).single());
            self.extended = b.and_then(|s| Utc.timestamp_opt(s, 0).single());
            self.extended_last = last;
        }
    }
}

/// The frame to show for `when`.
pub fn frame_for(when: DateTime<Utc>, now: DateTime<Utc>, runs: &Runs) -> Frame {
    if (when - now).num_minutes() < 8 {
        // the mosaic updates every ~5 minutes
        let bucket = Utc.timestamp_opt(now.timestamp() / 300 * 300, 0).single().unwrap_or(now);
        return Frame { kind: Kind::Observed, layer: Some("nexrad-n0q-900913".into()), valid: bucket, run: None };
    }
    if let Some(r) = runs.short {
        let m = ((when - r).num_minutes() as f64 / 15.0).round() as i64 * 15;
        if (0..=1080).contains(&m) {
            return Frame { kind: Kind::Hrrr, layer: Some(format!("hrrr::REFD-F{m:04}-0")), valid: r + chrono::Duration::minutes(m), run: Some(r) };
        }
    }
    if let Some(r) = runs.extended {
        let m = ((when - r).num_minutes() as f64 / 60.0).round() as i64 * 60;
        if (1140..=runs.extended_last).contains(&m) {
            return Frame { kind: Kind::Hrrr, layer: Some(format!("hrrr::REFD-F{m:04}-0")), valid: r + chrono::Duration::minutes(m), run: Some(r) };
        }
    }
    let hour = Utc.timestamp_opt(when.timestamp() / 3600 * 3600, 0).single().unwrap_or(when);
    Frame { kind: Kind::Model, layer: None, valid: hour, run: None }
}

/// NWS-style reflectivity colours (dBZ), used for the model lattice and the legend.
pub fn dbz_color(dbz: f32) -> Color32 {
    const STOPS: [(f32, [u8; 3]); 10] = [
        (10.0, [0x6e, 0xc9, 0xe8]),
        (20.0, [0x2f, 0x9e, 0x5a]),
        (25.0, [0x29, 0xb8, 0x3a]),
        (30.0, [0x1d, 0x8f, 0x23]),
        (35.0, [0xe8, 0xd8, 0x2a]),
        (40.0, [0xe8, 0xa2, 0x1d]),
        (45.0, [0xe0, 0x5a, 0x1a]),
        (50.0, [0xc8, 0x1e, 0x1e]),
        (55.0, [0xa8, 0x14, 0x5c]),
        (65.0, [0xb0, 0x5c, 0xe0]),
    ];
    // below ~0.15 mm/h is drizzle a model smears over whole regions: leave it out
    if dbz < 8.0 {
        return Color32::TRANSPARENT;
    }
    let mut c = STOPS[0].1;
    for (v, col) in STOPS {
        if dbz >= v {
            c = col;
        }
    }
    let a = if dbz < 12.0 { ((dbz - 8.0) / 4.0 * 160.0) as u8 } else { 200 };
    Color32::from_rgba_unmultiplied(c[0], c[1], c[2], a)
}

/// Rain rate (mm/h) to reflectivity with Marshall-Palmer (Z = 200 R^1.6).
pub fn rate_to_dbz(mm_h: f32) -> f32 {
    if mm_h <= 0.02 {
        return 0.0;
    }
    10.0 * (200.0 * mm_h.powf(1.6)).log10()
}

#[derive(Clone)]
struct Series {
    t0: i64,
    precip: Vec<f32>,
    code: Vec<u8>,
}

/// Model precipitation on a lattice snapped to whole steps (so panning reuses fetched points),
/// rendered to a texture for the requested hour.
#[derive(Default)]
pub struct ModelGrid {
    points: HashMap<(i32, i32), (Series, Instant)>,
    task: Task<Vec<((i32, i32), Series)>>,
    asked: Option<Instant>,
    texture: Option<(String, egui::TextureHandle)>,
    /// lattice bounds of the current texture: west, south, east, north
    pub bounds: [f64; 4],
    pub loading: bool,
}

const UNIT: f64 = 0.125;

impl ModelGrid {
    /// Make sure the lattice over the view is fetched and the texture for `when` is built.
    /// Returns the texture and its bounds (west, south, east, north) when ready.
    pub fn update(&mut self, be: &Backend, ctx: &egui::Context, view: (f64, f64, f64, f64), zoom: f64, when: DateTime<Utc>) -> Option<(egui::TextureId, [f64; 4])> {
        let (w, s, e, n) = view;
        let base = if zoom < 5.0 { 1.0 } else if zoom < 6.5 { 0.5 } else { 0.25 };
        let mut step = base;
        while ((e - w) / step + 3.0) * ((n - s) / step + 3.0) > 260.0 {
            step *= 2.0;
        }
        let k = (step / UNIT).round() as i32;
        let (i0, i1) = (((w / step).floor() as i32 - 1) * k, ((e / step).ceil() as i32 + 1) * k);
        let (j0, j1) = (((s / step).floor() as i32 - 1) * k, ((n / step).ceil() as i32 + 1) * k);
        let mut missing = Vec::new();
        let mut j = j0;
        while j <= j1 {
            let mut i = i0;
            while i <= i1 {
                if self.points.get(&(i, j)).is_none_or(|(_, at)| at.elapsed() > Duration::from_secs(3600)) {
                    missing.push((i, j));
                }
                i += k;
            }
            j += k;
        }
        if let Some(list) = self.task.poll() {
            for (key, s) in list {
                self.points.insert(key, (s, Instant::now()));
            }
            self.loading = false;
            self.texture = None;
        }
        if !missing.is_empty() && !self.task.pending() && self.asked.is_none_or(|t| t.elapsed() > Duration::from_secs(5)) {
            self.asked = Some(Instant::now());
            self.loading = true;
            missing.truncate(200);
            let st = be.server.clone();
            self.task = be.spawn(async move {
                let mut out = Vec::new();
                for chunk in missing.chunks(100) {
                    let lats: Vec<String> = chunk.iter().map(|(_, j)| format!("{:.3}", *j as f64 * UNIT)).collect();
                    let lons: Vec<String> = chunk.iter().map(|(i, _)| format!("{:.3}", *i as f64 * UNIT)).collect();
                    let url = format!(
                        "https://api.open-meteo.com/v1/forecast?latitude={}&longitude={}&hourly=precipitation,weather_code&forecast_days=6&timeformat=unixtime",
                        lats.join(","),
                        lons.join(",")
                    );
                    let Ok(v) = st.cache.get_json(&url, Duration::from_secs(3600)).await else { continue };
                    let list: Vec<Value> = match v {
                        Value::Array(a) => a,
                        o => vec![o],
                    };
                    for (key, o) in chunk.iter().zip(list.iter()) {
                        let h = &o["hourly"];
                        let t0 = h["time"].as_array().and_then(|a| a.first()).and_then(|x| x.as_i64()).unwrap_or(0);
                        let precip = h["precipitation"].as_array().map(|a| a.iter().map(|x| x.as_f64().unwrap_or(0.0) as f32).collect()).unwrap_or_default();
                        let code = h["weather_code"].as_array().map(|a| a.iter().map(|x| x.as_i64().unwrap_or(0) as u8).collect()).unwrap_or_default();
                        out.push((*key, Series { t0, precip, code }));
                    }
                }
                out
            });
        }
        // build the texture once every lattice point has data
        let hour = when.timestamp() / 3600 * 3600;
        let tex_key = format!("{i0},{i1},{j0},{j1},{k},{hour}");
        if self.texture.as_ref().is_some_and(|t| t.0 == tex_key) {
            return self.texture.as_ref().map(|t| (t.1.id(), self.bounds));
        }
        let nx = ((i1 - i0) / k + 1) as usize;
        let ny = ((j1 - j0) / k + 1) as usize;
        let mut grid = vec![0f32; nx * ny];
        let mut storm = vec![false; nx * ny];
        for (b, jj) in (0..ny).enumerate() {
            for ii in 0..nx {
                let key = (i0 + ii as i32 * k, j0 + jj as i32 * k);
                let Some((s, _)) = self.points.get(&key) else {
                    // keep showing the last texture while the rest loads
                    return self.texture.as_ref().map(|t| (t.1.id(), self.bounds));
                };
                let idx = ((hour - s.t0) / 3600).max(0) as usize;
                grid[b * nx + ii] = s.precip.get(idx).copied().unwrap_or(0.0);
                storm[b * nx + ii] = s.code.get(idx).is_some_and(|c| *c >= 95);
            }
        }
        let (west, east) = (i0 as f64 * UNIT, i1 as f64 * UNIT);
        let (south, north) = (j0 as f64 * UNIT, j1 as f64 * UNIT);
        let img = render(&grid, &storm, nx, ny, [west, south, east, north]);
        if std::env::var("FP_DEBUG_RADAR").is_ok() {
            eprintln!("model grid {nx}x{ny} step {step} bounds {west},{south},{east},{north} wet {} max {:.2}", grid.iter().filter(|v| **v > 0.1).count(), grid.iter().cloned().fold(0.0, f32::max));
        }
        let tex = ctx.load_texture("model-precip", img, egui::TextureOptions::LINEAR);
        self.bounds = [west, south, east, north];
        self.texture = Some((tex_key, tex));
        self.texture.as_ref().map(|t| (t.1.id(), self.bounds))
    }
}

/// Bilinear-interpolated precipitation to an image whose rows are even in Web Mercator y,
/// so it lines up with the chart when stretched over the lattice bounds.
fn render(grid: &[f32], storm: &[bool], nx: usize, ny: usize, b: [f64; 4]) -> egui::ColorImage {
    let w = ((nx - 1) * 10).clamp(64, 512);
    let h = ((ny - 1) * 10).clamp(64, 512);
    let merc = |lat: f64| (std::f64::consts::FRAC_PI_4 + lat.to_radians() / 2.0).tan().ln();
    let (y_top, y_bot) = (merc(b[3]), merc(b[1]));
    let mut px = vec![Color32::TRANSPARENT; w * h];
    for row in 0..h {
        let my = y_top + (y_bot - y_top) * (row as f64 + 0.5) / h as f64;
        let lat = (2.0 * my.exp().atan() - std::f64::consts::FRAC_PI_2).to_degrees();
        let fy = ((lat - b[1]) / (b[3] - b[1]) * (ny - 1) as f64).clamp(0.0, (ny - 1) as f64);
        let (y0, ty) = (fy.floor() as usize, (fy - fy.floor()) as f32);
        let y1 = (y0 + 1).min(ny - 1);
        for col in 0..w {
            let fx = (col as f64 + 0.5) / w as f64 * (nx - 1) as f64;
            let (x0, tx) = (fx.floor() as usize, (fx - fx.floor()) as f32);
            let x1 = (x0 + 1).min(nx - 1);
            let v = |x: usize, y: usize| grid[y * nx + x];
            let r = (v(x0, y0) * (1.0 - tx) + v(x1, y0) * tx) * (1.0 - ty) + (v(x0, y1) * (1.0 - tx) + v(x1, y1) * tx) * ty;
            let mut dbz = rate_to_dbz(r);
            let near = |x: usize, y: usize| storm[y * nx + x];
            if dbz > 15.0 && (near(x0, y0) || near(x1, y0) || near(x0, y1) || near(x1, y1)) {
                dbz = dbz.max(48.0);
            }
            px[row * w + col] = dbz_color(dbz);
        }
    }
    egui::ColorImage { size: [w, h], source_size: egui::vec2(w as f32, h as f32), pixels: px }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_by_lead_time() {
        let now = Utc.with_ymd_and_hms(2026, 10, 25, 4, 40, 0).unwrap();
        let runs = Runs { short: Some(Utc.with_ymd_and_hms(2026, 10, 25, 3, 0, 0).unwrap()), extended: Some(Utc.with_ymd_and_hms(2026, 10, 24, 18, 0, 0).unwrap()), extended_last: 2820, ..Default::default() };
        assert_eq!(frame_for(now, now, &runs).kind, Kind::Observed);
        let f = frame_for(now + chrono::Duration::hours(2), now, &runs);
        assert_eq!(f.kind, Kind::Hrrr);
        assert_eq!(f.layer.as_deref(), Some("hrrr::REFD-F0225-0"), "3:00 run + 3 h 40 → nearest 15 min");
        // 30 h ahead: past the 18 h run, inside the extended run (init 18Z yesterday → +40 h 40 min)
        let f = frame_for(now + chrono::Duration::hours(30), now, &runs);
        assert_eq!(f.kind, Kind::Hrrr);
        assert_eq!(f.layer.as_deref(), Some("hrrr::REFD-F2460-0"));
        let f = frame_for(now + chrono::Duration::hours(60), now, &runs);
        assert_eq!(f.kind, Kind::Model);
    }

    #[test]
    fn reflectivity() {
        assert_eq!(rate_to_dbz(0.0), 0.0);
        assert!((rate_to_dbz(1.0) - 23.0).abs() < 0.1);
        assert!(rate_to_dbz(20.0) > 43.0);
        assert_eq!(dbz_color(2.0), Color32::TRANSPARENT);
    }
}
