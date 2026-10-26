//! Sign-in for the web app, built to be exposed to the internet: one password for the whole app
//! (Argon2id hash in data/auth.json, mode 0600), session cookies (HttpOnly, SameSite=Strict,
//! Secure over HTTPS; 30 days idle, 90 days at most), throttling per address and overall, and the
//! login screen's greeting name and picture. Every API call and chart tile needs a session; the
//! page itself and the sign-in endpoints don't.
//!
//! The first password needs the one-time setup code the server prints when it starts (also in
//! data/setup-code.txt), so whoever finds a fresh install first can't claim it. Forgotten
//! password: `flightplan-server set-password` on the server. Behind a reverse proxy, set
//! FP_TRUST_PROXY=1 so client addresses (and HTTPS) come from X-Forwarded-For / -Proto.
//! The desktop app calls the library directly and never goes through this.

use crate::{ApiError, ApiResult, AppState};
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use axum::{
    body::Bytes,
    extract::{ConnectInfo, State},
    http::{header, HeaderMap, Method, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    path::PathBuf,
    sync::{Arc, OnceLock},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const COOKIE: &str = "fp_session";
/// a session ends after this long unused
const IDLE_DAYS: u64 = 30;
/// and after this long whatever happens
const MAX_DAYS: u64 = 90;
const MAX_IMAGE: usize = 8 * 1024 * 1024;
pub const MIN_PASSWORD: usize = 10;
/// longer inputs are refused before hashing (Argon2 cost grows with input)
const MAX_PASSWORD: usize = 256;

/// Settings read once from the environment.
struct Env {
    /// take the client address and scheme from the reverse proxy's headers
    trust_proxy: bool,
    /// always mark cookies Secure (when TLS ends somewhere we can't see)
    secure_cookies: bool,
}

fn env() -> &'static Env {
    static E: OnceLock<Env> = OnceLock::new();
    E.get_or_init(|| {
        let on = |k: &str| std::env::var(k).is_ok_and(|v| matches!(v.trim(), "1" | "true" | "yes" | "on"));
        Env { trust_proxy: on("FP_TRUST_PROXY"), secure_cookies: on("FP_SECURE_COOKIES") }
    })
}

#[derive(Default, Clone, Serialize, Deserialize)]
struct Session {
    /// SHA-256 of the cookie value (the token itself is never stored)
    hash: String,
    created: u64,
    last_seen: u64,
}

#[derive(Default, Clone, Serialize, Deserialize)]
struct Stored {
    /// Argon2 PHC string; None until a password has been set
    #[serde(default)]
    password: Option<String>,
    /// shown in the login screen's greeting
    #[serde(default)]
    name: String,
    /// content type of the custom login picture (saved next to auth.json), if any
    #[serde(default)]
    image: Option<String>,
    #[serde(default)]
    sessions: Vec<Session>,
}

/// Failed attempts from one address, for backing off.
struct Failures {
    count: u32,
    until: Instant,
    last: Instant,
}

pub struct Auth {
    path: PathBuf,
    stored: Mutex<Stored>,
    /// auth.json's modification time when last read or written, to notice outside changes
    /// (`flightplan-server set-password` while the server runs)
    mtime: Mutex<Option<SystemTime>>,
    failures: Mutex<HashMap<IpAddr, Failures>>,
    /// recent failures from anywhere: a spread-out guessing attempt slows everyone's tries down
    recent: Mutex<Vec<Instant>>,
    /// one-time code that authorises setting the first password
    setup_code: Mutex<Option<String>>,
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn sha(s: &str) -> String {
    Sha256::digest(s.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

fn random(n: usize) -> anyhow::Result<Vec<u8>> {
    let mut b = vec![0u8; n];
    getrandom::getrandom(&mut b).map_err(|e| anyhow::anyhow!("no randomness: {e}"))?;
    Ok(b)
}

fn token() -> anyhow::Result<String> {
    Ok(random(32)?.iter().map(|x| format!("{x:02x}")).collect())
}

/// "K7QM-2XPA-9RTF": 12 characters with nothing easy to misread (no 0/O, 1/I/L).
fn new_setup_code() -> anyhow::Result<String> {
    const ABC: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";
    let c: String = random(12)?.iter().map(|b| ABC[*b as usize % ABC.len()] as char).collect();
    Ok(format!("{}-{}-{}", &c[0..4], &c[4..8], &c[8..12]))
}

/// Codes as typed: case, spaces and dashes don't matter.
fn norm_code(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_uppercase()).collect()
}

/// Equal without leaking how much of it matched through timing.
fn same(a: &str, b: &str) -> bool {
    let (x, y) = (Sha256::digest(a.as_bytes()), Sha256::digest(b.as_bytes()));
    x.iter().zip(y.iter()).fold(0u8, |acc, (p, q)| acc | (p ^ q)) == 0
}

pub fn hash_password(pw: &str) -> anyhow::Result<String> {
    let salt = SaltString::encode_b64(&random(16)?).map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(Argon2::default().hash_password(pw.as_bytes(), &salt).map_err(|e| anyhow::anyhow!("{e}"))?.to_string())
}

fn verify(pw: &str, phc: &str) -> bool {
    PasswordHash::new(phc).is_ok_and(|h| Argon2::default().verify_password(pw.as_bytes(), &h).is_ok())
}

/// Passwords guessers try first (lower case; compared after lower-casing).
const COMMON: &[&str] = &[
    "password", "password1", "password12", "password123", "password1234", "passw0rd", "123456789", "1234567890", "12345678910", "qwertyuiop", "qwerty123",
    "qwerty1234", "1q2w3e4r5t", "1qaz2wsx3edc", "abcdefghij", "iloveyou12", "letmein123", "welcome123", "administrator", "admin12345", "changeme123",
    "football12", "baseball12", "sunshine12", "princess12", "dragon1234", "monkey1234", "trustno1234", "flightplan", "flightplan1", "flightplan123",
    "cessna172", "cessna1723", "pilot12345", "aviation12", "airplane12", "skyhawk172", "0000000000", "1111111111", "abc1234567", "zaq12wsxcde",
];

/// Why a password won't do, or None when it's fine.
pub fn password_problem(pw: &str, name: &str) -> Option<String> {
    let n = pw.chars().count();
    if n < MIN_PASSWORD {
        return Some(format!("Use at least {MIN_PASSWORD} characters. A few unrelated words make a good one."));
    }
    if n > MAX_PASSWORD {
        return Some(format!("Use at most {MAX_PASSWORD} characters."));
    }
    let lower = pw.to_lowercase();
    if COMMON.contains(&lower.as_str()) {
        return Some("That one is on every list of common passwords. Pick something only you would think of.".into());
    }
    let first = lower.chars().next().unwrap_or(' ');
    if lower.chars().all(|c| c == first) {
        return Some("Not just one character over and over.".into());
    }
    let name = name.trim().to_lowercase();
    if name.chars().count() >= 3 && lower.replace(|c: char| !c.is_alphanumeric(), "").trim_end_matches(|c: char| c.is_ascii_digit()) == name.replace(' ', "") {
        return Some("Not your name.".into());
    }
    None
}

fn check_password(pw: &str, name: &str) -> Result<(), ApiError> {
    match password_problem(pw, name) {
        Some(p) => Err(ApiError::status(StatusCode::BAD_REQUEST, p)),
        None => Ok(()),
    }
}

/// Argon2 takes ~20 MB and a good fraction of a second per check: at most two at a time, so a
/// flood of sign-in attempts can't exhaust memory or CPU.
fn argon_permits() -> &'static tokio::sync::Semaphore {
    static S: OnceLock<tokio::sync::Semaphore> = OnceLock::new();
    S.get_or_init(|| tokio::sync::Semaphore::new(2))
}

async fn argon<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> anyhow::Result<T> {
    let _permit = argon_permits().acquire().await?;
    Ok(tokio::task::spawn_blocking(f).await?)
}

impl Auth {
    pub fn load(path: PathBuf) -> Self {
        let a = Auth {
            path,
            stored: Mutex::new(Stored::default()),
            mtime: Mutex::new(None),
            failures: Mutex::new(HashMap::new()),
            recent: Mutex::new(Vec::new()),
            setup_code: Mutex::new(None),
        };
        drop(a.get());
        a
    }

    fn image_path(&self) -> PathBuf {
        self.path.with_file_name("login-image")
    }

    fn code_path(&self) -> PathBuf {
        self.path.with_file_name("setup-code.txt")
    }

    fn file_mtime(&self) -> Option<SystemTime> {
        std::fs::metadata(&self.path).and_then(|m| m.modified()).ok()
    }

    /// The stored state, re-read first if auth.json changed on disk.
    fn get(&self) -> parking_lot::MutexGuard<'_, Stored> {
        let mut g = self.stored.lock();
        let m = self.file_mtime();
        let mut seen = self.mtime.lock();
        if m != *seen {
            *g = std::fs::read(&self.path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
            *seen = m;
        }
        g
    }

    fn write_private(path: &std::path::Path, bytes: &[u8]) -> anyhow::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, bytes)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
        }
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    fn save(&self, s: &Stored) -> anyhow::Result<()> {
        Self::write_private(&self.path, &serde_json::to_vec_pretty(s)?)?;
        *self.mtime.lock() = self.file_mtime();
        Ok(())
    }

    pub fn configured(&self) -> bool {
        self.get().password.is_some()
    }

    /// Before the first password: the one-time setup code, made once and kept in
    /// data/setup-code.txt until a password is set.
    pub fn setup_code(&self) -> Option<String> {
        if self.configured() {
            return None;
        }
        let mut c = self.setup_code.lock();
        if c.is_none() {
            let from_file = std::fs::read_to_string(self.code_path()).ok().map(|s| s.trim().to_string()).filter(|s| norm_code(s).len() == 12);
            let code = match from_file {
                Some(s) => s,
                None => {
                    let s = new_setup_code().ok()?;
                    let _ = Self::write_private(&self.code_path(), format!("{s}\n").as_bytes());
                    s
                }
            };
            *c = Some(code);
        }
        c.clone()
    }

    /// Log the setup code where whoever runs the server will see it.
    pub fn announce(&self) {
        if let Some(code) = self.setup_code() {
            tracing::warn!(
                "\n\n    Flightplan is locked and has no password yet.\n    Open it in a browser and enter this setup code: {code}\n    (also saved in {})\n",
                self.code_path().display()
            );
        }
    }

    fn code_ok(&self, typed: &str) -> bool {
        self.setup_code().is_some_and(|c| same(&norm_code(&c), &norm_code(typed)))
    }

    /// Set the password (and optionally the name), signing every session out.
    pub fn set_password(&self, pw: &str, name: Option<&str>) -> anyhow::Result<()> {
        let hash = hash_password(pw)?;
        let mut s = self.get();
        s.password = Some(hash);
        if let Some(n) = name {
            s.name = n.trim().chars().filter(|c| !c.is_control()).take(40).collect();
        }
        s.sessions.clear();
        self.save(&s)?;
        drop(s);
        // the setup code is spent
        let _ = std::fs::remove_file(self.code_path());
        *self.setup_code.lock() = None;
        Ok(())
    }

    /// A new session; returns the cookie value.
    fn open_session(&self) -> anyhow::Result<String> {
        let t = token()?;
        let mut s = self.get();
        let t0 = now();
        s.sessions.retain(|x| live(x, t0));
        s.sessions.push(Session { hash: sha(&t), created: t0, last_seen: t0 });
        // a handful of devices is plenty; drop the oldest
        while s.sessions.len() > 50 {
            s.sessions.remove(0);
        }
        self.save(&s)?;
        Ok(t)
    }

    /// Whether the request carries a live session (and note that it was used).
    fn valid(&self, headers: &HeaderMap) -> bool {
        let Some(t) = cookie(headers) else { return false };
        let h = sha(&t);
        let mut s = self.get();
        let t0 = now();
        match s.sessions.iter_mut().find(|x| same(&x.hash, &h)) {
            Some(x) if live(x, t0) => {
                // sliding expiry; written to disk with the next sign-in / sign-out
                x.last_seen = t0;
                true
            }
            _ => false,
        }
    }

    fn close_session(&self, headers: &HeaderMap) {
        if let Some(t) = cookie(headers) {
            let h = sha(&t);
            let mut s = self.get();
            s.sessions.retain(|x| x.hash != h);
            let _ = self.save(&s);
        }
    }

    fn close_all(&self) -> anyhow::Result<()> {
        let mut s = self.get();
        s.sessions.clear();
        self.save(&s)
    }

    /// Seconds this address must wait before trying again (0 = go ahead).
    fn wait(&self, ip: IpAddr) -> u64 {
        let f = self.failures.lock();
        f.get(&ip).map(|x| x.until.saturating_duration_since(Instant::now()).as_secs()).unwrap_or(0)
    }

    fn failed(&self, ip: IpAddr) {
        let t = Instant::now();
        let mut f = self.failures.lock();
        // forget addresses that went quiet, so a flood of addresses can't grow this forever
        if f.len() > 10_000 {
            f.retain(|_, x| t.duration_since(x.last) < Duration::from_secs(3600));
        }
        let e = f.entry(ip).or_insert(Failures { count: 0, until: t, last: t });
        if t.duration_since(e.last) > Duration::from_secs(24 * 3600) {
            e.count = 0;
        }
        e.count += 1;
        e.last = t;
        // five tries free, then 30 s doubling up to an hour
        if e.count >= 5 {
            let secs = (30u64 << (e.count - 5).min(7)).min(3600);
            e.until = t + Duration::from_secs(secs);
        }
        drop(f);
        let mut r = self.recent.lock();
        r.retain(|x| t.duration_since(*x) < Duration::from_secs(900));
        r.push(t);
    }

    /// Extra delay for every attempt while many are failing overall (a guesser using many
    /// addresses).
    fn global_delay(&self) -> Duration {
        let t = Instant::now();
        let mut r = self.recent.lock();
        r.retain(|x| t.duration_since(*x) < Duration::from_secs(900));
        match r.len() {
            0..=20 => Duration::ZERO,
            21..=100 => Duration::from_secs(2),
            _ => Duration::from_secs(5),
        }
    }

    fn succeeded(&self, ip: IpAddr) {
        self.failures.lock().remove(&ip);
    }

    /// Wait out the back-off for this address (refusing while it's locked out) and the overall
    /// slow-down.
    async fn gate(&self, ip: IpAddr) -> Result<(), ApiError> {
        let wait = self.wait(ip);
        if wait > 0 {
            return Err(ApiError::status(StatusCode::TOO_MANY_REQUESTS, format!("Too many tries. Wait {} and try again.", human_wait(wait))));
        }
        tokio::time::sleep(self.global_delay()).await;
        Ok(())
    }
}

