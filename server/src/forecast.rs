//! Forecast data for planning ahead, fetched as the agencies publish it (nothing here is
//! computed): an airport's METAR / TAF / NBM text guidance (Iowa Environmental Mesonet), with
//! the nearest reporting station when the airport has none; the WPC forecast chart and SPC
//! convective outlook areas a route crosses (NOAA map services); when those products' day 1
//! ends; and which HRRR runs IEM serves tiles for. Used by the desktop app directly and by the
//! web app through the endpoints below.

use crate::{ApiResult, AppState};
use axum::{
    extract::{Query, State},
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{sync::Arc, time::Duration};

pub const BASE: &str = "https://mapservices.weather.noaa.gov/vector/rest/services";
pub const WPC_CHART: &str = "outlooks/natl_fcst_wx_chart";
pub const WPC_QPF: &str = "precip/wpc_qpf";
pub const SPC: &str = "outlooks/SPC_wx_outlks";
const IEM: &str = "https://mesonet.agron.iastate.edu";

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

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/wx/station", get(station_api))
        .route("/wx/official", post(official_api))
        .route("/wx/outlooks", get(outlooks_api))
        .route("/wx/hrrr", get(hrrr_api))
}

/// What we know about one airport's weather source. `nbs` / `nbe` are IEM `mos.json`
/// documents (NBM short range / extended), null when no nearby site has them.
#[derive(Clone, Debug, Default, Serialize)]
pub struct StationRaw {
    /// the airport the pilot planned
    pub airport: String,
    /// the station whose METAR / TAF / NBM we use
    pub station: String,
    /// NM from the airport to `station`
    pub offset_nm: f64,
    pub metar: Option<Value>,
    pub taf: Option<Value>,
    pub nbs: Option<Value>,
    pub nbe: Option<Value>,
}

fn has_rows(v: &Value) -> bool {
    v["data"].as_array().is_some_and(|a| !a.is_empty())
}

/// METAR, TAF and NBM for an airport, falling back to the nearest reporting station within
/// 30 NM when the airport has none.
pub async fn station(st: Arc<AppState>, airport: String, icao: String, lat: f64, lon: f64) -> StationRaw {
    let mut candidates: Vec<(String, f64)> = vec![(icao, 0.0)];
    if let Ok(data) = st.try_data() {
        if let Ok(near) = data.db.airports_near(lat, lon, 30.0, false) {
            for (p, d) in near.into_iter().take(12) {
                let id = if p.id.len() == 3 && p.id.chars().all(|c| c.is_ascii_alphabetic()) { format!("K{}", p.id) } else { p.id.clone() };
                if !candidates.iter().any(|c| c.0 == id) && id.len() == 4 && id.chars().all(|c| c.is_ascii_alphanumeric()) {
                    candidates.push((id, d));
                }
            }
        }
    }
    let mut out = StationRaw { airport, station: candidates[0].0.clone(), ..Default::default() };
    // METAR / TAF for the first few candidates in one request each
    let ids: Vec<String> = candidates.iter().take(8).map(|c| c.0.clone()).collect();
    let metars = crate::wx::metars(&st, Some(&ids.join(",")), None).await.unwrap_or(Value::Null);
    let tafs = crate::wx::tafs(&st, &ids.join(",")).await.unwrap_or(Value::Null);
    let find = |v: &Value, id: &str| v.as_array().and_then(|a| a.iter().find(|x| x["icaoId"].as_str() == Some(id)).cloned());
    // the closest candidate that reports, preferring one with a TAF
    let pick = candidates.iter().take(8).find(|c| find(&tafs, &c.0).is_some()).or_else(|| candidates.iter().take(8).find(|c| find(&metars, &c.0).is_some()));
    if let Some((id, d)) = pick {
        out.station = id.clone();
        out.offset_nm = *d;
    }
    out.metar = find(&metars, &out.station);
    out.taf = find(&tafs, &out.station);
    // NBS and NBE in parallel; NBM covers far more sites than report METARs, so try the chosen
    // station, then the other candidates
    let tries: Vec<(String, f64)> = std::iter::once((out.station.clone(), out.offset_nm)).chain(candidates.iter().take(5).cloned()).collect();
    let nbm = |model: &'static str| {
        let st = st.clone();
        let tries = tries.clone();
        async move {
            for (id, d) in tries {
                let url = format!("{IEM}/api/1/mos.json?station={id}&model={model}");
                if let Ok(v) = st.cache.get_json(&url, Duration::from_secs(3600)).await {
                    if has_rows(&v) {
                        return Some((id, d, v));
                    }
                }
            }
            None
        }
    };
    let (nbs, nbe) = tokio::join!(nbm("NBS"), nbm("NBE"));
    for (i, r) in [nbs, nbe].into_iter().enumerate() {
        if let Some((id, d, v)) = r {
            if out.metar.is_none() && out.taf.is_none() {
                out.station = id;
                out.offset_nm = d;
            }
            if i == 0 {
                out.nbs = Some(v);
            } else {
                out.nbe = Some(v);
            }
        }
    }
    out
}

