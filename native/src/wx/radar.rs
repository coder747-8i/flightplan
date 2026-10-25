//! The weather picture for any time in the next five days, always an official product shown as
//! published. Now: the observed NEXRAD mosaic. Up to ~48 h: HRRR simulated reflectivity (Iowa
//! Environmental Mesonet tiles; 15-minute steps from the latest run to 18 h, hourly from the
//! latest extended run as far as IEM serves it, usually 36-48 h). Then the WPC National Forecast
//! Chart (days 1-3) and the WPC precipitation forecast (days 4-5) from NOAA's map service.

use super::official::{self, Outlooks};
use crate::backend::{Backend, Task};
use chrono::{DateTime, Duration, TimeZone, Utc};
use egui::Color32;
use serde_json::Value;
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Observed,
    Hrrr,
    /// WPC National Forecast Chart for a day (1-3)
    WpcChart(u8),
    /// WPC 48-hour precipitation forecast, days 4-5
    WpcQpf,
    /// SPC convective outlook for a day (1-8)
    Spc(u8),
}

/// What the weather layer shows for a requested time.
#[derive(Clone, Debug)]
pub struct Frame {
    pub kind: Kind,
    /// a key for the tile cache: the IEM layer name or the NOAA product and issue
    pub layer: String,
    /// time the picture is valid for (forecast charts: the end of their day)
    pub valid: DateTime<Utc>,
    /// model run, for HRRR
    pub run: Option<DateTime<Utc>>,
}

impl Frame {
    pub fn source(&self) -> &'static str {
        match self.kind {
            Kind::Observed => "Radar (NEXRAD)",
            Kind::Hrrr => "Forecast radar (HRRR)",
            Kind::WpcChart(_) => "WPC forecast chart",
            Kind::WpcQpf => "WPC precipitation forecast",
            Kind::Spc(_) => "SPC convective outlook",
        }
    }

    /// "Forecast radar (HRRR) · Sun 14:00 · 03Z run"
    pub fn label(&self) -> String {
        let local = self.valid.with_timezone(&chrono::Local);
        match self.kind {
            Kind::Observed => format!("{} · live", self.source()),
            Kind::Hrrr => format!("{} · {}{}", self.source(), local.format("%a %H:%M"), self.run.map(|r| format!(" · {}Z run", r.format("%H"))).unwrap_or_default()),
            Kind::WpcChart(d) => format!("{} · day {d}, through {}", self.source(), local.format("%a %H:%M")),
            Kind::WpcQpf => format!("{} · days 4-5", self.source()),
            Kind::Spc(d) => format!("{} · day {d}", self.source()),
        }
    }

    pub fn tile_url(&self, z: u8, x: u32, y: u32) -> String {
        match self.kind {
            Kind::Observed | Kind::Hrrr => format!("https://mesonet.agron.iastate.edu/cache/tile.py/1.0.0/{}/{z}/{x}/{y}.png", self.layer),
            Kind::WpcChart(d) => official::export_url(official::WPC_CHART, &official::wpc_chart_layers(d), z, x, y),
            Kind::WpcQpf => official::export_url(official::WPC_QPF, &[4], z, x, y),
            Kind::Spc(d) => official::export_url(official::SPC, &[official::spc_layer(d)], z, x, y),
        }
    }
}