fn live(x: &Session, t: u64) -> bool {
    t.saturating_sub(x.last_seen) < IDLE_DAYS * 86400 && t.saturating_sub(x.created) < MAX_DAYS * 86400
}

fn human_wait(s: u64) -> String {
    if s < 90 { format!("{s} s") } else { format!("{} min", s.div_ceil(60)) }
}

fn cookie(headers: &HeaderMap) -> Option<String> {
    headers.get_all(header::COOKIE).iter().filter_map(|v| v.to_str().ok()).flat_map(|v| v.split(';')).find_map(|kv| {
        let (k, v) = kv.trim().split_once('=')?;
        (k == COOKIE && v.len() == 64 && v.bytes().all(|b| b.is_ascii_hexdigit())).then(|| v.to_string())
    })
}

fn set_cookie(value: &str, max_age: u64, https: bool) -> (header::HeaderName, String) {
    let secure = if https || env().secure_cookies { "; Secure" } else { "" };
    (header::SET_COOKIE, format!("{COOKIE}={value}; Path=/; HttpOnly; SameSite=Strict; Max-Age={max_age}{secure}"))
}

fn session_cookie(t: &str, c: &Client) -> (header::HeaderName, String) {
    set_cookie(t, IDLE_DAYS * 86400, c.https)
}

