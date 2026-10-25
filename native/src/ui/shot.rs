//! Scripted screenshots for checking the UI (development aid).
//! FP_SHOT_SCRIPT is a file with one step per line: `<action> [argument] > <name>`; after each
//! action the app waits for the chart to settle and saves <FP_SHOT_DIR>/<name>.png, then quits
//! after the last step. Actions: panel <route|layers|weather|aircraft|settings|help|none>,
//! route <text>, view <lon> <lat> <zoom>, night, day, airport <id>, onboarding <step>,
//! finish-onboarding, briefing, export, wizard, import, tour, depart <hours>, wait.

use crate::app::{App, Dialog, Panel, Selection};
use std::path::PathBuf;
use std::time::Instant;

pub struct Shot {
    steps: Vec<(String, String, String)>,
    i: usize,
    dir: PathBuf,
    started: Option<Instant>,
    requested: bool,
}

impl Shot {
    pub fn from_env() -> Option<Shot> {
        let script = std::fs::read_to_string(std::env::var("FP_SHOT_SCRIPT").ok()?).ok()?;
        let dir = PathBuf::from(std::env::var("FP_SHOT_DIR").unwrap_or_else(|_| ".".into()));
        let _ = std::fs::create_dir_all(&dir);
        let steps = parse(&script);
        Some(Shot { steps, i: 0, dir, started: None, requested: false })
    }

    fn act(&self, app: &mut App) {
        let (a, arg, _) = &self.steps[self.i];
        act(app, a, arg, &self.dir);
    }

    /// Returns true when the script has finished.
    pub fn step(&mut self, ctx: &egui::Context, app: &mut App) -> bool {
        if self.i >= self.steps.len() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return true;
        }
        let started = match self.started {
            Some(s) => s,
            None => {
                self.act(app);
                let now = Instant::now();
                self.started = Some(now);
                now
            }
        };
        ctx.request_repaint();
        let settled = started.elapsed().as_secs_f32() > 2.5 && settled(app);
        if !self.requested && (settled || started.elapsed().as_secs() > 15) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            self.requested = true;
            return false;
        }
        if self.requested {
            let img = ctx.input(|i| i.events.iter().find_map(|e| if let egui::Event::Screenshot { image, .. } = e { Some(image.clone()) } else { None }));
            if let Some(img) = img {
                let name = &self.steps[self.i].2;
                if !name.is_empty() {
                    let path = self.dir.join(format!("{name}.png"));
                    let rgba: Vec<u8> = img.pixels.iter().flat_map(|p| p.to_array()).collect();
                    if let Some(buf) = image::RgbaImage::from_raw(img.size[0] as u32, img.size[1] as u32, rgba) {
                        let _ = buf.save(&path);
                        eprintln!("shot {}", path.display());
                    }
                }
                self.i += 1;
                self.started = None;
                self.requested = false;
            }
        }
        false
    }
}

/// Parse a script: `<action> [argument] > <name>` per line.
pub fn parse(script: &str) -> Vec<(String, String, String)> {
    script
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .map(|l| {
            let (cmd, name) = l.split_once('>').unwrap_or((l, ""));
            let cmd = cmd.trim();
            let (a, arg) = cmd.split_once(' ').unwrap_or((cmd, ""));
            (a.to_string(), arg.trim().to_string(), name.trim().to_string())
        })
        .collect()
}

/// Whether the app has settled enough for a screenshot.
pub fn settled(app: &App) -> bool {
    !matches!(app.data_job.state, "downloading" | "uploading" | "copying" | "extracting" | "checking")
        && !app.tiles.busy() && !app.map.animating() && app.briefing.as_ref().is_none_or(|b| !b.capturing())
}

/// Apply one scripted action to the app.
pub fn act(app: &mut App, a: &str, arg: &str, dir: &std::path::Path) {
    let arg = &arg.to_string();
    match a {
            "panel" => {
                app.panel = match arg.as_str() {
                    "route" => Panel::Route,
                    "layers" => Panel::Layers,
                    "weather" => Panel::Weather,
                    "aircraft" => Panel::Aircraft,
                    "settings" => Panel::Settings,
                    "help" => Panel::Help,
                    _ => Panel::None,
                }
            }
            "route" => {
                let (places, _) = super::route::resolve(app, arg);
                app.saved.route.replace(places);
                app.fit_route();
            }
            "view" => {
                let v: Vec<f64> = arg.split_whitespace().filter_map(|x| x.parse().ok()).collect();
                if v.len() == 3 {
                    app.map = crate::map::view::MapView::new(v[0], v[1], v[2]);
                }
            }
            "night" => app.set_night(true),
            "day" => app.set_night(false),
            "airport" => app.selection = Some(Selection::Airport(arg.clone())),
            "onboarding" => app.onboarding.reopen(arg.parse().unwrap_or(0)),
            "finish-onboarding" => app.onboarding.open = false,
            "briefing" => app.briefing = Some(super::briefing::Briefing::new(app)),
            "pdf" => {
                if let Some(b) = &app.briefing {
                    match super::briefing::build_pdf(app, b) {
                        Ok(bytes) => {
                            let _ = std::fs::write(dir.join(arg), bytes);
                        }
                        Err(e) => eprintln!("pdf failed: {e:#}"),
                    }
                }
            }
            "install-zip" => {
                let _g = app.be.rt().enter();
                match flightplan_server::setup::start_zip(app.be.server.clone(), std::path::PathBuf::from(arg)) {
                    Err(e) => eprintln!("install failed: {e:#}"),
                    Ok(()) => eprintln!("install started: {:?}", flightplan_server::setup::current(&app.be.server).1.state),
                }
            }
            "close" => {
                app.briefing = None;
                app.dialog = None;
                app.selection = None;
            }
            "export" => app.dialog = Some(Dialog::Export),
            "wizard" => app.dialog = Some(Dialog::Wizard(Box::new(super::wizard::Wizard::new(None)))),
            "import" => app.dialog = Some(Dialog::Import(Box::default())),
            "tour" => app.tour = Some(super::tour::Tour::default()),
            "profile" => app.saved.profile_open = arg != "off",
            "cruise" => app.saved.route.cruise = arg.parse().unwrap_or(6500.0),
            "settings-open" => app.panel = Panel::Settings,
            // depart <hours from now>
            "depart" => {
                let h: f64 = arg.parse().unwrap_or(0.0);
                super::timeline::set_departure(app, chrono::Utc::now() + chrono::Duration::minutes((h * 60.0) as i64));
            }
            "wx" => {
                for k in arg.split(',') {
                    match k {
                        "radar" => app.saved.wx.radar = true,
                        "winds" => app.saved.wx.winds = true,
                        "pireps" => app.saved.wx.pireps = true,
                        "gairmet" => app.saved.wx.gairmet = true,
                        _ => {}
                    }
                }
            }
            _ => {}
        }
}
