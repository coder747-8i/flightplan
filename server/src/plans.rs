//! Saved flight plans, one by one (so the web app and a connected desktop app can't overwrite
//! each other's lists), and a live stream of changes: every save or delete here, and every
//! document saved through userstore, is announced to that user's open web apps and desktop apps
//! as a server-sent event, so a plan saved on one shows up on the other straight away.
//!
//! Plans are kept per user in <user dir>/plans.json, newest first, in the web app's format:
//! {id, name, savedAt, route: {waypoints, cruise, departure, stopMinutes}, routeText,
//! fuelStops, aircraftId, aircraft, distanceNm, eteMin, fuelGal}.

use crate::auth::{user_dir, CurrentUser};
use crate::{ApiError, ApiResult, AppState};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
    response::IntoResponse,
    routing::{get, put},
    Json, Router,
};
use serde_json::{json, Value};
use std::{convert::Infallible, sync::Arc, time::Duration};

/// A change to one user's documents: (user id, what changed: "plans", "aircraft", ...).
pub type Change = (String, String);

pub fn routes() -> Router<Arc<AppState>> {
    Router::new().route("/plans", get(list)).route("/plans/{id}", put(save).delete(remove)).route("/events", get(events))
}

/// Tell the user's open apps that something changed.
pub fn announce(s: &AppState, user: &str, what: &str) {
    let _ = s.events.send((user.to_string(), what.to_string()));
}

/// One lock for every plans file: saves are rare and quick.
fn lock() -> &'static parking_lot::Mutex<()> {
    static L: std::sync::OnceLock<parking_lot::Mutex<()>> = std::sync::OnceLock::new();
    L.get_or_init(|| parking_lot::Mutex::new(()))
}

fn file(s: &AppState, user: &str) -> std::path::PathBuf {
    user_dir(s, user).join("plans.json")
}

fn read(s: &AppState, user: &str) -> Vec<Value> {
    std::fs::read(file(s, user)).ok().and_then(|b| serde_json::from_slice::<Vec<Value>>(&b).ok()).unwrap_or_default()
}

fn write(s: &AppState, user: &str, plans: &[Value]) -> anyhow::Result<()> {
    let p = file(s, user);
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d)?;
    }
    let tmp = p.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(plans)?)?;
    std::fs::rename(&tmp, &p)?;
    Ok(())
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

async fn list(State(s): State<Arc<AppState>>, CurrentUser(u): CurrentUser) -> Json<Value> {
    let _g = lock().lock();
    Json(Value::Array(read(&s, &u)))
}

/// Add or replace one plan. A plan with the same name is replaced too (saving under a name
/// overwrites it, as in both apps).
async fn save(State(s): State<Arc<AppState>>, CurrentUser(u): CurrentUser, Path(id): Path<String>, Json(plan): Json<Value>) -> ApiResult<Json<Value>> {
    if !valid_id(&id) || plan["id"].as_str() != Some(id.as_str()) {
        return Err(ApiError::status(StatusCode::BAD_REQUEST, "the plan's id doesn't match"));
    }
    let name = plan["name"].as_str().unwrap_or("").trim().to_string();
    if name.is_empty() || name.chars().count() > 120 || !plan["route"]["waypoints"].is_array() {
        return Err(ApiError::status(StatusCode::BAD_REQUEST, "a plan needs a name and a route"));
    }
    {
        let _g = lock().lock();
        let mut plans = read(&s, &u);
        plans.retain(|p| p["id"].as_str() != Some(id.as_str()) && p["name"].as_str().map(str::trim) != Some(name.as_str()));
        plans.insert(0, plan);
        plans.truncate(500);
        write(&s, &u, &plans)?;
    }
    announce(&s, &u, "plans");
    Ok(Json(json!({"ok": true})))
}

async fn remove(State(s): State<Arc<AppState>>, CurrentUser(u): CurrentUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    {
        let _g = lock().lock();
        let mut plans = read(&s, &u);
        let n = plans.len();
        plans.retain(|p| p["id"].as_str() != Some(id.as_str()));
        if plans.len() != n {
            write(&s, &u, &plans)?;
        }
    }
    announce(&s, &u, "plans");
    Ok(Json(json!({"ok": true})))
}

/// Server-sent events for the signed-in user: `{"what": "plans"}` and so on; `{"what": "all"}`
/// when some were missed. Kept alive every 20 s so proxies don't drop it.
async fn events(State(s): State<Arc<AppState>>, CurrentUser(u): CurrentUser) -> impl IntoResponse {
    let rx = s.events.subscribe();
    let stream = futures::stream::unfold((rx, u), |(mut rx, u)| async move {
        loop {
            match rx.recv().await {
                Ok((who, what)) if who == u => return Some((Ok::<_, Infallible>(Event::default().data(json!({"what": what}).to_string())), (rx, u))),
                Ok(_) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => return Some((Ok(Event::default().data(r#"{"what":"all"}"#)), (rx, u))),
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return None,
            }
        }
    });
    // proxies (nginx) must not buffer the stream
    ([("x-accel-buffering", "no"), ("cache-control", "no-cache")], Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(20))))
}

#[cfg(test)]
mod tests {
    #[test]
    fn ids() {
        assert!(super::valid_id("abc_123-x"));
        assert!(!super::valid_id("../etc") && !super::valid_id(""));
    }
}