/// Whether sign-in applies: always for the standalone server, never inside the desktop app.
fn enforced(s: &AppState) -> bool {
    s.cfg.host.is_none()
}

/// What anyone may reach: the page and its assets, health, the sign-in endpoints, and viewing
/// (not changing) the login picture.
fn public(method: &Method, path: &str) -> bool {
    if !(path.starts_with("/api/") || path.starts_with("/tiles/")) {
        return true;
    }
    let read = matches!(*method, Method::GET | Method::HEAD);
    matches!(path, "/api/health" | "/api/auth/status" | "/api/auth/login" | "/api/auth/setup" | "/api/auth/setup/check") || (read && path == "/api/auth/image")
}

/// Middleware: everything but the public paths needs a session.
pub async fn require(State(s): State<Arc<AppState>>, req: axum::extract::Request, next: axum::middleware::Next) -> Response {
    if !enforced(&s) || public(req.method(), req.uri().path()) || s.auth.valid(req.headers()) {
        return next.run(req).await;
    }
    (StatusCode::UNAUTHORIZED, Json(json!({"error": "sign in first", "signIn": true}))).into_response()
}

/// Who is asking: their address, and whether they came over HTTPS. Behind a trusted reverse
/// proxy (FP_TRUST_PROXY=1) both come from its X-Forwarded-For / X-Forwarded-Proto headers.
pub struct Client {
    pub ip: IpAddr,
    pub https: bool,
}

