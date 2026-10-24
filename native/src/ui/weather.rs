//! Weather panel: chart overlays, advisories in effect, and METAR / TAF along the route.

use super::widgets::{self, faint, section};
use crate::app::{App, Panel};
use crate::backend::Task;
use crate::theme;
use crate::wx::decode::str_of;
use crate::wx::state::WIND_LEVELS;
use egui::{CornerRadius, Frame, Margin, RichText};
use egui_phosphor::regular as ph;
use serde_json::Value;

#[derive(Default)]
pub struct RouteWx {
    key: String,
    task: Task<(Value, Value)>,
    pub list: Vec<(String, Option<Value>, Option<Value>)>,
    pub loading: bool,
}

impl RouteWx {
    /// Keep METARs / TAFs for the route's airports current.
    pub fn update(&mut self, app_ids: Vec<String>, be: &crate::backend::Backend, force: bool) {
        let key = app_ids.join(",");
        if (key != self.key || force) && !self.task.pending() {
            self.key = key.clone();
            if app_ids.is_empty() {
                self.list.clear();
                return;
            }
            self.loading = true;
            let st = be.server.clone();
            self.task = be.spawn(async move {
                let m = flightplan_server::wx::metars(&st, Some(&key), None).await.unwrap_or(Value::Null);
                let t = flightplan_server::wx::tafs(&st, &key).await.unwrap_or(Value::Null);
                (m, t)
            });
        }
        if let Some((m, tf)) = self.task.poll() {
            self.loading = false;
            let find = |v: &Value, id: &str| v.as_array().and_then(|a| a.iter().find(|x| x["icaoId"].as_str() == Some(id)).cloned());
            self.list = self.key.split(',').filter(|s| !s.is_empty()).map(|id| (id.to_string(), find(&m, id), find(&tf, id))).collect();
        }
    }
}

pub fn age(unix: i64) -> String {
    let m = ((chrono::Utc::now().timestamp() - unix) as f64 / 60.0).round() as i64;
    if m < 1 {
        "just now".into()
    } else if m < 60 {
        format!("{m} min ago")
    } else {
        format!("{} h {} min ago", m / 60, m % 60)
    }
}

pub fn station(ui: &mut egui::Ui, app: &mut App, id: &str, metar: Option<&Value>, taf: Option<&Value>) {
    let t = app.t;
    ui.horizontal(|ui| {
        ui.label(RichText::new(id).font(theme::bold(13.0)));
        if let Some(m) = metar {
            let cat = str_of(&m["fltCat"]);
            if !cat.is_empty() {
                widgets::chip(ui, &t, &cat, Some(egui::Color32::WHITE), Some(theme::flt_cat(&cat)));
            }
            if let Some(o) = m["obsTime"].as_i64() {
                faint(ui, &t, age(o));
            }
        }
    });
    match metar {
        Some(m) => {
            if super::wxtext::show(ui, &t, super::wxtext::Kind::Metar(&str_of(&m["rawOb"])), app.saved.raw_wx, false) {
                app.saved.raw_wx = !app.saved.raw_wx;
            }
        }
        None => {
            faint(ui, &t, "No METAR");
        }
    }
    if let Some(tf) = taf {
        ui.add_space(4.0);
        if super::wxtext::show(ui, &t, super::wxtext::Kind::Taf(&str_of(&tf["rawTAF"])), app.saved.raw_wx, false) {
            app.saved.raw_wx = !app.saved.raw_wx;
        }
    }
}

