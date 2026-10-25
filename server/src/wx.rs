//! Weather + TFR endpoints. Sources: aviationweather.gov Data API (METAR, TAF, PIREP,
//! AIRMET/SIGMET, G-AIRMET, winds-aloft text), FAA TFR geoserver, Windy Point Forecast
//! (paid key, preferred) with Open-Meteo GFS as the fallback for winds aloft.

use crate::{ApiResult, AppState};
use axum::{
    extract::{Query, State},
    http::header,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{collections::HashMap, sync::Arc, time::Duration};

type S = State<Arc<AppState>>;
const AWC: &str = "https://aviationweather.gov/api/data";

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/wx/metar", get(metar))
        .route("/wx/taf", get(taf))
        .route("/wx/pirep", get(pirep))
        .route("/wx/airsigmet", get(airsigmet))
        .route("/wx/gairmet", get(gairmet))
        .route("/wx/windtemp", get(windtemp))
        .route("/wx/winds", post(winds))
        .route("/tfr", get(tfr))
}

/// METARs by station ids (comma/space separated) or for a "lat0,lon0,lat1,lon1" box.
pub async fn metars(s: &AppState, ids: Option<&str>, bbox: Option<&str>) -> anyhow::Result<Value> {
    let url = match (ids, bbox) {
        (Some(ids), _) => format!("{AWC}/metar?ids={}&format=json&taf=false", clean(ids).replace(' ', ",")),
        (None, Some(b)) => format!("{AWC}/metar?bbox={}&format=json", clean(b)),
        _ => anyhow::bail!("ids or bbox required"),
    };
    json_or_empty(s, &url, 300).await
}

pub async fn tafs(s: &AppState, ids: &str) -> anyhow::Result<Value> {
    json_or_empty(s, &format!("{AWC}/taf?ids={}&format=json", clean(ids).replace(' ', ",")), 600).await
}

pub async fn pireps(s: &AppState, bbox: &str, age_h: f64) -> anyhow::Result<Value> {
    json_or_empty(s, &format!("{AWC}/pirep?bbox={}&format=json&age={}", clean(bbox), age_h.clamp(0.5, 24.0)), 300).await
}

pub async fn airsigmets(s: &AppState) -> anyhow::Result<Value> {
    json_or_empty(s, &format!("{AWC}/airsigmet?format=json"), 300).await
}

pub async fn gairmets(s: &AppState) -> anyhow::Result<Value> {
    json_or_empty(s, &format!("{AWC}/gairmet?format=json"), 600).await
}

/// aviationweather.gov answers an empty body (not "[]") when nothing matches.
async fn json_or_empty(s: &AppState, url: &str, ttl: u64) -> anyhow::Result<Value> {
    let (b, _) = s.cache.get(url, Duration::from_secs(ttl)).await?;
    if b.iter().all(|c| c.is_ascii_whitespace()) {
        return Ok(Value::Array(vec![]));
    }
    Ok(serde_json::from_slice(&b)?)
}

fn raw(body: crate::proxy::Bytes, ct: &str, max_age: u32) -> Response {
    ([(header::CONTENT_TYPE, ct.to_string()), (header::CACHE_CONTROL, format!("public, max-age={max_age}"))], body.to_vec()).into_response()
}

fn clean(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, ',' | '.' | '-' | ' ')).collect()
}

#[derive(Deserialize)]
struct WxQ {
    ids: Option<String>,
    bbox: Option<String>,
    age: Option<f64>,
}

async fn metar(State(s): S, Query(q): Query<WxQ>) -> ApiResult<Response> {
    let url = match (&q.ids, &q.bbox) {
        (Some(ids), _) => format!("{AWC}/metar?ids={}&format=json&taf=false", clean(ids).replace(' ', ",")),
        (None, Some(b)) => format!("{AWC}/metar?bbox={}&format=json", clean(b)),
        _ => return Err(anyhow::anyhow!("ids or bbox required").into()),
    };
    let (b, _) = s.cache.get(&url, Duration::from_secs(300)).await?;
    Ok(raw(b, "application/json", 120))
}

