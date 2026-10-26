//! Chart & airport data ("data pack"): what is installed, and installing a new one by
//! download, by uploading a .zip from the setup screen, or by copying from a folder.
//!
//! A data pack is a zip holding aviation.sqlite, airspace_index.json, the three .pmtiles files
//! and manifest.json (made by pipeline/make_datapack.py). Files are matched by name, so any
//! folder layout inside the zip works. New data is unpacked to a staging folder, checked,
//! and only then moved over the live copy.

use crate::{airspace, db, ApiError, ApiResult, AppState, Config, Data};
use axum::{
    body::Body,
    extract::State,
    http::{header, HeaderMap, StatusCode},
    routing::{get, post},
    Json, Router,
};
use futures::StreamExt;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::io::AsyncWriteExt;

const BUILD_FILES: &[&str] = &["aviation.sqlite", "airspace_index.json"];
const TILE_FILES: &[&str] = &["aero.pmtiles", "airspace.pmtiles", "obstacles.pmtiles"];
const MANIFEST: &str = "manifest.json";

/// What the installed data pack says about itself (manifest.json). Data built locally with the
/// pipeline has no manifest, so every field is optional.
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataInfo {
    /// NASR cycle effective date, YYYY-MM-DD
    #[serde(default)]
    pub cycle: Option<String>,
    /// next cycle date, after which the data is out of date
    #[serde(default)]
    pub expires: Option<String>,
    #[serde(default)]
    pub built: Option<String>,
    /// where this copy was installed from (URL, file name or folder)
    #[serde(default)]
    pub installed_from: Option<String>,
    #[serde(default)]
    pub installed_at: Option<String>,
}

/// Progress of the one install that may run at a time.
#[derive(Clone, Serialize)]
pub struct Job {
    /// idle | downloading | uploading | copying | extracting | checking | done | error
    pub state: &'static str,
    pub done: u64,
    pub total: u64,
    pub message: String,
}

impl Default for Job {
    fn default() -> Self {
        Job { state: "idle", done: 0, total: 0, message: String::new() }
    }
}

pub struct Jobs {
    job: Mutex<Job>,
    cancel: AtomicBool,
    /// no overall timeout: data packs are hundreds of MB
    http: reqwest::Client,
}

impl Default for Jobs {
    fn default() -> Self {
        let http = reqwest::Client::builder()
            .user_agent(concat!("flightplan/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(20))
            .read_timeout(Duration::from_secs(90))
            .build()
            .unwrap_or_default();
        Jobs { job: Mutex::new(Job::default()), cancel: AtomicBool::new(false), http }
    }
}

impl Jobs {
    fn set(&self, state: &'static str, done: u64, total: u64, message: impl Into<String>) {
        *self.job.lock() = Job { state, done, total, message: message.into() };
    }

    fn progress(&self, done: u64) {
        self.job.lock().done = done;
    }

    /// Claim the job slot, or refuse when an install is already running.
    fn begin(&self, state: &'static str, total: u64, message: &str) -> ApiResult<()> {
        let mut j = self.job.lock();
        if !matches!(j.state, "idle" | "done" | "error") {
            return Err(ApiError::status(StatusCode::CONFLICT, "an install is already running"));
        }
        self.cancel.store(false, Ordering::SeqCst);
        *j = Job { state, done: 0, total, message: message.into() };
        Ok(())
    }

    fn cancelled(&self) -> anyhow::Result<()> {
        if self.cancel.load(Ordering::SeqCst) {
            anyhow::bail!("cancelled");
        }
        Ok(())
    }
}

/// Open the installed data set (fails when any file is missing or unreadable).
pub fn load_data(cfg: &Config) -> anyhow::Result<Data> {
    let missing: Vec<&str> = BUILD_FILES
        .iter()
        .filter(|f| !cfg.build_dir.join(f).is_file())
        .chain(TILE_FILES.iter().filter(|f| !cfg.tiles_dir.join(f).is_file()))
        .copied()
        .collect();
    if !missing.is_empty() {
        anyhow::bail!("missing {}", missing.join(", "));
    }
    let db = db::Db::open(&cfg.build_dir.join("aviation.sqlite"))?;
    let airports = db.airport_count()?;
    if airports == 0 {
        anyhow::bail!("aviation.sqlite has no airports");
    }
    let airspace = airspace::Airspace::load(&cfg.build_dir.join("airspace_index.json"))?;
    let info = std::fs::read(cfg.build_dir.join(MANIFEST)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    tracing::info!("chart data loaded: {airports} airports");
    Ok(Data { db, airspace, info })
}

/// Where a data-pack file goes, by name; None for files we don't use.
fn destination(cfg: &Config, name: &str) -> Option<PathBuf> {
    if BUILD_FILES.contains(&name) || name == MANIFEST {
        Some(cfg.build_dir.join(name))
    } else if TILE_FILES.contains(&name) {
        Some(cfg.tiles_dir.join(name))
    } else {
        None
    }
}

fn staging(cfg: &Config) -> PathBuf {
    cfg.cache_dir.join("pack-staging")
}

fn fresh_dir(dir: &Path) -> anyhow::Result<()> {
    if dir.exists() {
        std::fs::remove_dir_all(dir)?;
    }
    std::fs::create_dir_all(dir)?;
    Ok(())
}

/// Copy with progress, checking for cancel between chunks.
fn copy_counted(mut from: impl Read, to: &Path, jobs: &Jobs, done: &mut u64) -> anyhow::Result<()> {
    let mut out = std::io::BufWriter::new(std::fs::File::create(to)?);
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = from.read(&mut buf)?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n])?;
        *done += n as u64;
        jobs.progress(*done);
        jobs.cancelled()?;
    }
    out.flush()?;
    Ok(())
}

