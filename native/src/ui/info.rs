//! Details card on the right: an airport (info, weather, runways, frequencies, FBOs) or the
//! list of things under a click.

use super::widgets::{self, faint, glass, section};
use crate::app::{App, Panel, Selection};
use crate::backend::Task;
use crate::model::place::{Kind, Place};
use crate::theme;
use crate::wx::decode::{expand, str_of};
use egui::{vec2, Align2, CornerRadius, Frame, Margin, RichText};
use egui_phosphor::regular as ph;
use serde_json::Value;

#[derive(Default, Clone, Copy, PartialEq)]
enum Tab {
    #[default]
    Info,
    Weather,
    Runways,
    Freqs,
    Fbo,
    Notams,
}

#[derive(Default)]
pub struct InfoCard {
    id: String,
    data: Option<Value>,
    error: String,
    tab: Tab,
    wx: Task<(Value, Value)>,
    metar: Option<Value>,
    taf: Option<Value>,
    fbo: Option<Value>,
    show_others: bool,
    notams: Task<Value>,
    notam_data: Option<Value>,
}

fn s(v: &Value) -> String {
    str_of(v)
}

fn use_label(u: &str) -> String {
    u.replace("LCL/P", "Tower").replace("LCL/S", "Tower (sec)").replace("GND/P", "Ground").replace("GND/S", "Ground (sec)").replace("CD/P", "Clearance").replace("APCH/P DEP/P", "Approach/Departure").replace("APCH/P", "Approach").replace("DEP/P", "Departure").replace(" IC", "")
}

fn surface_name(sf: &str) -> String {
    sf.split('-')
        .map(|x| match x {
            "ASPH" => "Asphalt",
            "CONC" => "Concrete",
            "TURF" => "Turf",
            "GRVL" => "Gravel",
            "DIRT" => "Dirt",
            "WATER" => "Water",
            "SAND" => "Sand",
            "GRASS" => "Grass",
            o => o,
        })
        .collect::<Vec<_>>()
        .join(" / ")
}

