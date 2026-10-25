//! Full-page briefing: summary, hazards, a chart snapshot of the route, navlog, winds aloft,
//! weather, NOTAMs, weight & balance and the end airports. Saves as a kneeboard-friendly PDF
//! (navlog with blank ATA / fuel columns) and prints through the system PDF viewer.

use super::widgets::{self, faint, section};
use crate::app::App;
use crate::backend::Task;
use crate::io::pdf::{Align, Pdf, C, MARGIN, PAGE_W};
use crate::model::aircraft::compute_wb;
use crate::model::place::Kind;
use crate::nav::navlog::{deg, fmt_time, thousands, wind_at, Phase};
use crate::theme;
use crate::wx::decode::{decode_metar, decode_taf, str_of};
use chrono::{DateTime, Duration, Local, Utc};
use egui::{vec2, CornerRadius, Frame, Margin, RichText};
use egui_phosphor::regular as ph;
use serde_json::Value;
use std::time::Instant;

enum Snap {
    Fit(u8),
    Wait(Instant),
    Requested(Instant),
    Done,
}

pub struct Briefing {
    snap: Snap,
    camera: ([f64; 2], f64),
    hid_profile: bool,
    pub image: Option<(egui::ColorImage, egui::TextureHandle)>,
    airports: Vec<Value>,
    notams: Task<Value>,
    notam_items: Option<Value>,
    status: String,
}

impl Briefing {
    pub fn new(app: &mut App) -> Self {
        let route = &app.saved.route;
        let airports: Vec<Value> = [route.waypoints.first(), route.waypoints.last()]
            .into_iter()
            .flatten()
            .filter(|w| w.kind == Kind::Airport)
            .filter_map(|w| app.be.server.try_data().ok()?.db.airport(&w.id).ok().flatten())
            .collect();
        let ids: Vec<String> = route.waypoints.iter().filter(|w| w.kind == Kind::Airport).map(|w| w.wx_id()).collect();
        app.route_wx.update(ids.clone(), &app.be, true);
        let mut notams = Task::default();
        if app.keys.notams && route.waypoints.len() >= 2 {
            // airports by id, the route by points every ~25 NM, and the ARTCCs crossed
            let mut points = Vec::new();
            let mut last = -100.0;
            for s in &app.plan.profile.samples {
                if s.d - last >= 25.0 {
                    points.push([s.lon, s.lat]);
                    last = s.d;
                }
            }
            let artccs: Vec<String> = airports.iter().filter_map(|a| a["artcc"].as_str().map(String::from)).filter(|s| !s.is_empty()).collect();
            let st = app.be.server.clone();
            notams = app.be.spawn(async move { flightplan_server::notam::fetch(st, flightplan_server::notam::NotamReq { locations: ids, points, radius: Some(25.0), artccs }).await });
        }
        let snapshot = route.waypoints.len() >= 2;
        let hid_profile = app.saved.profile_open;
        if snapshot {
            app.saved.profile_open = false;
            app.selection = None;
        }
        Briefing {
            snap: if snapshot { Snap::Fit(0) } else { Snap::Done },
            camera: (app.map.center, app.map.zoom),
            hid_profile,
            image: None,
            airports,
            notams,
            notam_items: None,
            status: String::new(),
        }
    }

    /// True while the chart is being framed and captured (the rest of the UI hides).
    pub fn capturing(&self) -> bool {
        !matches!(self.snap, Snap::Done)
    }
}

fn hhmm(d: DateTime<Utc>) -> String {
    d.with_timezone(&Local).format("%H:%M").to_string()
}
fn zz(d: DateTime<Utc>) -> String {
    d.format("%H%MZ").to_string()
}

