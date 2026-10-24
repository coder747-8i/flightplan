//! NOTAMs from the FAA NOTAM API (api.faa.gov; needs FAA_NOTAM_CLIENT_ID / FAA_NOTAM_CLIENT_SECRET).
//! For a route we ask for: each airport by location, every point along the route by radius
//! (catches en route, obstacle, nav and airspace NOTAMs), the ARTCCs crossed, and FDC NOTAMs
//! (TFRs, procedure changes) near the route. Results are merged and de-duplicated.

use crate::{ApiResult, AppState};
use axum::{extract::State, routing::post, Json, Router};
use futures::{stream, StreamExt};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{collections::HashMap, sync::Arc, time::Duration};

const API: &str = "https://external-api.faa.gov/notamapi/v1/notams";

pub fn routes() -> Router<Arc<AppState>> {
    Router::new().route("/notams", post(notams))
}

#[derive(Deserialize, Default)]
pub struct NotamReq {
    /// airport / facility identifiers (ICAO or FAA)
    #[serde(default)]
    pub locations: Vec<String>,
    /// [lon, lat] points to search around (route samples)
    #[serde(default)]
    pub points: Vec<[f64; 2]>,
    /// radius around each point, NM (max 100)
    pub radius: Option<f64>,
    /// ARTCC / FIR identifiers (e.g. ZAB, ZLA)
    #[serde(default)]
    pub artccs: Vec<String>,
}

async fn query(s: Arc<AppState>, id: String, secret: String, params: Vec<(&'static str, String)>) -> anyhow::Result<Vec<Value>> {
    let mut all = Vec::new();
    for page in 1..=5 {
        let mut p: Vec<(&str, String)> = params.clone();
        p.push(("responseFormat", "geoJson".into()));
        p.push(("pageSize", "1000".into()));
        p.push(("pageNum", page.to_string()));
        let key = format!("notam:{}", serde_json::to_string(&p)?);
        let v = match s.cache.peek_json(&key, Duration::from_secs(600)) {
            Some(v) => v,
            None => {
                let v: Value = s
                    .cache
                    .http()
                    .get(API)
                    .header("client_id", &id)
                    .header("client_secret", &secret)
                    .query(&p)
                    .send()
                    .await?
                    .error_for_status()?
                    .json()
                    .await?;
                s.cache.put_json(&key, &v);
                v
            }
        };
        let items = v["items"].as_array().cloned().unwrap_or_default();
        let n = items.len();
        all.extend(items);
        if page >= v["totalPages"].as_i64().unwrap_or(1) || n == 0 {
            break;
        }
    }
    Ok(all)
}

/// Category from the NOTAM keyword / Q-code, for grouping in the UI.
fn category(text: &str, class: &str) -> &'static str {
    let t = text.to_uppercase();
    let word = t.split_whitespace().find(|w| w.chars().all(|c| c.is_ascii_alphabetic()) && w.len() >= 2).unwrap_or("");
    if class == "FDC" {
        if t.contains("TEMPORARY FLIGHT RESTRICTION") || t.contains("TFR") || t.contains("99.7") || t.contains("91.141") || t.contains("91.137") {
            return "TFR";
        }
        return "FDC / procedures";
    }
    match word {
        "RWY" => "Runway",
        "TWY" => "Taxiway",
        "APRON" | "RAMP" => "Apron",
        "AD" | "AERODROME" => "Aerodrome",
        "OBST" => "Obstacle",
        "NAV" => "Navaid",
        "COM" => "Communications",
        "SVC" => "Services",
        "AIRSPACE" => "Airspace",
        "IAP" | "SID" | "STAR" | "ODP" | "CHART" | "DATA" => "Procedures",
        "GPS" | "GNSS" | "WAAS" => "GPS",
        _ => {
            if t.contains("AIRSPACE") || t.contains("UAS") || t.contains("PJE") || t.contains("PARACHUTE") {
                "Airspace"
            } else if t.contains("OBST") || t.contains("CRANE") || t.contains("TOWER") {
                "Obstacle"
            } else {
                "Other"
            }
        }
    }
}

fn normalise(item: &Value) -> Option<(String, Value)> {
    let core = &item["properties"]["coreNOTAMData"];
    let n = &core["notam"];
    let id = n["id"].as_str()?.to_string();
    let text = n["text"].as_str().unwrap_or_default().to_string();
    let class = n["classification"].as_str().unwrap_or_default().to_string();
    let simple = core["notamTranslation"]
        .as_array()
        .and_then(|a| a.iter().find(|t| t["type"] == "LOCAL_FORMAT"))
        .and_then(|t| t["simpleText"].as_str())
        .unwrap_or(&text)
        .to_string();
    Some((
        id.clone(),
        json!({
            "id": id,
            "number": n["number"],
            "location": n["location"].as_str().or(n["icaoLocation"].as_str()).unwrap_or_default(),
            "icao": n["icaoLocation"],
            "class": class,
            "type": n["type"],
            "issued": n["issued"],
            "start": n["effectiveStart"],
            "end": n["effectiveEnd"],
            "text": simple,
            "icaoText": text,
            "category": category(&text, &class),
            "lower": n["minimumFL"],
            "upper": n["maximumFL"],
            "geometry": item["geometry"],
        }),
    ))
}

async fn notams(State(s): State<Arc<AppState>>, Json(req): Json<NotamReq>) -> ApiResult<Json<Value>> {
    Ok(Json(fetch(s, req).await))
}

/// NOTAMs for a route, merged and de-duplicated: {"configured", "items", "errors"}.
pub async fn fetch(s: Arc<AppState>, req: NotamReq) -> Value {
    let keys = s.settings.get();
    let (Some(id), Some(secret)) = (keys.notam_id, keys.notam_secret) else {
        return json!({"configured": false, "items": []});
    };
    let radius = req.radius.unwrap_or(25.0).clamp(5.0, 100.0);
    let mut jobs: Vec<Vec<(&'static str, String)>> = Vec::new();
    for l in req.locations.iter().take(30) {
        let l = l.trim().to_uppercase();
        if l.len() == 4 {
            jobs.push(vec![("icaoLocation", l)]);
        } else {
            jobs.push(vec![("domesticLocation", l)]);
        }
    }
    for p in req.points.iter().take(40) {
        jobs.push(vec![("locationLongitude", format!("{:.4}", p[0])), ("locationLatitude", format!("{:.4}", p[1])), ("locationRadius", format!("{radius:.0}"))]);
    }
    for a in req.artccs.iter().take(10) {
        jobs.push(vec![("domesticLocation", a.trim().to_uppercase())]);
    }
    let futs: Vec<_> = jobs.into_iter().map(|p| query(s.clone(), id.clone(), secret.clone(), p)).collect();
    let results: Vec<anyhow::Result<Vec<Value>>> = stream::iter(futs).buffer_unordered(6).collect().await;
    let mut merged: HashMap<String, Value> = HashMap::new();
    let mut errors = Vec::new();
    for r in results {
        match r {
            Ok(items) => {
                for it in &items {
                    if let Some((k, v)) = normalise(it) {
                        merged.entry(k).or_insert(v);
                    }
                }
            }
            Err(e) => errors.push(e.to_string()),
        }
    }
    let mut items: Vec<Value> = merged.into_values().collect();
    items.sort_by(|a, b| a["location"].as_str().cmp(&b["location"].as_str()).then(a["category"].as_str().cmp(&b["category"].as_str())));
    json!({"configured": true, "items": items, "errors": errors})
}
