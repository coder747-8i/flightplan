//! Official NWS forecast products beyond the radar's reach, shown as NOAA publishes them (map
//! images rendered by NOAA's map service, mapservices.weather.noaa.gov) and read along the route
//! for the go / no-go call:
//! - WPC National Forecast Chart, days 1-3: fronts and areas of rain, thunderstorms, snow,
//!   freezing rain, severe storms, heavy rain.
//! - WPC quantitative precipitation forecast, days 4-5.
//! - SPC convective outlooks: categorical days 1-3, probabilistic days 4-8.
//! Nothing here is computed by the app: it picks which product covers a time and shows or
//! reads it.

use crate::backend::{Backend, Task};
use chrono::{DateTime, Duration, TimeZone, Timelike, Utc};
use std::time::Instant;

pub use flightplan_server::forecast::{spc_layer, wpc_chart_layers, Hit, BASE, SPC, WPC_CHART, WPC_QPF};

/// When the current products' day 1 ends (they run 12Z to 12Z).
#[derive(Default)]
pub struct Outlooks {
    pub wpc_day1_end: Option<DateTime<Utc>>,
    pub spc_day1_end: Option<DateTime<Utc>>,
    at: Option<Instant>,
    task: Task<(Option<i64>, Option<i64>)>,
}

impl Outlooks {
    /// Fixed day boundaries (tests).
    pub fn fixed(wpc_day1_end: DateTime<Utc>, spc_day1_end: DateTime<Utc>) -> Self {
        Outlooks { wpc_day1_end: Some(wpc_day1_end), spc_day1_end: Some(spc_day1_end), ..Default::default() }
    }
}

/// The next 12Z after `now`, the usual end of a day-1 product.
fn next_12z(now: DateTime<Utc>) -> DateTime<Utc> {
    let today = now.date_naive().and_hms_opt(12, 0, 0).map(|d| d.and_utc()).unwrap_or(now);
    if now.hour() < 12 { today } else { today + Duration::days(1) }
}

impl Outlooks {
    pub fn update(&mut self, be: &Backend) {
        if self.at.is_none_or(|t| t.elapsed().as_secs() > 1800) && !self.task.pending() {
            self.at = Some(Instant::now());
            let st = be.server.clone();
            self.task = be.spawn(async move { flightplan_server::forecast::outlook_ends(&st).await });
        }
        if let Some((w, s)) = self.task.poll() {
            self.wpc_day1_end = w.and_then(|t| Utc.timestamp_opt(t, 0).single());
            self.spc_day1_end = s.and_then(|t| Utc.timestamp_opt(t, 0).single());
        }
    }

    fn day(end1: DateTime<Utc>, t: DateTime<Utc>) -> u8 {
        if t <= end1 {
            1
        } else {
            (1 + ((t - end1).num_minutes() as f64 / 1440.0).ceil() as i64).clamp(1, 99) as u8
        }
    }

    /// WPC chart day (1-3) covering `t`.
    pub fn wpc_day(&self, t: DateTime<Utc>) -> u8 {
        Self::day(self.wpc_day1_end.unwrap_or_else(|| next_12z(Utc::now())), t)
    }

    /// SPC outlook day (1-8) covering `t`.
    pub fn spc_day(&self, t: DateTime<Utc>) -> u8 {
        Self::day(self.spc_day1_end.unwrap_or_else(|| next_12z(Utc::now())), t)
    }
}

/// Web Mercator bounds (metres) of a tile, for the map service's export call.
pub fn tile_bbox(z: u8, x: u32, y: u32) -> [f64; 4] {
    const R: f64 = 20037508.342789244;
    let n = (1u64 << z) as f64;
    let size = 2.0 * R / n;
    let xmin = -R + x as f64 * size;
    let ymax = R - y as f64 * size;
    [xmin, ymax - size, xmin + size, ymax]
}

/// A 512 px PNG of some layers of a NOAA map service for one tile.
pub fn export_url(service: &str, layers: &[u32], z: u8, x: u32, y: u32) -> String {
    let b = tile_bbox(z, x, y);
    let ids: Vec<String> = layers.iter().map(|l| l.to_string()).collect();
    format!(
        "{BASE}/{service}/MapServer/export?bbox={:.1},{:.1},{:.1},{:.1}&bboxSR=3857&imageSR=3857&size=512,512&format=png32&transparent=true&layers=show:{}&dpi=96&f=image",
        b[0],
        b[1],
        b[2],
        b[3],
        ids.join(",")
    )
}

/// What the route crosses in the WPC chart (days 1-3) and the SPC outlooks (days 1-8).
pub async fn along_route(st: std::sync::Arc<flightplan_server::AppState>, path: Vec<[f64; 2]>) -> Vec<Hit> {
    flightplan_server::forecast::along_route(st, path).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_and_layers() {
        assert_eq!(wpc_chart_layers(2)[0], 14);
        assert_eq!(spc_layer(3), 17);
        assert_eq!(spc_layer(6), 23);
        let end = Utc.with_ymd_and_hms(2026, 10, 25, 12, 0, 0).unwrap();
        assert_eq!(Outlooks::day(end, end - Duration::hours(3)), 1);
        assert_eq!(Outlooks::day(end, end + Duration::hours(3)), 2);
        assert_eq!(Outlooks::day(end, end + Duration::hours(30)), 3);
    }

    #[test]
    fn tile_bounds() {
        let b = tile_bbox(0, 0, 0);
        assert!((b[0] + 20037508.34).abs() < 1.0 && (b[3] - 20037508.34).abs() < 1.0);
    }
}
