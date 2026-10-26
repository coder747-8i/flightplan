//! Small JSON documents the UI saves on the server (aircraft profiles, app preferences, personal
//! minimums), one set per user (saved plans have their own endpoints in plans.rs), so
//! they survive a cleared browser cache and are shared by every device that opens the app.

use crate::auth::CurrentUser;
use crate::{ApiError, ApiResult, AppState};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde_json::Value;
use std::sync::Arc;

const KEYS: &[&str] = &["aircraft", "prefs", "minimums"];

pub fn routes() -> Router<Arc<AppState>> {
    Router::new().route("/user/{key}", get(read).put(write))
}

fn file(s: &AppState, user: &str, key: &str) -> ApiResult<std::path::PathBuf> {
    if !KEYS.contains(&key) {
        return Err(ApiError::status(StatusCode::NOT_FOUND, "unknown document"));
    }
    Ok(crate::auth::user_dir(s, user).join(format!("{key}.json")))
}

/// The saved document, or `null` when nothing has been saved yet.
async fn read(State(s): State<Arc<AppState>>, CurrentUser(u): CurrentUser, Path(key): Path<String>) -> ApiResult<Json<Value>> {
    let path = file(&s, &u, &key)?;
    Ok(Json(match tokio::fs::read(&path).await {
        Ok(b) => serde_json::from_slice(&b).unwrap_or(Value::Null),
        Err(_) => Value::Null,
    }))
}

async fn write(State(s): State<Arc<AppState>>, CurrentUser(u): CurrentUser, Path(key): Path<String>, Json(v): Json<Value>) -> ApiResult<Json<Value>> {
    let path = file(&s, &u, &key)?;
    if let Some(d) = path.parent() {
        tokio::fs::create_dir_all(d).await?;
    }
    // write-then-rename so a crash never leaves half a file
    let tmp = path.with_extension("json.tmp");
    tokio::fs::write(&tmp, serde_json::to_vec_pretty(&v)?).await?;
    tokio::fs::rename(&tmp, &path).await?;
    crate::plans::announce(&s, &u, &key);
    Ok(Json(serde_json::json!({"ok": true})))
}