/// The address the proxy saw: the last X-Forwarded-For entry (the one it appended), or
/// X-Real-IP.
fn forwarded_ip(h: &HeaderMap) -> Option<IpAddr> {
    let xff = h.get_all("x-forwarded-for").iter().filter_map(|v| v.to_str().ok()).flat_map(|v| v.split(',')).map(str::trim).filter(|v| !v.is_empty()).last().map(String::from);
    xff.or_else(|| h.get("x-real-ip").and_then(|v| v.to_str().ok()).map(|s| s.trim().to_string())).and_then(|s| s.parse().ok())
}

/// Whether the request came over HTTPS (as the trusted proxy says).
pub fn is_https(h: &HeaderMap) -> bool {
    env().trust_proxy && h.get("x-forwarded-proto").and_then(|v| v.to_str().ok()).is_some_and(|p| p.trim().eq_ignore_ascii_case("https"))
}

pub fn client_of(h: &HeaderMap, peer: Option<IpAddr>) -> Client {
    let unknown = IpAddr::from([255, 255, 255, 255]);
    let ip = if env().trust_proxy { forwarded_ip(h).or(peer) } else { peer };
    Client { ip: ip.unwrap_or(unknown), https: is_https(h) }
}

impl<S: Send + Sync> axum::extract::FromRequestParts<S> for Client {
    type Rejection = std::convert::Infallible;
    async fn from_request_parts(parts: &mut axum::http::request::Parts, _: &S) -> Result<Self, Self::Rejection> {
        let peer = parts.extensions.get::<ConnectInfo<SocketAddr>>().map(|c| c.0.ip());
        Ok(client_of(&parts.headers, peer))
    }
}

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/auth/status", get(status))
        .route("/auth/login", post(login))
        .route("/auth/setup/check", post(setup_check))
        .route("/auth/setup", post(setup))
        .route("/auth/logout", post(logout))
        .route("/auth/logout-all", post(logout_all))
        .route("/auth/password", post(change_password))
        .route("/auth/name", post(set_name))
        .route("/auth/image", get(image).put(put_image).delete(delete_image).layer(axum::extract::DefaultBodyLimit::max(MAX_IMAGE + 64 * 1024)))
}

