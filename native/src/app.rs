//! The application: state, the frame loop, and how the chart, panels and dialogs fit together.

use crate::backend::{Backend, Task};
use crate::map::render::Look;
use crate::map::style::{self, Palette};
use crate::map::tiles::Tiles;
use crate::map::view::{MapView, Proj};
use crate::model::plan::Plan;
use crate::store::{Fleet, Prefs, Saved, Store};
use crate::theme::{self, Tokens};
use crate::ui;
use crate::wx::state::Wx;
use flightplan_server::setup::DataInfo;
use serde_json::Value;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Panel {
    None,
    Route,
    Layers,
    Weather,
    Aircraft,
    Help,
    Settings,
}

#[derive(Clone, Debug)]
pub enum Decode {
    Metar(String),
    Pirep(String),
    Sigmet(Value),
    Gairmet(Value),
}

/// One thing under a click on the chart.
#[derive(Clone, Debug)]
pub struct PointItem {
    pub title: String,
    pub subtitle: String,
    pub color: egui::Color32,
    pub airport: Option<String>,
    pub decode: Option<Decode>,
}

#[derive(Clone, Debug)]
pub enum Selection {
    Airport(String),
    Point { lon: f64, lat: f64, items: Vec<PointItem> },
}

pub enum Dialog {
    Export,
    Wizard(Box<ui::wizard::Wizard>),
    Import(Box<ui::import::Import>),
}

/// Which API keys are configured (from the backend's settings).
#[derive(Default, Clone)]
pub struct Keys {
    pub windy_map: bool,
    pub windy_point: bool,
    pub notams: bool,
}

pub struct App {
    pub be: Backend,
    pub store: Store,
    pub saved: Saved,
    pub fleet: Fleet,
    /// flight plans saved in the app
    pub plans: Vec<crate::store::SavedPlan>,
    pub prefs: Prefs,
    pub plan: Plan,
    pub wx: Wx,
    /// day boundaries of the official WPC / SPC products
    pub outlooks: crate::wx::official::Outlooks,
    pub map: MapView,
    pub tiles: Tiles,
    pub palette: Palette,
    pub t: Tokens,
    pub look: Look,
    pub look_rev: u64,
    look_key: String,
    pub panel: Panel,
    pub selection: Option<Selection>,
    pub dialog: Option<Dialog>,
    pub briefing: Option<ui::briefing::Briefing>,
    pub onboarding: ui::onboarding::Onboarding,
    pub tour: Option<ui::tour::Tour>,
    pub keys: Keys,
    pub data_info: Option<DataInfo>,
    pub data_job: flightplan_server::setup::Job,
    pub data_changed: bool,
    pub route_ui: ui::route::RouteUi,
    pub route_wx: ui::weather::RouteWx,
    pub search: ui::search::Search,
    pub info: ui::info::InfoCard,
    pub settings_ui: ui::keys::KeysUi,
    pub chart: ui::chart::ChartUi,
    pub timeline: ui::timeline::Timeline,
    pub altitude_open: bool,
    pub toast: Option<(String, Instant)>,
    pub last_proj: Option<Proj>,
    saved_hash: u64,
    fleet_hash: u64,
    save_at: Option<Instant>,
    applied_night: Option<bool>,
    pub notams: Task<Value>,
    pub screenshot: Option<ui::shot::Shot>,
    /// the briefing is capturing the chart: draw nothing but the chart
    pub capturing: bool,
    pub local_build: crate::localbuild::LocalBuild,
}

fn hash_of<T: serde::Serialize>(v: &T) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    serde_json::to_string(v).unwrap_or_default().hash(&mut h);
    h.finish()
}

impl App {
    pub fn new(cc: &eframe::CreationContext) -> anyhow::Result<Self> {
        let ctx = cc.egui_ctx.clone();
        theme::install_fonts(&ctx);
        egui_extras_install(&ctx);
        let be = Backend::new(ctx.clone())?;
        let store = Store::new(be.user_dir());
        let saved = store.load_saved();
        let fleet = store.load_fleet();
        let plans = store.load_plans();
        let prefs = store.load_prefs();
        let palette = if saved.night { style::night() } else { style::day() };
        let t = theme::tokens(saved.night);
        let tiles = Tiles::new(ctx.clone(), be.rt().handle().clone(), be.http.clone(), be.cache_dir().join("chart"), be.cfg.tiles_dir.clone());
        let map = MapView::new(saved.view[0], saved.view[1], saved.view[2]);
        let look = Look { palette: palette.clone(), layers: saved.layers.clone(), altitude: saved.altitude, size: 512 };
        let (data_info, data_job) = flightplan_server::setup::current(&be.server);
        let onboarding = ui::onboarding::Onboarding::new(prefs.onboarded, data_info.is_some());
        let mut app = App {
            saved_hash: hash_of(&saved),
            fleet_hash: hash_of(&fleet),
            route_ui: ui::route::RouteUi::new(&saved.route),
            route_wx: Default::default(),
            be,
            store,
            saved,
            fleet,
            plans,
            prefs,
            plan: Plan::default(),
            wx: Wx::default(),
            outlooks: Default::default(),
            map,
            tiles,
            palette,
            t,
            look,
            look_rev: 1,
            look_key: String::new(),
            panel: Panel::None,
            selection: None,
            dialog: None,
            briefing: None,
            onboarding,
            tour: None,
            keys: Keys::default(),
            data_info,
            data_job,
            data_changed: false,
            search: Default::default(),
            info: Default::default(),
            settings_ui: Default::default(),
            chart: Default::default(),
            timeline: Default::default(),
            altitude_open: false,
            toast: None,
            last_proj: None,
            save_at: None,
            applied_night: None,
            notams: Task::default(),
            screenshot: ui::shot::Shot::from_env(),
            capturing: false,
            local_build: Default::default(),
        };
        app.refresh_keys();
        Ok(app)
    }