/// Unpack the files we use from a data-pack zip into the staging folder.
fn extract(cfg: &Config, jobs: &Jobs, zip_path: &Path) -> anyhow::Result<PathBuf> {
    let stage = staging(cfg);
    fresh_dir(&stage)?;
    let mut zip = zip::ZipArchive::new(std::fs::File::open(zip_path)?).map_err(|e| anyhow::anyhow!("not a valid data pack zip: {e}"))?;
    let mut wanted = Vec::new();
    let mut total = 0;
    for i in 0..zip.len() {
        let f = zip.by_index(i)?;
        if f.is_dir() {
            continue;
        }
        // enclosed_name rejects absolute paths and ".." (zip-slip); only the file name is used anyway
        let Some(name) = f.enclosed_name().and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned())) else { continue };
        if destination(cfg, &name).is_some() {
            total += f.size();
            wanted.push((i, name));
        }
    }
    jobs.set("extracting", 0, total, "Unpacking chart data");
    let mut done = 0;
    for (i, name) in wanted {
        let f = zip.by_index(i)?;
        copy_counted(f, &stage.join(&name), jobs, &mut done)?;
    }
    Ok(stage)
}

/// Look for the data files in a folder: a data-pack folder, an unzipped pack, or a repository
/// checkout (data/build + tiles).
fn find_in_folder(dir: &Path) -> Result<Vec<(String, PathBuf)>, Vec<&'static str>> {
    let mut found = Vec::new();
    let mut missing = Vec::new();
    for name in BUILD_FILES.iter().chain(TILE_FILES).chain([&MANIFEST]) {
        let candidates = [dir.join(name), dir.join("build").join(name), dir.join("data/build").join(name), dir.join("tiles").join(name)];
        match candidates.into_iter().find(|p| p.is_file()) {
            Some(p) => found.push((name.to_string(), p)),
            None if *name != MANIFEST => missing.push(*name),
            None => {}
        }
    }
    if missing.is_empty() { Ok(found) } else { Err(missing) }
}