async fn status(State(s): State<Arc<AppState>>, headers: HeaderMap) -> Json<Value> {
    let st = s.auth.get().clone();
    let configured = st.password.is_some();
    Json(json!({
        "enforced": enforced(&s),
        "configured": configured,
        "signedIn": !enforced(&s) || (configured && s.auth.valid(&headers)),
        "name": st.name,
        "hasImage": st.image.is_some(),
        "minPassword": MIN_PASSWORD,
    }))
}

#[derive(Deserialize)]
struct LoginReq {
    password: String,
}

async fn login(State(s): State<Arc<AppState>>, c: Client, Json(r): Json<LoginReq>) -> ApiResult<Response> {
    s.auth.gate(c.ip).await?;
    let phc = s.auth.get().password.clone();
    let Some(phc) = phc else { return Err(ApiError::status(StatusCode::CONFLICT, "No password has been set yet.")) };
    let pw = r.password;
    let ok = pw.chars().count() <= MAX_PASSWORD && argon(move || verify(&pw, &phc)).await.unwrap_or(false);
    if !ok {
        s.auth.failed(c.ip);
        tokio::time::sleep(Duration::from_millis(400)).await;
        return Err(ApiError::status(StatusCode::UNAUTHORIZED, "That password isn't right."));
    }
    s.auth.succeeded(c.ip);
    let t = s.auth.open_session()?;
    Ok(([session_cookie(&t, &c)], Json(json!({"ok": true}))).into_response())
}