async fn taf(State(s): S, Query(q): Query<WxQ>) -> ApiResult<Response> {
    let url = match (&q.ids, &q.bbox) {
        (Some(ids), _) => format!("{AWC}/taf?ids={}&format=json", clean(ids).replace(' ', ",")),
        (None, Some(b)) => format!("{AWC}/taf?bbox={}&format=json", clean(b)),
        _ => return Err(anyhow::anyhow!("ids or bbox required").into()),
    };
    let (b, _) = s.cache.get(&url, Duration::from_secs(600)).await?;
    Ok(raw(b, "application/json", 300))
}

async fn pirep(State(s): S, Query(q): Query<WxQ>) -> ApiResult<Response> {
    let bbox = clean(q.bbox.as_deref().unwrap_or("20,-130,55,-60"));
    let url = format!("{AWC}/pirep?bbox={bbox}&format=json&age={}", q.age.unwrap_or(3.0).clamp(0.5, 24.0));
    let (b, _) = s.cache.get(&url, Duration::from_secs(300)).await?;
    Ok(raw(b, "application/json", 120))
}

async fn airsigmet(State(s): S) -> ApiResult<Response> {
    let (b, _) = s.cache.get(&format!("{AWC}/airsigmet?format=json"), Duration::from_secs(300)).await?;
    Ok(raw(b, "application/json", 120))
}

async fn gairmet(State(s): S) -> ApiResult<Response> {
    let (b, _) = s.cache.get(&format!("{AWC}/gairmet?format=json"), Duration::from_secs(600)).await?;
    Ok(raw(b, "application/json", 300))
}

#[derive(Deserialize)]
struct WindTempQ {
    region: Option<String>,
    level: Option<String>,
    fcst: Option<String>,
}
async fn windtemp(State(s): S, Query(q): Query<WindTempQ>) -> ApiResult<Response> {
    let url = format!(
        "{AWC}/windtemp?region={}&level={}&fcst={}",
        clean(q.region.as_deref().unwrap_or("all")),
        clean(q.level.as_deref().unwrap_or("low")),
        clean(q.fcst.as_deref().unwrap_or("06"))
    );
    let (b, _) = s.cache.get(&url, Duration::from_secs(1800)).await?;
    Ok(raw(b, "text/plain; charset=utf-8", 600))
}

async fn tfr(State(s): S) -> ApiResult<Response> {
    Ok(([(header::CACHE_CONTROL, "public, max-age=300")], Json(tfrs(&s).await?)).into_response())
}

/// Active TFRs as GeoJSON, with the FAA list's type/description merged into each shape.
pub async fn tfrs(s: &AppState) -> anyhow::Result<Value> {
    let shapes = s
        .cache
        .get_json(
            "https://tfr.faa.gov/geoserver/TFR/ows?service=WFS&version=1.1.0&request=GetFeature&typeName=TFR:V_TFR_LOC&maxFeatures=1000&outputFormat=application/json&srsname=EPSG:4326",
            Duration::from_secs(900),
        )
        .await?;
    let list = s.cache.get_json("https://tfr.faa.gov/tfrapi/exportTfrList", Duration::from_secs(900)).await.unwrap_or(Value::Null);
    let mut by_id: HashMap<String, Value> = HashMap::new();
    if let Some(arr) = list.as_array() {
        for t in arr {
            if let Some(id) = t["notam_id"].as_str() {
                by_id.insert(id.to_string(), t.clone());
            }
        }
    }
    let mut feats = Vec::new();
    for mut f in shapes["features"].as_array().cloned().unwrap_or_default() {
        let key = f["properties"]["NOTAM_KEY"].as_str().unwrap_or_default().to_string();
        let notam = key.split('-').next().unwrap_or_default().to_string();
        let info = by_id.get(&notam).cloned().unwrap_or(Value::Null);
        f["properties"] = json!({
            "notam": notam,
            "title": f["properties"]["TITLE"],
            "legal": f["properties"]["LEGAL"],
            "type": info["type"],
            "description": info["description"],
            "facility": f["properties"]["CNS_LOCATION_ID"],
            "state": f["properties"]["STATE"],
            "url": format!("https://tfr.faa.gov/tfr3/?page=detail_{}", notam.replace('/', "_")),
        });
        feats.push(f);
    }
    Ok(json!({"type": "FeatureCollection", "features": feats}))
}

#[derive(Deserialize)]
struct WindsReq {
    /// [lon, lat] points (route waypoints or samples)
    points: Vec<[f64; 2]>,
}

