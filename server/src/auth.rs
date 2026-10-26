//! Sign-in for the web app, built to be exposed to the internet. Users live in data/auth.json
//! (mode 0600) with Argon2id password hashes. By default it's single-user: the owner is
//! preselected and only the password is asked for. In multi-user mode people sign in with their
//! name and password and each gets their own data folder (plans, aircraft, minimums).
//!
//! Browsers get session cookies (HttpOnly, SameSite=Strict, Secure over HTTPS; 30 days idle, 90
//! days at most). The desktop app pairs with a short code shown in the web app and then uses a
//! device token (`Authorization: Bearer fpd_…`) to sync plans. Tokens are stored only as hashes.
//! Sign-in attempts are throttled per address and overall, and Argon2 runs at most twice at once.
//!
//! The first password needs the one-time setup code the server prints when it starts (also in
//! data/setup-code.txt), so whoever finds a fresh install first can't claim it. Forgotten
//! password: `flightplan-server set-password [name]` on the server. Behind a reverse proxy, set
//! FP_TRUST_PROXY=1 so client addresses (and HTTPS) come from X-Forwarded-For / -Proto.
//! The desktop app's embedded server never goes through this.

use crate::{ApiError, ApiResult, AppState};
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use axum::{
    body::Bytes,
    extract::{ConnectInfo, Path, State},
    http::{header, HeaderMap, Method, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
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
/// the first user; their files stay where single-user installs always kept them
pub const OWNER: &str = "owner";
/// a session ends after this long unused
const IDLE_DAYS: u64 = 30;
/// and after this long whatever happens
const MAX_DAYS: u64 = 90;
/// a desktop app that hasn't synced for this long has to pair again
const DEVICE_IDLE_DAYS: u64 = 180;
const PAIRING_MINUTES: u64 = 10;
const MAX_IMAGE: usize = 8 * 1024 * 1024;
pub const MIN_PASSWORD: usize = 10;
/// longer inputs are refused before hashing (Argon2 cost grows with input)
const MAX_PASSWORD: usize = 256;
const MAX_USERS: usize = 25;

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

#[derive(Clone, Serialize, Deserialize)]
struct User {
    /// "owner", or random hex for people added later (also their data folder's name)
    id: String,
    name: String,
    /// Argon2 PHC string
    password: String,
    #[serde(default)]
    owner: bool,
    #[serde(default)]
    created: u64,
}

#[derive(Default, Clone, Serialize, Deserialize)]
struct Session {
    /// SHA-256 of the cookie value (the token itself is never stored)
    hash: String,
    #[serde(default)]
    user: String,
    created: u64,
    last_seen: u64,
}

#[derive(Clone, Serialize, Deserialize)]
struct Device {
    /// SHA-256 of the token
    hash: String,
    user: String,
    /// what the desktop app called itself ("Justus's MacBook")
    name: String,
    created: u64,
    last_seen: u64,
}

#[derive(Default, Clone, Serialize, Deserialize)]
struct Stored {
    /// from the single-password version; moved into `users` on load
    #[serde(default, skip_serializing_if = "Option::is_none")]
    password: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    name: String,
    #[serde(default)]
    users: Vec<User>,
    /// several people sign in with name + password (default: just the owner, password only)
    #[serde(default)]
    multi: bool,
    /// content type of the custom login picture (saved next to auth.json), if any
    #[serde(default)]
    image: Option<String>,
    #[serde(default)]
    sessions: Vec<Session>,
    #[serde(default)]
    devices: Vec<Device>,
}

impl Stored {
    /// Older auth.json files had one password and no users.
    fn migrate(&mut self) {
        if self.users.is_empty() {
            if let Some(p) = self.password.take() {
                self.users.push(User { id: OWNER.into(), name: std::mem::take(&mut self.name), password: p, owner: true, created: now() });
            }
        }
        self.password = None;
        self.name.clear();
        for s in self.sessions.iter_mut().filter(|s| s.user.is_empty()) {
            s.user = OWNER.into();
        }
    }

    fn owner(&self) -> Option<&User> {
        self.users.iter().find(|u| u.owner)
    }

    fn user(&self, id: &str) -> Option<&User> {
        self.users.iter().find(|u| u.id == id)
    }

    fn by_name(&self, name: &str) -> Option<&User> {
        let n = name.trim().to_lowercase();
        self.users.iter().find(|u| u.name.trim().to_lowercase() == n)
    }

    /// Sign one user out everywhere: browsers and desktop apps.
    fn sign_out(&mut self, user: &str) {
        self.sessions.retain(|s| s.user != user);
        self.devices.retain(|d| d.user != user);
    }
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
    /// desktop pairing codes: (normalised code, user, expiry)
    pairing: Mutex<Vec<(String, String, Instant)>>,
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

fn hex(n: usize) -> anyhow::Result<String> {
    Ok(random(n)?.iter().map(|x| format!("{x:02x}")).collect())
}

fn token() -> anyhow::Result<String> {
    hex(32)
}

/// "K7QM-2XPA-9RTF": 12 characters with nothing easy to misread (no 0/O, 1/I/L).
fn new_code() -> anyhow::Result<String> {
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

/// A hash to check against when the name is unknown, so a wrong name takes as long as a wrong
/// password and names can't be probed by timing.
fn dummy_hash() -> &'static str {
    static D: OnceLock<String> = OnceLock::new();
    D.get_or_init(|| hash_password("not a real password").unwrap_or_default())
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

/// A display name: trimmed, no control characters, at most 40 characters.
fn clean_name(n: &str) -> String {
    n.trim().chars().filter(|c| !c.is_control()).take(40).collect()
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
            pairing: Mutex::new(Vec::new()),
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
            g.migrate();
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
        !self.get().users.is_empty()
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
                    let s = new_code().ok()?;
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

    /// Set a user's password (by name; empty = the owner), signing them out everywhere. With no
    /// users yet this creates the owner. Used by setup and `flightplan-server set-password`.
    pub fn set_password(&self, who: &str, pw: &str, rename: Option<&str>) -> anyhow::Result<()> {
        let hash = hash_password(pw)?;
        let mut s = self.get();
        if s.users.is_empty() {
            s.users.push(User { id: OWNER.into(), name: String::new(), password: String::new(), owner: true, created: now() });
        }
        let id = if who.trim().is_empty() { s.owner().map(|u| u.id.clone()) } else { s.by_name(who).map(|u| u.id.clone()) };
        let Some(id) = id else { anyhow::bail!("no user called {who}") };
        if let Some(u) = s.users.iter_mut().find(|u| u.id == id) {
            u.password = hash;
            if let Some(n) = rename {
                u.name = clean_name(n);
            }
        }
        s.sign_out(&id);
        self.save(&s)?;
        drop(s);
        // the setup code is spent
        let _ = std::fs::remove_file(self.code_path());
        *self.setup_code.lock() = None;
        Ok(())
    }

    /// A new browser session for a user; returns the cookie value.
    fn open_session(&self, user: &str) -> anyhow::Result<String> {
        let t = token()?;
        let mut s = self.get();
        let t0 = now();
        s.sessions.retain(|x| live(x.created, x.last_seen, t0, IDLE_DAYS));
        s.sessions.push(Session { hash: sha(&t), user: user.into(), created: t0, last_seen: t0 });
        // a few dozen browsers is plenty; drop the oldest
        while s.sessions.len() > 200 {
            s.sessions.remove(0);
        }
        self.save(&s)?;
        Ok(t)
    }

    /// The user a request is signed in as: a browser session cookie, or a desktop app's device
    /// token. Notes the use (sliding expiry, written with the next change).
    fn user_of(&self, headers: &HeaderMap) -> Option<String> {
        let t0 = now();
        let mut s = self.get();
        if let Some(t) = bearer(headers) {
            let h = sha(&t);
            let d = s.devices.iter_mut().find(|d| same(&d.hash, &h)).filter(|d| live(d.created, d.last_seen, t0, DEVICE_IDLE_DAYS))?;
            d.last_seen = t0;
            let u = d.user.clone();
            return s.user(&u).map(|u| u.id.clone());
        }
        let t = cookie(headers)?;
        let h = sha(&t);
        let x = s.sessions.iter_mut().find(|x| same(&x.hash, &h)).filter(|x| live(x.created, x.last_seen, t0, IDLE_DAYS))?;
        x.last_seen = t0;
        let u = x.user.clone();
        s.user(&u).map(|u| u.id.clone())
    }

    fn close_session(&self, headers: &HeaderMap) {
        if let Some(t) = cookie(headers) {
            let h = sha(&t);
            let mut s = self.get();
            s.sessions.retain(|x| x.hash != h);
            let _ = self.save(&s);
        }
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

    /// A pairing code for a user's desktop app (replaces their previous one).
    fn new_pairing(&self, user: &str) -> anyhow::Result<String> {
        let code = new_code()?;
        let mut p = self.pairing.lock();
        let t = Instant::now();
        p.retain(|x| x.2 > t && x.1 != user);
        p.push((norm_code(&code), user.into(), t + Duration::from_secs(PAIRING_MINUTES * 60)));
        Ok(code)
    }

    /// Spend a pairing code: the user it was made for.
    fn take_pairing(&self, typed: &str) -> Option<String> {
        let n = norm_code(typed);
        let mut p = self.pairing.lock();
        let t = Instant::now();
        p.retain(|x| x.2 > t);
        let i = p.iter().position(|x| same(&x.0, &n))?;
        Some(p.remove(i).1)
    }
}

fn live(created: u64, last_seen: u64, t: u64, idle_days: u64) -> bool {
    t.saturating_sub(last_seen) < idle_days * 86400 && (idle_days != IDLE_DAYS || t.saturating_sub(created) < MAX_DAYS * 86400)
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

/// A desktop app's device token: "Authorization: Bearer fpd_<64 hex>".
fn bearer(headers: &HeaderMap) -> Option<String> {
    let v = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let t = v.strip_prefix("Bearer ")?.trim().strip_prefix("fpd_")?;
    (t.len() == 64 && t.bytes().all(|b| b.is_ascii_hexdigit())).then(|| t.to_string())
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
    matches!(path, "/api/health" | "/api/auth/status" | "/api/auth/login" | "/api/auth/setup" | "/api/auth/setup/check" | "/api/auth/pair") || (read && path == "/api/auth/image")
}

/// Who a request is signed in as (set by `require`; the owner when sign-in doesn't apply).
#[derive(Clone)]
pub struct CurrentUser(pub String);

impl<S: Send + Sync> axum::extract::FromRequestParts<S> for CurrentUser {
    type Rejection = std::convert::Infallible;
    async fn from_request_parts(parts: &mut axum::http::request::Parts, _: &S) -> Result<Self, Self::Rejection> {
        Ok(parts.extensions.get::<CurrentUser>().cloned().unwrap_or(CurrentUser(OWNER.into())))
    }
}

/// Middleware: everything but the public paths needs a signed-in user.
pub async fn require(State(s): State<Arc<AppState>>, mut req: axum::extract::Request, next: axum::middleware::Next) -> Response {
    if !enforced(&s) {
        return next.run(req).await;
    }
    if let Some(u) = s.auth.user_of(req.headers()) {
        req.extensions_mut().insert(CurrentUser(u));
        return next.run(req).await;
    }
    if public(req.method(), req.uri().path()) {
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

/// Where a user's own documents (plans, aircraft, minimums, prefs) live.
pub fn user_dir(s: &AppState, user: &str) -> PathBuf {
    if user == OWNER || !user.bytes().all(|b| b.is_ascii_hexdigit()) || user.is_empty() {
        s.cfg.user_dir.clone()
    } else {
        s.cfg.user_dir.join("users").join(user)
    }
}

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/auth/status", get(status))
        .route("/auth/login", post(login))
        .route("/auth/setup/check", post(setup_check))
        .route("/auth/setup", post(setup))
        .route("/auth/me", get(me))
        .route("/auth/logout", post(logout))
        .route("/auth/logout-all", post(logout_all))
        .route("/auth/password", post(change_password))
        .route("/auth/name", post(set_name))
        .route("/auth/mode", post(set_mode))
        .route("/auth/users", get(list_users).post(add_user))
        .route("/auth/users/{id}", delete(remove_user))
        .route("/auth/pairing", post(pairing))
        .route("/auth/pair", post(pair))
        .route("/auth/devices", get(list_devices))
        .route("/auth/devices/{id}", delete(remove_device))
        .route("/auth/device", delete(forget_this_device))
        .route("/auth/image", get(image).put(put_image).delete(delete_image).layer(axum::extract::DefaultBodyLimit::max(MAX_IMAGE + 64 * 1024)))
}

async fn status(State(s): State<Arc<AppState>>, headers: HeaderMap) -> Json<Value> {
    let st = s.auth.get().clone();
    let configured = !st.users.is_empty();
    // single-user: the greeting names the owner; multi-user: people type their name
    let name = if st.multi { String::new() } else { st.owner().map(|u| u.name.clone()).unwrap_or_default() };
    Json(json!({
        "enforced": enforced(&s),
        "configured": configured,
        "signedIn": !enforced(&s) || (configured && s.auth.user_of(&headers).is_some()),
        "multi": st.multi,
        "name": name,
        "hasImage": st.image.is_some(),
        "minPassword": MIN_PASSWORD,
    }))
}

async fn me(State(s): State<Arc<AppState>>, CurrentUser(u): CurrentUser) -> ApiResult<Json<Value>> {
    let st = s.auth.get();
    let user = st.user(&u).ok_or_else(|| ApiError::status(StatusCode::UNAUTHORIZED, "sign in first"))?;
    Ok(Json(json!({"id": user.id, "name": user.name, "owner": user.owner, "multi": st.multi})))
}

fn owner_only(s: &AppState, u: &str) -> Result<(), ApiError> {
    if s.auth.get().user(u).is_some_and(|x| x.owner) || !enforced(s) {
        Ok(())
    } else {
        Err(ApiError::status(StatusCode::FORBIDDEN, "Only the owner can change that."))
    }
}

#[derive(Deserialize)]
struct LoginReq {
    #[serde(default)]
    name: String,
    password: String,
}

async fn login(State(s): State<Arc<AppState>>, c: Client, Json(r): Json<LoginReq>) -> ApiResult<Response> {
    s.auth.gate(c.ip).await?;
    let (target, phc, multi) = {
        let st = s.auth.get();
        if st.users.is_empty() {
            return Err(ApiError::status(StatusCode::CONFLICT, "No password has been set yet."));
        }
        let u = if st.multi { st.by_name(&r.name) } else { st.owner() };
        (u.map(|u| u.id.clone()), u.map(|u| u.password.clone()), st.multi)
    };
    let pw = r.password;
    let known = phc.is_some();
    let phc = phc.unwrap_or_else(|| dummy_hash().to_string());
    let ok = pw.chars().count() <= MAX_PASSWORD && argon(move || verify(&pw, &phc)).await.unwrap_or(false) && known;
    let Some(user) = target.filter(|_| ok) else {
        s.auth.failed(c.ip);
        tokio::time::sleep(Duration::from_millis(400)).await;
        let msg = if multi { "That name and password don't match." } else { "That password isn't right." };
        return Err(ApiError::status(StatusCode::UNAUTHORIZED, msg));
    };
    s.auth.succeeded(c.ip);
    let t = s.auth.open_session(&user)?;
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
    /// several people will sign in (default: just you)
    #[serde(default)]
    multi: bool,
}

/// The owner and their password, with the setup code.
async fn setup(State(s): State<Arc<AppState>>, c: Client, Json(r): Json<SetupReq>) -> ApiResult<Response> {
    if s.auth.configured() {
        return Err(ApiError::status(StatusCode::CONFLICT, "A password is already set. Sign in instead."));
    }
    s.auth.gate(c.ip).await?;
    if !s.auth.code_ok(&r.code) {
        s.auth.failed(c.ip);
        return Err(ApiError::status(StatusCode::UNAUTHORIZED, "That isn't the setup code."));
    }
    if r.multi && clean_name(&r.name).is_empty() {
        return Err(ApiError::status(StatusCode::BAD_REQUEST, "With several people, everyone signs in with their name: enter yours."));
    }
    check_password(&r.password, &r.name)?;
    let st = s.clone();
    let (pw, name) = (r.password, r.name);
    argon(move || st.auth.set_password("", &pw, Some(&name))).await??;
    {
        let mut st = s.auth.get();
        st.multi = r.multi;
        s.auth.save(&st)?;
    }
    s.auth.succeeded(c.ip);
    let t = s.auth.open_session(OWNER)?;
    Ok(([session_cookie(&t, &c)], Json(json!({"ok": true}))).into_response())
}

async fn logout(State(s): State<Arc<AppState>>, c: Client, headers: HeaderMap) -> Response {
    s.auth.close_session(&headers);
    ([set_cookie("", 0, c.https)], Json(json!({"ok": true}))).into_response()
}

/// Sign this user out of every browser and desktop app, this one included.
async fn logout_all(State(s): State<Arc<AppState>>, c: Client, CurrentUser(u): CurrentUser) -> ApiResult<Response> {
    let mut st = s.auth.get();
    st.sign_out(&u);
    s.auth.save(&st)?;
    Ok(([set_cookie("", 0, c.https)], Json(json!({"ok": true}))).into_response())
}

#[derive(Deserialize)]
struct PasswordReq {
    current: String,
    new: String,
}

/// Change your password; your other browsers and desktop apps are signed out.
async fn change_password(State(s): State<Arc<AppState>>, c: Client, CurrentUser(u): CurrentUser, Json(r): Json<PasswordReq>) -> ApiResult<Response> {
    s.auth.gate(c.ip).await?;
    let (name, phc) = {
        let st = s.auth.get();
        let user = st.user(&u).ok_or_else(|| ApiError::status(StatusCode::UNAUTHORIZED, "sign in first"))?;
        (user.name.clone(), user.password.clone())
    };
    check_password(&r.new, &name)?;
    let cur = r.current;
    if cur.chars().count() > MAX_PASSWORD || !argon(move || verify(&cur, &phc)).await.unwrap_or(false) {
        s.auth.failed(c.ip);
        return Err(ApiError::status(StatusCode::UNAUTHORIZED, "The current password isn't right."));
    }
    let st = s.clone();
    let new = r.new;
    argon(move || st.auth.set_password(&name, &new, None)).await??;
    let t = s.auth.open_session(&u)?;
    Ok(([session_cookie(&t, &c)], Json(json!({"ok": true}))).into_response())
}

#[derive(Deserialize)]
struct NameReq {
    name: String,
}

/// Change your own name (the greeting, and your sign-in name with several people).
async fn set_name(State(s): State<Arc<AppState>>, CurrentUser(u): CurrentUser, Json(r): Json<NameReq>) -> ApiResult<Json<Value>> {
    let name = clean_name(&r.name);
    let mut st = s.auth.get();
    if st.multi && name.is_empty() {
        return Err(ApiError::status(StatusCode::BAD_REQUEST, "With several people, everyone needs a name to sign in with."));
    }
    if st.users.iter().any(|x| x.id != u && x.name.trim().eq_ignore_ascii_case(&name) && !name.is_empty()) {
        return Err(ApiError::status(StatusCode::CONFLICT, "Someone else already has that name."));
    }
    let user = st.users.iter_mut().find(|x| x.id == u).ok_or_else(|| ApiError::status(StatusCode::UNAUTHORIZED, "sign in first"))?;
    user.name = name;
    s.auth.save(&st)?;
    Ok(Json(json!({"ok": true})))
}

#[derive(Deserialize)]
struct ModeReq {
    multi: bool,
}

/// Single-user (password only) or several people (name and password). Owner only.
async fn set_mode(State(s): State<Arc<AppState>>, CurrentUser(u): CurrentUser, Json(r): Json<ModeReq>) -> ApiResult<Json<Value>> {
    owner_only(&s, &u)?;
    let mut st = s.auth.get();
    if !r.multi && st.users.len() > 1 {
        return Err(ApiError::status(StatusCode::CONFLICT, "Remove the other people first: with just you, nobody else can sign in."));
    }
    if r.multi && st.owner().is_some_and(|o| o.name.trim().is_empty()) {
        return Err(ApiError::status(StatusCode::BAD_REQUEST, "Add your name first: with several people, everyone signs in with their name."));
    }
    st.multi = r.multi;
    s.auth.save(&st)?;
    Ok(Json(json!({"ok": true})))
}

async fn list_users(State(s): State<Arc<AppState>>, CurrentUser(u): CurrentUser) -> ApiResult<Json<Value>> {
    owner_only(&s, &u)?;
    let st = s.auth.get();
    Ok(Json(Value::Array(st.users.iter().map(|x| json!({"id": x.id, "name": x.name, "owner": x.owner, "created": x.created})).collect())))
}

#[derive(Deserialize)]
struct AddUserReq {
    name: String,
    password: String,
}

/// Add a person (several-people mode, owner only). They can change the password themselves.
async fn add_user(State(s): State<Arc<AppState>>, CurrentUser(u): CurrentUser, Json(r): Json<AddUserReq>) -> ApiResult<Json<Value>> {
    owner_only(&s, &u)?;
    let name = clean_name(&r.name);
    {
        let st = s.auth.get();
        if !st.multi {
            return Err(ApiError::status(StatusCode::CONFLICT, "Switch to several people first."));
        }
        if name.is_empty() {
            return Err(ApiError::status(StatusCode::BAD_REQUEST, "Enter their name: they sign in with it."));
        }
        if st.by_name(&name).is_some() {
            return Err(ApiError::status(StatusCode::CONFLICT, "Someone already has that name."));
        }
        if st.users.len() >= MAX_USERS {
            return Err(ApiError::status(StatusCode::CONFLICT, "That's as many people as Flightplan takes."));
        }
    }
    check_password(&r.password, &name)?;
    let pw = r.password;
    let hash = argon(move || hash_password(&pw)).await??;
    let id = hex(8)?;
    let mut st = s.auth.get();
    st.users.push(User { id: id.clone(), name, password: hash, owner: false, created: now() });
    s.auth.save(&st)?;
    Ok(Json(json!({"ok": true, "id": id})))
}

/// Remove a person and sign them out (owner only). Their files stay in data/user/users/<id>.
async fn remove_user(State(s): State<Arc<AppState>>, CurrentUser(u): CurrentUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    owner_only(&s, &u)?;
    let mut st = s.auth.get();
    if st.user(&id).is_some_and(|x| x.owner) {
        return Err(ApiError::status(StatusCode::CONFLICT, "The owner can't be removed."));
    }
    st.users.retain(|x| x.id != id);
    st.sign_out(&id);
    s.auth.save(&st)?;
    Ok(Json(json!({"ok": true})))
}

/// A short-lived code that connects a desktop app to this account.
async fn pairing(State(s): State<Arc<AppState>>, CurrentUser(u): CurrentUser) -> ApiResult<Json<Value>> {
    let code = s.auth.new_pairing(&u)?;
    Ok(Json(json!({"code": code, "minutes": PAIRING_MINUTES})))
}

#[derive(Deserialize)]
struct PairReq {
    code: String,
    /// what the desktop app calls itself
    #[serde(default)]
    device: String,
}

/// The desktop app trades a pairing code for its device token (shown once, kept hashed).
async fn pair(State(s): State<Arc<AppState>>, c: Client, Json(r): Json<PairReq>) -> ApiResult<Json<Value>> {
    s.auth.gate(c.ip).await?;
    let Some(user) = s.auth.take_pairing(&r.code) else {
        s.auth.failed(c.ip);
        tokio::time::sleep(Duration::from_millis(400)).await;
        return Err(ApiError::status(StatusCode::UNAUTHORIZED, "That pairing code isn't right or has expired. Make a new one in the web app: Settings, Sign-in."));
    };
    s.auth.succeeded(c.ip);
    let t = token()?;
    let mut st = s.auth.get();
    let name = st.user(&user).map(|u| u.name.clone()).unwrap_or_default();
    let device = clean_name(&r.device);
    st.devices.retain(|d| live(d.created, d.last_seen, now(), DEVICE_IDLE_DAYS));
    st.devices.push(Device { hash: sha(&t), user, name: if device.is_empty() { "Desktop app".into() } else { device }, created: now(), last_seen: now() });
    s.auth.save(&st)?;
    Ok(Json(json!({"token": format!("fpd_{t}"), "name": name})))
}

async fn list_devices(State(s): State<Arc<AppState>>, CurrentUser(u): CurrentUser) -> Json<Value> {
    let st = s.auth.get();
    Json(Value::Array(st.devices.iter().filter(|d| d.user == u).map(|d| json!({"id": &d.hash[..16], "name": d.name, "created": d.created, "lastSeen": d.last_seen})).collect()))
}

/// Disconnect one of your desktop apps.
async fn remove_device(State(s): State<Arc<AppState>>, CurrentUser(u): CurrentUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let mut st = s.auth.get();
    st.devices.retain(|d| !(d.user == u && d.hash.starts_with(&id) && id.len() >= 16));
    s.auth.save(&st)?;
    Ok(Json(json!({"ok": true})))
}

/// The desktop app disconnecting itself.
async fn forget_this_device(State(s): State<Arc<AppState>>, headers: HeaderMap) -> ApiResult<Json<Value>> {
    if let Some(t) = bearer(&headers) {
        let h = sha(&t);
        let mut st = s.auth.get();
        st.devices.retain(|d| d.hash != h);
        s.auth.save(&st)?;
    }
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

async fn put_image(State(s): State<Arc<AppState>>, CurrentUser(u): CurrentUser, body: Bytes) -> ApiResult<Json<Value>> {
    owner_only(&s, &u)?;
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

async fn delete_image(State(s): State<Arc<AppState>>, CurrentUser(u): CurrentUser) -> ApiResult<Json<Value>> {
    owner_only(&s, &u)?;
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

    fn with_cookie(t: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(header::COOKIE, format!("other=1; {COOKIE}={t}").parse().unwrap());
        h
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
        a.set_password("", "tailwind over the ridge", Some("Justus")).unwrap();
        assert!(a.setup_code().is_none() && !dir.join("setup-code.txt").exists());
        assert_eq!(a.get().owner().unwrap().name, "Justus");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn migrates_single_password_files() {
        let dir = tmp();
        std::fs::create_dir_all(&dir).unwrap();
        let h = hash_password("tailwind over the ridge").unwrap();
        std::fs::write(dir.join("auth.json"), json!({"password": h, "name": "Justus", "sessions": [{"hash": sha("x"), "created": now(), "last_seen": now()}]}).to_string()).unwrap();
        let a = Auth::load(dir.join("auth.json"));
        let st = a.get();
        assert_eq!(st.users.len(), 1);
        assert!(st.owner().is_some_and(|o| o.name == "Justus" && o.id == OWNER));
        assert_eq!(st.sessions[0].user, OWNER);
        assert!(!st.multi);
        drop(st);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn sessions_devices_and_paths() {
        let dir = tmp();
        let a = Auth::load(dir.join("auth.json"));
        a.set_password("", "abcdefghij", Some("Justus")).unwrap();
        let t = a.open_session(OWNER).unwrap();
        assert_eq!(a.user_of(&with_cookie(&t)).as_deref(), Some(OWNER));
        // desktop pairing: a code, once, for the user who made it
        let code = a.new_pairing(OWNER).unwrap();
        assert_eq!(a.take_pairing(&code.to_lowercase()).as_deref(), Some(OWNER));
        assert!(a.take_pairing(&code).is_none(), "codes work once");
        {
            let mut st = a.get();
            st.devices.push(Device { hash: sha(&"b".repeat(64)), user: OWNER.into(), name: "Mac".into(), created: now(), last_seen: now() });
            a.save(&st).unwrap();
        }
        let mut bh = HeaderMap::new();
        bh.insert(header::AUTHORIZATION, format!("Bearer fpd_{}", "b".repeat(64)).parse().unwrap());
        assert_eq!(a.user_of(&bh).as_deref(), Some(OWNER));
        // a new password signs everyone out, desktop apps included
        a.set_password("", "ijklmnopqr", None).unwrap();
        assert!(a.user_of(&with_cookie(&t)).is_none() && a.user_of(&bh).is_none());
        // another process (the set-password command) changing the file is picked up
        let t2 = a.open_session(OWNER).unwrap();
        assert!(a.user_of(&with_cookie(&t2)).is_some());
        std::thread::sleep(Duration::from_millis(20));
        Auth::load(dir.join("auth.json")).set_password("Justus", "qrstuvwxyz", None).unwrap();
        assert!(a.user_of(&with_cookie(&t2)).is_none(), "sessions from before the outside reset are gone");
        // stored on disk without the token
        let raw = std::fs::read_to_string(dir.join("auth.json")).unwrap();
        assert!(!raw.contains(&t2) && raw.contains("Justus"));
        // expiry: idle and absolute
        let t0 = now();
        assert!(live(t0 - 86400, t0, t0, IDLE_DAYS));
        assert!(!live(t0 - 40 * 86400, t0 - 31 * 86400, t0, IDLE_DAYS));
        assert!(!live(t0 - 91 * 86400, t0, t0, IDLE_DAYS));
        assert!(live(t0 - 200 * 86400, t0, t0, DEVICE_IDLE_DAYS), "desktop apps have no absolute limit while in use");
        let _ = std::fs::remove_dir_all(dir);
        let (get, put) = (Method::GET, Method::PUT);
        assert!(public(&get, "/") && public(&get, "/assets/x.js") && public(&Method::POST, "/api/auth/login") && public(&get, "/api/auth/image") && public(&Method::POST, "/api/auth/pair"));
        assert!(!public(&get, "/api/airport/KSDL") && !public(&get, "/tiles/aero.pmtiles") && !public(&Method::POST, "/api/auth/logout") && !public(&get, "/api/plans"));
        assert!(!public(&put, "/api/auth/image") && !public(&Method::DELETE, "/api/auth/image"), "changing the picture needs a session");
    }

    #[test]
    fn several_people() {
        let dir = tmp();
        let a = Auth::load(dir.join("auth.json"));
        a.set_password("", "abcdefghij", Some("Justus")).unwrap();
        let mut st = a.get();
        st.multi = true;
        st.users.push(User { id: "a1b2c3d4e5f60708".into(), name: "Sam".into(), password: hash_password("klmnopqrst").unwrap(), owner: false, created: now() });
        a.save(&st).unwrap();
        assert!(st.by_name(" sam ").is_some() && st.by_name("nobody").is_none());
        drop(st);
        // Sam signing out everywhere leaves Justus signed in
        let tj = a.open_session(OWNER).unwrap();
        let ts = a.open_session("a1b2c3d4e5f60708").unwrap();
        a.get().sign_out("a1b2c3d4e5f60708");
        assert!(a.user_of(&with_cookie(&tj)).is_some());
        assert!(a.user_of(&with_cookie(&ts)).is_none());
        let _ = std::fs::remove_dir_all(dir);
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
        let mut bad = HeaderMap::new();
        bad.insert(header::AUTHORIZATION, "Bearer fpd_../../x".parse().unwrap());
        assert!(bearer(&bad).is_none());
    }
}
