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
use futures::{stream, StreamExt};
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

/// Active TFRs as GeoJSON, with the FAA list's type/description merged into each shape.
async fn tfr(State(s): S) -> ApiResult<Response> {
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
    Ok(([(header::CACHE_CONTROL, "public, max-age=300")], Json(json!({"type": "FeatureCollection", "features": feats}))).into_response())
}

#[derive(Deserialize)]
struct WindsReq {
    /// [lon, lat] points (route waypoints or samples)
    points: Vec<[f64; 2]>,
}

// pressure level -> standard-atmosphere altitude (ft), for labelling
const LEVELS: [(u32, f64); 10] =
    [(1000, 360.0), (925, 2500.0), (850, 4800.0), (800, 6400.0), (700, 9900.0), (600, 14000.0), (500, 18300.0), (400, 23600.0), (300, 30100.0), (250, 34000.0)];

/// Winds and temperatures aloft at each point, hourly. Uses Windy when a key is configured.
async fn winds(State(s): S, Json(req): Json<WindsReq>) -> ApiResult<Json<Value>> {
    let pts: Vec<[f64; 2]> = req.points.into_iter().take(40).collect();
    let windy = s.windy_point_key.clone();
    let results: Vec<Value> = stream::iter(pts.into_iter().map(|p| {
        let s = s.clone();
        let windy = windy.clone();
        async move {
            let r = match &windy {
                Some(key) => windy_point(&s, key, p[1], p[0]).await,
                None => open_meteo(&s, p[1], p[0]).await,
            };
            match r {
                Ok(v) => v,
                Err(e) => json!({"lat": p[1], "lon": p[0], "error": e.to_string()}),
            }
        }
    }))
    .buffered(6)
    .collect()
    .await;
    Ok(Json(json!({"source": if windy.is_some() { "windy" } else { "open-meteo" }, "points": results})))
}

fn uv_to_dir_kt(u: f64, v: f64) -> (f64, f64) {
    let spd = (u * u + v * v).sqrt() * 1.943_844;
    let dir = (u.atan2(v).to_degrees() + 180.0 + 360.0) % 360.0;
    (dir.round(), spd.round())
}

async fn windy_point(s: &AppState, key: &str, lat: f64, lon: f64) -> anyhow::Result<Value> {
    let levels: Vec<String> = std::iter::once("surface".to_string()).chain(LEVELS.iter().map(|(p, _)| format!("{p}h"))).collect();
    let cache_key = format!("windy:{:.2},{:.2}", lat, lon);
    let body = json!({"lat": lat, "lon": lon, "model": "gfs", "parameters": ["wind", "temp", "gh"], "levels": levels, "key": key});
    let v: Value = match s.cache.peek_json(&cache_key, Duration::from_secs(1800)) {
        Some(v) => v,
        None => {
            let v: Value = s.cache.http().post("https://api.windy.com/api/point-forecast/v2").json(&body).send().await?.error_for_status()?.json().await?;
            s.cache.put_json(&cache_key, &v);
            v
        }
    };
    let ts: Vec<i64> = v["ts"].as_array().map(|a| a.iter().filter_map(|x| x.as_i64()).collect()).unwrap_or_default();
    let mut out_levels = Vec::new();
    for (p, std_alt) in LEVELS {
        let u = &v[format!("wind_u-{p}h")];
        let vv = &v[format!("wind_v-{p}h")];
        let t = &v[format!("temp-{p}h")];
        let gh = &v[format!("gh-{p}h")];
        let mut times = Vec::new();
        for (i, tsv) in ts.iter().enumerate() {
            let (Some(uu), Some(vvv)) = (u[i].as_f64(), vv[i].as_f64()) else { continue };
            let (dir, spd) = uv_to_dir_kt(uu, vvv);
            times.push(json!({"t": tsv / 1000, "dir": dir, "spd": spd, "temp": t[i].as_f64().map(|k| ((k - 273.15) * 10.0).round() / 10.0),
                              "alt": gh[i].as_f64().map(|m| (m * 3.28084).round()).unwrap_or(std_alt)}));
        }
        out_levels.push(json!({"p": p, "alt": std_alt, "times": times}));
    }
    Ok(json!({"lat": lat, "lon": lon, "levels": out_levels}))
}

async fn open_meteo(s: &AppState, lat: f64, lon: f64) -> anyhow::Result<Value> {
    let mut vars = Vec::new();
    for (p, _) in LEVELS {
        for v in ["wind_speed", "wind_direction", "temperature", "geopotential_height"] {
            vars.push(format!("{v}_{p}hPa"));
        }
    }
    let url = format!(
        "https://api.open-meteo.com/v1/gfs?latitude={lat:.3}&longitude={lon:.3}&hourly={}&wind_speed_unit=kn&forecast_days=2&timeformat=unixtime",
        vars.join(",")
    );
    let v = s.cache.get_json(&url, Duration::from_secs(1800)).await?;
    let h = &v["hourly"];
    let ts: Vec<i64> = h["time"].as_array().map(|a| a.iter().filter_map(|x| x.as_i64()).collect()).unwrap_or_default();
    let mut out_levels = Vec::new();
    for (p, std_alt) in LEVELS {
        let mut times = Vec::new();
        for (i, t) in ts.iter().enumerate() {
            let (Some(dir), Some(spd)) = (h[format!("wind_direction_{p}hPa")][i].as_f64(), h[format!("wind_speed_{p}hPa")][i].as_f64()) else { continue };
            times.push(json!({"t": t, "dir": dir.round(), "spd": spd.round(), "temp": h[format!("temperature_{p}hPa")][i].as_f64(),
                              "alt": h[format!("geopotential_height_{p}hPa")][i].as_f64().map(|m| (m * 3.28084).round()).unwrap_or(std_alt)}));
        }
        out_levels.push(json!({"p": p, "alt": std_alt, "times": times}));
    }
    Ok(json!({"lat": lat, "lon": lon, "levels": out_levels}))
}
