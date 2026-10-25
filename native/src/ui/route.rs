//! Route panel: the go / no-go call, route entry, aircraft / altitude / departure, waypoints
//! with leg info and totals; export / import.

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

/// Airway identifiers: Victor (V), RNAV T, jet (J) and Q routes, colored airways.
pub fn is_airway(tok: &str) -> bool {
    let mut c = tok.chars();
    matches!(c.next(), Some('V' | 'T' | 'J' | 'Q' | 'A' | 'B' | 'G' | 'R')) && tok.len() >= 2 && tok.len() <= 5 && c.all(|x| x.is_ascii_digit())
}

/// Resolve a route string ("KNYL BZA 3300N11400W KPHX", "KSDL PXR V327 FLG KGCN");
/// ambiguous fixes pick the one nearest the previous point. An airway between two points
/// expands to the fixes along it.
pub fn resolve(app: &App, text: &str) -> (Vec<Place>, Vec<String>) {
    let Ok(data) = app.be.server.try_data() else { return (vec![], text.split_whitespace().map(String::from).collect()) };
    let mut out: Vec<Place> = Vec::new();
    let mut missing = Vec::new();
    let mut prev: Option<(f64, f64)> = None;
    let toks: Vec<String> = text.to_uppercase().split([' ', ',', '\n', '\t']).filter(|t| !t.is_empty() && *t != "DCT").map(String::from).collect();
    let mut pending_airway: Option<String> = None;
    for tok in toks {
        if is_airway(&tok) && data.db.resolve(&tok, prev).ok().flatten().is_none() {
            if !data.db.has_airways() {
                missing.push(format!("{tok} (airways need an updated data pack: Settings, Chart data)"));
            } else if out.is_empty() {
                missing.push(format!("{tok} (an airway needs a fix before it)"));
            } else {
                pending_airway = Some(tok);
            }
            continue;
        }
        match data.db.resolve(&tok, prev) {
            Ok(Some(p)) => {
                if let (Some(awy), Some(last)) = (pending_airway.take(), out.last()) {
                    match data.db.airway_between(&awy, (&last.id, last.lat, last.lon), &p.id) {
                        Ok(Some(points)) => out.extend(points.iter().map(Place::from_server)),
                        _ => missing.push(format!("{awy} (doesn't connect {} and {})", last.id, p.id)),
                    }
                }
                prev = Some((p.lat, p.lon));
                out.push(Place::from_server(&p));
            }
            _ => missing.push(tok.to_string()),
        }
    }
    if let Some(awy) = pending_airway {
        missing.push(format!("{awy} (an airway needs a fix after it)"));
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
            let has = !app.saved.route.waypoints.is_empty() && !app.saved.locked;
            if ui.add_enabled(has, egui::Button::new(RichText::new(ph::TRASH).size(15.0)).frame(false)).on_hover_text("Clear route").clicked() {
                action = Some("clear");
            }
            if ui.add_enabled(app.saved.route.waypoints.len() >= 2 && !app.saved.locked, egui::Button::new(RichText::new(ph::ARROWS_DOWN_UP).size(15.0)).frame(false)).on_hover_text("Reverse route").clicked() {
                action = Some("reverse");
            }
            if ui.add_enabled(!app.saved.locked, egui::Button::new(RichText::new(ph::FOLDER_OPEN).size(15.0)).frame(false)).on_hover_text("Open a Garmin .fpl or GPX file").clicked() {
                action = Some("import");
            }
            // save the plan in the app, under a name
            let name_id = egui::Id::new("save-plan-name");
            let resp = ui.add_enabled_ui(app.saved.route.waypoints.len() >= 2, |ui| {
                ui.menu_button(RichText::new(ph::FLOPPY_DISK).size(15.0), |ui| {
                    ui.set_width(240.0);
                    ui.label(RichText::new("Save flight plan").font(theme::semibold(13.0)));
                    let mut name: String = ui.data(|d| d.get_temp(name_id)).unwrap_or_else(|| {
                        let w = &app.saved.route.waypoints;
                        match (w.first(), w.last()) {
                            (Some(a), Some(b)) => format!("{} to {}", a.id, b.id),
                            _ => String::new(),
                        }
                    });
                    let r = ui.add(egui::TextEdit::singleline(&mut name).hint_text("Name").desired_width(220.0));
                    ui.data_mut(|d| d.insert_temp(name_id, name.clone()));
                    let stops = app.saved.route.fuel_stops().len();
                    if stops > 0 {
                        faint(ui, &t, format!("{} Saves the {stops} fuel stop{} too", ph::GAS_PUMP, if stops == 1 { "" } else { "s" }));
                    }
                    let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if widgets::primary(ui, &t, format!("{}  Save", ph::FLOPPY_DISK)).clicked() || enter {
                        app.save_plan(&name);
                        ui.data_mut(|d| d.remove::<String>(name_id));
                        ui.close();
                    }
                })
            });
            resp.response.on_hover_text("Save this flight plan in the app");
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
    let has_route = app.saved.route.waypoints.len() >= 2;

    // ---- the call ----
    if has_route {
        let top = ui.cursor().min;
        match app.plan.verdict.clone() {
            Some(v) => {
                super::verdict::card(ui, app, &v, "route-verdict");
                super::tour::mark(ui.ctx(), "route-verdict", egui::Rect::from_min_max(top, egui::pos2(top.x + ui.available_width(), ui.cursor().min.y)));
            }
            None => {
                ui.horizontal(|ui| {
                    ui.add(egui::Spinner::new().size(14.0));
                    faint(ui, &t, "Checking weather, terrain and airspace…");
                });
            }
        }
        if app.plan.forecast.loading {
            faint(ui, &t, "Fetching forecasts…");
        }
        ui.add_space(6.0);
    }

    // view-only mode: everything that changes the plan is read-only
    let locked = app.saved.locked;
    if locked {
        widgets::para(ui, format!("{}  View only. Press L or the lock in the rail to edit.", ph::LOCK_SIMPLE), t.text2, 11.5);
        app.saved.route.editing = false;
    }
    ui.scope(|ui| {
        if locked {
            ui.disable();
        }
        // ---- route entry ----
        let entry = ui.horizontal(|ui| {
            let w = ui.available_width() - 70.0;
            let te = egui::TextEdit::singleline(&mut app.route_ui.text).font(theme::mono(13.0)).desired_width(w).hint_text("KSDL PXR V327 FLG KGCN");
            let r = ui.add(te);
            if r.changed() {
                app.route_ui.text = app.route_ui.text.to_uppercase();
            }
            let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let load = ui.add(egui::Button::new(RichText::new(ph::ARROW_RIGHT).size(15.0).color(egui::Color32::WHITE)).fill(t.accent).min_size(vec2(30.0, 28.0))).on_hover_text("Load route (Enter)").clicked();
            let editing = app.saved.route.editing;
            let add = egui::Button::new(RichText::new(ph::CURSOR_CLICK).size(15.0).color(if editing { egui::Color32::WHITE } else { t.text2 })).fill(if editing { t.accent } else { t.surface2 }).min_size(vec2(30.0, 28.0));
            if ui.add(add).on_hover_text(if editing { "Adding waypoints: click the map. Click here to stop." } else { "Click the map to add waypoints. You can also drag the route line to insert a waypoint, or drag a waypoint to move it." }).clicked() {
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
        super::tour::mark(ui.ctx(), "route-entry", entry.response.rect);
        if app.saved.route.editing {
            widgets::para(ui, format!("{}  Click the map to add waypoints", ph::CURSOR_CLICK), t.accent, 11.5);
        }
        if !app.route_ui.unresolved.is_empty() {
            widgets::para(ui, format!("{}  Not found: {}", ph::WARNING, app.route_ui.unresolved.join(", ")), t.warn, 12.0);
        }
        if !app.route_ui.import_error.is_empty() {
            widgets::para(ui, format!("{}  {}", ph::WARNING, app.route_ui.import_error), t.warn, 12.0);
        }

        // ---- aircraft · altitude · departure ----
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let current = app.fleet.active().map(|a| if a.tail.is_empty() { a.type_code.clone() } else { a.tail.clone() }).unwrap_or_default();
            egui::ComboBox::from_id_salt("route-ac").selected_text(format!("{}  {current}", ph::AIRPLANE_TILT)).width(104.0).show_ui(ui, |ui| {
                for a in &app.fleet.list {
                    ui.selectable_value(&mut app.fleet.active, a.id.clone(), format!("{} · {}", if a.tail.is_empty() { &a.type_code } else { &a.tail }, a.name));
                }
            });
            let alt = ui.add(egui::DragValue::new(&mut app.saved.route.cruise).speed(50.0).range(500.0..=45000.0).custom_formatter(|v, _| format!("{} ft", thousands(v))).custom_parser(|s| s.replace([',', ' '], "").trim_end_matches("ft").parse().ok()));
            alt.on_hover_text("Cruise altitude: drag or type");
            ui.menu_button(RichText::new(ph::CARET_DOWN).size(12.0), |ui| {
                for a in [3500.0, 4500.0, 5500.0, 6500.0, 7500.0, 8500.0, 9500.0, 10500.0, 11500.0, 12500.0] {
                    if ui.button(format!("{} ft", thousands(a))).clicked() {
                        app.saved.route.cruise = a;
                        ui.close();
                    }
                }
            });
            let dep = app.saved.route.departure_time().with_timezone(&chrono::Local);
            let label = if app.saved.route.departure.is_empty() { format!("{}  Now", ph::CLOCK) } else { format!("{}  {}", ph::CLOCK, dep.format("%a %H:%M")) };
            ui.menu_button(label, |ui| {
                ui.set_width(220.0);
                faint(ui, &t, "Departure (local). Or drag the timeline under the chart.");
                let r = ui.add(egui::TextEdit::singleline(&mut app.saved.route.departure).hint_text("YYYY-MM-DD HH:MM").desired_width(200.0));
                if r.lost_focus() {
                    app.saved.route.departure = normalize_time(&app.saved.route.departure);
                }
                ui.horizontal_wrapped(|ui| {
                    if ui.small_button("Now").clicked() {
                        app.saved.route.departure.clear();
                    }
                    for (label, h) in [("+1 h", 1), ("+3 h", 3), ("+1 day", 24)] {
                        if ui.small_button(label).clicked() {
                            let base = app.saved.route.departure_time() + chrono::Duration::hours(h);
                            super::timeline::set_departure(app, base);
                        }
                    }
                });
            });
        });
        if let Some((alt, east)) = suggested(app) {
            if app.saved.route.cruise != alt {
                let tip = format!("{}, clears terrain within 1 NM by 1,000 ft or more", if east { "Eastbound: odd thousands + 500" } else { "Westbound: even thousands + 500" });
                if widgets::link(ui, &t, format!("Use {} ft (suggested VFR altitude)", thousands(alt))).on_hover_text(tip).clicked() {
                    app.saved.route.cruise = alt;
                }
            }
        }

        // ---- waypoints ----
        if app.saved.route.waypoints.is_empty() {
            ui.add_space(8.0);
            widgets::para(ui, "Type a route above and press Enter, search with /, or click the map after pressing the pointer button.", t.text2, 12.5);
        } else {
            section(ui, &t, "Waypoints");
        }
        let n = app.saved.route.waypoints.len();
        let mut op: Option<(&str, usize)> = None;
        let mut fly: Option<(f64, f64)> = None;
        let rows = app.plan.log.as_ref().map(|l| l.rows.clone()).unwrap_or_default();
        let mut start = 0.0;
        for i in 0..n {
            let w = app.saved.route.waypoints[i].clone();
            let row = ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let (r, _) = ui.allocate_exact_size(vec2(12.0, 16.0), egui::Sense::hover());
                let end = i == 0 || i + 1 == n;
                ui.painter().circle(r.center(), if end { 5.0 } else { 3.5 }, if end { t.accent } else { t.surface_solid }, egui::Stroke::new(2.0, t.accent));
                let label = ui.add(egui::Label::new(RichText::new(&w.id).font(theme::bold(13.0))).sense(egui::Sense::click())).on_hover_text("Show on the chart");
                if label.clicked() {
                    fly = Some((w.lon, w.lat));
                }
                if w.fuel_stop {
                    ui.label(RichText::new(format!("{} Fuel stop", ph::GAS_PUMP)).font(theme::semibold(11.0)).color(t.accent));
                }
                let sub = if w.kind == Kind::Airport || w.detail.is_empty() { w.name.clone() } else { w.detail.clone() };
                ui.add(egui::Label::new(RichText::new(sub).size(11.5).color(t.text3)).truncate());
            });
            let can_stop = w.kind == Kind::Airport && i > 0 && i + 1 < n;
            // row actions only on hover, to keep the list quiet
            let hovered = ui.rect_contains_pointer(row.response.rect.expand2(vec2(0.0, 2.0)));
            if hovered {
                let r = row.response.rect;
                let mut x = r.right();
                let pump_tip = if w.fuel_stop { "Not a fuel stop" } else { "Make this a fuel stop: land, top off, go on" };
                for (icon, tip, what, enabled) in [(ph::X, "Remove", "remove", true), (ph::CARET_DOWN, "Move down", "down", i + 1 < n), (ph::CARET_UP, "Move up", "up", i > 0), (ph::GAS_PUMP, pump_tip, "fuel", can_stop)] {
                    if what == "fuel" && !can_stop {
                        continue;
                    }
                    let br = egui::Rect::from_min_size(egui::pos2(x - 20.0, r.top()), vec2(20.0, r.height()));
                    x -= 22.0;
                    let color = if what == "fuel" { t.accent } else { t.text3 };
                    let resp = ui.put(br, egui::Button::new(RichText::new(icon).size(12.0).color(color)).frame(false)).on_hover_text(tip);
                    if enabled && resp.clicked() {
                        op = Some((what, i));
                    }
                }
            }
            if i + 1 < n {
                let b = &app.saved.route.waypoints[i + 1];
                let d = distance_nm(w.lat, w.lon, b.lat, b.lon);
                let mc = bearing(w.lat, w.lon, b.lat, b.lon) - variation_now(w.lat, w.lon);
                let ete: f64 = rows.iter().filter(|r| r.cum_dist - r.dist >= start - 0.01 && r.cum_dist <= start + d + 0.01).map(|r| r.ete).sum();
                start += d;
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(12.0, 16.0), egui::Sense::hover());
                    ui.painter().vline(r.center().x, r.y_range(), egui::Stroke::new(1.5, t.accent.gamma_multiply(0.45)));
                    let txt = if ete > 0.0 { format!("{}°M · {d:.0} NM · {}", deg(mc), fmt_time(ete)) } else { format!("{}°M · {d:.0} NM", deg(mc)) };
                    ui.label(RichText::new(txt).size(11.0).color(t.text3));
                });
            }
        }
        match op {
            Some(("up", i)) => app.saved.route.move_to(i, i - 1),
            Some(("down", i)) => app.saved.route.move_to(i, i + 1),
            Some(("fuel", i)) => app.saved.route.toggle_fuel_stop(i),
            Some(("remove", i)) => {
                let uid = app.saved.route.waypoints[i].uid;
                app.saved.route.remove(uid);
            }
            _ => {}
        }
        if let Some((lon, lat)) = fly {
            app.map.fly_to(lon, lat, 11.0);
        }
        // fuel stops: what each flight lands with, and the time on the ground
        if let Some(log) = app.plan.log.clone().filter(|l| l.segments.len() > 1) {
            ui.add_space(6.0);
            egui::Frame::new().fill(t.accent_soft).corner_radius(egui::CornerRadius::same(9)).inner_margin(egui::Margin::symmetric(10, 8)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                for (k, seg) in log.segments.iter().enumerate() {
                    let from = if k == 0 { app.saved.route.waypoints[0].id.clone() } else { log.segments[k - 1].to.clone() };
                    let low = seg.fuel_at_landing < log.reserve_fuel;
                    ui.label(
                        RichText::new(format!("{from} → {}: {:.0} NM · {} · lands with {:.1} gal", seg.to, seg.end_d - seg.start_d, fmt_time((seg.arrive - seg.depart).num_seconds() as f64 / 60.0), seg.fuel_at_landing))
                            .size(11.5)
                            .color(if low { t.bad } else { t.text2 }),
                    );
                }
                ui.horizontal(|ui| {
                    faint(ui, &t, format!("{}  Top off at each stop, on the ground", ph::GAS_PUMP));
                    ui.add(egui::DragValue::new(&mut app.saved.route.stop_minutes).range(10.0..=240.0).speed(1.0).suffix(" min"));
                });
            });
        } else if app.saved.route.waypoints.len() > 2 && app.plan.log.as_ref().is_some_and(|l| l.fuel_at_landing < l.reserve_fuel) {
            widgets::para(ui, format!("{}  Not enough fuel non-stop? Hover an airport in the list and tap the pump to make it a fuel stop.", ph::GAS_PUMP), t.text2, 11.5);
        }
    });

    // ---- totals ----
    if let Some(log) = app.plan.log.clone() {
        ui.add_space(8.0);
        egui::Frame::new().fill(t.surface2).corner_radius(egui::CornerRadius::same(10)).inner_margin(egui::Margin::symmetric(10, 8)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.columns(4, |c| {
                let stat = |ui: &mut egui::Ui, v: String, unit: &str, bad: bool| {
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new(v).font(theme::bold(15.0)).color(if bad { t.bad } else { t.text }));
                        ui.label(RichText::new(unit).size(10.5).color(t.text3));
                    });
                };
                stat(&mut c[0], format!("{:.0}", log.total_dist), "NM", false);
                stat(&mut c[1], fmt_time(log.total_time), if log.segments.len() > 1 { "time incl. stops" } else { "time" }, false);
                stat(&mut c[2], format!("{:.1}", log.trip_fuel), "gal burn", false);
                stat(&mut c[3], format!("{:.1}", log.fuel_at_landing), "gal left", log.fuel_at_landing < log.reserve_fuel);
            });
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let label = if app.saved.profile_open { "Hide profile" } else { "Profile" };
            if widgets::button(ui, format!("{}  {label}", ph::CHART_LINE)).clicked() {
                app.saved.profile_open = !app.saved.profile_open;
            }
            if widgets::button(ui, format!("{}  Briefing", ph::FILE_TEXT)).clicked() {
                app.briefing = Some(super::briefing::Briefing::new(app));
            }
        });
        let src = match app.plan.wind_source.as_str() {
            "nws-fb" => "NWS winds-aloft forecast (FB)",
            _ => "loading…",
        };
        faint(ui, &t, format!("Winds aloft: {src}"));
    }
    super::notams::route_section(ui, app);
    saved_plans(app, ui);
    if app.plan.loading {
        faint(ui, &t, "Updating the profile…");
    }
    if !app.plan.error.is_empty() {
        widgets::para(ui, &app.plan.error, t.bad, 12.0);
    }
}