fn copy_folder(cfg: &Config, jobs: &Jobs, files: &[(String, PathBuf)]) -> anyhow::Result<PathBuf> {
    let stage = staging(cfg);
    fresh_dir(&stage)?;
    let total = files.iter().filter_map(|(_, p)| p.metadata().ok()).map(|m| m.len()).sum();
    jobs.set("copying", 0, total, "Copying chart data");
    let mut done = 0;
    for (name, path) in files {
        copy_counted(std::fs::File::open(path)?, &stage.join(name), jobs, &mut done)?;
    }
    Ok(stage)
}

/// Rename, retrying briefly (Windows refuses while a reader still has the file open), and
/// falling back to copy + delete across drives.
fn move_file(from: &Path, to: &Path) -> anyhow::Result<()> {
    let mut last = None;
    for attempt in 0..20 {
        match std::fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(e) if e.raw_os_error() == Some(18) || e.kind() == std::io::ErrorKind::CrossesDevices => {
                std::fs::copy(from, to)?;
                std::fs::remove_file(from)?;
                return Ok(());
            }
            Err(e) => {
                last = Some(e);
                std::thread::sleep(Duration::from_millis(100 + attempt * 50));
            }
        }
    }
    Err(anyhow::anyhow!("couldn't replace {}: {}", to.display(), last.map(|e| e.to_string()).unwrap_or_default()))
}

