//! Sync saved flight plans with a Flightplan server (the web app's), both ways and instantly.
//! The desktop app pairs once with a code from the web app (Settings → Sign-in) and keeps the
//! device token it gets in sync.json (mode 0600). After that every save or delete here is sent
//! to the server, and the server's change stream (/api/events) brings in plans saved in the web
//! app (or another desktop) as soon as they're saved.
//!
//! Plans travel in the web app's format (camelCase, string waypoint uids). On each (re)connect
//! the lists are merged: plans the server doesn't have yet are uploaded, plans deleted on the
//! server since the last sync are deleted here.

use crate::backend::{Backend, Task};
use crate::model::place::{new_uid, Kind, Place, Waypoint};
use crate::model::route::Route;
use crate::store::SavedPlan;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::mpsc;

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct SyncConfig {
    /// the server's address, e.g. https://flightplan.example.com
    pub url: String,
    /// device token ("fpd_…")
    pub token: String,
    /// the account's name on the server (for "Connected as …")
    #[serde(default)]
    pub account: String,
    /// what this computer is called in the web app's list of desktop apps
    #[serde(default)]
    pub device: String,
    /// plan ids the server had at the last sync: one missing from the server now was deleted there
    #[serde(default)]
    pub known: Vec<String>,
}

enum Ev {
    /// plans changed on the server (or the stream reconnected)
    Changed,
    /// the server no longer accepts this computer's token
    Unauthorized,
}

#[derive(Default)]
pub struct Sync {
    path: PathBuf,
    pub cfg: Option<SyncConfig>,
    /// form fields in Settings
    pub url_input: String,
    pub code_input: String,
    pub device_input: String,
    pub error: String,
    pub last_sync: Option<chrono::DateTime<chrono::Local>>,
    connect: Task<anyhow::Result<SyncConfig>>,
    fetch: Task<anyhow::Result<Vec<Value>>>,
    pushes: Vec<Task<anyhow::Result<()>>>,
    events: Option<mpsc::Receiver<Ev>>,
    stream: Option<tokio::task::AbortHandle>,
    /// fetch again once the current one is done
    again: bool,
    /// the next fetch merges with local plans (after connecting or starting up)
    merge: bool,
}