impl InfoCard {
    fn load(&mut self, app_server: &std::sync::Arc<flightplan_server::AppState>, be: &crate::backend::Backend, id: &str) {
        self.id = id.to_string();
        self.data = None;
        self.error.clear();
        self.metar = None;
        self.taf = None;
        self.fbo = None;
        self.notam_data = None;
        self.notams = Task::default();
        self.tab = Tab::Info;
        match app_server.try_data().and_then(|d| d.db.airport(id)) {
            Ok(Some(v)) => {
                let wx_id = v["icao"].as_str().filter(|x| !x.is_empty()).map(String::from).unwrap_or_else(|| if id.len() == 3 { format!("K{id}") } else { id.to_string() });
                let st = be.server.clone();
                self.wx = be.spawn(async move {
                    let m = flightplan_server::wx::metars(&st, Some(&wx_id), None).await.unwrap_or(Value::Null);
                    let t = flightplan_server::wx::tafs(&st, &wx_id).await.unwrap_or(Value::Null);
                    (m, t)
                });
                self.data = Some(v);
            }
            Ok(None) => self.error = "Not found".into(),
            Err(e) => self.error = format!("{e:#}"),
        }
    }
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(sel) = app.selection.clone() else { return };
    let t = app.t;
    let screen = ctx.content_rect();
    let mut close = false;
    let mut route_add: Option<(Place, bool)> = None;
    let mut stop_toggle: Option<usize> = None;
    // where this airport sits in the route (fuel stops are the airports in between)
    let n_wp = app.saved.route.waypoints.len();
    let stops_at: Vec<(usize, String, bool)> = app.saved.route.waypoints.iter().enumerate().filter(|(i, w)| *i > 0 && *i + 1 < n_wp && w.kind == Kind::Airport).map(|(i, w)| (i, w.id.clone(), w.fuel_stop)).collect();
    let mut open_airport: Option<String> = None;
    let mut toggle_raw = false;
    let mut open_settings = false;
    if let Selection::Airport(id) = &sel {
        if app.info.id != *id {
            let server = app.be.server.clone();
            app.info.load(&server, &app.be, id);
        }
        if let Some(v) = app.info.notams.poll() {
            app.info.notam_data = Some(v);
        }
        if let Some((m, tf)) = app.info.wx.poll() {
            app.info.metar = m.as_array().and_then(|a| a.first().cloned());
            app.info.taf = tf.as_array().and_then(|a| a.first().cloned());
        }
    }
    let max_h = screen.height() - 84.0 - if app.saved.profile_open && app.saved.route.waypoints.len() >= 2 { 260.0 } else { 0.0 };
    egui::Area::new("info".into()).anchor(Align2::RIGHT_TOP, vec2(-12.0, 60.0)).order(egui::Order::Middle).show(ctx, |ui| {
        glass(&t).show(ui, |ui| {
            ui.set_width(360.0);
            ui.set_max_height(max_h.max(240.0));
            Frame::new().inner_margin(Margin { left: 16, right: 10, top: 12, bottom: 12 }).show(ui, |ui| match &sel {
                Selection::Airport(_) => {
                    let info = &mut app.info;
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| match &info.data {
                            Some(d) => {
                                let code = d["icao"].as_str().filter(|x| !x.is_empty()).unwrap_or(d["id"].as_str().unwrap_or("")).to_string();
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(&code).font(theme::bold(20.0)).color(if d["towered"].as_bool().unwrap_or(false) { t.blue } else { t.magenta }));
                                    if let Some(m) = &info.metar {
                                        let cat = s(&m["fltCat"]);
                                        if !cat.is_empty() {
                                            widgets::chip(ui, &t, &cat, Some(egui::Color32::WHITE), Some(theme::flt_cat(&cat)));
                                        }
                                    }
                                });
                                ui.label(RichText::new(s(&d["name"])).font(theme::semibold(14.0)));
                                faint(ui, &t, format!("{}, {} · {}{}", s(&d["city"]), s(&d["state"]), if s(&d["use"]) == "PR" { "Private" } else { "Public" }, if d["towered"].as_bool().unwrap_or(false) { " · Towered" } else { "" }));
                            }
                            None => {
                                ui.label(RichText::new(if info.error.is_empty() { "Loading…" } else { &info.error }).font(theme::semibold(14.0)));
                            }
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                            if widgets::icon_button(ui, &t, ph::X, "Close").clicked() {
                                close = true;
                            }
                        });
                    });
                    let Some(d) = info.data.clone() else { return };
                    let code = d["icao"].as_str().filter(|x| !x.is_empty()).unwrap_or(d["id"].as_str().unwrap_or("")).to_string();
                    let place = Place { kind: Kind::Airport, id: code.clone(), name: s(&d["name"]), detail: format!("{}, {}", s(&d["city"]), s(&d["state"])), lat: d["lat"].as_f64().unwrap_or(0.0), lon: d["lon"].as_f64().unwrap_or(0.0) };
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if widgets::primary(ui, &t, format!("{}  Add to route", ph::PLUS)).clicked() {
                            route_add = Some((place.clone(), false));
                        }
                        if widgets::button(ui, format!("{}  Start here", ph::NAVIGATION_ARROW)).on_hover_text("Start a new route here").clicked() {
                            route_add = Some((place.clone(), true));
                        }
                        let plain = d["id"].as_str().unwrap_or("");
                        if let Some((i, _, on)) = stops_at.iter().find(|(_, id, _)| *id == code || id == plain) {
                            let label = if *on { format!("{}  Fuel stop {}", ph::GAS_PUMP, ph::CHECK) } else { format!("{}  Fuel stop", ph::GAS_PUMP) };
                            if widgets::button(ui, label).on_hover_text("Land here, top off and go on").clicked() {
                                stop_toggle = Some(*i);
                            }
                        }
                    });
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        for (tab, label) in [(Tab::Info, "Info"), (Tab::Weather, "Weather"), (Tab::Runways, "Runways"), (Tab::Freqs, "Freqs"), (Tab::Fbo, "FBO"), (Tab::Notams, "NOTAMs")] {
                            if ui.selectable_label(info.tab == tab, RichText::new(label).font(theme::semibold(12.0))).clicked() {
                                info.tab = tab;
                            }
                        }
                    });
                    widgets::rule(ui, &t);
                    egui::ScrollArea::vertical().auto_shrink([false, true]).max_height(max_h - 190.0).show(ui, |ui| {
                        ui.set_width(330.0);
                        match info.tab {
                            Tab::Info => airport_info(ui, &t, &d, app.saved.raw_wx),
                            Tab::Weather => {
                                if let Some(m) = &info.metar {
                                    section(ui, &t, &format!("METAR · {}", super::weather::age(m["obsTime"].as_i64().unwrap_or(0))));
                                    toggle_raw |= super::wxtext::show(ui, &t, super::wxtext::Kind::Metar(&s(&m["rawOb"])), app.saved.raw_wx, false);
                                } else {
                                    widgets::para(ui, "No METAR for this field. Check the Weather panel for nearby stations.", t.text2, 12.5);
                                }
                                if let Some(tf) = &info.taf {
                                    section(ui, &t, "TAF");
                                    toggle_raw |= super::wxtext::show(ui, &t, super::wxtext::Kind::Taf(&s(&tf["rawTAF"])), app.saved.raw_wx, false);
                                }
                                let awos = d["awos"].as_array().cloned().unwrap_or_default();
                                if !awos.is_empty() {
                                    section(ui, &t, "AWOS / ASOS");
                                    for w in awos {
                                        widgets::muted(ui, &t, format!("{} {} {}", s(&w["type"]), s(&w["freq"]), if s(&w["phone"]).is_empty() { String::new() } else { format!("· {}", s(&w["phone"])) }));
                                    }
                                }
                            }
                            Tab::Runways => {
                                for r in d["runways"].as_array().into_iter().flatten() {
                                    Frame::new().fill(t.surface2).corner_radius(CornerRadius::same(9)).inner_margin(Margin::same(9)).show(ui, |ui| {
                                        ui.set_width(ui.available_width());
                                        ui.horizontal(|ui| {
                                            ui.label(RichText::new(s(&r["id"])).font(theme::bold(13.0)));
                                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                ui.label(format!("{} × {} ft", crate::nav::navlog::thousands(r["len"].as_f64().unwrap_or(0.0)), r["width"].as_f64().unwrap_or(0.0)));
                                            });
                                        });
                                        let mut sub = surface_name(&s(&r["surface"]));
                                        if !s(&r["cond"]).is_empty() {
                                            sub += &format!(" · {}", s(&r["cond"]).to_lowercase());
                                        }
                                        let li = s(&r["lighting"]);
                                        if !li.is_empty() && li != "NONE" {
                                            sub += &format!(" · lights {}", li.to_lowercase());
                                        }
                                        faint(ui, &t, sub);
                                        for e in r["ends"].as_array().into_iter().flatten() {
                                            ui.horizontal_wrapped(|ui| {
                                                ui.label(RichText::new(s(&e["id"])).font(theme::bold(12.0)));
                                                if e["rightTraffic"].as_bool().unwrap_or(false) {
                                                    widgets::chip(ui, &t, "RP", Some(t.warn), None);
                                                }
                                                for k in ["ils", "vgsi"] {
                                                    if !s(&e[k]).is_empty() {
                                                        widgets::chip(ui, &t, &s(&e[k]), None, None);
                                                    }
                                                }
                                                if let Some(dd) = e["displaced"].as_f64().filter(|v| *v > 0.0) {
                                                    faint(ui, &t, format!("displaced {dd} ft"));
                                                }
                                                if let Some(tdz) = e["tdz"].as_f64() {
                                                    faint(ui, &t, format!("TDZE {}", tdz.round()));
                                                }
                                            });
                                        }
                                    });
                                    ui.add_space(4.0);
                                }
                            }
                            Tab::Freqs => {
                                let order = ["ATIS", "D-ATIS", "AWOS", "ASOS", "CTAF", "UNICOM", "CD/P", "GND/P", "LCL/P", "LCL/S", "APCH/P DEP/P", "APCH/P", "DEP/P", "CLASS B", "CLASS C", "TRSA"];
                                let mut f: Vec<Value> = d["freqs"].as_array().cloned().unwrap_or_default().into_iter().filter(|f| s(&f["freq"]).parse::<f64>().is_ok_and(|v| v > 108.0 && v < 137.0)).collect();
                                f.sort_by_key(|x| order.iter().position(|o| s(&x["use"]).starts_with(o)).unwrap_or(99));
                                egui::Grid::new("freqs").num_columns(3).striped(true).spacing(vec2(12.0, 4.0)).show(ui, |ui| {
                                    for x in f {
                                        ui.label(use_label(&s(&x["use"])));
                                        ui.label(RichText::new(s(&x["freq"])).font(theme::bold(12.5)));
                                        faint(ui, &t, s(&x["sector"]));
                                        ui.end_row();
                                    }
                                });
                            }
                            Tab::Notams => {
                                if info.notam_data.is_none() && !info.notams.pending() && app.keys.notams {
                                    let id = d["icao"].as_str().filter(|x| !x.is_empty()).unwrap_or(d["id"].as_str().unwrap_or("")).to_string();
                                    info.notams = crate::wx::notams::airport(&app.be, &id);
                                }
                                let (raw, settings) = super::notams::airport_tab(ui, &t, app.keys.notams, app.saved.raw_wx, info.notam_data.as_ref());
                                toggle_raw |= raw;
                                open_settings |= settings;
                            }
                            Tab::Fbo => {
                                if info.fbo.is_none() {
                                    let id = d["icao"].as_str().filter(|x| !x.is_empty()).unwrap_or(d["id"].as_str().unwrap_or("")).to_string();
                                    info.fbo = Some(app.be.server.try_data().ok().and_then(|db| db.db.fbos(&id).ok().flatten()).unwrap_or(Value::Array(vec![])));
                                }
                                fbos(ui, &t, info.fbo.as_ref().unwrap(), &d, &mut info.show_others);
                            }
                        }
                    });
                }
                Selection::Point { lon, lat, items } => {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(RichText::new("At this point").font(theme::semibold(14.0)));
                            faint(ui, &t, format!("{lat:.4}, {lon:.4}"));
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                            if widgets::icon_button(ui, &t, ph::X, "Close").clicked() {
                                close = true;
                            }
                        });
                    });
                    egui::ScrollArea::vertical().max_height(max_h - 70.0).show(ui, |ui| {
                        ui.set_width(330.0);
                        for it in items {
                            let r = Frame::new().inner_margin(Margin { left: 12, right: 6, top: 6, bottom: 6 }).show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.label(RichText::new(&it.title).font(theme::semibold(12.5)));
                                if !it.subtitle.is_empty() {
                                    ui.add(egui::Label::new(RichText::new(&it.subtitle).size(11.5).color(t.text3)).wrap());
                                }
                                if let Some(dc) = &it.decode {
                                    toggle_raw |= super::wxtext::show(ui, &t, dc.into(), app.saved.raw_wx, true);
                                }
                            });
                            let rect = r.response.rect;
                            ui.painter().line_segment([rect.left_top() + vec2(3.0, 4.0), rect.left_bottom() + vec2(3.0, -4.0)], egui::Stroke::new(3.0, it.color));
                            if let Some(a) = &it.airport {
                                if r.response.interact(egui::Sense::click()).clicked() {
                                    open_airport = Some(a.clone());
                                }
                            }
                        }
                    });
                }
            });
        });
    });
    if toggle_raw {
        app.saved.raw_wx = !app.saved.raw_wx;
    }
    if open_settings {
        app.panel = Panel::Settings;
    }
    if let Some(i) = stop_toggle {
        app.saved.route.toggle_fuel_stop(i);
    }
    if let Some((p, direct)) = route_add {
        if direct {
            app.saved.route.replace(vec![p]);
        } else {
            app.saved.route.add(p, None);
        }
        app.panel = Panel::Route;
    }
    if let Some(a) = open_airport {
        app.selection = Some(Selection::Airport(a));
    } else if close {
        app.selection = None;
        app.info.id.clear();
    }
}