pub fn panel(app: &mut App, ctx: &egui::Context) {
    let t = app.t;
    let ids: Vec<String> = app.saved.route.waypoints.iter().filter(|w| w.kind == crate::model::place::Kind::Airport).map(|w| w.wx_id()).collect();
    app.route_wx.update(ids, &app.be, false);
    let subtitle = app.wx.updated.map(|u| format!("Advisories updated {}", u.format("%H:%M"))).unwrap_or_else(|| "Loading…".into());
    let close = widgets::side_panel(ctx, &t, "panel-weather", "Weather", &subtitle, 340.0, app, |_, _| false, |ui, app| {
        // Windy itself is a web map; the desktop app draws winds aloft natively and links out
        Frame::new().fill(t.blue.gamma_multiply(0.12)).corner_radius(CornerRadius::same(12)).inner_margin(Margin::same(12)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(format!("{}  Winds aloft on the chart", ph::WIND)).font(theme::semibold(13.0)));
            widgets::para(ui, "Wind barbs and temperatures at a level you pick, for your departure time. Uses Windy's point forecast when you have a key, Open-Meteo GFS otherwise.", t.text2, 11.5);
            ui.horizontal(|ui| {
                widgets::switch(ui, &t, &mut app.saved.wx.winds);
                egui::ComboBox::from_id_salt("wind-level").selected_text(WIND_LEVELS[app.saved.wind_level.min(WIND_LEVELS.len() - 1)].1).show_ui(ui, |ui| {
                    for (i, (_, label)) in WIND_LEVELS.iter().enumerate() {
                        ui.selectable_value(&mut app.saved.wind_level, i, *label);
                    }
                });
            });
            ui.horizontal(|ui| {
                if widgets::link(ui, &t, format!("{} Open Windy.com here", ph::ARROW_SQUARE_OUT)).clicked() {
                    let (lon, lat) = app.map.center_lonlat();
                    let _ = open::that(format!("https://www.windy.com/?{lat:.3},{lon:.3},{}", app.map.zoom.round()));
                }
            });
        });

        section(ui, &t, "On the chart");
        widgets::toggle_row(ui, &t, &mut app.saved.wx.metars, "METAR flight category", "Dots: VFR, MVFR, IFR, LIFR", None);
        ui.horizontal(|ui| {
            for c in ["VFR", "MVFR", "IFR", "LIFR"] {
                ui.label(RichText::new("●").color(theme::flt_cat(c)));
                ui.label(RichText::new(c).size(11.0).color(t.text2));
            }
        });
        widgets::toggle_row(ui, &t, &mut app.saved.wx.radar, "Radar (NEXRAD)", "Composite reflectivity, ~5 min", None);
        if app.saved.wx.radar {
            ui.horizontal(|ui| {
                faint(ui, &t, "Opacity");
                ui.add(egui::Slider::new(&mut app.saved.radar_opacity, 0.2..=1.0).show_value(false));
            });
        }
        let n_tfr = app.wx.tfrs["features"].as_array().map(|a| a.len()).unwrap_or(0);
        widgets::toggle_row(ui, &t, &mut app.saved.wx.tfr, "TFRs", &format!("{n_tfr} active shapes"), Some(egui::Color32::from_rgb(0xd2, 0x3a, 0x3a)));
        let active = app.wx.active_sigmets().len();
        widgets::toggle_row(ui, &t, &mut app.saved.wx.sigmet, "AIRMETs & SIGMETs", &format!("{active} in effect"), Some(egui::Color32::from_rgb(0xe0, 0x45, 0x2b)));
        widgets::toggle_row(ui, &t, &mut app.saved.wx.gairmet, "G-AIRMETs", "IFR, mountain obscuration, turbulence, icing, freezing level", Some(egui::Color32::from_rgb(0x7b, 0x4f, 0xc9)));
        let hours = app.wx.gairmet_hours();
        if app.saved.wx.gairmet && hours.len() > 1 {
            ui.horizontal_wrapped(|ui| {
                for h in hours {
                    let on = app.wx.gairmet_hour == h;
                    if ui.selectable_label(on, format!("+{h} h")).clicked() {
                        app.wx.gairmet_hour = h;
                    }
                }
            });
        }
        widgets::toggle_row(ui, &t, &mut app.saved.wx.pireps, "PIREPs", "Last 3 hours; blue icing, orange turbulence", Some(egui::Color32::from_rgb(0xd4, 0x8a, 0x0b)));

        let sigs: Vec<Value> = app.wx.active_sigmets().into_iter().cloned().collect();
        if !sigs.is_empty() {
            section(ui, &t, "AIRMETs & SIGMETs in effect");
            let id = ui.id().with("allsig");
            let mut all = ui.data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
            for s in sigs.iter().take(if all { 99 } else { 4 }) {
                Frame::new().fill(t.surface2).corner_radius(CornerRadius::same(9)).inner_margin(Margin::same(10)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    if super::wxtext::show(ui, &t, super::wxtext::Kind::Sigmet(s), app.saved.raw_wx, true) {
                        app.saved.raw_wx = !app.saved.raw_wx;
                    }
                });
                ui.add_space(4.0);
            }
            if sigs.len() > 4 && widgets::link(ui, &t, if all { "Show fewer".to_string() } else { format!("Show all {}", sigs.len()) }).clicked() {
                all = !all;
                ui.data_mut(|d| d.insert_temp(id, all));
            }
        }

        section(ui, &t, "Along your route");
        let list = app.route_wx.list.clone();
        if list.is_empty() {
            widgets::para(ui, "Add airports to your route to see their METARs and TAFs here.", t.text2, 12.5);
        }
        for (id, m, tf) in &list {
            station(ui, app, id, m.as_ref(), tf.as_ref());
            widgets::rule(ui, &t);
        }
        ui.add_space(6.0);
        widgets::para(ui, "Sources: aviationweather.gov, FAA TFR, Iowa Environmental Mesonet, Open-Meteo / Windy. Check an official briefing (1800wxbrief) before flight.", t.text3, 11.0);
    });
    if close {
        app.panel = Panel::None;
    }
}