/// Check the staged files, then swap them in for the live data.
fn activate(s: &AppState, stage: &Path, source: &str) -> anyhow::Result<()> {
    s.setup.set("checking", 0, 0, "Checking the new data");
    let staged = Config { build_dir: stage.to_path_buf(), tiles_dir: stage.to_path_buf(), ..s.cfg.clone() };
    drop(load_data(&staged).map_err(|e| anyhow::anyhow!("the data pack is incomplete or damaged: {e:#}"))?);

    // let in-flight requests finish with the old copy, then close it so its files can be replaced
    if let Some(old) = s.data.write().take() {
        for _ in 0..100 {
            if Arc::strong_count(&old) == 1 {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    std::fs::create_dir_all(&s.cfg.build_dir)?;
    std::fs::create_dir_all(&s.cfg.tiles_dir)?;

    let mut info: DataInfo = std::fs::read(stage.join(MANIFEST)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    info.installed_from = Some(source.to_string());
    info.installed_at = Some(now_iso());
    std::fs::write(stage.join(MANIFEST), serde_json::to_vec_pretty(&info)?)?;

    let mut result = Ok(());
    for name in BUILD_FILES.iter().chain(TILE_FILES).chain([&MANIFEST]) {
        if let Some(dest) = destination(&s.cfg, name) {
            if let Err(e) = move_file(&stage.join(name), &dest) {
                result = Err(e);
                break;
            }
        }
    }
    // whatever happened, serve what is on disk now
    match load_data(&s.cfg) {
        Ok(d) => *s.data.write() = Some(Arc::new(d)),
        Err(e) if result.is_ok() => result = Err(e),
        Err(_) => {}
    }
    let _ = std::fs::remove_dir_all(stage);
    result
}

fn now_iso() -> String {
    // seconds are plenty here; avoids a date crate
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let days = (secs / 86_400) as i64;
    let (y, m, d) = civil_from_days(days);
    let t = secs % 86_400;
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", t / 3600, t / 60 % 60, t % 60)
}

/// Days since 1970-01-01 to (year, month, day), proleptic Gregorian (H. Hinnant's algorithm).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// Where the data pack that is installed is kept, to hand out to desktop apps (/api/datapack).
pub fn pack_path(cfg: &Config) -> PathBuf {
    cfg.cache_dir.parent().unwrap_or(&cfg.cache_dir).join("datapack").join("flightplan-data.zip")
}

/// Keep an installed pack's zip for /api/datapack: moved when it was our own download, copied
/// when it belongs to someone else.
fn keep_pack(cfg: &Config, zip: &Path, take: bool) {
    let dest = pack_path(cfg);
    let r = (|| -> anyhow::Result<()> {
        std::fs::create_dir_all(dest.parent().unwrap_or(Path::new(".")))?;
        let tmp = dest.with_extension("zip.tmp");
        if take && std::fs::rename(zip, &tmp).is_ok() {
        } else {
            std::fs::copy(zip, &tmp)?;
        }
        std::fs::rename(&tmp, &dest)?;
        Ok(())
    })();
    if let Err(e) = r {
        tracing::warn!("couldn't keep the data pack for desktop apps: {e:#}");
    }
}

/// Run the blocking install steps on a worker thread and record the outcome. `keep` is the zip
/// to keep for desktop apps on success (and whether it may be moved).
fn finish(s: Arc<AppState>, source: String, step: impl FnOnce(&AppState) -> anyhow::Result<PathBuf> + Send + 'static, cleanup: Option<PathBuf>, keep: Option<(PathBuf, bool)>) {
    tokio::task::spawn_blocking(move || {
        let r = step(&s).and_then(|stage| activate(&s, &stage, &source));
        if r.is_ok() {
            if let Some((zip, take)) = &keep {
                keep_pack(&s.cfg, zip, *take);
            }
        }
        if let Some(p) = cleanup {
            let _ = std::fs::remove_file(p);
        }
        match r {
            Ok(()) => s.setup.set("done", 0, 0, "Chart data installed"),
            Err(e) => {
                tracing::warn!("data install failed: {e:#}");
                let _ = std::fs::remove_dir_all(staging(&s.cfg));
                s.setup.set("error", 0, 0, format!("{e:#}"));
            }
        }
    });
}

// ---- library API (the desktop app calls these directly instead of the HTTP routes) ----

/// Installed data info (None when no data) and the current install job.
pub fn current(s: &AppState) -> (Option<DataInfo>, Job) {
    (s.data.read().as_ref().map(|d| d.info.clone()), s.setup.job.lock().clone())
}

/// Start downloading and installing a data pack.
pub fn start_download(s: Arc<AppState>, url: String) -> anyhow::Result<()> {
    start_download_with(s, url, None)
}

/// Start downloading and installing a data pack, with a device token for a Flightplan server
/// (the desktop app getting its chart data from its own server).
pub fn start_download_with(s: Arc<AppState>, url: String, token: Option<String>) -> anyhow::Result<()> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        anyhow::bail!("the data pack address must start with https://");
    }
    s.setup.begin("downloading", 0, "Connecting").map_err(|e| anyhow::anyhow!(e.1.to_string()))?;
    let st = s.clone();
    tokio::spawn(async move {
        let tmp = st.cfg.cache_dir.join("pack-download.zip");
        match fetch(&st, &url, &tmp, token.as_deref()).await {
            Ok(()) => {
                let cleanup = tmp.clone();
                let keep = Some((tmp.clone(), true));
                finish(st.clone(), url, move |s| extract(&s.cfg, &s.setup, &tmp), Some(cleanup), keep)
            }
            Err(e) => {
                let _ = tokio::fs::remove_file(&tmp).await;
                st.setup.set("error", 0, 0, format!("{e:#}"));
            }
        }
    });
    Ok(())
}

/// Install from a data-pack zip on disk.
pub fn start_zip(s: Arc<AppState>, zip: PathBuf) -> anyhow::Result<()> {
    s.setup.begin("extracting", 0, "Unpacking chart data").map_err(|e| anyhow::anyhow!(e.1.to_string()))?;
    let name = zip.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "data pack".into());
    let keep = Some((zip.clone(), false));
    finish(s, name, move |s| extract(&s.cfg, &s.setup, &zip), None, keep);
    Ok(())
}

/// Install by copying from a folder (unzipped pack or a repository checkout).
pub fn start_folder(s: Arc<AppState>, dir: PathBuf) -> anyhow::Result<()> {
    let files = find_in_folder(&dir).map_err(|missing| anyhow::anyhow!("{} doesn't contain the chart data (missing {})", dir.display(), missing.join(", ")))?;
    s.setup.begin("copying", 0, "Copying chart data").map_err(|e| anyhow::anyhow!(e.1.to_string()))?;
    finish(s, dir.display().to_string(), move |s| copy_folder(&s.cfg, &s.setup, &files), None, None);
    Ok(())
}

pub fn cancel_job(s: &AppState) {
    s.setup.cancel.store(true, Ordering::SeqCst);
}

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/setup/status", get(status))
        .route("/setup/download", post(download))
        .route("/setup/upload", post(upload))
        .route("/setup/folder", post(folder))
        .route("/setup/cancel", post(cancel))
        .route("/datapack", get(datapack_info))
        .route("/datapack/file", get(datapack_file))
}