/// The flight plans saved in the app: load or delete.
fn saved_plans(app: &mut App, ui: &mut egui::Ui) {
    let t = app.t;
    if app.plans.is_empty() {
        return;
    }
    let mut load: Option<String> = None;
    let mut delete: Option<String> = None;
    let open_default = app.saved.route.waypoints.is_empty();
    widgets::fold(ui, &t, "route-saved-plans", &format!("Saved plans ({})", app.plans.len()), open_default, |ui| {
        for p in &app.plans {
            egui::Frame::new().fill(t.surface2).corner_radius(egui::CornerRadius::same(9)).inner_margin(egui::Margin::symmetric(10, 7)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(ui.available_width() - 80.0);
                        ui.label(RichText::new(&p.name).font(theme::semibold(12.5)));
                        ui.add(egui::Label::new(RichText::new(&p.route_text).font(theme::mono(11.0)).color(t.text2)).truncate());
                        let mut meta = Vec::new();
                        if p.distance_nm > 0.0 {
                            meta.push(format!("{:.0} NM · {}", p.distance_nm, fmt_time(p.ete_min)));
                        }
                        if !p.fuel_stops.is_empty() {
                            meta.push(format!("{} {}", ph::GAS_PUMP, p.fuel_stops.join(", ")));
                        }
                        if !p.aircraft.is_empty() {
                            meta.push(p.aircraft.clone());
                        }
                        if let Ok(d) = chrono::DateTime::parse_from_rfc3339(&p.saved_at) {
                            meta.push(format!("saved {}", d.format("%b %-d %H:%M")));
                        }
                        ui.add(egui::Label::new(RichText::new(meta.join(" · ")).size(10.5).color(t.text3)).wrap());
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(egui::Button::new(RichText::new(ph::TRASH).size(13.0).color(t.text3)).frame(false)).on_hover_text("Delete").clicked() {
                            delete = Some(p.id.clone());
                        }
                        if ui.add_enabled(!app.saved.locked, egui::Button::new(RichText::new("Load").size(12.0)).min_size(egui::vec2(0.0, 26.0))).on_disabled_hover_text("View only: press L to edit").clicked() {
                            load = Some(p.id.clone());
                        }
                    });
                });
            });
            ui.add_space(4.0);
        }
    });
    if let Some(id) = load {
        app.load_plan(&id);
    }
    if let Some(id) = delete {
        app.delete_plan(&id);
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
