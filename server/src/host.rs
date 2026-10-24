//! Native integration supplied by the desktop app: save dialogs, folder picker, printing. The web
//! UI calls these over HTTP so the same code runs in a browser (where it falls back to
//! ordinary downloads) and in the app's webview (where blob downloads don't work everywhere).

use crate::{ApiError, ApiResult, AppState};
use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, Query, State},
    http::StatusCode,
    routing::post,
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{path::PathBuf, sync::Arc};

pub trait Host: Send + Sync + 'static {
    /// Ask where to save a file. `filter` is (label, extensions without the dot).
    fn save_file(&self, suggested_name: &str, filter: (&str, &[&str])) -> Option<PathBuf>;
    /// Ask for a folder.
    fn pick_folder(&self, title: &str) -> Option<PathBuf>;
    /// Open a web page in the user's browser.
    fn open_url(&self, url: &str);
    /// Print the page (WKWebView ignores window.print()).
    fn print(&self);
}

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/host/save", post(save).layer(DefaultBodyLimit::max(64 << 20)))
        .route("/host/open", post(open))
        .route("/host/print", post(print))
}

fn host(s: &AppState) -> ApiResult<Arc<dyn Host>> {
    s.cfg.host.clone().ok_or_else(|| ApiError::status(StatusCode::NOT_FOUND, "only available in the desktop app"))
}

#[derive(Deserialize)]
struct SaveQ {
    name: String,
    /// label shown in the dialog's type filter, e.g. "Garmin flight plan"
    label: Option<String>,
}

/// Body = file contents. Answers {"path": ...} or {"cancelled": true}.
async fn save(State(s): State<Arc<AppState>>, Query(q): Query<SaveQ>, body: Bytes) -> ApiResult<Json<Value>> {
    let host = host(&s)?;
    // a bare file name only: the dialog decides the folder
    let name: String = q.name.chars().filter(|c| !matches!(c, '/' | '\\' | ':' | '\0')).collect();
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_string()).unwrap_or_default();
    let label = q.label.unwrap_or_else(|| format!("{} file", ext.to_uppercase()));
    let picked = tokio::task::spawn_blocking(move || {
        let exts: Vec<&str> = if ext.is_empty() { vec![] } else { vec![ext.as_str()] };
        host.save_file(&name, (&label, &exts))
    })
    .await?;
    Ok(Json(match picked {
        Some(path) => {
            tokio::fs::write(&path, &body).await?;
            json!({"path": path})
        }
        None => json!({"cancelled": true}),
    }))
}

#[derive(Deserialize)]
struct OpenReq {
    url: String,
}

async fn open(State(s): State<Arc<AppState>>, Json(r): Json<OpenReq>) -> ApiResult<Json<Value>> {
    let host = host(&s)?;
    if !(r.url.starts_with("https://") || r.url.starts_with("http://")) {
        return Err(ApiError::status(StatusCode::BAD_REQUEST, "only web links can be opened"));
    }
    host.open_url(&r.url);
    Ok(Json(json!({"ok": true})))
}

async fn print(State(s): State<Arc<AppState>>) -> ApiResult<Json<Value>> {
    host(&s)?.print();
    Ok(Json(json!({"ok": true})))
}
