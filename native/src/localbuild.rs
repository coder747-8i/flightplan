//! "Build on this computer": runs the data pipeline (pipeline/build_datapack.sh, embedded in the
//! app) in a work folder under the data directory, shows its progress, and installs the pack
//! it makes. Needs bash, Python 3, git, make and a C++ compiler (macOS / Linux; WSL on Windows).

use parking_lot::Mutex;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::Instant;

const FILES: &[(&str, &str)] = &[
    ("build_datapack.sh", include_str!("../../pipeline/build_datapack.sh")),
    ("build_tiles.sh", include_str!("../../pipeline/build_tiles.sh")),
    ("fetch_faa.py", include_str!("../../pipeline/fetch_faa.py")),
    ("fetch_arcgis.py", include_str!("../../pipeline/fetch_arcgis.py")),
    ("fetch_overture_fbo.py", include_str!("../../pipeline/fetch_overture_fbo.py")),
    ("build_nasr.py", include_str!("../../pipeline/build_nasr.py")),
    ("build_airspace.py", include_str!("../../pipeline/build_airspace.py")),
    ("build_airspace_index.py", include_str!("../../pipeline/build_airspace_index.py")),
    ("build_airways.py", include_str!("../../pipeline/build_airways.py")),
    ("build_fbo.py", include_str!("../../pipeline/build_fbo.py")),
    ("make_datapack.py", include_str!("../../pipeline/make_datapack.py")),
];

#[derive(Clone, Debug, PartialEq)]
pub enum State {
    Idle,
    Running,
    /// built; the pack is being installed
    Built(PathBuf),
    Failed(String),
    Cancelled,
}

pub struct Inner {
    pub state: State,
    /// the step the script is on ("== ..." lines)
    pub step: String,
    pub log: Vec<String>,
    pub started: Option<Instant>,
    child: Option<Child>,
}

#[derive(Clone)]
pub struct LocalBuild(pub Arc<Mutex<Inner>>);

impl Default for LocalBuild {
    fn default() -> Self {
        LocalBuild(Arc::new(Mutex::new(Inner { state: State::Idle, step: String::new(), log: vec![], started: None, child: None })))
    }
}

/// What's missing to run a build, with how to get it on this OS.
pub fn missing_tools() -> Vec<(&'static str, &'static str)> {
    if cfg!(windows) {
        return vec![("bash", "Building on Windows runs in WSL: install Ubuntu from the Microsoft Store, then run pipeline/build_datapack.sh there and import the zip.")];
    }
    let hint_mac = "xcode-select --install && brew install sqlite";
    let hint_linux = "sudo apt install build-essential git python3-venv libsqlite3-dev zlib1g-dev (Fedora: sudo dnf install gcc-c++ make git python3 sqlite-devel zlib-devel)";
    let hint = if cfg!(target_os = "macos") { hint_mac } else { hint_linux };
    let mut out = Vec::new();
    for (tool, label) in [("bash", "bash"), ("python3", "Python 3"), ("git", "git"), ("make", "make"), ("c++", "a C++ compiler")] {
        if which(tool).is_none() {
            out.push((label, hint));
        }
    }
    out
}

/// PATH for the build, with the usual Homebrew / local locations a GUI app doesn't inherit.
fn path_env() -> String {
    let mut p = std::env::var("PATH").unwrap_or_default();
    for extra in ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin"] {
        if !p.split(':').any(|x| x == extra) {
            p = format!("{p}:{extra}");
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        p = format!("{p}:{}/.local/bin", Path::new(&home).display());
    }
    p
}

fn which(tool: &str) -> Option<PathBuf> {
    path_env().split(':').map(|d| Path::new(d).join(tool)).find(|p| p.is_file())
}

impl LocalBuild {
    pub fn running(&self) -> bool {
        self.0.lock().state == State::Running
    }

    /// Write the scripts into `work/pipeline` and start the build.
    pub fn start(&self, work: &Path) -> anyhow::Result<()> {
        let dir = work.join("pipeline");
        std::fs::create_dir_all(&dir)?;
        for (name, body) in FILES {
            let p = dir.join(name);
            std::fs::write(&p, body)?;
            #[cfg(unix)]
            if name.ends_with(".sh") {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755))?;
            }
        }
        let out = work.join("dist").join("flightplan-data.zip");
        let mut cmd = Command::new("bash");
        // its own process group, so Cancel stops python / make / tippecanoe too
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }
        let mut child = cmd
            .arg(dir.join("build_datapack.sh"))
            .arg("--out")
            .arg(&out)
            .current_dir(work)
            .env("PATH", path_env())
            .env("PYTHONUNBUFFERED", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        {
            let mut g = self.0.lock();
            g.state = State::Running;
            g.step = "Starting".into();
            g.log.clear();
            g.started = Some(Instant::now());
            g.child = Some(child);
        }
        let push = |me: LocalBuild, r: Box<dyn std::io::Read + Send>| {
            std::thread::spawn(move || {
                for line in BufReader::new(r).lines().map_while(Result::ok) {
                    // strip terminal colour codes
                    let clean: String = regex::Regex::new(r"\x1b\[[0-9;]*m").map(|re| re.replace_all(&line, "").into_owned()).unwrap_or(line);
                    let mut g = me.0.lock();
                    if let Some(s) = clean.strip_prefix("== ") {
                        g.step = s.to_string();
                    }
                    if !clean.trim().is_empty() {
                        g.log.push(clean);
                        let n = g.log.len();
                        if n > 400 {
                            g.log.drain(0..n - 400);
                        }
                    }
                }
            })
        };
        let t1 = stdout.map(|s| push(self.clone(), Box::new(s)));
        let t2 = stderr.map(|s| push(self.clone(), Box::new(s)));
        let me = self.clone();
        std::thread::spawn(move || {
            if let Some(t) = t1 {
                let _ = t.join();
            }
            if let Some(t) = t2 {
                let _ = t.join();
            }
            // the pipes closed, so the process is ending; poll without holding the lock long
            let status = loop {
                let mut g = me.0.lock();
                match g.child.as_mut().map(|c| c.try_wait()) {
                    Some(Ok(Some(s))) => break Some(Ok(s)),
                    Some(Err(e)) => break Some(Err(e)),
                    None => break None,
                    Some(Ok(None)) => {}
                }
                drop(g);
                std::thread::sleep(std::time::Duration::from_millis(200));
            };
            let mut g = me.0.lock();
            g.child = None;
            if g.state != State::Running {
                return; // cancelled
            }
            g.state = match status {
                Some(Ok(s)) if s.success() && out.is_file() => State::Built(out),
                Some(Ok(s)) => State::Failed(match g.log.iter().rev().find(|l| !l.starts_with("  ")) {
                    Some(last) => format!("The build stopped (exit {}): {last}", s.code().unwrap_or(-1)),
                    None => format!("The build stopped (exit {})", s.code().unwrap_or(-1)),
                }),
                Some(Err(e)) => State::Failed(e.to_string()),
                None => State::Failed("the build process went away".into()),
            };
        });
        Ok(())
    }

    pub fn cancel(&self) {
        let mut g = self.0.lock();
        if let Some(c) = g.child.as_mut() {
            #[cfg(unix)]
            let _ = Command::new("kill").args(["-TERM", &format!("-{}", c.id())]).status();
            let _ = c.kill();
        }
        g.state = State::Cancelled;
    }
}
