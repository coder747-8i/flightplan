//! `flightplan-server`: serve the app from a repository checkout (see run.sh). The desktop app
//! embeds the same server instead. `flightplan-server set-password` sets the sign-in password
//! (asks for it, or reads one line from a pipe). Environment: PORT (8787), FP_BIND (0.0.0.0),
//! FP_TRUST_PROXY and FP_SECURE_COOKIES (see auth.rs).

use std::{net::SocketAddr, path::PathBuf};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_max_level(tracing::Level::INFO).init();
    let root = PathBuf::from(std::env::var("FP_ROOT").unwrap_or_else(|_| "..".into()));
    let _ = dotenvy::from_path(root.join(".env"));
    let cfg = flightplan_server::Config::from_root(&root);
    if std::env::args().nth(1).as_deref() == Some("set-password") {
        return set_password(&cfg);
    }
    let state = flightplan_server::state(cfg)?;
    let port: u16 = std::env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(8787);
    // behind a reverse proxy, FP_BIND=127.0.0.1 keeps the app reachable only through it
    let ip: std::net::IpAddr = std::env::var("FP_BIND").ok().and_then(|b| b.trim().parse().ok()).unwrap_or([0, 0, 0, 0].into());
    let addr = SocketAddr::from((ip, port));
    flightplan_server::serve(state, tokio::net::TcpListener::bind(addr).await?).await
}

/// Set (or reset) the web app's password from the command line; signs every device out.
fn set_password(cfg: &flightplan_server::Config) -> anyhow::Result<()> {
    use std::io::{BufRead, IsTerminal, Write};
    let auth = flightplan_server::auth::Auth::load(cfg.settings_file.with_file_name("auth.json"));
    let read = |prompt: &str| -> anyhow::Result<String> {
        if std::io::stdin().is_terminal() {
            eprint!("{prompt}");
            std::io::stderr().flush()?;
        }
        let mut line = String::new();
        std::io::stdin().lock().read_line(&mut line)?;
        Ok(line.trim_end_matches(['\r', '\n']).to_string())
    };
    let name = read("Your name for the greeting (Enter to keep): ")?;
    let pw = read(&format!("New password ({}+ characters, shown as you type): ", flightplan_server::auth::MIN_PASSWORD))?;
    if let Some(problem) = flightplan_server::auth::password_problem(&pw, &name) {
        anyhow::bail!("{problem}");
    }
    auth.set_password(&pw, (!name.trim().is_empty()).then_some(name.as_str()))?;
    eprintln!("Password saved. Every device has to sign in again.");
    Ok(())
}