fn snapshot(app: &mut App, ctx: &egui::Context, b: &mut Briefing) {
    match b.snap {
        Snap::Fit(n) => {
            if n == 0 {
                let w = &app.saved.route.waypoints;
                let (mut west, mut south, mut east, mut north) = (180.0f64, 90.0f64, -180.0f64, -90.0f64);
                for p in w {
                    west = west.min(p.lon);
                    east = east.max(p.lon);
                    south = south.min(p.lat);
                    north = north.max(p.lat);
                }
                app.map.fit(ctx.content_rect(), west, south, east, north, [70.0, 70.0, 70.0, 70.0], 10.0, false);
            }
            b.snap = if n >= 2 { Snap::Wait(Instant::now()) } else { Snap::Fit(n + 1) };
            ctx.request_repaint();
        }
        Snap::Wait(at) => {
            let settled = !app.tiles.busy() && at.elapsed().as_millis() > 700;
            if settled || at.elapsed().as_secs() > 5 {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
                b.snap = Snap::Requested(Instant::now());
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        Snap::Requested(at) => {
            let shot = ctx.input(|i| i.events.iter().find_map(|e| if let egui::Event::Screenshot { image, .. } = e { Some(image.clone()) } else { None }));
            if let Some(img) = shot {
                let tex = ctx.load_texture("briefing-map", (*img).clone(), egui::TextureOptions::LINEAR);
                b.image = Some(((*img).clone(), tex));
            }
            if b.image.is_some() || at.elapsed().as_secs() > 3 {
                b.snap = Snap::Done;
                app.map.center = b.camera.0;
                app.map.zoom = b.camera.1;
                app.saved.profile_open = b.hid_profile;
            }
            ctx.request_repaint();
        }
        Snap::Done => {}
    }
}

/// Draw the briefing; returns it while it stays open.
pub fn show(app: &mut App, ctx: &egui::Context, mut b: Briefing) -> Option<Briefing> {
    if b.capturing() {
        snapshot(app, ctx, &mut b);
        return Some(b);
    }
    if let Some(v) = b.notams.poll() {
        b.notam_items = Some(v);
    }
    let ids: Vec<String> = app.saved.route.waypoints.iter().filter(|w| w.kind == Kind::Airport).map(|w| w.wx_id()).collect();
    app.route_wx.update(ids, &app.be, false);
    let t = app.t;
    let screen = ctx.content_rect();
    let mut close = false;
    let mut save: Option<bool> = None;
    egui::Area::new("briefing".into()).fixed_pos(screen.min).order(egui::Order::Foreground).show(ctx, |ui| {
        ui.set_min_size(screen.size());
        ui.painter().rect_filled(screen, 0.0, t.bg);
        Frame::new().fill(t.surface_solid).inner_margin(Margin::symmetric(20, 10)).show(ui, |ui| {
            ui.set_width(screen.width() - 40.0);
            ui.horizontal(|ui| {
                if widgets::button(ui, format!("{}  Back to chart", ph::X)).clicked() {
                    close = true;
                }
                if !b.status.is_empty() {
                    faint(ui, &t, &b.status);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::primary(ui, &t, format!("{}  Print", ph::PRINTER)).clicked() {
                        save = Some(true);
                    }
                    if widgets::button(ui, format!("{}  Save PDF…", ph::FILE_PDF)).clicked() {
                        save = Some(false);
                    }
                    if widgets::button(ui, format!("{}  Official briefing (1800wxbrief)", ph::ARROW_SQUARE_OUT)).clicked() {
                        let _ = open::that("https://www.1800wxbrief.com/");
                    }
                });
            });
        });
        let content_w = 1080f32.min(screen.width() - 48.0);
        egui::ScrollArea::vertical().id_salt("brief-scroll").show(ui, |ui| {
            ui.add_space(16.0);
            ui.horizontal(|ui| {
                ui.add_space((screen.width() - content_w) / 2.0);
                ui.vertical(|ui| {
                    ui.set_width(content_w);
                    body(app, ui, &b);
                });
            });
            ui.add_space(60.0);
        });
    });
    if let Some(print) = save {
        match build_pdf(app, &b) {
            Ok(bytes) => {
                let name = format!("briefing-{}.pdf", super::aircraft::slug(&app.saved.route.text()));
                let path = if print {
                    Some(std::env::temp_dir().join(&name))
                } else {
                    rfd::FileDialog::new().set_title("Save briefing").set_file_name(&name).add_filter("PDF", &["pdf"]).save_file()
                };
                if let Some(p) = path {
                    match std::fs::write(&p, bytes) {
                        Ok(()) => {
                            if print {
                                let _ = open::that(&p);
                                b.status = "Opened in your PDF viewer: print from there.".into();
                            } else {
                                b.status = format!("Saved {}", p.display());
                            }
                        }
                        Err(e) => b.status = format!("Couldn't save: {e}"),
                    }
                }
            }
            Err(e) => b.status = format!("Couldn't build the PDF: {e:#}"),
        }
    }
    if close || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        return None;
    }
    Some(b)
}

