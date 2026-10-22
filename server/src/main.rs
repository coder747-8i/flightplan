//! Flightplan API server: airport data, search, terrain profiles, weather/TFR proxy, Windy,
//! and static hosting for the web app + PMTiles chart.

mod db;
mod geo;
mod proxy;
mod terrain;
mod wx;

use axum::{
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{net::SocketAddr, path::PathBuf, sync::Arc, time::Duration};
use tower_http::{compression::CompressionLayer, services::ServeDir};

pub struct AppState {
    pub db: db::Db,
    pub terrain: terrain::Terrain,
    pub cache: proxy::Cached,
    pub windy_point_key: Option<String>,
    pub windy_map_key: Option<String>,
}

type S = State<Arc<AppState>>;

pub struct ApiError(anyhow::Error);
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        tracing::warn!("api error: {:#}", self.0);
        (StatusCode::BAD_GATEWAY, Json(json!({"error": self.0.to_string()}))).into_response()
    }
}
impl<E: Into<anyhow::Error>> From<E> for ApiError {
    fn from(e: E) -> Self {
        ApiError(e.into())
    }
}
type ApiResult<T> = Result<T, ApiError>;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_max_level(tracing::Level::INFO).init();
    let root = PathBuf::from(std::env::var("FP_ROOT").unwrap_or_else(|_| "..".into()));
    let _ = dotenvy::from_path(root.join(".env"));
    let http = reqwest::Client::builder()
        .user_agent("flightplan/0.1 (personal flight planner)")
        .timeout(Duration::from_secs(30))
        .build()?;
    let state = Arc::new(AppState {
        db: db::Db::open(&root.join("data/build/aviation.sqlite"))?,
        terrain: terrain::Terrain::new(root.join("data/cache/terrarium"), http.clone()),
        cache: proxy::Cached::new(http),
        windy_point_key: std::env::var("WINDY_POINT_KEY").ok().filter(|k| !k.is_empty()),
        windy_map_key: std::env::var("WINDY_MAP_KEY").ok().filter(|k| !k.is_empty()),
    });

    let api = Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/config", get(config))
        .route("/airport/{id}", get(airport))
        .route("/search", get(search))
        .route("/resolve", get(resolve))
        .route("/nearest", get(nearest))
        .route("/profile", post(profile))
        .route("/obstacles", get(obstacles))
        .route("/elevation", get(elevation))
        .merge(wx::routes());

    let web = root.join("web/dist");
    let app = Router::new()
        .nest("/api", api)
        .nest_service("/tiles", ServeDir::new(root.join("tiles")))
        .fallback_service(ServeDir::new(&web).fallback(tower_http::services::ServeFile::new(web.join("index.html"))))
        .layer(CompressionLayer::new())
        .with_state(state);

    let port: u16 = std::env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(8787);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("listening on http://{addr}");
    axum::serve(tokio::net::TcpListener::bind(addr).await?, app).await?;
    Ok(())
}

async fn config(State(s): S) -> Json<Value> {
    Json(json!({
        "windyMapKey": s.windy_map_key,
        "windyPoint": s.windy_point_key.is_some(),
    }))
}

async fn airport(State(s): S, Path(id): Path<String>) -> ApiResult<Response> {
    Ok(match s.db.airport(&id)? {
        Some(v) => Json(v).into_response(),
        None => (StatusCode::NOT_FOUND, Json(json!({"error": "not found"}))).into_response(),
    })
}

#[derive(Deserialize)]
struct Q {
    q: String,
}
async fn search(State(s): S, Query(q): Query<Q>) -> ApiResult<Json<Value>> {
    Ok(Json(serde_json::to_value(s.db.search(&q.q)?)?))
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
    for tok in q.ids.split([' ', ',', '\n', '\t']).filter(|t| !t.is_empty() && *t != "DCT") {
        let p = s.db.resolve(tok, prev)?;
        if let Some(p) = &p {
            prev = Some((p.lat, p.lon));
        }
        out.push(json!({"token": tok, "place": p}));
    }
    Ok(Json(Value::Array(out)))
}

#[derive(Deserialize)]
struct Near {
    lat: f64,
    lon: f64,
    r: Option<f64>,
}
async fn nearest(State(s): S, Query(q): Query<Near>) -> ApiResult<Json<Value>> {
    let list = s.db.airports_near(q.lat, q.lon, q.r.unwrap_or(25.0).min(150.0), true)?;
    Ok(Json(Value::Array(list.into_iter().take(30).map(|(p, d)| json!({"place": p, "dist": (d * 10.0).round() / 10.0})).collect())))
}

async fn profile(State(s): S, Json(req): Json<terrain::ProfileReq>) -> ApiResult<Json<Value>> {
    let samples = s.terrain.profile(&req).await?;
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
        let cand = s.db.obstacles_in(la0 - pad, lo0 - pad * 1.3, la1 + pad, lo1 + pad * 1.3, 200)?;
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
    Ok(Json(json!({"samples": samples, "obstacles": obs})))
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
    Ok(Json(serde_json::to_value(s.db.obstacles_in(v[0], v[1], v[2], v[3], q.min.unwrap_or(200))?)?))
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
