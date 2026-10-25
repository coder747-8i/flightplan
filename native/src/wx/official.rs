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
use chrono::{DateTime, Duration, NaiveDate, TimeZone, Timelike, Utc};
use serde_json::{json, Value};
use std::time::Instant;

pub const BASE: &str = "https://mapservices.weather.noaa.gov/vector/rest/services";
pub const WPC_CHART: &str = "outlooks/natl_fcst_wx_chart";
pub const WPC_QPF: &str = "precip/wpc_qpf";
pub const SPC: &str = "outlooks/SPC_wx_outlks";

/// The WPC chart's weather layers for a day (fronts and areas; no highs/lows or fire weather).
pub fn wpc_chart_layers(day: u8) -> Vec<u32> {
    let o = (day as u32 - 1) * 12;
    [2, 3, 4, 5, 6, 7, 8, 10, 11].iter().map(|l| l + o).collect()
}

/// SPC layer for a day: the categorical outlook for days 1-3, the probabilistic one for 4-8.
pub fn spc_layer(day: u8) -> u32 {
    match day {
        1 => 1,
        2 => 9,
        3 => 17,
        d => 21 + (d as u32 - 4),
    }
}

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

/// "…Valid through: Sun Morning Oct 25 2026…" → 2026-10-25 12Z.
fn valid_through(popup: &str) -> Option<DateTime<Utc>> {
    let rest = popup.split("Valid through:").nth(1)?;
    let words: Vec<&str> = rest.split(|c: char| c.is_whitespace() || c == '<').filter(|w| !w.is_empty()).collect();
    // last three words before any markup: "Oct", "25", "2026"
    for w in words.windows(3) {
        if let Ok(d) = NaiveDate::parse_from_str(&format!("{} {} {}", w[0], w[1], w[2]), "%b %d %Y") {
            return d.and_hms_opt(12, 0, 0).map(|d| d.and_utc());
        }
    }
    None
}