#[derive(Deserialize)]
struct CodeReq {
    code: String,
}

/// Check the setup code before the walkthrough asks for a password.
async fn setup_check(State(s): State<Arc<AppState>>, c: Client, Json(r): Json<CodeReq>) -> ApiResult<Json<Value>> {
    if s.auth.configured() {
        return Err(ApiError::status(StatusCode::CONFLICT, "A password is already set. Sign in instead."));
    }
    s.auth.gate(c.ip).await?;
    if !s.auth.code_ok(&r.code) {
        s.auth.failed(c.ip);
        tokio::time::sleep(Duration::from_millis(400)).await;
        return Err(ApiError::status(StatusCode::UNAUTHORIZED, "That isn't the setup code. Check the server's output or data/setup-code.txt."));
    }
    Ok(Json(json!({"ok": true})))
}

#[derive(Deserialize)]
struct SetupReq {
    code: String,
    password: String,
    #[serde(default)]
    name: String,
}

/// The first password, with the setup code.
async fn setup(State(s): State<Arc<AppState>>, c: Client, Json(r): Json<SetupReq>) -> ApiResult<Response> {
    if s.auth.configured() {
        return Err(ApiError::status(StatusCode::CONFLICT, "A password is already set. Sign in instead."));
    }
    s.auth.gate(c.ip).await?;
    if !s.auth.code_ok(&r.code) {
        s.auth.failed(c.ip);
        return Err(ApiError::status(StatusCode::UNAUTHORIZED, "That isn't the setup code."));
    }
    check_password(&r.password, &r.name)?;
    let st = s.clone();
    let (pw, name) = (r.password, r.name);
    argon(move || st.auth.set_password(&pw, Some(&name))).await??;
    s.auth.succeeded(c.ip);
    let t = s.auth.open_session()?;
    Ok(([session_cookie(&t, &c)], Json(json!({"ok": true}))).into_response())
}

async fn logout(State(s): State<Arc<AppState>>, c: Client, headers: HeaderMap) -> Response {
    s.auth.close_session(&headers);
    ([set_cookie("", 0, c.https)], Json(json!({"ok": true}))).into_response()
}

/// Sign every device out, this one included.
async fn logout_all(State(s): State<Arc<AppState>>, c: Client) -> ApiResult<Response> {
    s.auth.close_all()?;
    Ok(([set_cookie("", 0, c.https)], Json(json!({"ok": true}))).into_response())
}

#[derive(Deserialize)]
struct PasswordReq {
    current: String,
    new: String,
}

/// Change the password; every other device is signed out.
async fn change_password(State(s): State<Arc<AppState>>, c: Client, Json(r): Json<PasswordReq>) -> ApiResult<Response> {
    s.auth.gate(c.ip).await?;
    let name = s.auth.get().name.clone();
    check_password(&r.new, &name)?;
    let phc = s.auth.get().password.clone().unwrap_or_default();
    let cur = r.current;
    if cur.chars().count() > MAX_PASSWORD || !argon(move || verify(&cur, &phc)).await.unwrap_or(false) {
        s.auth.failed(c.ip);
        return Err(ApiError::status(StatusCode::UNAUTHORIZED, "The current password isn't right."));
    }
    let st = s.clone();
    let new = r.new;
    argon(move || st.auth.set_password(&new, None)).await??;
    let t = s.auth.open_session()?;
    Ok(([session_cookie(&t, &c)], Json(json!({"ok": true}))).into_response())
}

