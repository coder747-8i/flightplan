//! Bridge to the Rust backend (the same library the web server uses): airport database,
//! terrain, weather, NOTAMs and data-pack installs, run on a tokio runtime. Results come back
//! to the UI thread through `Task`s that are polled once per frame.

use flightplan_server::{AppState, Config};
use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::oneshot;

pub struct Backend {
    runtime: Option<tokio::runtime::Runtime>,
    pub server: Arc<AppState>,
    pub http: reqwest::Client,
    pub cfg: Config,
    /// root of everything we store (user files, data pack, caches)
    pub data_dir: PathBuf,
    pub ctx: egui::Context,
}

/// A value being computed in the background.
pub struct Task<T> {
    rx: Option<oneshot::Receiver<T>>,
}

impl<T> Default for Task<T> {
    fn default() -> Self {
        Task { rx: None }
    }
}

impl<T> Task<T> {
    /// The result once, when it arrives.
    pub fn poll(&mut self) -> Option<T> {
        let rx = self.rx.as_mut()?;
        match rx.try_recv() {
            Ok(v) => {
                self.rx = None;
                Some(v)
            }
            Err(oneshot::error::TryRecvError::Empty) => None,
            Err(oneshot::error::TryRecvError::Closed) => {
                self.rx = None;
                None
            }
        }
    }
    pub fn pending(&self) -> bool {
        self.rx.is_some()
    }
}

pub fn data_dir() -> PathBuf {
    if let Ok(d) = std::env::var("FP_DATA_DIR") {
        return PathBuf::from(d);
    }
    directories::ProjectDirs::from("io.github", "coder747-8i", "Flightplan")
        .map(|p| p.data_dir().to_path_buf())
        .unwrap_or_else(|| std::env::temp_dir().join("flightplan"))
}

impl Backend {
    pub fn new(ctx: egui::Context) -> anyhow::Result<Self> {
        let rt = tokio::runtime::Builder::new_multi_thread().enable_all().worker_threads(4).thread_name("fp-worker").build()?;
        // FP_ROOT points at a repository checkout (data/build + tiles), for development
        let (cfg, data_dir) = match std::env::var("FP_ROOT") {
            Ok(root) => {
                let root = PathBuf::from(root);
                (Config::from_root(&root), root.join("data"))
            }
            Err(_) => {
                let d = data_dir();
                std::fs::create_dir_all(&d)?;
                (Config::app_dir(&d, d.join("web")), d)
            }
        };
        let server = {
            let _guard = rt.enter();
            flightplan_server::state(cfg.clone())?
        };
        let http = reqwest::Client::builder()
            .user_agent(concat!("flightplan-desktop/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(std::time::Duration::from_secs(15))
            .read_timeout(std::time::Duration::from_secs(45))
            .build()?;
        Ok(Backend { runtime: Some(rt), server, http, cfg, data_dir, ctx })
    }

    pub fn rt(&self) -> &tokio::runtime::Runtime {
        self.runtime.as_ref().expect("runtime")
    }

    pub fn spawn<T: Send + 'static>(&self, fut: impl Future<Output = T> + Send + 'static) -> Task<T> {
        let (tx, rx) = oneshot::channel();
        let ctx = self.ctx.clone();
        self.rt().spawn(async move {
            let v = fut.await;
            let _ = tx.send(v);
            ctx.request_repaint();
        });
        Task { rx: Some(rx) }
    }

    pub fn spawn_blocking<T: Send + 'static>(&self, f: impl FnOnce() -> T + Send + 'static) -> Task<T> {
        let (tx, rx) = oneshot::channel();
        let ctx = self.ctx.clone();
        self.rt().spawn_blocking(move || {
            let _ = tx.send(f());
            ctx.request_repaint();
        });
        Task { rx: Some(rx) }
    }

    pub fn user_dir(&self) -> PathBuf {
        self.cfg.user_dir.clone()
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.cfg.cache_dir.clone()
    }
}

impl Drop for Backend {
    /// Don't wait for downloads or tile jobs still in flight when the window closes.
    fn drop(&mut self) {
        if let Some(rt) = self.runtime.take() {
            rt.shutdown_background();
        }
    }
}
