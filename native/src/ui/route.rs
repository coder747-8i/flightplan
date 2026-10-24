//! Route panel: route entry, flight settings, waypoints with leg info, totals and the
//! hazard advisor; export / import.

use super::tour::mark;
use super::widgets::{self, faint, section};
use crate::app::{App, Dialog, Panel};
use crate::model::place::{Kind, Place};
use crate::model::route::Route;
use crate::nav::geo::{bearing, distance_nm};
use crate::nav::magvar::variation_now;
use crate::nav::navlog::{deg, fmt_time, thousands};
use crate::theme;
use egui::{vec2, RichText};
use egui_phosphor::regular as ph;

pub struct RouteUi {
    pub text: String,
    synced_rev: u64,
    pub unresolved: Vec<String>,
    pub import_error: String,
}

impl RouteUi {
    pub fn new(r: &Route) -> Self {
        RouteUi { text: r.text(), synced_rev: r.rev, unresolved: vec![], import_error: String::new() }
    }
}

/// Resolve a route string ("KNYL BZA 3300N11400W KPHX"); ambiguous fixes pick the one
/// nearest the previous point.
pub fn resolve(app: &App, text: &str) -> (Vec<Place>, Vec<String>) {
    let Ok(data) = app.be.server.try_data() else { return (vec![], text.split_whitespace().map(String::from).collect()) };
    let mut out = Vec::new();
    let mut missing = Vec::new();
    let mut prev: Option<(f64, f64)> = None;
    for tok in text.to_uppercase().split([' ', ',', '\n', '\t']).filter(|t| !t.is_empty() && *t != "DCT") {
        match data.db.resolve(tok, prev) {
            Ok(Some(p)) => {
                prev = Some((p.lat, p.lon));
                out.push(Place::from_server(&p));
            }
            _ => missing.push(tok.to_string()),
        }
    }
    (out, missing)
}

/// Open a Garmin .fpl or GPX file. Named points are matched to the database when one with
/// that id lies within 3 NM of the file's position; anything else stays a lat/long waypoint.
pub fn import_file(app: &mut App) {
    let Some(path) = rfd::FileDialog::new().set_title("Open a flight plan").add_filter("Flight plans", &["fpl", "gpx", "xml"]).pick_file() else { return };
    app.route_ui.import_error.clear();
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) => {
            app.route_ui.import_error = format!("{}: {e}", path.display());
            return;
        }
    };
    match crate::io::flightplan::parse_route_file(&text) {
        Ok(mut pts) => {
            if let Ok(data) = app.be.server.try_data() {
                for p in pts.iter_mut() {
                    if p.id.len() <= 5 && p.id.chars().all(|c| c.is_ascii_alphanumeric()) {
                        if let Ok(Some(db)) = data.db.resolve(&p.id.to_uppercase(), Some((p.lat, p.lon))) {
                            if distance_nm(db.lat, db.lon, p.lat, p.lon) < 3.0 {
                                *p = Place::from_server(&db);
                            }
                        }
                    }
                }
            }
            app.saved.route.replace(pts);
            app.fit_route();
        }
        Err(e) => app.route_ui.import_error = format!("{}: {e}", path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()),
    }
}

fn suggested(app: &App) -> Option<(f64, bool)> {
    let w = &app.saved.route.waypoints;
    if w.len() < 2 {
        return None;
    }
    let (a, b) = (&w[0], &w[w.len() - 1]);
    let mc = (bearing(a.lat, a.lon, b.lat, b.lon) - variation_now(a.lat, a.lon) + 360.0) % 360.0;
    let east = mc < 180.0;
    let terrain = app.plan.profile.samples.iter().map(|s| s.max).fold(0.0, f64::max);
    let base = (((terrain + 1000.0 - 500.0) / 1000.0).ceil() * 1000.0 + 500.0).max(3500.0);
    let mut alt = base;
    if east != (((alt / 1000.0).floor() as i64) % 2 == 1) {
        alt += 1000.0;
    }
    Some((alt, east))
}

