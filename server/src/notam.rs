//! NOTAMs from the FAA: the NOTAM Management Service (NMS) API, the FAA's current system
//! (credentials by email from notams@faa.gov; OAuth client-credentials token, GeoJSON), with the
//! legacy NOTAM API (external-api.faa.gov, headers) as a fallback for older api.faa.gov keys.
//! Credentials: FAA_NOTAM_CLIENT_ID / FAA_NOTAM_CLIENT_SECRET or Settings.
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
const NMS_TOKEN: &str = "https://api-nms.aim.faa.gov/v1/auth/token";
const NMS_API: &str = "https://api-nms.aim.faa.gov/nmsapi/v1/notams";

/// Which FAA service the saved credentials belong to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Service {
    Nms,
    Legacy,
}

/// (client id, token, expiry) of the last NMS token.
static NMS: parking_lot::Mutex<Option<(String, String, std::time::Instant)>> = parking_lot::Mutex::new(None);

/// An NMS access token for these credentials (cached until shortly before it expires).
pub async fn nms_token(s: &AppState, id: &str, secret: &str) -> anyhow::Result<String> {
    if let Some((cid, tok, until)) = NMS.lock().clone() {
        if cid == id && std::time::Instant::now() < until {
            return Ok(tok);
        }
    }
    let r = s.cache.http().post(NMS_TOKEN).basic_auth(id, Some(secret)).header("Content-Type", "application/x-www-form-urlencoded").body("grant_type=client_credentials").send().await?;
    let status = r.status();
    let v: Value = r.json().await.unwrap_or(Value::Null);
    if !status.is_success() {
        let why = v["Error"].as_str().or(v["error_description"].as_str()).or(v["error"].as_str()).unwrap_or("rejected");
        anyhow::bail!("NMS token: HTTP {status} {why}");
    }
    let tok = v["access_token"].as_str().ok_or_else(|| anyhow::anyhow!("NMS token: no access_token"))?.to_string();
    let ttl = v["expires_in"].as_u64().unwrap_or(1800).saturating_sub(60).max(30);
    *NMS.lock() = Some((id.to_string(), tok.clone(), std::time::Instant::now() + Duration::from_secs(ttl)));
    Ok(tok)
}

/// Which service accepts these credentials: NMS first, then the legacy API.
pub async fn service_for(s: &AppState, id: &str, secret: &str) -> anyhow::Result<Service> {
    let nms = nms_token(s, id, secret).await;
    if nms.is_ok() {
        return Ok(Service::Nms);
    }
    let r = s.cache.http().get(API).header("client_id", id).header("client_secret", secret).query(&[("icaoLocation", "KPHX"), ("pageSize", "1")]).send().await?;
    if r.status().is_success() {
        return Ok(Service::Legacy);
    }
    Err(nms.err().unwrap_or_else(|| anyhow::anyhow!("rejected")))
}

