//! Weather on the chart: advisories (TFR, AIRMET/SIGMET, G-AIRMET) refreshed every few
//! minutes, METARs / PIREPs for the visible area after the view settles, and the NWS
//! winds-aloft forecast (FB) at its stations.

use crate::backend::{Backend, Task};
use crate::store::WxShow;
use serde_json::Value;
use std::time::{Duration, Instant};

/// Levels of the NWS winds-aloft forecast offered on the chart: (ft, label).
pub const WIND_LEVELS: [(u32, &str); 9] = [(3000, "3,000 ft"), (6000, "6,000 ft"), (9000, "9,000 ft"), (12000, "12,000 ft"), (18000, "FL180"), (24000, "FL240"), (30000, "FL300"), (34000, "FL340"), (39000, "FL390")];

pub struct WindCell {
    pub lon: f64,
    pub lat: f64,
    pub dir: f64,
    pub spd: f64,
    pub temp: Option<f64>,
}

#[derive(Default)]
pub struct Wx {
    pub tfrs: Value,
    pub sigmets: Vec<Value>,
    pub gairmets: Vec<Value>,
    pub gairmet_hour: i64,
    pub metars: Vec<Value>,
    pub pireps: Vec<Value>,
    pub wind_grid: Vec<Option<crate::nav::navlog::WindPoint>>,
    pub updated: Option<chrono::DateTime<chrono::Local>>,
    /// bumped when advisories change (the hazard advisor re-runs)
    pub generation: u64,
    adv_at: Option<Instant>,
    adv_task: Task<(Value, Value, Value)>,
    view_key: String,
    view_changed: Option<Instant>,
    view_at: Option<Instant>,
    metar_task: Task<Value>,
    pirep_task: Task<Value>,
    grid_at: Option<Instant>,
    grid_task: Task<Value>,
}

impl Wx {
    /// `bounds` is (west, south, east, north).
    pub fn update(&mut self, be: &Backend, show: &WxShow, bounds: (f64, f64, f64, f64), zoom: f64) {
        if self.adv_at.is_none_or(|t| t.elapsed() > Duration::from_secs(300)) && !self.adv_task.pending() {
            self.adv_at = Some(Instant::now());
            let s = be.server.clone();
            self.adv_task = be.spawn(async move {
                let t = flightplan_server::wx::tfrs(&s).await.unwrap_or(Value::Null);
                let a = flightplan_server::wx::airsigmets(&s).await.unwrap_or(Value::Null);
                let g = flightplan_server::wx::gairmets(&s).await.unwrap_or(Value::Null);
                (t, a, g)
            });
        }
        if let Some((t, a, g)) = self.adv_task.poll() {
            if !t.is_null() {
                self.tfrs = t;
            }
            if let Value::Array(a) = a {
                self.sigmets = a;
            }
            if let Value::Array(g) = g {
                self.gairmets = g;
            }
            self.updated = Some(chrono::Local::now());
            self.generation += 1;
            be.ctx.request_repaint_after(Duration::from_secs(300));
        }

        // view-dependent data once the view has been still for 400 ms (and every 5 min)
        let (w, s, e, n) = bounds;
        let key = format!("{w:.2},{s:.2},{e:.2},{n:.2},{}{}{}", show.metars, show.pireps, show.winds);
        if key != self.view_key {
            self.view_key = key;
            self.view_changed = Some(Instant::now());
        }
        let settled = self.view_changed.is_some_and(|t| t.elapsed() > Duration::from_millis(400));
        let stale = self.view_at.is_none_or(|t| t.elapsed() > Duration::from_secs(300));
        if settled || stale {
            self.view_changed = None;
            self.view_at = Some(Instant::now());
            let bbox = format!("{s:.2},{w:.2},{n:.2},{e:.2}");
            if show.metars && zoom >= 4.5 && !self.metar_task.pending() {
                let st = be.server.clone();
                let b = bbox.clone();
                self.metar_task = be.spawn(async move { flightplan_server::wx::metars(&st, None, Some(&b)).await.unwrap_or(Value::Null) });
            }
            if show.pireps && zoom >= 4.0 && !self.pirep_task.pending() {
                let st = be.server.clone();
                let b = bbox.clone();
                self.pirep_task = be.spawn(async move { flightplan_server::wx::pireps(&st, &b, 3.0).await.unwrap_or(Value::Null) });
            }
        } else if self.view_changed.is_some() {
            be.ctx.request_repaint_after(Duration::from_millis(420));
        }
        if let Some(Value::Array(m)) = self.metar_task.poll() {
            self.metars = m;
        }
        if let Some(Value::Array(p)) = self.pirep_task.poll() {
            self.pireps = p;
        }
        // the winds-aloft layer: every NWS FB station, refreshed every 30 minutes
        if show.winds && self.grid_at.is_none_or(|t| t.elapsed() > Duration::from_secs(1800)) && !self.grid_task.pending() {
            self.grid_at = Some(Instant::now());
            let st = be.server.clone();
            self.grid_task = be.spawn(async move { flightplan_server::wx::fb_points(&st).await });
        }
        if let Some(v) = self.grid_task.poll() {
            self.wind_grid = v["points"].as_array().map(|a| a.iter().map(|p| serde_json::from_value(p.clone()).ok()).collect()).unwrap_or_default();
        }
    }

    /// G-AIRMET forecast hours on offer.
    pub fn gairmet_hours(&self) -> Vec<i64> {
        let mut h: Vec<i64> = self.gairmets.iter().filter_map(|g| g["forecastHour"].as_i64()).collect();
        h.sort();
        h.dedup();
        h
    }

    pub fn active_sigmets(&self) -> Vec<&Value> {
        let now = chrono::Utc::now().timestamp();
        self.sigmets.iter().filter(|s| s["validTimeTo"].as_i64().is_none_or(|t| t > now)).collect()
    }

    /// FB winds at a level (ft) for the bulletin whose "for use" window covers `when`.
    pub fn wind_cells(&self, level_ft: u32, when: chrono::DateTime<chrono::Utc>) -> Vec<WindCell> {
        let t = when.timestamp();
        self.wind_grid
            .iter()
            .flatten()
            .filter_map(|p| {
                let l = p.levels.iter().find(|l| l.alt as u32 == level_ft)?;
                let best = l.times.iter().find(|x| x.from.is_none_or(|f| t >= f) && x.to.is_none_or(|e| t < e))?;
                Some(WindCell { lon: p.lon, lat: p.lat, dir: best.dir, spd: best.spd, temp: best.temp })
            })
            .collect()
    }
}

pub const HAZARD_COLORS: [(&str, u32); 15] = [
    ("CONVECTIVE", 0xe0452b),
    ("TS", 0xe0452b),
    ("TURB", 0xd48a0b),
    ("TURB-HI", 0xd48a0b),
    ("TURB-LO", 0xe0a23a),
    ("ICE", 0x2f8fd6),
    ("IFR", 0x7b4fc9),
    ("MT_OBSC", 0xa0613a),
    ("MTN OBSCN", 0xa0613a),
    ("MTN", 0xa0613a),
    ("SFC_WND", 0x5a8f3c),
    ("LLWS", 0xc0397a),
    ("FZLVL", 0x3a8fd6),
    ("M_FZLVL", 0x3a8fd6),
    ("ASH", 0x777777),
];

pub fn hazard_color(h: &str) -> egui::Color32 {
    let v = HAZARD_COLORS.iter().find(|(k, _)| *k == h).map(|x| x.1).unwrap_or(0x888888);
    egui::Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}