/// Init times of the HRRR runs IEM serves: the latest (to 18 h) and the latest extended.
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
        if self.at.is_none_or(|t| t.elapsed().as_secs() > 600) && !self.task.pending() {
            self.at = Some(Instant::now());
            let st = be.server.clone();
            self.task = be.spawn(async move {
                let init = |v: Value| v["model_init_utc"].as_str().and_then(|s| DateTime::parse_from_rfc3339(s).ok()).map(|d| d.timestamp());
                let base = "https://mesonet.agron.iastate.edu/data/gis/images/4326/hrrr";
                let ttl = std::time::Duration::from_secs(300);
                let a = st.cache.get_json(&format!("{base}/refd_0000.json"), ttl).await.ok().and_then(init);
                let b = st.cache.get_json(&format!("{base}/refd_1140.json"), ttl).await.ok().and_then(init);
                // the extended run's tiles stop short of 48 h at times: find the last hour served
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

    /// The last time HRRR covers.
    pub fn reach(&self) -> Option<DateTime<Utc>> {
        let ext = self.extended.map(|r| r + Duration::minutes(self.extended_last));
        let short = self.short.map(|r| r + Duration::hours(18));
        ext.max(short)
    }
}

/// The frame to show for `when` (None past what the forecasts cover).
pub fn frame_for(when: DateTime<Utc>, now: DateTime<Utc>, runs: &Runs, outlooks: &Outlooks) -> Option<Frame> {
    if (when - now).num_minutes() < 8 {
        // the mosaic updates every ~5 minutes
        let bucket = Utc.timestamp_opt(now.timestamp() / 300 * 300, 0).single().unwrap_or(now);
        return Some(Frame { kind: Kind::Observed, layer: "nexrad-n0q-900913".into(), valid: bucket, run: None });
    }
    if let Some(r) = runs.short {
        let m = ((when - r).num_minutes() as f64 / 15.0).round() as i64 * 15;
        if (0..=1080).contains(&m) {
            return Some(Frame { kind: Kind::Hrrr, layer: format!("hrrr::REFD-F{m:04}-0"), valid: r + Duration::minutes(m), run: Some(r) });
        }
    }
    if let Some(r) = runs.extended {
        let m = ((when - r).num_minutes() as f64 / 60.0).round() as i64 * 60;
        if (1140..=runs.extended_last).contains(&m) {
            return Some(Frame { kind: Kind::Hrrr, layer: format!("hrrr::REFD-F{m:04}-0"), valid: r + Duration::minutes(m), run: Some(r) });
        }
    }
    let day = outlooks.wpc_day(when);
    let end1 = outlooks.wpc_day1_end.unwrap_or(now);
    if day <= 3 {
        return Some(Frame { kind: Kind::WpcChart(day), layer: format!("wpc-chart-{day}-{}", end1.timestamp()), valid: end1 + Duration::days(day as i64 - 1), run: None });
    }
    if day <= 5 {
        return Some(Frame { kind: Kind::WpcQpf, layer: format!("wpc-qpf45-{}", end1.timestamp()), valid: end1 + Duration::days(4), run: None });
    }
    None
}

/// The SPC convective outlook covering `when` (days 1-8).
pub fn spc_frame(when: DateTime<Utc>, outlooks: &Outlooks) -> Option<Frame> {
    let day = outlooks.spc_day(when);
    let end1 = outlooks.spc_day1_end.unwrap_or_else(Utc::now);
    (day <= 8).then(|| Frame { kind: Kind::Spc(day), layer: format!("spc-{day}-{}", end1.timestamp()), valid: end1 + Duration::days(day as i64 - 1), run: None })
}

/// NWS reflectivity colours (dBZ), for the radar legend.
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
    let mut c = STOPS[0].1;
    for (v, col) in STOPS {
        if dbz >= v {
            c = col;
        }
    }
    Color32::from_rgb(c[0], c[1], c[2])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_by_lead_time() {
        let now = Utc.with_ymd_and_hms(2026, 10, 25, 4, 40, 0).unwrap();
        let runs = Runs { short: Some(Utc.with_ymd_and_hms(2026, 10, 25, 3, 0, 0).unwrap()), extended: Some(Utc.with_ymd_and_hms(2026, 10, 24, 18, 0, 0).unwrap()), extended_last: 2820, ..Default::default() };
        let d1 = Utc.with_ymd_and_hms(2026, 10, 25, 12, 0, 0).unwrap();
        let outlooks = Outlooks::fixed(d1, d1);
        let f = |h: i64| frame_for(now + Duration::hours(h), now, &runs, &outlooks).map(|f| (f.kind, f.layer));
        assert_eq!(f(0).unwrap().0, Kind::Observed);
        assert_eq!(f(2).unwrap().1, "hrrr::REFD-F0225-0", "3:00 run + 3 h 40 → nearest 15 min");
        // 30 h ahead: past the 18 h run, inside the extended run (init 18Z yesterday → +40 h 40 min)
        assert_eq!(f(30).unwrap().1, "hrrr::REFD-F2460-0");
        // 50 h ahead: Tue 06:40Z, WPC day 3 (day 1 ends Sun 12Z, day 3 Tue 12Z); 60 h: day 4
        assert_eq!(f(50).unwrap().0, Kind::WpcChart(3));
        assert_eq!(f(60).unwrap().0, Kind::WpcQpf);
        assert_eq!(f(90).unwrap().0, Kind::WpcQpf);
        assert!(f(200).is_none());
    }
}
