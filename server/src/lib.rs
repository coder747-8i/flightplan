//! Flightplan API server: airport data, search, terrain profiles, weather/TFR proxy, Windy,
//! data-pack setup, sign-in, and static hosting for the web app + PMTiles chart.
//! Used by the `flightplan-server` binary (browser/LAN use) and embedded in the desktop app.

pub mod airspace;
pub mod auth;
pub mod db;
pub mod forecast;
pub mod geo;
mod host;
pub mod notam;
pub mod plans;
pub mod proxy;
pub mod settings;
pub mod setup;
pub mod terrain;
mod userstore;
pub mod wx;

pub use host::Host;

use axum::{
    extract::{Path, Query, State},
    http::{header, Method, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use parking_lot::RwLock;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tower_http::{compression::CompressionLayer, services::ServeDir};

/// Where everything lives. `from_root` is the repository layout used by `run.sh`;
/// the desktop app keeps its data in the per-user app-data folder.
#[derive(Clone)]
pub struct Config {
    /// aviation.sqlite, airspace_index.json, manifest.json
    pub build_dir: PathBuf,
    /// *.pmtiles
    pub tiles_dir: PathBuf,
    /// terrain tiles and temporary downloads
    pub cache_dir: PathBuf,
    pub settings_file: PathBuf,
    /// aircraft and app preferences saved from the UI
    pub user_dir: PathBuf,
    /// built web app (web/dist)
    pub web_dir: PathBuf,
    /// data pack offered by the setup screen's "Download" button
    pub default_data_url: String,
    /// native dialogs (desktop app only)
    pub host: Option<Arc<dyn Host>>,
    /// when set, requests whose Host header is not in this list are refused (DNS-rebinding guard)
    pub allowed_hosts: Option<Vec<String>>,
}

/// Where the setup screen downloads the data pack from: the `data` release that
/// .github/workflows/data.yml refreshes every FAA cycle. Override at build time with FP_DATA_URL.
pub const DEFAULT_DATA_URL: &str = match option_env!("FP_DATA_URL") {
    Some(u) if !u.is_empty() => u,
    _ => "https://github.com/coder747-8i/flightplan/releases/download/data/flightplan-data.zip",
};

impl Config {
    pub fn from_root(root: &std::path::Path) -> Self {
        Self {
            build_dir: root.join("data/build"),
            tiles_dir: root.join("tiles"),
            cache_dir: root.join("data/cache"),
            settings_file: root.join("data/settings.json"),
            user_dir: root.join("data/user"),
            web_dir: root.join("web/dist"),
            default_data_url: DEFAULT_DATA_URL.into(),
            host: None,
            allowed_hosts: None,
        }
    }

    /// Everything under one per-user folder; the web app ships separately (read-only).
    pub fn app_dir(data: &std::path::Path, web_dir: PathBuf) -> Self {
        Self {
            build_dir: data.join("build"),
            tiles_dir: data.join("tiles"),
            cache_dir: data.join("cache"),
            settings_file: data.join("settings.json"),
            user_dir: data.join("user"),
            web_dir,
            default_data_url: DEFAULT_DATA_URL.into(),
            host: None,
            allowed_hosts: None,
        }
    }
}

/// The chart/airport data set. Absent until a data pack is installed; swapped in place when
/// a new one is.
pub struct Data {
    pub db: db::Db,
    pub airspace: airspace::Airspace,
    pub info: setup::DataInfo,
}

pub struct AppState {
    pub cfg: Config,
    pub data: RwLock<Option<Arc<Data>>>,
    pub terrain: terrain::Terrain,
    pub cache: proxy::Cached,
    pub settings: settings::Settings,
    pub setup: setup::Jobs,
    /// web app sign-in (data/auth.json)
    pub auth: auth::Auth,
    /// changes to users' documents, for the live update stream (plans.rs)
    pub events: tokio::sync::broadcast::Sender<plans::Change>,
}

impl AppState {
    /// The installed data set, for callers outside the HTTP API.
    pub fn try_data(&self) -> anyhow::Result<Arc<Data>> {
        self.data.read().clone().ok_or_else(|| anyhow::anyhow!("chart data is not installed yet"))
    }

    /// The installed data set, or a 503 the UI turns into "finish setup".
    pub fn data(&self) -> Result<Arc<Data>, ApiError> {
        self.data
            .read()
            .clone()
            .ok_or_else(|| ApiError::status(StatusCode::SERVICE_UNAVAILABLE, "chart data is not installed yet: finish setup first"))
    }
}

type S = State<Arc<AppState>>;

pub struct ApiError(pub StatusCode, pub anyhow::Error);
impl ApiError {
    pub fn status(code: StatusCode, msg: impl Into<String>) -> Self {
        ApiError(code, anyhow::anyhow!(msg.into()))
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        tracing::warn!("api error: {:#}", self.1);
        (self.0, Json(json!({"error": self.1.to_string()}))).into_response()
    }
}
impl<E: Into<anyhow::Error>> From<E> for ApiError {
    fn from(e: E) -> Self {
        ApiError(StatusCode::BAD_GATEWAY, e.into())
    }
}
type ApiResult<T> = Result<T, ApiError>;

/// Build the shared state. Missing data is not an error: the app starts in setup mode.
pub fn state(cfg: Config) -> anyhow::Result<Arc<AppState>> {
    let http = reqwest::Client::builder()
        .user_agent(concat!("flightplan/", env!("CARGO_PKG_VERSION"), " (personal flight planner)"))
        .connect_timeout(Duration::from_secs(20))
        .read_timeout(Duration::from_secs(60))
        .build()?;
    let data = match setup::load_data(&cfg) {
        Ok(d) => Some(Arc::new(d)),
        Err(e) => {
            tracing::warn!("no chart data loaded ({e:#}); the app will open the setup screen");
            None
        }
    };
    let st = Arc::new(AppState {
        terrain: terrain::Terrain::new(cfg.cache_dir.join("terrarium"), http.clone()),
        cache: proxy::Cached::new(http),
        settings: settings::Settings::load(cfg.settings_file.clone()),
        auth: auth::Auth::load(cfg.settings_file.with_file_name("auth.json")),
        events: tokio::sync::broadcast::channel(64).0,
        setup: setup::Jobs::default(),
        data: RwLock::new(data),
        cfg,
    });
    // a fresh standalone server prints the code that authorises its first password
    if st.cfg.host.is_none() {
        st.auth.announce();
    }
    Ok(st)
}

pub fn router(state: Arc<AppState>) -> Router {
    let api = Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/config", get(config))
        .route("/airport/{id}", get(airport))
        .route("/search", get(search))
        .route("/resolve", get(resolve))
        .route("/route", get(route_text))
        .route("/nearest", get(nearest))
        .route("/profile", post(profile))
        .route("/obstacles", get(obstacles))
        .route("/elevation", get(elevation))
        .route("/fbo/{id}", get(fbo))
        .merge(wx::routes())
        .merge(forecast::routes())
        .merge(notam::routes())
        .merge(settings::routes())
        .merge(setup::routes())
        .merge(host::routes())
        .merge(userstore::routes())
        .merge(auth::routes())
        .merge(plans::routes());

    let web = state.cfg.web_dir.clone();
    Router::new()
        .nest("/api", api)
        .nest_service("/tiles", ServeDir::new(&state.cfg.tiles_dir))
        .fallback_service(ServeDir::new(&web).fallback(tower_http::services::ServeFile::new(web.join("index.html"))))
        .layer(axum::middleware::from_fn(cache_headers))
        .layer(axum::middleware::from_fn(security_headers))
        .layer(axum::middleware::from_fn_with_state(state.clone(), auth::require))
        .layer(axum::middleware::from_fn_with_state(state.clone(), guard))
        .layer(CompressionLayer::new())
        .with_state(state)
}

/// Serve until the listener fails.
pub async fn serve(state: Arc<AppState>, listener: tokio::net::TcpListener) -> anyhow::Result<()> {
    tracing::info!("listening on http://{}", listener.local_addr()?);
    // the peer address tells the sign-in code whether a request comes from this computer
    axum::serve(listener, router(state).into_make_service_with_connect_info::<std::net::SocketAddr>()).await?;
    Ok(())
}

/// Requests that change something must carry `X-Flightplan` (a custom header a cross-site page
/// can't send without a CORS preflight, which is never granted), and in the desktop app the
/// Host header must be our own address.
async fn guard(State(s): S, req: axum::extract::Request, next: axum::middleware::Next) -> Response {
    if let Some(allowed) = &s.cfg.allowed_hosts {
        let host = req.headers().get(header::HOST).and_then(|h| h.to_str().ok()).unwrap_or("");
        if !allowed.iter().any(|a| a == host) {
            return (StatusCode::FORBIDDEN, "unexpected Host header").into_response();
        }
    }
    let mutating = !matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS);
    if mutating && req.uri().path().starts_with("/api/") && !req.headers().contains_key("x-flightplan") {
        return (StatusCode::FORBIDDEN, Json(json!({"error": "missing X-Flightplan header"}))).into_response();
    }
    next.run(req).await
}

/// The page must always be revalidated (Safari otherwise keeps running an old bundle);
/// hashed build assets never change, so they can be cached for good.
async fn cache_headers(req: axum::extract::Request, next: axum::middleware::Next) -> Response {
    let path = req.uri().path().to_string();
    let mut res = next.run(req).await;
    // tiles too: a newly installed data pack replaces them under the same names
    let value = if path.starts_with("/assets/") {
        Some("public, max-age=31536000, immutable")
    } else if !path.starts_with("/api/") {
        Some("no-cache")
    } else {
        None
    };
    if let Some(v) = value {
        if !res.headers().contains_key(header::CACHE_CONTROL) {
            res.headers_mut().insert(header::CACHE_CONTROL, header::HeaderValue::from_static(v));
        }
    }
    res
}

/// What the page may load: our own scripts (plus Windy's and Leaflet's for the Windy view),
/// styles inline (Svelte sets style attributes), map tiles and weather from anywhere over
/// HTTPS, MapLibre's blob workers. No framing, no plugins, no inline scripts.
const CSP: &str = "default-src 'self'; script-src 'self' https://*.windy.com https://unpkg.com; style-src 'self' 'unsafe-inline' https://*.windy.com https://unpkg.com; img-src 'self' data: blob: https:; font-src 'self' data: https:; connect-src 'self' https:; worker-src 'self' blob:; child-src 'self' blob:; frame-src https://*.windy.com; object-src 'none'; base-uri 'self'; form-action 'self'; frame-ancestors 'none'";

/// Headers that make the app safer to put on the internet.
async fn security_headers(req: axum::extract::Request, next: axum::middleware::Next) -> Response {
    let https = auth::is_https(req.headers());
    let mut res = next.run(req).await;
    let h = res.headers_mut();
    let mut put = |k: &'static str, v: &'static str| {
        h.entry(k).or_insert(header::HeaderValue::from_static(v));
    };
    put("content-security-policy", CSP);
    put("x-content-type-options", "nosniff");
    put("x-frame-options", "DENY");
    put("referrer-policy", "same-origin");
    put("permissions-policy", "camera=(), microphone=(), payment=(), usb=()");
    put("cross-origin-opener-policy", "same-origin");
    if https {
        put("strict-transport-security", "max-age=31536000");
    }
    res
}

async fn config(State(s): S) -> Json<Value> {
    let k = s.settings.get();
    Json(json!({
        "windyMapKey": k.windy_map,
        "windyPoint": k.windy_point.is_some(),
        "notams": k.notam_id.is_some() && k.notam_secret.is_some(),
        "desktop": s.cfg.host.is_some(),
        "data": s.data.read().as_ref().map(|d| d.info.clone()),
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

async fn airport(State(s): S, Path(id): Path<String>) -> ApiResult<Response> {
    Ok(match s.data()?.db.airport(&id)? {
        Some(v) => Json(v).into_response(),
        None => (StatusCode::NOT_FOUND, Json(json!({"error": "not found"}))).into_response(),
    })
}

async fn fbo(State(s): S, Path(id): Path<String>) -> ApiResult<Response> {
    Ok(match s.data()?.db.fbos(&id)? {
        Some(v) => Json(v).into_response(),
        None => (StatusCode::NOT_FOUND, Json(json!([]))).into_response(),
    })
}

#[derive(Deserialize)]
struct Q {
    q: String,
}
async fn search(State(s): S, Query(q): Query<Q>) -> ApiResult<Json<Value>> {
    Ok(Json(serde_json::to_value(s.data()?.db.search(&q.q)?)?))
}

#[derive(Deserialize)]
struct Ids {
    ids: String,
}
/// Resolve a route string ("KNYL BZA 3300N11400W KPHX"); ambiguous fixes pick the one
/// nearest the previous point.
async fn resolve(State(s): S, Query(q): Query<Ids>) -> ApiResult<Json<Value>> {
    let mut out = Vec::new();
    let mut prev: Option<(f64, f64)> = None;
    let data = s.data()?;
    for tok in q.ids.split([' ', ',', '\n', '\t']).filter(|t| !t.is_empty() && *t != "DCT") {
        let p = data.db.resolve(tok, prev)?;
        if let Some(p) = &p {
            prev = Some((p.lat, p.lon));
        }
        out.push(json!({"token": tok, "place": p}));
    }
    Ok(Json(Value::Array(out)))
}

/// Airway identifiers: Victor (V), RNAV T, jet (J) and Q routes, colored airways.
pub fn is_airway(tok: &str) -> bool {
    let mut c = tok.chars();
    matches!(c.next(), Some('V' | 'T' | 'J' | 'Q' | 'A' | 'B' | 'G' | 'R')) && tok.len() >= 2 && tok.len() <= 5 && c.all(|x| x.is_ascii_digit())
}

/// Resolve a route string ("KNYL BZA 3300N11400W KPHX", "KSDL PXR V327 FLG KGCN"): ambiguous
/// fixes pick the one nearest the previous point, and an airway between two points expands to
/// the fixes along it. Returns the points and what couldn't be resolved (with the reason).
pub fn resolve_route(data: &Data, text: &str) -> (Vec<db::Place>, Vec<String>) {
    let mut out: Vec<db::Place> = Vec::new();
    let mut missing = Vec::new();
    let mut prev: Option<(f64, f64)> = None;
    let toks: Vec<String> = text.to_uppercase().split([' ', ',', '\n', '\t']).filter(|t| !t.is_empty() && *t != "DCT").map(String::from).collect();
    let mut pending_airway: Option<String> = None;
    for tok in toks {
        if is_airway(&tok) && data.db.resolve(&tok, prev).ok().flatten().is_none() {
            if !data.db.has_airways() {
                missing.push(format!("{tok} (airways need an updated data pack: Settings, Chart data)"));
            } else if out.is_empty() {
                missing.push(format!("{tok} (an airway needs a fix before it)"));
            } else {
                pending_airway = Some(tok);
            }
            continue;
        }
        match data.db.resolve(&tok, prev) {
            Ok(Some(p)) => {
                if let (Some(awy), Some(last)) = (pending_airway.take(), out.last()) {
                    match data.db.airway_between(&awy, (&last.id, last.lat, last.lon), &p.id) {
                        Ok(Some(points)) => out.extend(points),
                        _ => missing.push(format!("{awy} (doesn't connect {} and {})", last.id, p.id)),
                    }
                }
                prev = Some((p.lat, p.lon));
                out.push(p);
            }
            _ => missing.push(tok.to_string()),
        }
    }
    if let Some(awy) = pending_airway {
        missing.push(format!("{awy} (an airway needs a fix after it)"));
    }
    (out, missing)
}

#[derive(Deserialize)]
struct RouteQ {
    text: String,
}
/// A whole route string, airways expanded: {"places", "missing"}.
async fn route_text(State(s): S, Query(q): Query<RouteQ>) -> ApiResult<Json<Value>> {
    let data = s.data()?;
    let (places, missing) = resolve_route(&data, &q.text);
    Ok(Json(json!({"places": places, "missing": missing})))
}

#[derive(Deserialize)]
struct Near {
    lat: f64,
    lon: f64,
    r: Option<f64>,
}
async fn nearest(State(s): S, Query(q): Query<Near>) -> ApiResult<Json<Value>> {
    let list = s.data()?.db.airports_near(q.lat, q.lon, q.r.unwrap_or(25.0).min(150.0), true)?;
    Ok(Json(Value::Array(list.into_iter().take(30).map(|(p, d)| json!({"place": p, "dist": (d * 10.0).round() / 10.0})).collect())))
}

async fn profile(State(s): S, Json(req): Json<terrain::ProfileReq>) -> ApiResult<Json<Value>> {
    Ok(Json(route_profile(&s, &req).await?))
}

/// Terrain samples, obstacles within the corridor and airspace crossings along a route:
/// {"samples", "obstacles", "airspace"}.
pub async fn route_profile(s: &AppState, req: &terrain::ProfileReq) -> anyhow::Result<Value> {
    let samples = s.terrain.profile(req).await?;
    // obstacles within 1 NM of the route
    let (mut la0, mut lo0, mut la1, mut lo1) = (90.0f64, 180.0f64, -90.0f64, -180.0f64);
    for p in &req.path {
        la0 = la0.min(p[1]);
        la1 = la1.max(p[1]);
        lo0 = lo0.min(p[0]);
        lo1 = lo1.max(p[0]);
    }
    let corridor = req.corridor.unwrap_or(1.0);
    let pad = corridor / 60.0 + 0.02;
    let mut obs = Vec::new();
    if (la1 - la0) * (lo1 - lo0) < 400.0 {
        let mut d0 = 0.0;
        let legs: Vec<_> = req.path.windows(2).collect();
        let cand = s.try_data()?.db.obstacles_in(la0 - pad, lo0 - pad * 1.3, la1 + pad, lo1 + pad * 1.3, 200)?;
        for w in legs {
            let (a, b) = (w[0], w[1]);
            let len = geo::distance_nm(a[1], a[0], b[1], b[0]);
            for o in &cand {
                let (xt, at) = geo::cross_along(a[1], a[0], b[1], b[0], o.lat, o.lon);
                if xt.abs() <= corridor && (0.0..=len).contains(&at) {
                    obs.push(json!({"d": d0 + at, "xt": xt, "msl": o.msl, "agl": o.agl, "lit": o.lit, "type": o.kind, "lat": o.lat, "lon": o.lon}));
                }
            }
            d0 += len;
        }
    }
    let along: Vec<(f64, f64, f64)> = samples.iter().map(|x| (x.d, x.lat, x.lon)).collect();
    let crossings = s.try_data()?.airspace.crossings(&along);
    Ok(json!({"samples": samples, "obstacles": obs, "airspace": crossings}))
}

#[derive(Deserialize)]
struct BBox {
    bbox: String,
    min: Option<i64>,
}
async fn obstacles(State(s): S, Query(q): Query<BBox>) -> ApiResult<Json<Value>> {
    let v: Vec<f64> = q.bbox.split(',').filter_map(|x| x.parse().ok()).collect();
    if v.len() != 4 {
        return Err(anyhow::anyhow!("bbox=lat0,lon0,lat1,lon1").into());
    }
    Ok(Json(serde_json::to_value(s.data()?.db.obstacles_in(v[0], v[1], v[2], v[3], q.min.unwrap_or(200))?)?))
}

#[derive(Deserialize)]
struct LatLon {
    lat: f64,
    lon: f64,
}
async fn elevation(State(s): S, Query(q): Query<LatLon>) -> ApiResult<Response> {
    let e = s.terrain.elevation_ft(q.lat, q.lon).await?;
    Ok(([(header::CACHE_CONTROL, "public, max-age=86400")], Json(json!({"elev": e.round()}))).into_response())
}