    pub fn refresh_keys(&mut self) {
        let k = self.be.server.settings.get();
        self.keys = Keys { windy_map: k.windy_map.is_some(), windy_point: k.windy_point.is_some(), notams: k.notam_id.is_some() && k.notam_secret.is_some() };
    }

    pub fn toast(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), Instant::now()));
    }

    pub fn night(&self) -> bool {
        self.saved.night
    }

    pub fn set_night(&mut self, night: bool) {
        self.saved.night = night;
        self.palette = if night { style::night() } else { style::day() };
        self.t = theme::tokens(night);
    }

    /// Save the current route as a named plan (replacing one with the same name).
    pub fn save_plan(&mut self, name: &str) {
        let r = &self.saved.route;
        let ac = self.fleet.active();
        let log = self.plan.log.as_ref();
        let name = name.trim();
        let name = if name.is_empty() { r.text() } else { name.to_string() };
        let plan = crate::store::SavedPlan {
            id: self.plans.iter().find(|p| p.name == name).map(|p| p.id.clone()).unwrap_or_else(crate::model::aircraft::uid),
            name: name.clone(),
            saved_at: chrono::Local::now().to_rfc3339(),
            route: r.clone(),
            route_text: r.text(),
            fuel_stops: r.fuel_stops().into_iter().map(|i| r.waypoints[i].id.clone()).collect(),
            stop_minutes: r.stop_minutes,
            aircraft_id: ac.map(|a| a.id.clone()).unwrap_or_default(),
            aircraft: ac.map(|a| if a.tail.is_empty() { a.name.clone() } else { format!("{} · {}", a.tail, a.name) }).unwrap_or_default(),
            distance_nm: log.map(|l| l.total_dist).unwrap_or(0.0),
            ete_min: log.map(|l| l.total_time).unwrap_or(0.0),
            fuel_gal: log.map(|l| l.trip_fuel).unwrap_or(0.0),
            notes: String::new(),
        };
        self.plans.retain(|p| p.name != name);
        self.plans.insert(0, plan);
        self.store.save_plans(&self.plans);
        self.toast(format!("Saved \"{name}\""));
    }

    /// Load a saved plan into the route (and select its aircraft if it's still in the fleet).
    pub fn load_plan(&mut self, id: &str) {
        let Some(p) = self.plans.iter().find(|p| p.id == id).cloned() else { return };
        let rev = self.saved.route.rev + 1;
        let mut route = p.route.clone();
        route.rev = rev;
        route.editing = false;
        self.saved.route = route;
        if self.fleet.list.iter().any(|a| a.id == p.aircraft_id) {
            self.fleet.active = p.aircraft_id.clone();
        }
        self.fit_route();
        self.toast(format!("Loaded \"{}\"", p.name));
    }

    pub fn delete_plan(&mut self, id: &str) {
        self.plans.retain(|p| p.id != id);
        self.store.save_plans(&self.plans);
    }

    pub fn data_expired(&self) -> bool {
        self.data_info.as_ref().and_then(|i| i.expires.as_deref()).and_then(|e| chrono::NaiveDate::parse_from_str(e, "%Y-%m-%d").ok()).is_some_and(|d| d < chrono::Local::now().date_naive())
    }

    /// Fit the route into the view (leaving room for the panel and profile).
    pub fn fit_route(&mut self) {
        let w = &self.saved.route.waypoints;
        let Some(proj) = self.last_proj else { return };
        if w.is_empty() {
            return;
        }
        if w.len() == 1 {
            self.map.fly_to(w[0].lon, w[0].lat, 10.0);
            return;
        }
        let (mut west, mut south, mut east, mut north) = (180.0f64, 90.0f64, -180.0f64, -90.0f64);
        for p in w {
            west = west.min(p.lon);
            east = east.max(p.lon);
            south = south.min(p.lat);
            north = north.max(p.lat);
        }
        let left = if self.panel == Panel::None { 100.0 } else { 470.0 };
        let bottom = if self.saved.profile_open { 320.0 } else { 90.0 };
        self.map.fit(proj.rect, west, south, east, north, [left, 80.0, 80.0, bottom], 11.0, true);
    }

    fn poll_background(&mut self, ctx: &egui::Context) {
        // a local build finished: install its pack like an imported zip
        let built = match &self.local_build.0.lock().state {
            crate::localbuild::State::Built(p) => Some(p.clone()),
            _ => None,
        };
        if let Some(zip) = built {
            self.local_build.0.lock().state = crate::localbuild::State::Idle;
            let _g = self.be.rt().enter();
            if let Err(e) = flightplan_server::setup::start_zip(self.be.server.clone(), zip) {
                self.toast(format!("{e:#}"));
            }
        }
        if self.local_build.running() {
            ctx.request_repaint_after(Duration::from_millis(500));
        }
        let (info, job) = flightplan_server::setup::current(&self.be.server);
        let was_installed = self.data_info.is_some();
        let finished = self.data_job.state != "done" && job.state == "done";
        if finished || (info.is_some() != was_installed) {
            self.tiles.reload_archives();
            self.data_changed = true;
            self.look_rev += 1;
        }
        let busy = matches!(job.state, "downloading" | "uploading" | "copying" | "extracting" | "checking");
        self.data_info = info;
        self.data_job = job;
        if busy {
            ctx.request_repaint_after(Duration::from_millis(300));
        }
    }

    fn update_look(&mut self) {
        let key = format!("{}{:?}{:?}", self.saved.night, self.saved.layers, self.saved.altitude);
        if key != self.look_key {
            self.look_key = key;
            self.look_rev += 1;
            self.look = Look { palette: self.palette.clone(), layers: self.saved.layers.clone(), altitude: self.saved.altitude, size: self.look.size };
        }
    }

    fn autosave(&mut self) {
        let (lon, lat) = self.map.center_lonlat();
        self.saved.view = [lon, lat, self.map.zoom];
        let h = hash_of(&self.saved);
        let fh = hash_of(&self.fleet.list) ^ hash_of(&self.fleet.active);
        if h != self.saved_hash || fh != self.fleet_hash {
            let at = *self.save_at.get_or_insert_with(Instant::now);
            if at.elapsed() > Duration::from_secs(2) {
                if h != self.saved_hash {
                    self.store.save_saved(&self.saved);
                    self.saved_hash = h;
                }
                if fh != self.fleet_hash {
                    self.store.save_fleet(&mut self.fleet);
                    self.fleet_hash = fh;
                }
                self.save_at = None;
            } else {
                self.be.ctx.request_repaint_after(Duration::from_secs(2));
            }
        }
    }

    pub fn save_now(&mut self) {
        let (lon, lat) = self.map.center_lonlat();
        self.saved.view = [lon, lat, self.map.zoom];
        self.store.save_saved(&self.saved);
        self.store.save_fleet(&mut self.fleet);
    }
}