async fn winds(State(s): S, Json(req): Json<WindsReq>) -> ApiResult<Json<Value>> {
    Ok(Json(winds_at(&s, req.points).await))
}

/// Altitudes of the NWS winds-aloft forecast (FB / FD), ft MSL.
pub const FB_LEVELS: [f64; 9] = [3000.0, 6000.0, 9000.0, 12000.0, 18000.0, 24000.0, 30000.0, 34000.0, 39000.0];

/// One FB station: where it is and its forecasts (6, 12 and 24 h bulletins).
#[derive(Clone, Debug)]
pub struct FbStation {
    pub id: String,
    pub lat: f64,
    pub lon: f64,
    /// (valid, use-from, use-to) unix times and per-level (alt, dir, spd, temp)
    pub periods: Vec<((i64, i64, i64), Vec<(f64, Option<f64>, f64, Option<f64>)>)>,
}

/// Decode one FB group, e.g. "1728+13", "214136", "9900+04", "3508", "7315-55".
fn fb_group(g: &str, alt: f64) -> Option<(Option<f64>, f64, Option<f64>)> {
    let g = g.trim();
    if g.len() < 4 || !g[..4].chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let mut dd: i64 = g[..2].parse().ok()?;
    let mut ff: i64 = g[2..4].parse().ok()?;
    let temp = if g.len() > 4 {
        let t = &g[4..];
        let v: f64 = t.trim_start_matches(['+', '-']).parse().ok()?;
        // above 24,000 ft temperatures are always negative and carry no sign
        Some(if t.starts_with('-') || (alt > 24000.0 && !t.starts_with('+')) { -v } else { v })
    } else {
        None
    };
    if dd == 99 && ff == 0 {
        return Some((None, 0.0, temp)); // light and variable
    }
    if dd > 36 {
        // 100 kt or more: direction + 50, speed - 100
        dd -= 50;
        ff += 100;
    }
    Some((Some((dd * 10) as f64), ff as f64, temp))
}

/// Parse FB bulletins (region=all): station rows under each "VALID ... FOR USE ..." header.
pub fn parse_fb(text: &str, now: chrono::DateTime<chrono::Utc>) -> Vec<(String, (i64, i64, i64), Vec<(f64, Option<f64>, f64, Option<f64>)>)> {
    use chrono::{Datelike, TimeZone};
    let mut out = Vec::new();
    let mut period: Option<(i64, i64, i64)> = None;
    let mut cols: Vec<(usize, f64)> = Vec::new();
    for line in text.lines() {
        if let Some(rest) = line.trim_start().strip_prefix("VALID ") {
            // "250600Z   FOR USE 0200-0900Z."
            let parts: Vec<&str> = rest.split_whitespace().collect();
            let v = parts.first().and_then(|v| v.strip_suffix('Z')).filter(|v| v.len() == 6);
            let w = parts.iter().position(|p| *p == "USE").and_then(|i| parts.get(i + 1)).map(|w| w.trim_end_matches(['Z', '.']));
            period = (|| {
                let v = v?;
                let (day, hh, mm): (u32, u32, u32) = (v[..2].parse().ok()?, v[2..4].parse().ok()?, v[4..6].parse().ok()?);
                // the valid day is this month's, or next month's near a month end
                let mut valid = chrono::Utc.with_ymd_and_hms(now.year(), now.month(), day, hh, mm, 0).single()?;
                if valid < now - chrono::Duration::days(10) {
                    valid = valid.checked_add_months(chrono::Months::new(1))?;
                }
                let (a, b) = w?.split_once('-')?;
                let hm = |x: &str| -> Option<(i64, i64)> { Some((x.get(..2)?.parse().ok()?, x.get(2..4)?.parse().ok()?)) };
                let ((ah, am), (bh, bm)) = (hm(a)?, hm(b)?);
                let base = valid.date_naive().and_hms_opt(0, 0, 0)?.and_utc().timestamp();
                let mut from = base + ah * 3600 + am * 60;
                if from > valid.timestamp() {
                    from -= 86400;
                }
                let mut to = base + bh * 3600 + bm * 60;
                if to <= from {
                    to += 86400;
                }
                Some((valid.timestamp(), from, to))
            })();
            cols.clear();
            continue;
        }
        if line.starts_with("FT ") {
            // each level's field ends where its label ends
            cols.clear();
            let mut i = 0;
            for tok in line.split_whitespace() {
                let at = line[i..].find(tok).map(|p| p + i).unwrap_or(i);
                i = at + tok.len();
                if let Ok(alt) = tok.parse::<f64>() {
                    cols.push((i, alt));
                }
            }
            continue;
        }
        let (Some(p), false) = (period, cols.is_empty()) else { continue };
        let id = line.get(..3).unwrap_or("").trim();
        if id.len() != 3 || !id.chars().all(|c| c.is_ascii_alphanumeric()) {
            continue;
        }
        let mut levels = Vec::new();
        let mut start = 3;
        for (end, alt) in &cols {
            let field = line.get(start..(*end).min(line.len())).unwrap_or("");
            start = *end;
            if let Some((dir, spd, temp)) = fb_group(field, *alt) {
                levels.push((*alt, dir, spd, temp));
            }
        }
        if !levels.is_empty() {
            out.push((id.to_string(), p, levels));
        }
    }
    out
}