fn body(app: &mut App, ui: &mut egui::Ui, b: &Briefing) {
    let t = app.t;
    let (Some(log), Some(ac)) = (app.plan.log.clone(), app.fleet.active().cloned()) else {
        ui.add_space(60.0);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new("No route yet").font(theme::bold(20.0)));
            widgets::muted(ui, &t, "Build a route with at least two waypoints to get a navlog and briefing.");
        });
        return;
    };
    let dep = app.saved.route.departure_time();
    let eta = dep + Duration::milliseconds((log.total_time * 60_000.0) as i64);
    faint(ui, &t, format!("FLIGHT BRIEFING · generated {}", Local::now().format("%a %b %-d %Y %H:%M")));
    ui.label(RichText::new(app.saved.route.text()).font(theme::mono(22.0)));
    widgets::muted(ui, &t, format!("{} · {} · cruise {} ft · TAS {} kt", ac.tail, ac.name, thousands(app.saved.route.cruise), ac.perf.tas));
    ui.add_space(10.0);
    ui.columns(6, |c| {
        let kpi = |ui: &mut egui::Ui, label: &str, v: String, sub: String, bad: bool| {
            Frame::new().fill(t.surface_solid).stroke(egui::Stroke::new(1.0, t.line)).corner_radius(CornerRadius::same(10)).inner_margin(Margin::same(10)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(label.to_uppercase()).size(10.5).color(t.text3));
                ui.label(RichText::new(v).font(theme::bold(19.0)).color(if bad { t.bad } else { t.text }));
                ui.label(RichText::new(sub).size(11.0).color(t.text2));
            });
        };
        kpi(&mut c[0], "Departs", hhmm(dep), zz(dep), false);
        kpi(&mut c[1], "Arrives", hhmm(eta), zz(eta), false);
        kpi(&mut c[2], "Distance", format!("{:.0}", log.total_dist), "NM".into(), false);
        kpi(&mut c[3], "Time", fmt_time(log.total_time), "ETE".into(), false);
        kpi(&mut c[4], "Fuel", format!("{:.1}", log.trip_fuel + log.taxi_fuel), "gal incl. taxi".into(), false);
        kpi(&mut c[5], "At landing", format!("{:.1}", log.fuel_at_landing), format!("gal ({:.1} reserve)", log.reserve_fuel), log.fuel_at_landing < log.reserve_fuel);
    });
    section(ui, &t, "Go / no-go");
    match app.plan.verdict.clone() {
        Some(v) => super::verdict::card(ui, app, &v, "briefing-verdict"),
        None => {
            faint(ui, &t, "Still checking weather and terrain…");
        }
    }
    if let Some((_, tex)) = &b.image {
        ui.add_space(10.0);
        let size = tex.size_vec2();
        let w = ui.available_width();
        ui.add(egui::Image::new((tex.id(), vec2(w, w * size.y / size.x))).corner_radius(10));
    }
    section(ui, &t, "Navigation log");
    egui::ScrollArea::horizontal().id_salt("navlog-h").show(ui, |ui| {
        egui::Grid::new("navlog").striped(true).spacing(vec2(12.0, 4.0)).show(ui, |ui| {
            for h in ["From → To", "Alt", "TC", "Wind", "WCA", "TH", "Var", "MH", "TAS", "GS", "Dist", "ETE", "ETA", "Fuel", "Rem", "ATA", "Act fuel"] {
                ui.label(RichText::new(h).size(11.0).color(t.text3));
            }
            ui.end_row();
            for r in &log.rows {
                let phase = if r.phase == Phase::Cruise { String::new() } else { format!("  {}", r.phase.label()) };
                ui.label(RichText::new(format!("{} → {}{phase}", r.from, r.to)).font(theme::semibold(12.0)));
                for cell in [
                    thousands(r.alt),
                    deg(r.tc),
                    r.wind.map(|w| format!("{}/{}", deg(w.dir), w.spd)).unwrap_or_else(|| "—".into()),
                    format!("{}{:.0}", if r.wca >= 0.0 { "+" } else { "" }, r.wca),
                    deg(r.th),
                    format!("{:.0}{}", r.variation.abs(), if r.variation >= 0.0 { "E" } else { "W" }),
                ] {
                    ui.label(RichText::new(cell).size(12.0));
                }
                ui.label(RichText::new(deg(r.mh)).font(theme::bold(12.0)));
                ui.label(RichText::new(format!("{}", r.tas)).size(12.0));
                ui.label(RichText::new(format!("{}", r.gs.round())).font(theme::bold(12.0)));
                for cell in [format!("{:.1}", r.dist), fmt_time(r.ete), hhmm(r.eta), format!("{:.1}", r.fuel), format!("{:.1}", r.fuel_rem), "____".into(), "____".into()] {
                    ui.label(RichText::new(cell).size(12.0));
                }
                ui.end_row();
            }
            ui.label(RichText::new("Totals").font(theme::semibold(12.0)));
            for _ in 0..9 {
                ui.label("");
            }
            for cell in [format!("{:.1}", log.total_dist), fmt_time(log.total_time), hhmm(eta), format!("{:.1}", log.trip_fuel), format!("{:.1}", log.fuel_at_landing)] {
                ui.label(RichText::new(cell).font(theme::semibold(12.0)));
            }
            ui.end_row();
        });
    });
    let src = if app.plan.wind_source == "windy" { "Windy GFS" } else { "Open-Meteo GFS" };
    widgets::para(ui, format!("Courses true unless marked. Variation WMM-2025 (east positive, MH = TH − E var). Winds: {src}, interpolated to altitude at each leg's ETA. Taxi {} gal not included in the leg fuel.", log.taxi_fuel), t.text3, 11.0);

    section(ui, &t, "Winds and temperatures aloft");
    let alts = wind_alts(app.saved.route.cruise);
    egui::Grid::new("winds").striped(true).spacing(vec2(16.0, 4.0)).show(ui, |ui| {
        ui.label(RichText::new("Waypoint").size(11.0).color(t.text3));
        for a in &alts {
            ui.label(RichText::new(thousands(*a)).size(11.0).color(t.text3));
        }
        ui.end_row();
        for (i, w) in app.saved.route.waypoints.iter().enumerate() {
            ui.label(RichText::new(&w.id).font(theme::semibold(12.0)));
            for a in &alts {
                let wd = wind_at(app.plan.winds.get(i).and_then(|x| x.as_ref()), *a, dep);
                ui.label(RichText::new(wd.map(|w| format!("{}/{}{}", deg(w.dir), w.spd, w.temp.map(|t| format!("  {:+.0}°", t)).unwrap_or_default())).unwrap_or_else(|| "—".into())).size(12.0));
            }
            ui.end_row();
        }
    });

    section(ui, &t, "Weather");
    let list = app.route_wx.list.clone();
    for (id, m, tf) in &list {
        ui.horizontal(|ui| {
            if widgets::link(ui, &t, format!("NOTAMs {}", ph::ARROW_SQUARE_OUT)).clicked() {
                let _ = open::that(format!("https://notams.aim.faa.gov/notamSearch/nsapp.html#/results?designators={id}"));
            }
        });
        super::weather::station(ui, app, id, m.as_ref(), tf.as_ref());
        widgets::rule(ui, &t);
    }

    section(ui, &t, "NOTAMs");
    match &b.notam_items {
        Some(v) if v["configured"].as_bool() == Some(true) => {
            let items = v["items"].as_array().cloned().unwrap_or_default();
            faint(ui, &t, format!("{} NOTAMs for the route's airports, the route corridor and ARTCCs (FAA NOTAM API).", items.len()));
            for n in items.iter().take(150) {
                ui.horizontal_top(|ui| {
                    widgets::chip(ui, &t, &format!("{} · {}", str_of(&n["location"]), str_of(&n["category"])), None, None);
                    ui.add(egui::Label::new(RichText::new(str_of(&n["text"])).font(theme::mono(11.0))).wrap());
                });
            }
        }
        _ if app.keys.notams => {
            faint(ui, &t, "Loading NOTAMs…");
        }
        _ => {
            widgets::para(ui, "Add FAA NOTAM API credentials in Settings to list NOTAMs here. Meanwhile, use the NOTAM links above or 1800wxbrief.", t.text2, 12.0);
        }
    }

    ui.add_space(8.0);
    ui.columns(2, |c| {
        let ui = &mut c[0];
        section(ui, &t, "Weight & balance");
        let wb = compute_wb(&ac, log.trip_fuel);
        egui::Grid::new("wbt").striped(true).spacing(vec2(14.0, 4.0)).show(ui, |ui| {
            for h in ["Item", "Weight", "Arm", "Moment"] {
                ui.label(RichText::new(h).size(11.0).color(t.text3));
            }
            ui.end_row();
            for r in &wb.rows {
                ui.label(RichText::new(&r.name).size(12.0));
                ui.label(format!("{}", r.weight.round()));
                ui.label(format!("{:.1}", r.arm));
                ui.label(thousands(r.moment));
                ui.end_row();
            }
            for (label, p) in [("Takeoff", &wb.takeoff), ("Landing", &wb.landing)] {
                ui.label(RichText::new(label).font(theme::semibold(12.0)));
                ui.label(RichText::new(format!("{}", p.weight.round())).font(theme::semibold(12.0)));
                ui.label(RichText::new(format!("{:.2}", p.cg)).font(theme::semibold(12.0)));
                ui.label("");
                ui.end_row();
            }
        });
        for p in &wb.problems {
            widgets::para(ui, p, t.bad, 12.0);
        }
        super::wbchart::show(&mut c[1], &t, &ac, &wb, 240.0);
    });
    ui.add_space(8.0);
    ui.columns(b.airports.len().max(1), |c| {
        for (i, a) in b.airports.iter().enumerate() {
            let ui = &mut c[i];
            let code = if str_of(&a["icao"]).is_empty() { str_of(&a["id"]) } else { str_of(&a["icao"]) };
            section(ui, &t, &format!("{}: {code}", if i == 0 { "Departure" } else { "Destination" }));
            widgets::muted(ui, &t, format!("{} · elev {} ft", str_of(&a["name"]), thousands(a["elev"].as_f64().unwrap_or(0.0))));
            egui::Grid::new(("apt", i)).spacing(vec2(12.0, 3.0)).show(ui, |ui| {
                for (k, v) in key_freqs(a) {
                    ui.label(RichText::new(k).size(12.0).color(t.text3));
                    ui.label(RichText::new(v).font(theme::semibold(12.0)));
                    ui.end_row();
                }
                for r in a["runways"].as_array().into_iter().flatten() {
                    ui.label(RichText::new(format!("RWY {}", str_of(&r["id"]))).size(12.0).color(t.text3));
                    ui.label(format!("{} × {} {}", thousands(r["len"].as_f64().unwrap_or(0.0)), r["width"].as_f64().unwrap_or(0.0), str_of(&r["surface"])));
                    ui.end_row();
                }
            });
        }
    });
    ui.add_space(12.0);
    widgets::para(ui, "For planning only. Not a substitute for an official weather briefing, current charts, NOTAMs or the aircraft's POH. The pilot in command is responsible for the flight.", t.text3, 11.5);
}