fn write_private(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// "https://host:port" from what was typed ("host:8787", trailing slashes, a path).
pub fn normalize_url(s: &str) -> String {
    let s = s.trim().trim_end_matches('/');
    let s = if s.contains("://") { s.to_string() } else { format!("https://{s}") };
    match s.find("://").and_then(|i| s[i + 3..].find('/').map(|j| i + 3 + j)) {
        Some(cut) => s[..cut].to_string(),
        None => s,
    }
}

/// Plain http to something that isn't this machine or a home network sends plans and the token
/// unencrypted.
pub fn insecure(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("http://") else { return false };
    let host = rest.split([':', '/']).next().unwrap_or("");
    let private = host == "localhost" || host.ends_with(".local") || host.ends_with(".lan") || host.starts_with("192.168.") || host.starts_with("10.") || host.starts_with("127.") || (host.starts_with("172.") && host.split('.').nth(1).and_then(|o| o.parse::<u8>().ok()).is_some_and(|o| (16..=31).contains(&o)));
    !private
}

/// One request to the server with the device token. 401 comes back as the error "unauthorized".
async fn call(http: reqwest::Client, method: reqwest::Method, url: String, token: String, body: Option<Value>) -> anyhow::Result<Value> {
    let mut rq = http.request(method, &url).header("x-flightplan", "1");
    if !token.is_empty() {
        rq = rq.bearer_auth(token);
    }
    if let Some(b) = body {
        rq = rq.json(&b);
    }
    let r = rq.send().await?;
    let status = r.status();
    if status == reqwest::StatusCode::UNAUTHORIZED && !url.ends_with("/api/auth/pair") {
        anyhow::bail!("unauthorized");
    }
    let text = r.text().await.unwrap_or_default();
    let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    if !status.is_success() {
        let msg = v["error"].as_str().map(String::from).unwrap_or_else(|| format!("HTTP {status}"));
        anyhow::bail!("{msg}");
    }
    Ok(v)
}

/// The plan as the server (and the web app) keeps it.
pub fn to_wire(p: &SavedPlan) -> Value {
    let wps: Vec<Value> = p
        .route
        .waypoints
        .iter()
        .map(|w| json!({"kind": w.kind, "id": w.id, "name": w.name, "detail": w.detail, "lat": w.lat, "lon": w.lon, "uid": w.uid.to_string(), "fuelStop": w.fuel_stop}))
        .collect();
    json!({
        "id": p.id,
        "name": p.name,
        "savedAt": p.saved_at,
        "route": {"waypoints": wps, "cruise": p.route.cruise, "departure": p.route.departure, "stopMinutes": p.route.stop_minutes},
        "routeText": p.route_text,
        "fuelStops": p.fuel_stops,
        "aircraftId": p.aircraft_id,
        "aircraft": p.aircraft,
        "distanceNm": p.distance_nm,
        "eteMin": p.ete_min,
        "fuelGal": p.fuel_gal,
        "notes": p.notes,
    })
}

/// A plan from the server; None when it doesn't make sense.
pub fn from_wire(v: &Value) -> Option<SavedPlan> {
    let r = &v["route"];
    let waypoints: Vec<Waypoint> = r["waypoints"]
        .as_array()?
        .iter()
        .filter_map(|w| {
            let kind: Kind = serde_json::from_value(w["kind"].clone()).ok()?;
            let place = Place { kind, id: w["id"].as_str()?.to_string(), name: w["name"].as_str().unwrap_or("").to_string(), detail: w["detail"].as_str().unwrap_or("").to_string(), lat: w["lat"].as_f64()?, lon: w["lon"].as_f64()? };
            Some(Waypoint { place, uid: new_uid(), fuel_stop: w["fuelStop"].as_bool().unwrap_or(false) })
        })
        .collect();
    let mut route = Route::default();
    route.waypoints = waypoints;
    route.cruise = r["cruise"].as_f64().unwrap_or(6500.0);
    route.departure = r["departure"].as_str().unwrap_or("").to_string();
    route.stop_minutes = r["stopMinutes"].as_f64().filter(|m| *m > 0.0).unwrap_or(45.0);
    let s = |k: &str| v[k].as_str().unwrap_or("").to_string();
    Some(SavedPlan {
        id: v["id"].as_str()?.to_string(),
        name: v["name"].as_str()?.to_string(),
        saved_at: s("savedAt"),
        route_text: s("routeText"),
        fuel_stops: v["fuelStops"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default(),
        stop_minutes: r["stopMinutes"].as_f64().unwrap_or(45.0),
        aircraft_id: s("aircraftId"),
        aircraft: s("aircraft"),
        distance_nm: v["distanceNm"].as_f64().unwrap_or(0.0),
        ete_min: v["eteMin"].as_f64().unwrap_or(0.0),
        fuel_gal: v["fuelGal"].as_f64().unwrap_or(0.0),
        notes: s("notes"),
        route,
    })
}

/// Combine the server's plans with ours after being apart: theirs, plus ours they've never had
/// (to upload). Ours that were on the server last time and aren't now were deleted there.
pub fn merge(remote: Vec<SavedPlan>, local: &[SavedPlan], known: &[String]) -> (Vec<SavedPlan>, Vec<SavedPlan>) {
    let mut out = remote;
    let mut upload = Vec::new();
    for p in local {
        let on_server = out.iter().any(|r| r.id == p.id || r.name == p.name);
        if !on_server && !known.contains(&p.id) {
            upload.push(p.clone());
        }
    }
    out.extend(upload.iter().cloned());
    out.sort_by(|a, b| b.saved_at.cmp(&a.saved_at));
    (out, upload)
}

/// Read the server's change stream until the token stops working; reconnects after drops.
async fn listen(http: reqwest::Client, url: String, token: String, tx: mpsc::Sender<Ev>, wake: impl Fn() + Send + 'static) {
    loop {
        let r = http.get(format!("{url}/api/events")).bearer_auth(&token).header("accept", "text/event-stream").send().await;
        match r {
            Ok(mut resp) if resp.status().is_success() => {
                // anything may have changed while we weren't listening
                if tx.send(Ev::Changed).is_err() {
                    return;
                }
                wake();
                let mut buf = String::new();
                while let Ok(Some(chunk)) = resp.chunk().await {
                    buf.push_str(&String::from_utf8_lossy(&chunk));
                    while let Some(i) = buf.find('\n') {
                        let line: String = buf.drain(..=i).collect();
                        if let Some(data) = line.trim().strip_prefix("data:") {
                            let what = serde_json::from_str::<Value>(data.trim()).ok().and_then(|v| v["what"].as_str().map(String::from)).unwrap_or_default();
                            if what == "plans" || what == "all" {
                                if tx.send(Ev::Changed).is_err() {
                                    return;
                                }
                                wake();
                            }
                        }
                    }
                }
            }
            Ok(resp) if resp.status() == reqwest::StatusCode::UNAUTHORIZED => {
                let _ = tx.send(Ev::Unauthorized);
                wake();
                return;
            }
            _ => {}
        }
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }
}

impl Sync {
    pub fn load(user_dir: &Path) -> Self {
        let path = user_dir.join("sync.json");
        let cfg: Option<SyncConfig> = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok()).filter(|c: &SyncConfig| !c.token.is_empty());
        let device = std::env::var("HOSTNAME").ok().or_else(|| std::env::var("COMPUTERNAME").ok()).filter(|h| !h.trim().is_empty()).map(|h| format!("Flightplan desktop on {h}")).unwrap_or_else(|| format!("Flightplan desktop ({})", std::env::consts::OS));
        Sync { path, merge: cfg.is_some(), url_input: cfg.as_ref().map(|c| c.url.clone()).unwrap_or_default(), device_input: device, cfg, ..Default::default() }
    }

    fn save(&self) {
        match &self.cfg {
            Some(c) => {
                if let Ok(b) = serde_json::to_vec_pretty(c) {
                    if let Err(e) = write_private(&self.path, &b) {
                        tracing::warn!("couldn't save sync settings: {e:#}");
                    }
                }
            }
            None => {
                let _ = std::fs::remove_file(&self.path);
            }
        }
    }

    pub fn connected(&self) -> bool {
        self.cfg.is_some()
    }

    pub fn busy(&self) -> bool {
        self.connect.pending() || self.fetch.pending()
    }

    /// Pair with the server using a code from the web app.
    pub fn connect(&mut self, be: &Backend) {
        let url = normalize_url(&self.url_input);
        let code = self.code_input.trim().to_string();
        let device = self.device_input.trim().to_string();
        self.error.clear();
        let http = be.http.clone();
        self.connect = be.spawn(async move {
            let v = call(http, reqwest::Method::POST, format!("{url}/api/auth/pair"), String::new(), Some(json!({"code": code, "device": device}))).await?;
            let token = v["token"].as_str().ok_or_else(|| anyhow::anyhow!("the server didn't send a token: is it a Flightplan server?"))?.to_string();
            Ok(SyncConfig { url, token, account: v["name"].as_str().unwrap_or("").to_string(), device, known: vec![] })
        });
    }

    /// Stop syncing (and tell the server to forget this computer).
    pub fn disconnect(&mut self, be: &Backend) {
        if let Some(c) = self.cfg.take() {
            let http = be.http.clone();
            drop(be.spawn(async move { call(http, reqwest::Method::DELETE, format!("{}/api/auth/device", c.url), c.token, None).await.ok() }));
        }
        self.stop_stream();
        self.save();
        self.last_sync = None;
    }

    fn stop_stream(&mut self) {
        if let Some(h) = self.stream.take() {
            h.abort();
        }
        self.events = None;
    }

    /// Listen to the server's change stream; reconnects by itself.
    fn start_stream(&mut self, be: &Backend) {
        self.stop_stream();
        let Some(c) = self.cfg.clone() else { return };
        let (tx, rx) = mpsc::channel();
        let http = be.http.clone();
        let ctx = be.ctx.clone();
        let task = be.rt().spawn(listen(http, c.url, c.token, tx, move || ctx.request_repaint()));
        self.stream = Some(task.abort_handle());
        self.events = Some(rx);
    }

    fn refetch(&mut self, be: &Backend) {
        let Some(c) = self.cfg.clone() else { return };
        if self.fetch.pending() {
            self.again = true;
            return;
        }
        let http = be.http.clone();
        self.fetch = be.spawn(async move {
            let v = call(http, reqwest::Method::GET, format!("{}/api/plans", c.url), c.token, None).await?;
            Ok(v.as_array().cloned().unwrap_or_default())
        });
    }

    /// Send one saved plan to the server.
    pub fn push(&mut self, be: &Backend, p: &SavedPlan) {
        let Some(c) = self.cfg.clone() else { return };
        let http = be.http.clone();
        let body = to_wire(p);
        let url = format!("{}/api/plans/{}", c.url, p.id);
        self.pushes.push(be.spawn(async move { call(http, reqwest::Method::PUT, url, c.token, Some(body)).await.map(|_| ()) }));
    }

    pub fn delete(&mut self, be: &Backend, id: &str) {
        let Some(c) = self.cfg.clone() else { return };
        let http = be.http.clone();
        let url = format!("{}/api/plans/{id}", c.url);
        self.pushes.push(be.spawn(async move { call(http, reqwest::Method::DELETE, url, c.token, None).await.map(|_| ()) }));
    }

    fn signed_out(&mut self) -> String {
        self.cfg = None;
        self.stop_stream();
        self.save();
        self.error = "The server disconnected this computer (password changed, or removed in the web app). Connect again with a new code.".into();
        "Sync stopped: connect again".into()
    }

    /// Once a frame: connect results, server changes, merges. Returns a message for a toast.
    pub fn update(&mut self, be: &Backend, plans: &mut Vec<SavedPlan>, store: &crate::store::Store) -> Option<String> {
        let mut toast = None;
        if let Some(r) = self.connect.poll() {
            match r {
                Ok(c) => {
                    toast = Some(format!("Connected to {}", c.url));
                    self.cfg = Some(c);
                    self.code_input.clear();
                    self.merge = true;
                    self.save();
                }
                Err(e) => self.error = format!("Couldn't connect: {e:#}"),
            }
        }
        // (re)start listening when connected
        if self.cfg.is_some() && self.events.is_none() {
            self.start_stream(be);
        }
        let mut changed = false;
        let mut unauthorized = false;
        if let Some(rx) = &self.events {
            while let Ok(ev) = rx.try_recv() {
                match ev {
                    Ev::Changed => changed = true,
                    Ev::Unauthorized => unauthorized = true,
                }
            }
        }
        if unauthorized {
            return Some(self.signed_out());
        }
        if changed {
            self.refetch(be);
        }
        if let Some(r) = self.fetch.poll() {
            match r {
                Ok(list) => {
                    let remote: Vec<SavedPlan> = list.iter().filter_map(from_wire).collect();
                    let known: Vec<String> = remote.iter().map(|p| p.id.clone()).collect();
                    if self.merge {
                        self.merge = false;
                        let old_known = self.cfg.as_ref().map(|c| c.known.clone()).unwrap_or_default();
                        let (all, upload) = merge(remote, plans, &old_known);
                        for p in &upload {
                            self.push(be, p);
                        }
                        if !upload.is_empty() {
                            toast = Some(format!("Sent {} plan{} to the server", upload.len(), if upload.len() == 1 { "" } else { "s" }));
                        }
                        *plans = all;
                    } else {
                        *plans = remote;
                    }
                    store.save_plans(plans);
                    if let Some(c) = &mut self.cfg {
                        c.known = known;
                    }
                    self.save();
                    self.error.clear();
                    self.last_sync = Some(chrono::Local::now());
                }
                Err(e) if e.to_string() == "unauthorized" => return Some(self.signed_out()),
                Err(e) => self.error = format!("Couldn't sync: {e:#}"),
            }
            if std::mem::take(&mut self.again) {
                self.refetch(be);
            }
        }
        let mut failed = None;
        self.pushes.retain_mut(|t| match t.poll() {
            Some(Err(e)) => {
                failed = Some(e);
                false
            }
            Some(Ok(())) => false,
            None => true,
        });
        if let Some(e) = failed {
            if e.to_string() == "unauthorized" {
                return Some(self.signed_out());
            }
            self.error = format!("Couldn't send a change: {e:#}");
        }
        toast
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(id: &str, name: &str, at: &str) -> SavedPlan {
        let mut r = Route::default();
        r.replace(vec![Place::coord(33.6, -111.9), Place { kind: Kind::Airport, id: "KPRC".into(), name: "Prescott".into(), detail: String::new(), lat: 34.65, lon: -112.42 }]);
        r.waypoints[1].fuel_stop = true;
        SavedPlan { id: id.into(), name: name.into(), saved_at: at.into(), route: r, route_text: String::new(), fuel_stops: vec![], stop_minutes: 45.0, aircraft_id: String::new(), aircraft: String::new(), distance_nm: 66.0, ete_min: 40.0, fuel_gal: 6.0, notes: String::new() }
    }

    #[test]
    fn wire_roundtrip() {
        let p = plan("abc123", "To Prescott", "2026-10-25T10:00:00-07:00");
        let w = to_wire(&p);
        assert_eq!(w["route"]["waypoints"][1]["kind"], "airport");
        assert!(w["route"]["waypoints"][1]["uid"].is_string(), "the web app's uids are strings");
        let back = from_wire(&w).unwrap();
        assert_eq!(back.name, "To Prescott");
        assert_eq!(back.route.waypoints.len(), 2);
        assert!(back.route.waypoints[1].fuel_stop);
        // a plan the web app saved
        let web = json!({"id": "x1", "name": "Web", "savedAt": "2026-10-25T17:00:00.000Z", "route": {"waypoints": [{"kind": "navaid", "id": "PXR", "name": "Phoenix", "detail": "VORTAC", "lat": 33.4, "lon": -111.97, "uid": "k3j2"}], "cruise": 7500, "departure": "", "stopMinutes": 30}, "routeText": "PXR", "fuelStops": [], "aircraftId": "", "aircraft": "", "distanceNm": 0, "eteMin": 0, "fuelGal": 0});
        let p = from_wire(&web).unwrap();
        assert_eq!(p.route.cruise, 7500.0);
        assert_eq!(p.route.stop_minutes, 30.0);
    }

    #[test]
    fn merging() {
        let server = vec![plan("a", "A", "2026-10-25T12"), plan("b", "B", "2026-10-25T11")];
        let local = vec![plan("a", "A", "2026-10-25T12"), plan("c", "C new here", "2026-10-25T13"), plan("d", "D deleted on the server", "2026-10-24")];
        let (all, upload) = merge(server, &local, &["a".into(), "b".into(), "d".into()]);
        let ids: Vec<&str> = all.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, ["c", "a", "b"]);
        assert_eq!(upload.len(), 1);
        assert_eq!(upload[0].id, "c");
    }

    /// The real server: sign in, make a pairing code, pair like the desktop app, save a plan,
    /// and see the change arrive on the event stream.
    #[test]
    fn pairs_and_hears_changes() {
        let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
        rt.block_on(async {
            let root = std::env::temp_dir().join(format!("fp-sync-{}", new_uid()));
            std::fs::create_dir_all(&root).unwrap();
            let st = flightplan_server::state(flightplan_server::Config::from_root(&root)).unwrap();
            st.auth.set_password("", "tailwind over the ridge", Some("Justus")).unwrap();
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let url = format!("http://{}", listener.local_addr().unwrap());
            tokio::spawn(flightplan_server::serve(st, listener));
            let http = reqwest::Client::new();
            // the web app: sign in and ask for a pairing code
            let r = http.post(format!("{url}/api/auth/login")).header("x-flightplan", "1").json(&json!({"password": "tailwind over the ridge"})).send().await.unwrap();
            assert!(r.status().is_success());
            let cookie = r.headers()["set-cookie"].to_str().unwrap().split(';').next().unwrap().to_string();
            let v: Value = http.post(format!("{url}/api/auth/pairing")).header("x-flightplan", "1").header("cookie", &cookie).send().await.unwrap().json().await.unwrap();
            let code = v["code"].as_str().unwrap().to_string();
            // the desktop app: a wrong code is refused, the right one gives a token
            assert!(call(http.clone(), reqwest::Method::POST, format!("{url}/api/auth/pair"), String::new(), Some(json!({"code": "AAAA-BBBB-CCCC"}))).await.is_err());
            let v = call(http.clone(), reqwest::Method::POST, format!("{url}/api/auth/pair"), String::new(), Some(json!({"code": code, "device": "Test Mac"}))).await.unwrap();
            let token = v["token"].as_str().unwrap().to_string();
            assert_eq!(v["name"], "Justus");
            assert!(call(http.clone(), reqwest::Method::POST, format!("{url}/api/auth/pair"), String::new(), Some(json!({"code": code}))).await.is_err(), "codes work once");
            // listening, then a plan saved from the desktop shows up
            let (tx, rx) = mpsc::channel();
            let listening = tokio::spawn(listen(http.clone(), url.clone(), token.clone(), tx, || {}));
            let first = tokio::task::spawn_blocking({
                let rx_wait = std::time::Duration::from_secs(5);
                move || {
                    let a = rx.recv_timeout(rx_wait).map(|e| matches!(e, Ev::Changed));
                    (a, rx)
                }
            })
            .await
            .unwrap();
            assert_eq!(first.0, Ok(true), "a change on connecting");
            let rx = first.1;
            let p = plan("deskplan1", "From the desk", "2026-10-25T18:00:00Z");
            call(http.clone(), reqwest::Method::PUT, format!("{url}/api/plans/deskplan1"), token.clone(), Some(to_wire(&p))).await.unwrap();
            let got = tokio::task::spawn_blocking(move || rx.recv_timeout(std::time::Duration::from_secs(5)).map(|e| matches!(e, Ev::Changed))).await.unwrap();
            assert_eq!(got, Ok(true), "the save was announced");
            // the web app sees it
            let v: Value = http.get(format!("{url}/api/plans")).header("cookie", &cookie).send().await.unwrap().json().await.unwrap();
            assert_eq!(v[0]["name"], "From the desk");
            assert!(from_wire(&v[0]).is_some());
            // a token that's been disconnected stops working
            call(http.clone(), reqwest::Method::DELETE, format!("{url}/api/auth/device"), token.clone(), None).await.unwrap();
            let err = call(http.clone(), reqwest::Method::GET, format!("{url}/api/plans"), token.clone(), None).await.unwrap_err();
            assert_eq!(err.to_string(), "unauthorized");
            listening.abort();
            let _ = std::fs::remove_dir_all(root);
        });
    }

    #[test]
    fn urls() {
        assert_eq!(normalize_url("flightplan.example.com/"), "https://flightplan.example.com");
        assert_eq!(normalize_url("http://192.168.1.5:8787/settings"), "http://192.168.1.5:8787");
        assert!(insecure("http://flightplan.example.com"));
        assert!(!insecure("http://192.168.1.5:8787") && !insecure("https://x.example.com") && !insecure("http://localhost:8787"));
    }
}