static FB_COORDS: parking_lot::Mutex<Option<HashMap<String, Option<(f64, f64)>>>> = parking_lot::Mutex::new(None);

/// All FB stations with their 6 / 12 / 24 h forecasts, placed with the FAA database.
pub async fn fb_stations(s: &AppState) -> anyhow::Result<Vec<FbStation>> {
    let now = chrono::Utc::now();
    let mut by_id: HashMap<String, FbStation> = HashMap::new();
    for fcst in ["06", "12", "24"] {
        for level in ["low", "high"] {
            let url = format!("{AWC}/windtemp?region=all&level={level}&fcst={fcst}");
            let Ok((body, _)) = s.cache.get(&url, Duration::from_secs(1800)).await else { continue };
            for (id, period, levels) in parse_fb(&String::from_utf8_lossy(&body), now) {
                let e = by_id.entry(id.clone()).or_insert_with(|| FbStation { id, lat: f64::NAN, lon: f64::NAN, periods: vec![] });
                match e.periods.iter_mut().find(|p| p.0 == period) {
                    Some(p) => {
                        for l in levels {
                            if !p.1.iter().any(|x| x.0 == l.0) {
                                p.1.push(l);
                            }
                        }
                    }
                    None => e.periods.push((period, levels)),
                }
            }
        }
    }
    // FB ids are navaid / airport identifiers: look them up once
    let data = s.try_data()?;
    let mut cache = FB_COORDS.lock();
    let coords = cache.get_or_insert_with(HashMap::new);
    let mut out = Vec::new();
    for (id, mut st) in by_id {
        let at = *coords.entry(id.clone()).or_insert_with(|| {
            let place = data.db.resolve(&id, None).ok().flatten().or_else(|| data.db.resolve(&format!("K{id}"), None).ok().flatten());
            place.map(|p| (p.lat, p.lon))
        });
        let Some((lat, lon)) = at else { continue };
        st.lat = lat;
        st.lon = lon;
        out.push(st);
    }
    Ok(out)
}

fn fb_json(st: &FbStation) -> Value {
    let levels: Vec<Value> = FB_LEVELS
        .iter()
        .filter_map(|alt| {
            let times: Vec<Value> = st
                .periods
                .iter()
                .filter_map(|((valid, from, to), lv)| {
                    let (_, dir, spd, temp) = lv.iter().find(|l| l.0 == *alt)?;
                    Some(json!({"t": valid, "from": from, "to": to, "dir": dir.unwrap_or(0.0), "spd": spd, "temp": temp, "alt": alt}))
                })
                .collect();
            (!times.is_empty()).then(|| json!({"p": 0, "alt": alt, "times": times}))
        })
        .collect();
    json!({"lat": st.lat, "lon": st.lon, "station": st.id, "levels": levels})
}

/// Every FB station as a wind point (for the winds-aloft layer).
pub async fn fb_points(s: &AppState) -> Value {
    match fb_stations(s).await {
        Ok(list) => json!({"source": "nws-fb", "points": list.iter().map(fb_json).collect::<Vec<_>>()}),
        Err(e) => json!({"source": "nws-fb", "points": [], "error": e.to_string()}),
    }
}