#[derive(Deserialize)]
struct NameReq {
    name: String,
}

async fn set_name(State(s): State<Arc<AppState>>, Json(r): Json<NameReq>) -> ApiResult<Json<Value>> {
    let mut st = s.auth.get();
    st.name = r.name.trim().chars().filter(|c| !c.is_control()).take(40).collect();
    s.auth.save(&st)?;
    Ok(Json(json!({"ok": true})))
}

/// Image types accepted, by their first bytes (the declared type alone isn't trusted).
fn sniff(b: &[u8]) -> Option<&'static str> {
    if b.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if b.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if b.len() > 12 && &b[0..4] == b"RIFF" && &b[8..12] == b"WEBP" {
        Some("image/webp")
    } else if b.starts_with(b"GIF87a") || b.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if b.len() > 12 && &b[4..8] == b"ftyp" && (&b[8..12] == b"avif" || &b[8..12] == b"avis") {
        Some("image/avif")
    } else {
        None
    }
}

/// The custom login picture (public: the login screen shows it).
async fn image(State(s): State<Arc<AppState>>) -> Response {
    let ct = s.auth.get().image.clone();
    match (ct, std::fs::read(s.auth.image_path())) {
        (Some(ct), Ok(b)) => ([(header::CONTENT_TYPE, ct), (header::CACHE_CONTROL, "no-cache".to_string())], b).into_response(),
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn put_image(State(s): State<Arc<AppState>>, body: Bytes) -> ApiResult<Json<Value>> {
    if body.len() > MAX_IMAGE {
        return Err(ApiError::status(StatusCode::PAYLOAD_TOO_LARGE, "That picture is over 8 MB."));
    }
    let Some(ct) = sniff(&body) else { return Err(ApiError::status(StatusCode::UNSUPPORTED_MEDIA_TYPE, "Use a JPEG, PNG, WebP, GIF or AVIF picture.")) };
    std::fs::write(s.auth.image_path(), &body)?;
    let mut st = s.auth.get();
    st.image = Some(ct.to_string());
    s.auth.save(&st)?;
    Ok(Json(json!({"ok": true})))
}

async fn delete_image(State(s): State<Arc<AppState>>) -> ApiResult<Json<Value>> {
    let _ = std::fs::remove_file(s.auth.image_path());
    let mut st = s.auth.get();
    st.image = None;
    s.auth.save(&st)?;
    Ok(Json(json!({"ok": true})))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> PathBuf {
        std::env::temp_dir().join(format!("fp-auth-{}", token().unwrap()))
    }

    #[test]
    fn hashes_verify() {
        let h = hash_password("correct horse").unwrap();
        assert!(h.starts_with("$argon2id"));
        assert!(verify("correct horse", &h));
        assert!(!verify("wrong horse", &h));
    }

    #[test]
    fn password_rules() {
        assert!(password_problem("short", "").is_some());
        assert!(password_problem("Password123", "").is_some(), "common");
        assert!(password_problem("aaaaaaaaaaaa", "").is_some());
        assert!(password_problem("Justus2026", "Justus").is_some(), "name plus digits");
        assert!(password_problem("tailwind over the ridge", "Justus").is_none());
    }

    #[test]
    fn setup_code_flow() {
        let dir = tmp();
        let a = Auth::load(dir.join("auth.json"));
        let code = a.setup_code().unwrap();
        assert_eq!(norm_code(&code).len(), 12);
        assert!(!code.contains('0') && !code.contains('O') && !code.contains('1'));
        assert!(a.code_ok(&code.to_lowercase().replace('-', " ")), "case and separators don't matter");
        assert!(!a.code_ok("AAAA-BBBB-CCCC"));
        // the same code after a restart, until a password is set
        assert_eq!(Auth::load(dir.join("auth.json")).setup_code(), Some(code.clone()));
        a.set_password("tailwind over the ridge", Some("Justus")).unwrap();
        assert!(a.setup_code().is_none() && !dir.join("setup-code.txt").exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn sessions_and_paths() {
        let dir = tmp();
        let a = Auth::load(dir.join("auth.json"));
        a.set_password("abcdefghij", Some("Justus")).unwrap();
        let t = a.open_session().unwrap();
        let mut h = HeaderMap::new();
        h.insert(header::COOKIE, format!("other=1; {COOKIE}={t}").parse().unwrap());
        assert!(a.valid(&h));
        // a new password signs everyone out
        a.set_password("ijklmnopqr", None).unwrap();
        assert!(!a.valid(&h));
        // another process (the set-password command) changing the file is picked up
        let t2 = a.open_session().unwrap();
        let mut h2 = HeaderMap::new();
        h2.insert(header::COOKIE, format!("{COOKIE}={t2}").parse().unwrap());
        assert!(a.valid(&h2));
        std::thread::sleep(Duration::from_millis(20));
        Auth::load(dir.join("auth.json")).set_password("qrstuvwxyz", None).unwrap();
        assert!(!a.valid(&h2), "sessions from before the outside reset are gone");
        // stored on disk without the token
        let raw = std::fs::read_to_string(dir.join("auth.json")).unwrap();
        assert!(!raw.contains(&t2) && raw.contains("Justus"));
        // expiry: idle and absolute
        let t0 = now();
        assert!(live(&Session { hash: String::new(), created: t0 - 86400, last_seen: t0 }, t0));
        assert!(!live(&Session { hash: String::new(), created: t0 - 40 * 86400, last_seen: t0 - 31 * 86400 }, t0));
        assert!(!live(&Session { hash: String::new(), created: t0 - 91 * 86400, last_seen: t0 }, t0));
        let _ = std::fs::remove_dir_all(dir);
        let (get, put) = (Method::GET, Method::PUT);
        assert!(public(&get, "/") && public(&get, "/assets/x.js") && public(&Method::POST, "/api/auth/login") && public(&get, "/api/auth/image"));
        assert!(!public(&get, "/api/airport/KSDL") && !public(&get, "/tiles/aero.pmtiles") && !public(&Method::POST, "/api/auth/logout"));
        assert!(!public(&put, "/api/auth/image") && !public(&Method::DELETE, "/api/auth/image"), "changing the picture needs a session");
    }

    #[test]
    fn backs_off_after_failures() {
        let a = Auth::load(tmp().join("auth.json"));
        let ip: IpAddr = "10.0.0.9".parse().unwrap();
        for _ in 0..4 {
            a.failed(ip);
        }
        assert_eq!(a.wait(ip), 0);
        a.failed(ip);
        assert!(a.wait(ip) >= 29);
        a.succeeded(ip);
        assert_eq!(a.wait(ip), 0);
        for _ in 0..25 {
            a.failed("10.0.0.10".parse().unwrap());
        }
        assert!(a.global_delay() >= Duration::from_secs(2));
    }

    #[test]
    fn proxy_headers_only_when_trusted() {
        let mut h = HeaderMap::new();
        h.insert("x-forwarded-for", "6.6.6.6, 203.0.113.7".parse().unwrap());
        h.insert("x-forwarded-proto", "https".parse().unwrap());
        assert_eq!(forwarded_ip(&h), Some("203.0.113.7".parse().unwrap()), "the entry the proxy appended");
        // without FP_TRUST_PROXY the socket address is used and the headers are ignored
        let c = client_of(&h, Some("127.0.0.1".parse().unwrap()));
        assert_eq!(c.ip, "127.0.0.1".parse::<IpAddr>().unwrap());
        assert!(!c.https);
        assert!(sniff(b"\x89PNG\r\n\x1a\n....").is_some() && sniff(b"<svg/>").is_none());
        let mut bad = HeaderMap::new();
        bad.insert(header::COOKIE, format!("{COOKIE}=../../etc").parse().unwrap());
        assert!(cookie(&bad).is_none());
    }
}