pub fn panel(app: &mut App, ctx: &egui::Context) {
    let t = app.t;
    if app.route_ui.synced_rev != app.saved.route.rev {
        app.route_ui.synced_rev = app.saved.route.rev;
        app.route_ui.text = app.saved.route.text();
    }
    let subtitle = match &app.plan.log {
        Some(l) if app.saved.route.waypoints.len() >= 2 => format!("{} NM · {} · {:.1} gal", l.total_dist.round(), fmt_time(l.total_time), l.trip_fuel),
        _ => "Type a route or click the map".into(),
    };
    let mut action: Option<&str> = None;
    let close = widgets::side_panel(
        ctx,
        &t,
        "panel-route",
        "Route",
        &subtitle,
        380.0,
        app,
        |ui, app| {
            let has = !app.saved.route.waypoints.is_empty();
            if ui.add_enabled(has, egui::Button::new(RichText::new(ph::TRASH).size(15.0)).frame(false)).on_hover_text("Clear route").clicked() {
                action = Some("clear");
            }
            if ui.add_enabled(app.saved.route.waypoints.len() >= 2, egui::Button::new(RichText::new(ph::ARROWS_DOWN_UP).size(15.0)).frame(false)).on_hover_text("Reverse route").clicked() {
                action = Some("reverse");
            }
            if ui.add(egui::Button::new(RichText::new(ph::FOLDER_OPEN).size(15.0)).frame(false)).on_hover_text("Open a Garmin .fpl or GPX file").clicked() {
                action = Some("import");
            }
            let r = ui.add_enabled(has, egui::Button::new(RichText::new(ph::SHARE_NETWORK).size(15.0)).frame(false)).on_hover_text("Export to FltPlan.com, Garmin .fpl or GPX");
            mark(ui.ctx(), "route-export", r.rect);
            if r.clicked() {
                action = Some("export");
            }
            false
        },
        |ui, app| body(app, ui),
    );
    match action {
        Some("clear") => app.saved.route.clear(),
        Some("reverse") => app.saved.route.reverse(),
        Some("import") => import_file(app),
        Some("export") => app.dialog = Some(Dialog::Export),
        _ => {}
    }
    if close {
        app.panel = Panel::None;
    }
}