/// Winds and temperatures aloft at each [lon, lat] point: the NWS winds-aloft forecast (FB)
/// of the nearest station within 150 NM, unchanged. Each time carries the bulletin's
/// "for use" window; outside all of them there is no forecast.
pub async fn winds_at(s: &AppState, points: Vec<[f64; 2]>) -> Value {
    let stations = match fb_stations(s).await {
        Ok(v) => v,
        Err(e) => return json!({"source": "nws-fb", "points": [], "error": e.to_string()}),
    };
    let nm = |lat1: f64, lon1: f64, lat2: f64, lon2: f64| {
        let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
        let dl = (lon2 - lon1).to_radians();
        let a = ((p2 - p1) / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
        2.0 * 3440.065 * a.sqrt().asin()
    };
    let results: Vec<Value> = points
        .into_iter()
        .take(40)
        .map(|p| {
            let best = stations.iter().map(|st| (nm(p[1], p[0], st.lat, st.lon), st)).filter(|(d, _)| *d <= 150.0).min_by(|a, b| a.0.total_cmp(&b.0));
            match best {
                Some((d, st)) => {
                    let mut v = fb_json(st);
                    v["lat"] = json!(p[1]);
                    v["lon"] = json!(p[0]);
                    v["station_nm"] = json!(d.round());
                    v
                }
                None => json!({"lat": p[1], "lon": p[0], "error": "no winds-aloft station within 150 NM"}),
            }
        })
        .collect();
    json!({"source": "nws-fb", "points": results})
}

#[cfg(test)]
mod fb_tests {
    use super::*;
    use chrono::TimeZone;

    const SAMPLE: &str = "000
FBUS31 KWNO 250200
FD1US1
DATA BASED ON 250000Z
VALID 250600Z   FOR USE 0200-0900Z. TEMPS NEG ABV 24000

FT  3000    6000    9000   12000   18000   24000  30000  34000  39000
ABI      1728+13 1920+07 1713+06 2024-08 2039-20 214136 224346 246052
ABR 3508 9900+04 2617+04 2626-01 2442-14 2451-25 246841 247350 237757
ACK 9900 0209+08 0210+03 0213-03 0412-19 0719-31 071945 061248 990052
XYZ      7315-55
";

    #[test]
    fn decodes_fb() {
        let now = chrono::Utc.with_ymd_and_hms(2026, 10, 25, 4, 0, 0).unwrap();
        let rows = parse_fb(SAMPLE, now);
        let abi = rows.iter().find(|r| r.0 == "ABI").unwrap();
        let (valid, from, to) = abi.1;
        assert_eq!(valid, chrono::Utc.with_ymd_and_hms(2026, 10, 25, 6, 0, 0).unwrap().timestamp());
        assert_eq!(from, chrono::Utc.with_ymd_and_hms(2026, 10, 25, 2, 0, 0).unwrap().timestamp());
        assert_eq!(to, chrono::Utc.with_ymd_and_hms(2026, 10, 25, 9, 0, 0).unwrap().timestamp());
        assert!(abi.2.iter().all(|l| l.0 != 3000.0), "ABI has no 3000 ft forecast");
        assert_eq!(abi.2[0], (6000.0, Some(170.0), 28.0, Some(13.0)));
        let fl300 = abi.2.iter().find(|l| l.0 == 30000.0).unwrap();
        assert_eq!(*fl300, (30000.0, Some(210.0), 41.0, Some(-36.0)), "temps above 24,000 are negative");
        let abr = rows.iter().find(|r| r.0 == "ABR").unwrap();
        assert_eq!(abr.2[0], (3000.0, Some(350.0), 8.0, None));
        assert_eq!(abr.2[1], (6000.0, None, 0.0, Some(4.0)), "9900 is light and variable");
        let ack = rows.iter().find(|r| r.0 == "ACK").unwrap();
        assert_eq!(ack.2.last().unwrap(), &(39000.0, None, 0.0, Some(-52.0)));
        let xyz = rows.iter().find(|r| r.0 == "XYZ").unwrap();
        assert_eq!(xyz.2[0], (6000.0, Some(230.0), 115.0, Some(-55.0)), "100 kt+: direction + 50");
    }
}