fn airport_info(ui: &mut egui::Ui, t: &theme::Tokens, d: &Value, raw: bool) {
    egui::Grid::new("facts").num_columns(4).spacing(vec2(10.0, 6.0)).show(ui, |ui| {
        let ctaf = d["freqs"].as_array().and_then(|a| a.iter().find(|f| f["use"] == "CTAF")).map(|f| s(&f["freq"])).unwrap_or_else(|| "—".into());
        let tpa = d["tpa"].as_str().and_then(|v| v.parse::<f64>().ok()).or(d["tpa"].as_f64()).map(|v| format!("{} ft", crate::nav::navlog::thousands(v))).unwrap_or_else(|| "—".into());
        let facts = [
            ("Elevation", format!("{} ft", crate::nav::navlog::thousands(d["elev"].as_f64().unwrap_or(0.0)))),
            ("Pattern", tpa),
            ("Mag var", if s(&d["magVar"]).is_empty() { "—".into() } else { s(&d["magVar"]) }),
            ("CTAF", ctaf),
            ("Sectional", if s(&d["sectional"]).is_empty() { "—".into() } else { s(&d["sectional"]) }),
            ("ARTCC", if s(&d["artcc"]).is_empty() { "—".into() } else { s(&d["artcc"]) }),
        ];
        for (i, (k, v)) in facts.iter().enumerate() {
            ui.label(RichText::new(*k).size(11.0).color(t.text3));
            ui.label(RichText::new(v).font(theme::semibold(12.5)));
            if i % 2 == 1 {
                ui.end_row();
            }
        }
    });
    let fuel: Vec<String> = d["fuel"].as_array().into_iter().flatten().map(|f| s(&f["name"])).collect();
    if !fuel.is_empty() {
        section(ui, t, "Fuel");
        ui.horizontal_wrapped(|ui| {
            for f in fuel {
                widgets::chip(ui, t, &f, None, None);
            }
        });
    }
    section(ui, t, "Services");
    let mut svc = Vec::new();
    if !s(&d["repair"]["airframe"]).is_empty() {
        svc.push(format!("Airframe repair: {}", s(&d["repair"]["airframe"]).to_lowercase()));
    }
    if !s(&d["repair"]["powerplant"]).is_empty() {
        svc.push(format!("Powerplant repair: {}", s(&d["repair"]["powerplant"]).to_lowercase()));
    }
    let ox: Vec<String> = [s(&d["oxygen"]["bottled"]), s(&d["oxygen"]["bulk"])].into_iter().filter(|x| !x.is_empty()).collect();
    if !ox.is_empty() {
        svc.push(format!("Oxygen: {}", ox.join(", ").to_lowercase()));
    }
    let st: Vec<&str> = [(!s(&d["storage"]["hangar"]).is_empty()).then_some("hangars"), (!s(&d["storage"]["tiedown"]).is_empty()).then_some("tie-downs")].into_iter().flatten().collect();
    if !st.is_empty() {
        svc.push(format!("Transient storage: {}", st.join(", ")));
    }
    for x in d["services"].as_array().into_iter().flatten() {
        svc.push(s(x));
    }
    if d["landingFee"].as_bool().unwrap_or(false) || s(&d["landingFee"]) == "Y" {
        svc.push("Landing fee".into());
    }
    if d["customs"].as_bool().unwrap_or(false) || s(&d["customs"]) == "Y" {
        svc.push("Customs (airport of entry)".into());
    }
    for x in svc {
        ui.label(RichText::new(format!("• {x}")).size(12.0));
    }
    let att = d["attendance"].as_array().cloned().unwrap_or_default();
    if !att.is_empty() {
        section(ui, t, "Attended");
        for a in att {
            widgets::muted(ui, t, [s(&a["month"]), s(&a["day"]), s(&a["hour"])].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" · "));
        }
    }
    let contacts = d["contacts"].as_array().cloned().unwrap_or_default();
    if !contacts.is_empty() {
        section(ui, t, "Contacts");
        for c in contacts {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(s(&c["name"])).font(theme::semibold(12.0)));
                faint(ui, t, s(&c["title"]).to_lowercase());
                if !s(&c["phone"]).is_empty() {
                    ui.label(RichText::new(format!("{} {}", ph::PHONE, s(&c["phone"]))).size(12.0).color(t.blue));
                }
            });
        }
    }
    let groups = [("caution", "Cautions"), ("ppr", "Prior permission"), ("fuel", "Fuel & services"), ("customs", "Customs & fees"), ("noise", "Noise abatement"), ("pattern", "Traffic pattern"), ("atc", "ATC & clearance"), ("lighting", "Lighting"), ("runway", "Runway notes"), ("other", "Other remarks")];
    let remarks = d["remarks"].as_array().cloned().unwrap_or_default();
    for (k, title) in groups {
        let items: Vec<&Value> = remarks.iter().filter(|r| { let c = s(&r["category"]); (if c.is_empty() { "other".to_string() } else { c }) == k }).collect();
        if items.is_empty() {
            continue;
        }
        section(ui, t, title);
        for r in items {
            let text = if raw { s(&r["text"]) } else { expand(&s(&r["text"])) };
            ui.add(egui::Label::new(RichText::new(format!("• {text}")).size(12.0)).wrap());
        }
    }
}