/// One official product the route crosses.
#[derive(Clone, Debug, Serialize)]
pub struct Hit {
    pub product: &'static str,
    pub day: u8,
    /// e.g. "TSTM" / "General Thunderstorms Risk", or "Rain/Thunderstorms"
    pub label: String,
    pub detail: String,
}

/// What the route crosses in the WPC chart (days 1-3) and the SPC outlooks (days 1-8).
pub async fn along_route(st: Arc<AppState>, path: Vec<[f64; 2]>) -> Vec<Hit> {
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

/// When the current WPC chart's and SPC outlook's day 1 ends (unix seconds; they run 12Z to 12Z).
pub async fn outlook_ends(st: &AppState) -> (Option<i64>, Option<i64>) {
    let ttl = Duration::from_secs(1800);
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
}

/// Init times (unix seconds) of the HRRR runs IEM serves, the latest (to 18 h) and the latest
/// extended, plus the last forecast minute of the extended run IEM actually has tiles for.
pub async fn hrrr_runs(st: &AppState) -> (Option<i64>, Option<i64>, i64) {
    let init = |v: Value| v["model_init_utc"].as_str().and_then(|s| DateTime::parse_from_rfc3339(s).ok()).map(|d| d.timestamp());
    let base = format!("{IEM}/data/gis/images/4326/hrrr");
    let ttl = Duration::from_secs(300);
    let a = st.cache.get_json(&format!("{base}/refd_0000.json"), ttl).await.ok().and_then(init);
    let b = st.cache.get_json(&format!("{base}/refd_1140.json"), ttl).await.ok().and_then(init);
    // the extended run's tiles stop short of 48 h at times: find the last hour served
    let ok = |m: i64| {
        let http = st.cache.http().clone();
        async move {
            let url = format!("{IEM}/cache/tile.py/1.0.0/hrrr::REFD-F{m:04}-0/4/3/5.png");
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
}

#[derive(Deserialize)]
struct StationQ {
    airport: String,
    icao: Option<String>,
    lat: f64,
    lon: f64,
}

async fn station_api(State(s): State<Arc<AppState>>, Query(q): Query<StationQ>) -> Json<StationRaw> {
    let icao = q.icao.unwrap_or_else(|| if q.airport.len() == 3 { format!("K{}", q.airport) } else { q.airport.clone() });
    Json(station(s, q.airport.to_uppercase(), icao.to_uppercase(), q.lat, q.lon).await)
}

#[derive(Deserialize)]
struct PathReq {
    path: Vec<[f64; 2]>,
}

async fn official_api(State(s): State<Arc<AppState>>, Json(r): Json<PathReq>) -> Json<Vec<Hit>> {
    Json(along_route(s, r.path.into_iter().take(200).collect()).await)
}

async fn outlooks_api(State(s): State<Arc<AppState>>) -> ApiResult<Json<Value>> {
    let (wpc, spc) = outlook_ends(&s).await;
    Ok(Json(json!({"wpcDay1End": wpc, "spcDay1End": spc})))
}

async fn hrrr_api(State(s): State<Arc<AppState>>) -> Json<Value> {
    let (short, extended, last) = hrrr_runs(&s).await;
    Json(json!({"short": short, "extended": extended, "extendedLast": last}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn layers() {
        assert_eq!(wpc_chart_layers(2)[0], 14);
        assert_eq!(spc_layer(3), 17);
        assert_eq!(spc_layer(6), 23);
    }

    #[test]
    fn wpc_valid_through() {
        let p = "Day 1 Showers/Thunderstorms Area Issued by WPC: Sat Oct 24 2026<br>Valid through: Sun Morning Oct 25 2026<br>";
        assert_eq!(valid_through(p), Some(Utc.with_ymd_and_hms(2026, 10, 25, 12, 0, 0).unwrap()));
    }
}