fn body(app: &mut App, ui: &mut egui::Ui) {
    let t = app.t;
    let entry = ui.vertical(|ui| {
        let te = egui::TextEdit::multiline(&mut app.route_ui.text).font(theme::mono(13.0)).desired_rows(2).desired_width(f32::INFINITY).hint_text("KNYL BZA 3300N11400W KPHX");
        let r = ui.add(te);
        let enter = r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift);
        if r.changed() {
            app.route_ui.text = app.route_ui.text.to_uppercase().replace('\n', " ");
        }
        ui.horizontal(|ui| {
            let load = widgets::primary(ui, &t, "Load route").clicked();
            let editing = app.saved.route.editing;
            let label = if editing { format!("{}  Adding… (click map)", ph::CURSOR_CLICK) } else { format!("{}  Click map to add", ph::CURSOR_CLICK) };
            let b = egui::Button::new(RichText::new(label).color(if editing { t.accent } else { t.text })).min_size(vec2(0.0, 32.0)).stroke(egui::Stroke::new(1.0, if editing { t.accent } else { t.line_strong }));
            if ui.add(b).on_hover_text("Click the map to add waypoints").clicked() {
                app.saved.route.editing = !editing;
            }
            if load || enter {
                let text = app.route_ui.text.trim().to_string();
                let (places, missing) = resolve(app, &text);
                app.route_ui.unresolved = missing;
                app.saved.route.replace(places);
                app.route_ui.synced_rev = app.saved.route.rev;
                app.fit_route();
            }
        });
        if !app.route_ui.unresolved.is_empty() {
            widgets::para(ui, format!("{}  Not found: {}", ph::WARNING, app.route_ui.unresolved.join(", ")), t.warn, 12.0);
        }
        if !app.route_ui.import_error.is_empty() {
            widgets::para(ui, format!("{}  {}", ph::WARNING, app.route_ui.import_error), t.warn, 12.0);
        }
        widgets::para(ui, "Tip: drag the magenta line to add a waypoint (rubber-band), or drag a waypoint to move it.", t.text3, 11.5);
    });
    super::tour::mark(ui.ctx(), "route-entry", entry.response.rect);

    section(ui, &t, "Flight");
    egui::Grid::new("flight").num_columns(2).spacing(vec2(10.0, 8.0)).show(ui, |ui| {
        faint(ui, &t, "Aircraft");
        let current = app.fleet.active().map(|a| format!("{} · {}", if a.tail.is_empty() { &a.type_code } else { &a.tail }, a.name)).unwrap_or_default();
        egui::ComboBox::from_id_salt("route-ac").selected_text(current).width(230.0).show_ui(ui, |ui| {
            for a in &app.fleet.list {
                ui.selectable_value(&mut app.fleet.active, a.id.clone(), format!("{} · {}", if a.tail.is_empty() { &a.type_code } else { &a.tail }, a.name));
            }
        });
        ui.end_row();
        faint(ui, &t, "Cruise altitude");
        ui.horizontal(|ui| {
            ui.add(egui::DragValue::new(&mut app.saved.route.cruise).speed(50.0).range(500.0..=45000.0).suffix(" ft"));
            ui.menu_button(RichText::new(ph::CARET_DOWN), |ui| {
                for a in [3500.0, 4500.0, 5500.0, 6500.0, 7500.0, 8500.0, 9500.0, 10500.0, 11500.0, 12500.0] {
                    if ui.button(format!("{} ft", thousands(a))).clicked() {
                        app.saved.route.cruise = a;
                        ui.close();
                    }
                }
            });
        });
        ui.end_row();
        faint(ui, &t, "Departure (local)");
        ui.horizontal(|ui| {
            let r = ui.add(egui::TextEdit::singleline(&mut app.saved.route.departure).hint_text("now  ·  YYYY-MM-DD HH:MM").desired_width(160.0));
            if r.lost_focus() {
                app.saved.route.departure = normalize_time(&app.saved.route.departure);
            }
            if ui.small_button("Now").clicked() {
                app.saved.route.departure.clear();
            }
            if ui.small_button("+1 h").clicked() {
                let base = app.saved.route.departure_time() + chrono::Duration::hours(1);
                app.saved.route.departure = base.with_timezone(&chrono::Local).format("%Y-%m-%dT%H:%M").to_string();
            }
        });
        ui.end_row();
    });
    if let Some((alt, east)) = suggested(app) {
        if app.saved.route.cruise != alt {
            let text = format!("Suggested VFR altitude {} ({}, clears terrain by 1,000 ft+)", thousands(alt), if east { "eastbound, odd + 500" } else { "westbound, even + 500" });
            if ui.add(egui::Button::new(RichText::new(text).size(12.0).color(t.text2)).fill(t.surface2).wrap()).clicked() {
                app.saved.route.cruise = alt;
            }
        }
    }

    section(ui, &t, "Waypoints");
    if app.saved.route.waypoints.is_empty() {
        widgets::para(ui, "No waypoints yet. Type a route above, search with /, or turn on \"Click map to add\".", t.text2, 12.5);
    }
    let n = app.saved.route.waypoints.len();
    let mut op: Option<(&str, usize)> = None;
    let mut fly: Option<(f64, f64)> = None;
    let rows = app.plan.log.as_ref().map(|l| l.rows.clone()).unwrap_or_default();
    let mut start = 0.0;
    for i in 0..n {
        let w = app.saved.route.waypoints[i].clone();
        egui::Frame::new().fill(t.surface2).corner_radius(egui::CornerRadius::same(9)).inner_margin(egui::Margin::symmetric(8, 6)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                if ui.add_enabled(i > 0, egui::Button::new(RichText::new(ph::CARET_UP).size(12.0).color(t.text3)).frame(false).min_size(vec2(16.0, 18.0))).on_hover_text("Move up").clicked() {
                    op = Some(("up", i));
                }
                if ui.add_enabled(i + 1 < n, egui::Button::new(RichText::new(ph::CARET_DOWN).size(12.0).color(t.text3)).frame(false).min_size(vec2(16.0, 18.0))).on_hover_text("Move down").clicked() {
                    op = Some(("down", i));
                }
                ui.spacing_mut().item_spacing.x = 8.0;
                let sub = if w.kind == Kind::Airport { w.name.clone() } else if w.detail.is_empty() { w.name.clone() } else { w.detail.clone() };
                let label = ui.add(egui::Label::new(RichText::new(&w.id).font(theme::bold(13.5))).sense(egui::Sense::click())).on_hover_text("Show on the chart");
                ui.add(egui::Label::new(RichText::new(sub).size(11.5).color(t.text3)).truncate());
                if label.clicked() {
                    fly = Some((w.lon, w.lat));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(egui::Button::new(RichText::new(ph::X).size(13.0).color(t.text3)).frame(false)).on_hover_text("Remove").clicked() {
                        op = Some(("remove", i));
                    }
                });
            });
        });
        if i + 1 < n {
            let b = &app.saved.route.waypoints[i + 1];
            let d = distance_nm(w.lat, w.lon, b.lat, b.lon);
            let mc = bearing(w.lat, w.lon, b.lat, b.lon) - variation_now(w.lat, w.lon);
            let ete: f64 = rows.iter().filter(|r| r.cum_dist - r.dist >= start - 0.01 && r.cum_dist <= start + d + 0.01).map(|r| r.ete).sum();
            start += d;
            ui.horizontal(|ui| {
                ui.add_space(28.0);
                let txt = if ete > 0.0 { format!("{}°M    {d:.1} NM    {}", deg(mc), fmt_time(ete)) } else { format!("{}°M    {d:.1} NM", deg(mc)) };
                ui.label(RichText::new(txt).size(11.5).color(t.text3));
            });
        } else {
            ui.add_space(4.0);
        }
    }
    match op {
        Some(("up", i)) => app.saved.route.move_to(i, i - 1),
        Some(("down", i)) => app.saved.route.move_to(i, i + 1),
        Some(("remove", i)) => {
            let uid = app.saved.route.waypoints[i].uid;
            app.saved.route.remove(uid);
        }
        _ => {}
    }
    if let Some((lon, lat)) = fly {
        app.map.fly_to(lon, lat, 11.0);
    }

    if let Some(log) = app.plan.log.clone() {
        ui.add_space(8.0);
        ui.columns(4, |c| {
            let stat = |ui: &mut egui::Ui, icon: &str, v: String, unit: &str, bad: bool| {
                egui::Frame::new().fill(t.surface2).corner_radius(egui::CornerRadius::same(10)).inner_margin(egui::Margin::same(8)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(icon).color(t.text3));
                    ui.label(RichText::new(v).font(theme::bold(16.0)).color(if bad { t.bad } else { t.text }));
                    ui.label(RichText::new(unit).size(10.5).color(t.text3));
                });
            };
            stat(&mut c[0], ph::RULER, format!("{:.1}", log.total_dist), "NM", false);
            stat(&mut c[1], ph::CLOCK, fmt_time(log.total_time), "ETE", false);
            stat(&mut c[2], ph::GAS_PUMP, format!("{:.1}", log.trip_fuel), "gal trip", false);
            stat(&mut c[3], ph::AIRPLANE_LANDING, format!("{:.1}", log.fuel_at_landing), "gal at landing", log.fuel_at_landing < log.reserve_fuel);
        });
        section(ui, &t, "Hazard advisor");
        super::hazards::list(ui, &t, &app.plan.hazards, true);
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let label = if app.saved.profile_open { "Hide profile" } else { "Show profile" };
            if widgets::button(ui, format!("{}  {label}", ph::CHART_LINE)).clicked() {
                app.saved.profile_open = !app.saved.profile_open;
            }
            if widgets::button(ui, format!("{}  Navlog & briefing", ph::FILE_TEXT)).clicked() {
                app.briefing = Some(super::briefing::Briefing::new(app));
            }
        });
        let src = match app.plan.wind_source.as_str() {
            "windy" => "Windy (GFS)",
            "open-meteo" => "Open-Meteo GFS",
            _ => "loading…",
        };
        faint(ui, &t, format!("Winds: {src} · Magnetic variation: WMM-2025"));
    }
    if app.plan.loading {
        faint(ui, &t, "Updating the profile…");
    }
    if !app.plan.error.is_empty() {
        widgets::para(ui, &app.plan.error, t.bad, 12.0);
    }
}

/// Accept "2026-10-24 14:30", "14:30" (today) or empty.
fn normalize_time(s: &str) -> String {
    let s = s.trim();
    if s.is_empty() || s.eq_ignore_ascii_case("now") {
        return String::new();
    }
    let s2 = s.replace(' ', "T");
    if chrono::NaiveDateTime::parse_from_str(&s2, "%Y-%m-%dT%H:%M").is_ok() {
        return s2;
    }
    if let Ok(t) = chrono::NaiveTime::parse_from_str(s, "%H:%M") {
        return chrono::Local::now().date_naive().and_time(t).format("%Y-%m-%dT%H:%M").to_string();
    }
    s.to_string()
}