/// The chart data this server has, for desktop apps that get theirs from it.
async fn datapack_info(State(s): State<Arc<AppState>>) -> Json<Value> {
    let p = pack_path(&s.cfg);
    let size = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
    let info = s.data.read().as_ref().map(|d| d.info.clone());
    Json(json!({
        "available": size > 0 && info.is_some(),
        "size": size,
        "cycle": info.as_ref().and_then(|i| i.cycle.clone()),
        "expires": info.as_ref().and_then(|i| i.expires.clone()),
        "built": info.as_ref().and_then(|i| i.built.clone()),
    }))
}

/// The data-pack zip itself (resumable).
async fn datapack_file(State(s): State<Arc<AppState>>, req: axum::extract::Request) -> axum::response::Response {
    use axum::response::IntoResponse;
    use tower::ServiceExt;
    match tower_http::services::ServeFile::new(pack_path(&s.cfg)).oneshot(req).await {
        Ok(r) => r.map(Body::new).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn status(State(s): State<Arc<AppState>>) -> Json<Value> {
    let data = s.data.read().clone();
    Json(json!({
        "installed": data.is_some(),
        "info": data.as_ref().map(|d| d.info.clone()),
        "job": s.setup.job.lock().clone(),
        "defaultUrl": s.cfg.default_data_url,
        "dataDir": s.cfg.build_dir.parent().unwrap_or(&s.cfg.build_dir),
        "canPickFolder": s.cfg.host.is_some(),
    }))
}

#[derive(Deserialize)]
struct DownloadReq {
    url: Option<String>,
}

async fn download(State(s): State<Arc<AppState>>, Json(r): Json<DownloadReq>) -> ApiResult<Json<Value>> {
    let url = r.url.filter(|u| !u.trim().is_empty()).unwrap_or_else(|| s.cfg.default_data_url.clone()).trim().to_string();
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err(ApiError::status(StatusCode::BAD_REQUEST, "the data pack address must start with https://"));
    }
    s.setup.begin("downloading", 0, "Connecting")?;
    let st = s.clone();
    tokio::spawn(async move {
        let tmp = st.cfg.cache_dir.join("pack-download.zip");
        match fetch(&st, &url, &tmp, None).await {
            Ok(()) => {
                let keep = Some((tmp.clone(), true));
                finish(st.clone(), url, move |s| extract(&s.cfg, &s.setup, &tmp), Some(st.cfg.cache_dir.join("pack-download.zip")), keep)
            }
            Err(e) => {
                let _ = tokio::fs::remove_file(&tmp).await;
                st.setup.set("error", 0, 0, format!("{e:#}"));
            }
        }
    });
    Ok(Json(json!({"started": true})))
}

async fn fetch(s: &AppState, url: &str, to: &Path, token: Option<&str>) -> anyhow::Result<()> {
    let mut rq = s.setup.http.get(url);
    if let Some(t) = token {
        rq = rq.bearer_auth(t);
    }
    let resp = rq.send().await.map_err(|e| anyhow::anyhow!("couldn't reach {url}: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        if status == reqwest::StatusCode::NOT_FOUND || status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            anyhow::bail!(
                "no data pack at that address (HTTP {status}). If it is attached to a release in a private GitHub repository, download the zip in your browser while signed in and use \"Import a file\" instead."
            );
        }
        anyhow::bail!("download failed: HTTP {status}");
    }
    let total = resp.content_length().unwrap_or(0);
    s.setup.set("downloading", 0, total, "Downloading chart data");
    tokio::fs::create_dir_all(&s.cfg.cache_dir).await?;
    let mut out = tokio::io::BufWriter::new(tokio::fs::File::create(to).await?);
    let mut stream = resp.bytes_stream();
    let mut done = 0u64;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        out.write_all(&chunk).await?;
        done += chunk.len() as u64;
        s.setup.progress(done);
        s.setup.cancelled()?;
    }
    out.flush().await?;
    Ok(())
}