impl Outlooks {
    pub fn update(&mut self, be: &Backend) {
        if self.at.is_none_or(|t| t.elapsed().as_secs() > 1800) && !self.task.pending() {
            self.at = Some(Instant::now());
            let st = be.server.clone();
            self.task = be.spawn(async move {
                let ttl = std::time::Duration::from_secs(1800);
                // WPC: any day-1 area carries "Valid through: <day> Morning <date>"
                let mut wpc = None;
                for layer in [3, 4, 5, 6, 8] {
                    let url = format!("{BASE}/{WPC_CHART}/MapServer/{layer}/query?where=1%3D1&outFields=popupconte&returnGeometry=false&resultRecordCount=1&f=json");
                    if let Ok(v) = st.cache.get_json(&url, ttl).await {
                        if let Some(t) = v["features"][0]["attributes"]["popupconte"].as_str().and_then(valid_through) {
                            wpc = Some(t.timestamp());
                            break;
                        }
                    }
                }
                // SPC: the day-1 categorical outlook's expiry, "202610251200"
                let url = format!("{BASE}/{SPC}/MapServer/1/query?where=1%3D1&outFields=expire&returnGeometry=false&resultRecordCount=1&f=json");
                let spc = st.cache.get_json(&url, ttl).await.ok().and_then(|v| v["features"][0]["attributes"]["expire"].as_str().and_then(|s| chrono::NaiveDateTime::parse_from_str(s, "%Y%m%d%H%M").ok())).map(|n| n.and_utc().timestamp());
                (wpc, spc)
            });
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

/// One official product the route crosses.
#[derive(Clone, Debug)]
pub struct Hit {
    pub product: &'static str,
    pub day: u8,
    /// e.g. "TSTM" / "General Thunderstorms Risk", or "Rain/Thunderstorms"
    pub label: String,
    pub detail: String,
}

/// What the route crosses in the WPC chart (days 1-3) and the SPC outlooks (days 1-8).
pub async fn along_route(st: std::sync::Arc<flightplan_server::AppState>, path: Vec<[f64; 2]>) -> Vec<Hit> {
    if path.len() < 2 {
        return vec![];
    }
    let (mut w, mut s, mut e, mut n) = (180.0f64, 90.0f64, -180.0f64, -90.0f64);
    for p in &path {
        w = w.min(p[0]);
        e = e.max(p[0]);
        s = s.min(p[1]);
        n = n.max(p[1]);
    }
    let geom = json!({"paths": [path], "spatialReference": {"wkid": 4326}}).to_string();
    let extent = format!("{},{},{},{}", w - 1.0, s - 1.0, e + 1.0, n + 1.0);
    let identify = |service: &str, layers: String| {
        let url = format!("{BASE}/{service}/MapServer/identify");
        let q = vec![
            ("geometry", geom.clone()),
            ("geometryType", "esriGeometryPolyline".to_string()),
            ("sr", "4326".to_string()),
            ("layers", format!("all:{layers}")),
            ("tolerance", "1".to_string()),
            ("mapExtent", extent.clone()),
            ("imageDisplay", "800,600,96".to_string()),
            ("returnGeometry", "false".to_string()),
            ("f", "json".to_string()),
        ];
        let http = st.cache.http().clone();
        async move {
            let r = http.get(url).query(&q).send().await.ok()?.error_for_status().ok()?;
            r.json::<Value>().await.ok()
        }
    };
    let mut out = Vec::new();
    let spc_layers: Vec<String> = (1..=8).map(|d| spc_layer(d).to_string()).collect();
    if let Some(v) = identify(SPC, spc_layers.join(",")).await {
        for r in v["results"].as_array().into_iter().flatten() {
            let layer = r["layerId"].as_u64().unwrap_or(0) as u32;
            let Some(day) = (1..=8u8).find(|d| spc_layer(*d) == layer) else { continue };
            let a = &r["attributes"];
            let label = a["label"].as_str().or(a["LABEL"].as_str()).unwrap_or("").to_string();
            let label2 = a["label2"].as_str().unwrap_or("").to_string();
            let dn = a["dn"].as_str().and_then(|s| s.parse::<f64>().ok()).or(a["dn"].as_f64()).unwrap_or(0.0);
            if day >= 4 && dn <= 0.0 {
                continue;
            }
            let (label, detail) = if day >= 4 { (format!("{}% severe", dn), format!("{dn}% chance of severe thunderstorms within 25 miles")) } else { (label, label2) };
            if !label.is_empty() {
                out.push(Hit { product: "SPC convective outlook", day, label, detail });
            }
        }
    }
    let wpc_layers: Vec<String> = (1..=3u8).flat_map(|d| wpc_chart_layers(d).into_iter().skip(1)).map(|l| l.to_string()).collect();
    if let Some(v) = identify(WPC_CHART, wpc_layers.join(",")).await {
        for r in v["results"].as_array().into_iter().flatten() {
            let name = r["layerName"].as_str().unwrap_or("");
            // "Day 2 Rain/Thunderstorms" → day 2, "Rain/Thunderstorms"
            let mut parts = name.splitn(3, ' ');
            let (Some("Day"), Some(d), Some(what)) = (parts.next(), parts.next(), parts.next()) else { continue };
            let Ok(day) = d.parse::<u8>() else { continue };
            if !out.iter().any(|h: &Hit| h.product == "WPC forecast chart" && h.day == day && h.label == what) {
                out.push(Hit { product: "WPC forecast chart", day, label: what.to_string(), detail: String::new() });
            }
        }
    }
    out
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
    fn wpc_valid_through() {
        let p = "Day 1 Showers/Thunderstorms Area Issued by WPC: Sat Oct 24 2026<br>Valid through: Sun Morning Oct 25 2026<br>";
        assert_eq!(valid_through(p), Some(Utc.with_ymd_and_hms(2026, 10, 25, 12, 0, 0).unwrap()));
    }

    #[test]
    fn tile_bounds() {
        let b = tile_bbox(0, 0, 0);
        assert!((b[0] + 20037508.34).abs() < 1.0 && (b[3] - 20037508.34).abs() < 1.0);
    }
}