fn egui_extras_install(_ctx: &egui::Context) {}

impl eframe::App for App {
    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = &root.ctx().clone();
        if self.applied_night != Some(self.saved.night) {
            self.applied_night = Some(self.saved.night);
            theme::apply(ctx, &self.t);
        }
        let ppp = ctx.pixels_per_point();
        self.look.size = if ppp > 1.25 { 512 } else { 256 };
        self.poll_background(ctx);
        self.update_look();
        self.tiles.begin_frame();
        let ac = self.fleet.active().cloned();
        let expired = self.data_expired();
        self.outlooks.update(&self.be);
        self.plan.update(&self.be, &self.saved.route, ac.as_ref(), &self.wx, &self.outlooks, &self.saved.minimums, expired, self.keys.notams);
        if let Some(p) = self.last_proj {
            self.wx.update(&self.be, &self.saved.wx, p.bounds(), p.zoom);
        }

        self.capturing = self.briefing.as_ref().is_some_and(|b| b.capturing());
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(root, |ui| {
            ui::chart::show(self, ui);
        });
        // while the briefing captures the chart, only the chart is drawn
        let capturing = self.capturing;
        if !capturing {
            ui::shell::show(self, ctx);
        }
        if let Some(b) = self.briefing.take() {
            let keep = ui::briefing::show(self, ctx, b);
            self.briefing = keep;
        }
        if !capturing {
            ui::dialogs::show(self, ctx);
        }
        if capturing {
        } else if self.onboarding.open {
            ui::onboarding::show(self, ctx);
        } else if let Some(t) = self.tour.take() {
            self.tour = ui::tour::show(self, ctx, t);
        }
        if let Some((msg, at)) = &self.toast {
            if at.elapsed() > Duration::from_secs(4) {
                self.toast = None;
            } else {
                let msg = msg.clone();
                ui::widgets::toast(ctx, &self.t, &msg);
                ctx.request_repaint_after(Duration::from_millis(500));
            }
        }
        self.autosave();
        if let Some(mut s) = self.screenshot.take() {
            if !s.step(ctx, self) {
                self.screenshot = Some(s);
            }
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_now();
    }
}