/// One NMS query; `params` are NMS names (location, latitude / longitude / radius).
async fn query_nms(s: Arc<AppState>, id: String, secret: String, params: Vec<(&'static str, String)>) -> anyhow::Result<Vec<Value>> {
    let key = format!("notam:nms:{}", serde_json::to_string(&params)?);
    if let Some(v) = s.cache.peek_json(&key, Duration::from_secs(600)) {
        return Ok(v.as_array().cloned().unwrap_or_default());
    }
    let tok = nms_token(&s, &id, &secret).await?;
    let v: Value = s.cache.http().get(NMS_API).bearer_auth(tok).header("nmsResponseFormat", "GEOJSON").header("Accept", "application/json").query(&params).send().await?.error_for_status()?.json().await?;
    let items = v["data"]["geojson"].as_array().cloned().or_else(|| v["items"].as_array().cloned()).unwrap_or_default();
    s.cache.put_json(&key, &Value::Array(items.clone()));
    Ok(items)
}

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
    // legacy and NMS GeoJSON both nest properties.coreNOTAMData.notam; be lenient about it
    let props = &item["properties"];
    let core = if props["coreNOTAMData"].is_object() { &props["coreNOTAMData"] } else { props };
    let n = if core["notam"].is_object() { &core["notam"] } else { core };
    let id = n["id"].as_str().map(String::from).or_else(|| n["id"].as_i64().map(|x| x.to_string())).or_else(|| n["number"].as_str().map(|num| format!("{}-{num}", n["location"].as_str().unwrap_or(""))))?;
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
    let service = match service_for(&s, &id, &secret).await {
        Ok(sv) => sv,
        Err(e) => return json!({"configured": true, "items": [], "errors": [format!("The FAA didn't accept the NOTAM credentials: {e}")]}),
    };
    let mut jobs: Vec<Vec<(&'static str, String)>> = Vec::new();
    for l in req.locations.iter().take(30) {
        let l = l.trim().to_uppercase();
        jobs.push(match (service, l.len()) {
            (Service::Nms, _) => vec![("location", l)],
            (Service::Legacy, 4) => vec![("icaoLocation", l)],
            (Service::Legacy, _) => vec![("domesticLocation", l)],
        });
    }
    for p in req.points.iter().take(40) {
        jobs.push(match service {
            Service::Nms => vec![("latitude", format!("{:.4}", p[1])), ("longitude", format!("{:.4}", p[0])), ("radius", format!("{radius:.0}"))],
            Service::Legacy => vec![("locationLongitude", format!("{:.4}", p[0])), ("locationLatitude", format!("{:.4}", p[1])), ("locationRadius", format!("{radius:.0}"))],
        });
    }
    for a in req.artccs.iter().take(10) {
        let a = a.trim().to_uppercase();
        jobs.push(match service {
            Service::Nms => vec![("location", a)],
            Service::Legacy => vec![("domesticLocation", a)],
        });
    }
    let results: Vec<anyhow::Result<Vec<Value>>> = match service {
        Service::Nms => {
            let futs: Vec<_> = jobs.into_iter().map(|p: Vec<(&'static str, String)>| query_nms(s.clone(), id.clone(), secret.clone(), p)).collect();
            stream::iter(futs).buffer_unordered(4).collect().await
        }
        Service::Legacy => {
            let futs: Vec<_> = jobs.into_iter().map(|p: Vec<(&'static str, String)>| query(s.clone(), id.clone(), secret.clone(), p)).collect();
            stream::iter(futs).buffer_unordered(6).collect().await
        }
    };
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
    json!({"configured": true, "service": if service == Service::Nms { "nms" } else { "legacy" }, "items": items, "errors": errors})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_nms_and_legacy_features() {
        let f = json!({"type": "Feature", "geometry": {"type": "Point", "coordinates": [-112.4, 34.6]}, "properties": {"coreNOTAMData": {
            "notam": {"id": "1234567890123456", "number": "10/124", "location": "PRC", "icaoLocation": "KPRC", "classification": "DOMESTIC",
                      "effectiveStart": "2026-10-24T13:00:00.000Z", "effectiveEnd": "2026-10-30T23:59:00.000Z", "text": "RWY 03R/21L CLSD"},
            "notamTranslation": [{"type": "LOCAL_FORMAT", "simpleText": "!PRC 10/124 PRC RWY 03R/21L CLSD 2610241300-2610302359"}]}}});
        let (id, v) = normalise(&f).unwrap();
        assert_eq!(id, "1234567890123456");
        assert_eq!(v["location"], "PRC");
        assert_eq!(v["category"], "Runway");
        assert!(v["text"].as_str().unwrap().contains("CLSD"));
        // flat properties (schema drift) still read
        let flat = json!({"properties": {"id": 42, "number": "10/1", "location": "SDL", "text": "AD AP CLSD", "classification": "DOMESTIC"}});
        let (id, v) = normalise(&flat).unwrap();
        assert_eq!(id, "42");
        assert_eq!(v["category"], "Aerodrome");
    }
}
