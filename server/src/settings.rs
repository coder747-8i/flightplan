//! API keys, editable from the app's Settings panel. Stored in data/settings.json (mode 0600),
//! overriding .env. Keys are write-only over the API: reads return only "set" + the last 4
//! characters, except the Windy map key, which the browser needs to load Windy anyway.

use crate::{ApiResult, AppState};
use axum::{extract::State, routing::get, routing::post, Json, Router};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{path::PathBuf, sync::Arc};

#[derive(Default, Clone, Serialize, Deserialize)]
pub struct Keys {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub windy_map: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub windy_point: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notam_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notam_secret: Option<String>,
}

pub struct Settings {
    path: PathBuf,
    keys: RwLock<Keys>,
}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

impl Settings {
    pub fn load(path: PathBuf) -> Self {
        let mut keys = Keys { windy_map: env("WINDY_MAP_KEY"), windy_point: env("WINDY_POINT_KEY"), notam_id: env("FAA_NOTAM_CLIENT_ID"), notam_secret: env("FAA_NOTAM_CLIENT_SECRET") };
        if let Ok(b) = std::fs::read(&path) {
            if let Ok(saved) = serde_json::from_slice::<Keys>(&b) {
                // saved values win; an empty string saved in the app clears the .env value
                let pick = |s: Option<String>, e: Option<String>| match s {
                    Some(v) if v.is_empty() => None,
                    Some(v) => Some(v),
                    None => e,
                };
                keys = Keys {
                    windy_map: pick(saved.windy_map, keys.windy_map),
                    windy_point: pick(saved.windy_point, keys.windy_point),
                    notam_id: pick(saved.notam_id, keys.notam_id),
                    notam_secret: pick(saved.notam_secret, keys.notam_secret),
                };
            }
        }
        Self { path, keys: RwLock::new(keys) }
    }

    pub fn get(&self) -> Keys {
        self.keys.read().clone()
    }

    /// Save and use a new set of keys.
    pub fn replace(&self, k: Keys) -> anyhow::Result<()> {
        self.save(&k)?;
        *self.keys.write() = k;
        Ok(())
    }

    fn save(&self, k: &Keys) -> anyhow::Result<()> {
        // keep explicit clears as "" so .env doesn't resurrect them on restart
        let out = json!({
            "windy_map": k.windy_map.clone().unwrap_or_default(),
            "windy_point": k.windy_point.clone().unwrap_or_default(),
            "notam_id": k.notam_id.clone().unwrap_or_default(),
            "notam_secret": k.notam_secret.clone().unwrap_or_default(),
        });
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(&out)?)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
        }
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }
}

pub fn routes() -> Router<Arc<AppState>> {
    Router::new().route("/settings", get(read).put(write)).route("/settings/test", post(test))
}

fn masked(v: &Option<String>) -> Value {
    match v {
        Some(k) => json!({"set": true, "hint": format!("…{}", &k[k.len().saturating_sub(4)..])}),
        None => json!({"set": false}),
    }
}

async fn read(State(s): State<Arc<AppState>>) -> Json<Value> {
    let k = s.settings.get();
    Json(json!({
        "windyMap": masked(&k.windy_map),
        "windyPoint": masked(&k.windy_point),
        "notamId": masked(&k.notam_id),
        "notamSecret": masked(&k.notam_secret),
    }))
}

/// Body fields: omitted = unchanged, "" = clear, anything else = new value.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Update {
    windy_map: Option<String>,
    windy_point: Option<String>,
    notam_id: Option<String>,
    notam_secret: Option<String>,
}

async fn write(State(s): State<Arc<AppState>>, Json(u): Json<Update>) -> ApiResult<Json<Value>> {
    let mut k = s.settings.get();
    let apply = |slot: &mut Option<String>, v: Option<String>| {
        if let Some(v) = v {
            let v = v.trim().to_string();
            *slot = if v.is_empty() { None } else { Some(v) };
        }
    };
    apply(&mut k.windy_map, u.windy_map);
    apply(&mut k.windy_point, u.windy_point);
    apply(&mut k.notam_id, u.notam_id);
    apply(&mut k.notam_secret, u.notam_secret);
    s.settings.save(&k)?;
    *s.settings.keys.write() = k;
    s.cache.forget_prefix("windy:");
    s.cache.forget_prefix("notam:");
    Ok(read(State(s)).await)
}

#[derive(Deserialize)]
struct TestReq {
    which: String,
}

/// Try a saved key against its service and report whether it works.
async fn test(State(s): State<Arc<AppState>>, Json(t): Json<TestReq>) -> Json<Value> {
    let (ok, message) = test_key(&s, &t.which).await;
    Json(json!({"ok": ok, "message": message}))
}

/// (works, message) for "windyPoint", "windyMap" or "notam".
pub async fn test_key(s: &AppState, which: &str) -> (bool, String) {
    let k = s.settings.get();
    let http = s.cache.http();
    let result: anyhow::Result<String> = async {
        match which {
            "windyPoint" => {
                let key = k.windy_point.clone().ok_or_else(|| anyhow::anyhow!("no key saved"))?;
                let r = http
                    .post("https://api.windy.com/api/point-forecast/v2")
                    .json(&json!({"lat": 33.4, "lon": -112.0, "model": "gfs", "parameters": ["wind"], "levels": ["850h"], "key": key}))
                    .send()
                    .await?;
                let status = r.status();
                if status.is_success() { Ok("Windy point forecast key works".into()) } else { anyhow::bail!("Windy answered HTTP {status}: {}", r.text().await.unwrap_or_default().chars().take(160).collect::<String>()) }
            }
            "notam" => {
                let (id, secret) = (k.notam_id.clone().ok_or_else(|| anyhow::anyhow!("no client id saved"))?, k.notam_secret.clone().ok_or_else(|| anyhow::anyhow!("no client secret saved"))?);
                let r = http
                    .get("https://external-api.faa.gov/notamapi/v1/notams")
                    .header("client_id", id)
                    .header("client_secret", secret)
                    .query(&[("icaoLocation", "KPHX"), ("pageSize", "1")])
                    .send()
                    .await?;
                let status = r.status();
                match status.as_u16() {
                    200..=299 => Ok("FAA NOTAM API credentials work".into()),
                    401 => anyhow::bail!("The FAA didn't accept this client ID and secret (401 Unauthorized). Copy both again from your app on api.faa.gov (My Apps), and check the app has been approved for the NOTAM API."),
                    403 => anyhow::bail!("The FAA knows this key but it isn't allowed to use the NOTAM API yet (403). Check the app's API access on api.faa.gov; approval can take a while."),
                    429 => anyhow::bail!("The FAA is rate-limiting this key (429). Try again in a minute."),
                    _ => anyhow::bail!("FAA answered HTTP {status}"),
                }
            }
            "windyMap" => {
                // the map key is checked by Windy in the browser (it is domain-locked), so only report presence
                if k.windy_map.is_some() { Ok("Saved. Open Windy from the Weather panel to confirm it loads.".into()) } else { anyhow::bail!("no key saved") }
            }
            other => anyhow::bail!("unknown key {other}"),
        }
    }
    .await;
    match result {
        Ok(msg) => (true, msg),
        Err(e) => (false, e.to_string()),
    }
}