fn wind_alts(cruise: f64) -> Vec<f64> {
    let mut v: Vec<f64> = [3000.0, 6000.0, 9000.0, 12000.0, (cruise / 1000.0).round() * 1000.0].into_iter().filter(|a| *a > 0.0).collect();
    v.sort_by(f64::total_cmp);
    v.dedup();
    v
}

fn key_freqs(a: &Value) -> Vec<(String, String)> {
    let want = ["ATIS", "D-ATIS", "CTAF", "UNICOM", "CD/P", "GND/P", "LCL/P", "APCH/P DEP/P", "APCH/P", "DEP/P"];
    a["freqs"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|f| want.contains(&str_of(&f["use"]).as_str()) && str_of(&f["freq"]).parse::<f64>().is_ok_and(|v| v < 137.0))
        .map(|f| {
            let u = str_of(&f["use"]);
            let label = match u.as_str() {
                "LCL/P" => "Tower",
                "GND/P" => "Ground",
                "CD/P" => "Clearance",
                "APCH/P DEP/P" => "App/Dep",
                "APCH/P" => "Approach",
                "DEP/P" => "Departure",
                x => x,
            };
            (label.to_string(), str_of(&f["freq"]))
        })
        .collect()
}

pub fn build_pdf(app: &App, b: &Briefing) -> anyhow::Result<Vec<u8>> {
    let (Some(log), Some(ac)) = (app.plan.log.as_ref(), app.fleet.active()) else { anyhow::bail!("build a route first") };
    let mut p = Pdf::new(&format!("Briefing {}", app.saved.route.text()))?;
    let dep = app.saved.route.departure_time();
    let eta = dep + Duration::milliseconds((log.total_time * 60_000.0) as i64);
    let full_w = PAGE_W - 2.0 * MARGIN;
    p.para(&format!("FLIGHT BRIEFING · generated {}", Local::now().format("%a %b %-d %Y %H:%M")), 8.0, false, C::FAINT);
    p.para(&app.saved.route.text(), 16.0, true, C::TEXT);
    p.para(&format!("{} · {} · cruise {} ft · TAS {} kt", ac.tail, ac.name, thousands(app.saved.route.cruise), ac.perf.tas), 9.5, false, C::MUTED);
    p.y += 2.0;
    // KPI boxes
    let kpis = [
        ("DEPARTS", hhmm(dep), zz(dep)),
        ("ARRIVES", hhmm(eta), zz(eta)),
        ("DISTANCE", format!("{:.0}", log.total_dist), "NM".into()),
        ("TIME", fmt_time(log.total_time), "ETE".into()),
        ("FUEL", format!("{:.1}", log.trip_fuel + log.taxi_fuel), "gal incl. taxi".into()),
        ("AT LANDING", format!("{:.1}", log.fuel_at_landing), format!("gal ({:.1} reserve)", log.reserve_fuel)),
    ];
    let bw = (full_w - 5.0 * 2.0) / 6.0;
    p.ensure(18.0);
    let y0 = p.y;
    for (i, (k, v, s)) in kpis.iter().enumerate() {
        let x = MARGIN + i as f32 * (bw + 2.0);
        p.rect(x, y0, bw, 16.0, None, Some((C::LINE, 0.6)));
        p.text(x + 2.5, y0 + 2.0, k, 6.5, false, C::FAINT);
        let bad = i == 5 && log.fuel_at_landing < log.reserve_fuel;
        p.text(x + 2.5, y0 + 5.5, v, 13.0, true, if bad { C::BAD } else { C::TEXT });
        p.text(x + 2.5, y0 + 11.5, s, 7.0, false, C::MUTED);
    }
    p.y = y0 + 19.0;

    p.heading("Go / no-go");
    match &app.plan.verdict {
        None => p.para("The go / no-go check hadn't finished when this was generated.", 9.0, false, C::MUTED),
        Some(v) => {
            let c = match v.call {
                crate::nav::verdict::Call::Go => C::OK,
                crate::nav::verdict::Call::Caution => C::WARN,
                crate::nav::verdict::Call::NoGo => C::BAD,
            };
            p.ensure(10.0);
            let y = p.y;
            let word = v.call.word();
            let bw = p.width(word, 10.0, true) + 6.0;
            p.rect(MARGIN, y, bw, 6.0, Some(c), None);
            p.text(MARGIN + 3.0, y + 1.0, word, 10.0, true, C::WHITE);
            p.text(MARGIN + bw + 3.0, y + 1.2, &v.headline, 9.5, true, C::TEXT);
            p.y = y + 8.0;
            for f in v.problems() {
                let fc = if f.call == crate::nav::verdict::Call::NoGo { C::BAD } else { C::WARN };
                p.ensure(8.0);
                let y = p.y;
                p.rect(MARGIN, y + 0.3, 1.0, 3.6, Some(fc), None);
                let at = f.at.filter(|d| *d > 0.5 && !f.title.contains(" NM")).map(|d| format!(" · {d:.0} NM")).unwrap_or_default();
                p.para_at(MARGIN + 3.0, full_w - 3.0, &format!("{}{at}  ({})", f.title, f.category), 9.0, true, C::TEXT);
                if !f.detail.is_empty() {
                    p.para_at(MARGIN + 3.0, full_w - 3.0, &f.detail, 8.5, false, C::MUTED);
                }
                p.y += 1.0;
            }
            let checks: Vec<String> = v.checks().map(|f| if f.detail.is_empty() { f.title.clone() } else { format!("{}: {}", f.title, f.detail) }).collect();
            if !checks.is_empty() {
                p.para(&format!("Checked: {}.", checks.join("; ")), 8.0, false, C::MUTED);
            }
            p.para("Advisory only. The pilot in command makes the go / no-go decision.", 7.5, false, C::FAINT);
        }
    }
    if let Some((img, _)) = &b.image {
        // downscale the capture to at most 1800 px wide
        let (w, h) = (img.size[0], img.size[1]);
        let k = (w as f32 / 1800.0).max(1.0);
        let (nw, nh) = ((w as f32 / k) as usize, (h as f32 / k) as usize);
        let mut rgb = Vec::with_capacity(nw * nh * 3);
        for yy in 0..nh {
            for xx in 0..nw {
                let px = img.pixels[((yy as f32 * k) as usize).min(h - 1) * w + ((xx as f32 * k) as usize).min(w - 1)];
                rgb.extend([px.r(), px.g(), px.b()]);
            }
        }
        p.y += 2.0;
        p.image(rgb, nw, nh, full_w);
    }

    p.page_break();
    p.heading("Navigation log");
    let cols = [
        ("From → To", 30.0, Align::Left),
        ("Alt", 11.0, Align::Right),
        ("TC", 8.0, Align::Right),
        ("Wind", 13.0, Align::Right),
        ("WCA", 8.0, Align::Right),
        ("TH", 8.0, Align::Right),
        ("Var", 8.0, Align::Right),
        ("MH", 8.0, Align::Right),
        ("TAS", 8.0, Align::Right),
        ("GS", 8.0, Align::Right),
        ("Dist", 9.0, Align::Right),
        ("ETE", 9.0, Align::Right),
        ("ETA", 10.0, Align::Right),
        ("Fuel", 8.5, Align::Right),
        ("Rem", 8.5, Align::Right),
        ("ATA", 12.0, Align::Right),
        ("Act fuel", 12.0, Align::Right),
    ];
    let mut rows: Vec<Vec<String>> = log
        .rows
        .iter()
        .map(|r| {
            vec![
                format!("{} → {}{}", r.from, r.to, if r.phase == Phase::Cruise { String::new() } else { format!(" ({})", r.phase.label()) }),
                thousands(r.alt),
                deg(r.tc),
                r.wind.map(|w| format!("{}/{}", deg(w.dir), w.spd)).unwrap_or_else(|| "—".into()),
                format!("{}{:.0}", if r.wca >= 0.0 { "+" } else { "" }, r.wca),
                deg(r.th),
                format!("{:.0}{}", r.variation.abs(), if r.variation >= 0.0 { "E" } else { "W" }),
                deg(r.mh),
                format!("{}", r.tas),
                format!("{}", r.gs.round()),
                format!("{:.1}", r.dist),
                fmt_time(r.ete),
                hhmm(r.eta),
                format!("{:.1}", r.fuel),
                format!("{:.1}", r.fuel_rem),
                String::new(),
                String::new(),
            ]
        })
        .collect();
    rows.push(vec!["Totals".into(), "".into(), "".into(), "".into(), "".into(), "".into(), "".into(), "".into(), "".into(), "".into(), format!("{:.1}", log.total_dist), fmt_time(log.total_time), hhmm(eta), format!("{:.1}", log.trip_fuel), format!("{:.1}", log.fuel_at_landing), "".into(), "".into()]);
    p.table(&cols, &rows, 7.5, true);
    p.para(&format!("Courses true unless marked. Variation WMM-2025 (east positive, MH = TH − E var). Taxi {} gal not included in the leg fuel.", log.taxi_fuel), 7.5, false, C::FAINT);

    p.heading("Winds and temperatures aloft");
    let alts = wind_alts(app.saved.route.cruise);
    let mut wcols: Vec<(String, f32, Align)> = vec![("Waypoint".into(), 26.0, Align::Left)];
    for a in &alts {
        wcols.push((thousands(*a), 24.0, Align::Right));
    }
    let wcols_ref: Vec<(&str, f32, Align)> = wcols.iter().map(|(a, b, c)| (a.as_str(), *b, *c)).collect();
    let wrows: Vec<Vec<String>> = app
        .saved
        .route
        .waypoints
        .iter()
        .enumerate()
        .map(|(i, w)| {
            let mut r = vec![w.id.clone()];
            for a in &alts {
                let wd = wind_at(app.plan.winds.get(i).and_then(|x| x.as_ref()), *a, dep);
                r.push(wd.map(|w| format!("{}/{}{}", deg(w.dir), w.spd, w.temp.map(|t| format!(" {:+.0}°", t)).unwrap_or_default())).unwrap_or_else(|| "—".into()));
            }
            r
        })
        .collect();
    p.table(&wcols_ref, &wrows, 8.0, true);

    p.heading("Weather");
    for (id, m, tf) in &app.route_wx.list {
        p.ensure(12.0);
        let cat = m.as_ref().map(|m| str_of(&m["fltCat"])).unwrap_or_default();
        p.para(&format!("{id}  {cat}"), 10.0, true, C::from(theme::flt_cat(&cat)));
        match m {
            Some(m) => {
                p.para(&str_of(&m["rawOb"]), 7.5, false, C::FAINT);
                for l in decode_metar(&str_of(&m["rawOb"])) {
                    p.para_at(MARGIN + 3.0, full_w - 3.0, &format!("• {l}"), 8.5, false, C::TEXT);
                }
            }
            None => p.para("No METAR", 8.5, false, C::FAINT),
        }
        if let Some(tf) = tf {
            for per in decode_taf(&str_of(&tf["rawTAF"])) {
                p.para_at(MARGIN + 3.0, full_w - 3.0, &per.title, 8.5, true, C::TEXT);
                for l in per.lines {
                    p.para_at(MARGIN + 6.0, full_w - 6.0, &format!("• {l}"), 8.0, false, C::MUTED);
                }
            }
        }
        p.y += 2.0;
    }
    if let Some(v) = &b.notam_items {
        if v["configured"].as_bool() == Some(true) {
            p.heading("NOTAMs");
            for n in v["items"].as_array().into_iter().flatten().take(150) {
                p.para(&format!("{} · {} · {}", str_of(&n["location"]), str_of(&n["category"]), str_of(&n["number"])), 8.0, true, C::TEXT);
                p.para_at(MARGIN + 3.0, full_w - 3.0, &str_of(&n["text"]), 7.5, false, C::MUTED);
            }
        }
    }

    p.heading("Weight & balance");
    let wb = compute_wb(ac, log.trip_fuel);
    let mut wbrows: Vec<Vec<String>> = wb.rows.iter().map(|r| vec![r.name.clone(), format!("{}", r.weight.round()), format!("{:.1}", r.arm), thousands(r.moment)]).collect();
    wbrows.push(vec!["Takeoff".into(), format!("{}", wb.takeoff.weight.round()), format!("{:.2}", wb.takeoff.cg), String::new()]);
    wbrows.push(vec!["Landing".into(), format!("{}", wb.landing.weight.round()), format!("{:.2}", wb.landing.cg), String::new()]);
    p.table(&[("Item", 70.0, Align::Left), ("Weight", 25.0, Align::Right), ("Arm", 25.0, Align::Right), ("Moment", 30.0, Align::Right)], &wbrows, 8.5, false);
    for pr in &wb.problems {
        p.para(pr, 9.0, true, C::BAD);
    }
    // envelope chart (vector)
    let (cw, ch) = (110.0f32, 62.0f32);
    p.ensure(ch + 6.0);
    let (ox, oy) = (MARGIN + 12.0, p.y);
    let env = &ac.wb.envelope;
    let mut arms: Vec<f64> = env.iter().map(|q| q[0]).chain([wb.takeoff.cg, wb.landing.cg]).collect();
    let mut wts: Vec<f64> = env.iter().map(|q| q[1]).chain([wb.takeoff.weight, wb.landing.weight]).collect();
    arms.retain(|v| *v > 0.0);
    wts.retain(|v| *v > 0.0);
    if !arms.is_empty() && !wts.is_empty() {
        let x0 = arms.iter().cloned().fold(f64::MAX, f64::min).floor() - 1.0;
        let x1 = arms.iter().cloned().fold(f64::MIN, f64::max).ceil() + 1.0;
        let y0 = (wts.iter().cloned().fold(f64::MAX, f64::min) / 100.0).floor() * 100.0 - 100.0;
        let y1 = (wts.iter().cloned().fold(f64::MIN, f64::max) / 100.0).ceil() * 100.0 + 100.0;
        let fx = |a: f64| ox + ((a - x0) / (x1 - x0)) as f32 * cw;
        let fy = |w: f64| oy + (1.0 - ((w - y0) / (y1 - y0)) as f32) * ch;
        p.rect(ox, oy, cw, ch, None, Some((C::LINE, 0.5)));
        let mut tick = y0;
        while tick <= y1 {
            p.text(ox - 10.0, fy(tick) - 1.2, &format!("{}", tick as i64), 6.0, false, C::FAINT);
            tick += 200.0;
        }
        let ok = !wb.problems.iter().any(|q| q.contains("envelope"));
        let pts: Vec<(f32, f32)> = env.iter().map(|q| (fx(q[0]), fy(q[1]))).collect();
        p.polygon(&pts, None, Some((if ok { C::OK } else { C::BAD }, 1.2)));
        p.line(&[(fx(wb.takeoff.cg), fy(wb.takeoff.weight)), (fx(wb.landing.cg), fy(wb.landing.weight))], C::ACCENT, 1.0, Some([2.0, 1.5]));
        for (q, l) in [(&wb.takeoff, "TO"), (&wb.landing, "LDG")] {
            p.circle(fx(q.cg), fy(q.weight), 1.0, C::ACCENT);
            p.text(fx(q.cg) + 1.8, fy(q.weight) - 1.4, l, 6.5, true, C::MUTED);
        }
        p.text(ox, oy + ch + 1.5, "CG (inches aft of datum) vs weight (lb)", 6.5, false, C::FAINT);
    }
    p.y = oy + ch + 7.0;

    for (i, a) in b.airports.iter().enumerate() {
        p.heading(&format!("{}: {}", if i == 0 { "Departure" } else { "Destination" }, if str_of(&a["icao"]).is_empty() { str_of(&a["id"]) } else { str_of(&a["icao"]) }));
        p.para(&format!("{} · elev {} ft", str_of(&a["name"]), thousands(a["elev"].as_f64().unwrap_or(0.0))), 8.5, false, C::MUTED);
        let mut rows: Vec<Vec<String>> = key_freqs(a).into_iter().map(|(k, v)| vec![k, v]).collect();
        for r in a["runways"].as_array().into_iter().flatten() {
            rows.push(vec![format!("RWY {}", str_of(&r["id"])), format!("{} × {} {}", thousands(r["len"].as_f64().unwrap_or(0.0)), r["width"].as_f64().unwrap_or(0.0), str_of(&r["surface"]))]);
        }
        p.table(&[("", 40.0, Align::Left), ("", 80.0, Align::Left)], &rows, 8.5, false);
    }
    p.rule();
    p.para("For planning only. Not a substitute for an official weather briefing, current charts, NOTAMs or the aircraft's POH. The pilot in command is responsible for the flight.", 8.0, false, C::FAINT);
    Ok(p.finish())
}