/// Body = the data-pack zip. Streams it to disk, then installs in the background.
async fn upload(State(s): State<Arc<AppState>>, headers: HeaderMap, body: Body) -> ApiResult<Json<Value>> {
    let total = headers.get(header::CONTENT_LENGTH).and_then(|v| v.to_str().ok()).and_then(|v| v.parse().ok()).unwrap_or(0);
    let name = headers.get("x-file-name").and_then(|v| v.to_str().ok()).unwrap_or("uploaded file").to_string();
    s.setup.begin("uploading", total, "Reading the file")?;
    let tmp = s.cfg.cache_dir.join("pack-upload.zip");
    let r: anyhow::Result<()> = async {
        tokio::fs::create_dir_all(&s.cfg.cache_dir).await?;
        let mut out = tokio::io::BufWriter::new(tokio::fs::File::create(&tmp).await?);
        let mut stream = body.into_data_stream();
        let mut done = 0u64;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            out.write_all(&chunk).await?;
            done += chunk.len() as u64;
            s.setup.progress(done);
            s.setup.cancelled()?;
        }
        out.flush().await?;
        Ok(())
    }
    .await;
    if let Err(e) = r {
        let _ = tokio::fs::remove_file(&tmp).await;
        s.setup.set("error", 0, 0, format!("{e:#}"));
        return Err(e.into());
    }
    let cleanup = tmp.clone();
    let keep = Some((tmp.clone(), true));
    finish(s.clone(), name, move |s| extract(&s.cfg, &s.setup, &tmp), Some(cleanup), keep);
    Ok(Json(json!({"started": true})))
}

#[derive(Deserialize)]
struct FolderReq {
    /// omitted in the desktop app: a folder picker opens instead
    path: Option<String>,
}

async fn folder(State(s): State<Arc<AppState>>, Json(r): Json<FolderReq>) -> ApiResult<Json<Value>> {
    let dir = match r.path.filter(|p| !p.trim().is_empty()) {
        Some(p) => PathBuf::from(p.trim()),
        None => {
            let host = s.cfg.host.clone().ok_or_else(|| ApiError::status(StatusCode::BAD_REQUEST, "type the folder's path"))?;
            match tokio::task::spawn_blocking(move || host.pick_folder("Choose a folder with Flightplan chart data")).await? {
                Some(p) => p,
                None => return Ok(Json(json!({"cancelled": true}))),
            }
        }
    };
    let files = find_in_folder(&dir).map_err(|missing| {
        ApiError::status(StatusCode::BAD_REQUEST, format!("{} doesn't contain the chart data (missing {})", dir.display(), missing.join(", ")))
    })?;
    s.setup.begin("copying", 0, "Copying chart data")?;
    finish(s.clone(), dir.display().to_string(), move |s| copy_folder(&s.cfg, &s.setup, &files), None, None);
    Ok(Json(json!({"started": true, "path": dir})))
}

async fn cancel(State(s): State<Arc<AppState>>) -> Json<Value> {
    s.setup.cancel.store(true, Ordering::SeqCst);
    Json(json!({"ok": true}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(20_749), (2026, 10, 23));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
    }

    #[test]
    fn destinations() {
        let cfg = Config::from_root(Path::new("/r"));
        assert_eq!(destination(&cfg, "aero.pmtiles"), Some(PathBuf::from("/r/tiles/aero.pmtiles")));
        assert_eq!(destination(&cfg, "aviation.sqlite"), Some(PathBuf::from("/r/data/build/aviation.sqlite")));
        assert_eq!(destination(&cfg, "evil.sh"), None);
    }
}
