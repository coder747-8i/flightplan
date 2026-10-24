//! `flightplan-server`: serve the app from a repository checkout (see run.sh). The desktop app
//! embeds the same server instead.

use std::{net::SocketAddr, path::PathBuf};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_max_level(tracing::Level::INFO).init();
    let root = PathBuf::from(std::env::var("FP_ROOT").unwrap_or_else(|_| "..".into()));
    let _ = dotenvy::from_path(root.join(".env"));
    let state = flightplan_server::state(flightplan_server::Config::from_root(&root))?;
    let port: u16 = std::env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(8787);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    flightplan_server::serve(state, tokio::net::TcpListener::bind(addr).await?).await
}