fn fbos(ui: &mut egui::Ui, t: &theme::Tokens, list: &Value, airport: &Value, show_others: &mut bool) {
    let all = list.as_array().cloned().unwrap_or_default();
    let main: Vec<&Value> = all.iter().filter(|f| s(&f["kind"]) != "service").collect();
    let others: Vec<&Value> = all.iter().filter(|f| s(&f["kind"]) == "service").collect();
    let mut notes: Vec<String> = all.iter().flat_map(|f| f["notes"].as_array().cloned().unwrap_or_default()).map(|n| s(&n)).collect();
    notes.dedup();
    if notes.is_empty() {
        notes = airport["remarks"].as_array().into_iter().flatten().filter(|r| s(&r["category"]) == "fuel").map(|r| s(&r["plain"])).filter(|x| !x.is_empty()).collect();
    }
    if !notes.is_empty() {
        section(ui, t, "Fuel & service notes (FAA)");
        for n in notes {
            ui.add(egui::Label::new(RichText::new(format!("• {n}")).size(12.0)).wrap());
        }
    }
    if main.is_empty() {
        widgets::para(ui, "No FBO found for this field. Fuel and services on the Info tab come from the FAA airport record.", t.text2, 12.0);
    } else {
        section(ui, t, "FBOs");
    }
    for f in main {
        ui.horizontal(|ui| {
            ui.label(RichText::new(s(&f["name"])).font(theme::semibold(13.0)));
            if !s(&f["brand"]).is_empty() {
                widgets::chip(ui, t, &s(&f["brand"]), None, None);
            }
        });
        let fuel: Vec<String> = f["fuel"].as_array().into_iter().flatten().map(s).collect();
        if !fuel.is_empty() {
            ui.label(RichText::new(format!("{} {}", ph::GAS_PUMP, fuel.join(" · "))).size(12.0));
        }
        if !s(&f["phone"]).is_empty() {
            ui.label(RichText::new(format!("{} {}", ph::PHONE, s(&f["phone"]))).size(12.0).color(t.blue));
        }
        if !s(&f["website"]).is_empty() {
            let site = s(&f["website"]);
            let url = if site.starts_with("http") { site.clone() } else { format!("https://{site}") };
            if ui.link(RichText::new(format!("{} {}", ph::GLOBE, site.trim_start_matches("https://").trim_start_matches("http://").trim_start_matches("www.").trim_end_matches('/'))).size(12.0)).clicked() {
                let _ = open::that(url);
            }
        }
        if !s(&f["address"]).is_empty() {
            faint(ui, t, format!("{} {}", ph::MAP_PIN, s(&f["address"])));
        }
        let src: Vec<String> = f["sources"].as_array().into_iter().flatten().map(s).collect();
        faint(ui, t, format!("Source: {}", src.join(", ")));
        widgets::rule(ui, t);
    }
    if !others.is_empty() {
        if widgets::link(ui, t, format!("{} {} other aviation businesses (maintenance, charter, schools)", if *show_others { "Hide" } else { "Show" }, others.len())).clicked() {
            *show_others = !*show_others;
        }
        if *show_others {
            for f in others {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(s(&f["name"])).font(theme::semibold(12.0)));
                    if !s(&f["phone"]).is_empty() {
                        faint(ui, t, s(&f["phone"]));
                    }
                });
            }
        }
    }
    widgets::para(ui, "FBO listings come from Overture Maps (open places data) and FAA remarks. Call ahead to confirm services and prices.", t.text3, 11.0);
}
