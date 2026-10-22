//! Small TTL cache in front of public weather / TFR feeds, so the browser never hits them
//! directly (no CORS trouble) and repeated map pans don't hammer the sources.

use parking_lot::Mutex;
use std::collections::HashMap;
use std::time::{Duration, Instant};

pub struct Cached {
    http: reqwest::Client,
    store: Mutex<HashMap<String, (Instant, Bytes, String)>>,
}

pub type Bytes = std::sync::Arc<Vec<u8>>;

impl Cached {
    pub fn new(http: reqwest::Client) -> Self {
        Self { http, store: Mutex::new(HashMap::new()) }
    }

    pub fn http(&self) -> &reqwest::Client {
        &self.http
    }

    /// GET `url`, reusing a cached body younger than `ttl`. Returns (body, content-type).
    pub async fn get(&self, url: &str, ttl: Duration) -> anyhow::Result<(Bytes, String)> {
        if let Some((at, body, ct)) = self.store.lock().get(url) {
            if at.elapsed() < ttl {
                return Ok((body.clone(), ct.clone()));
            }
        }
        let resp = self.http.get(url).send().await?;
        let status = resp.status();
        let ct = resp.headers().get(reqwest::header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("application/json").to_string();
        let body = std::sync::Arc::new(resp.bytes().await?.to_vec());
        if !status.is_success() {
            // serve a stale copy rather than an error if we have one
            if let Some((_, b, c)) = self.store.lock().get(url) {
                return Ok((b.clone(), c.clone()));
            }
            anyhow::bail!("{url}: HTTP {status}");
        }
        let mut s = self.store.lock();
        if s.len() > 2000 {
            s.retain(|_, (at, _, _)| at.elapsed() < Duration::from_secs(3600));
        }
        s.insert(url.to_string(), (Instant::now(), body.clone(), ct.clone()));
        Ok((body, ct))
    }

    /// Cached value for a non-URL key (e.g. a POST API), if younger than `ttl`.
    pub fn peek_json(&self, key: &str, ttl: Duration) -> Option<serde_json::Value> {
        let s = self.store.lock();
        let (at, body, _) = s.get(key)?;
        if at.elapsed() >= ttl {
            return None;
        }
        serde_json::from_slice(body).ok()
    }

    pub fn put_json(&self, key: &str, v: &serde_json::Value) {
        let body = std::sync::Arc::new(serde_json::to_vec(v).unwrap_or_default());
        self.store.lock().insert(key.to_string(), (Instant::now(), body, "application/json".into()));
    }

    pub async fn get_json(&self, url: &str, ttl: Duration) -> anyhow::Result<serde_json::Value> {
        let (b, _) = self.get(url, ttl).await?;
        Ok(serde_json::from_slice(&b)?)
    }
}
