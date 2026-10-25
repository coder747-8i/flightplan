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

/// Forecast at the planned time for the departure or destination, one compact block.
fn at_your_time(ui: &mut egui::Ui, app: &App, role: &str, stn: Option<&crate::wx::forecast::StationWx>, when: chrono::DateTime<chrono::Utc>) {
    let t = app.t;
    let Some(s) = stn.filter(|s| !s.station.is_empty()) else { return };
    let local = when.with_timezone(&chrono::Local);
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("{role} {}", s.airport)).font(theme::semibold(12.5)));
        faint(ui, &t, format!("{}", local.format("%a %H:%M")));
    });
    match s.at(when) {
        Some(c) => {
            ui.horizontal_wrapped(|ui| {
                if let Some(cat) = c.category() {
                    widgets::chip(ui, &t, cat, Some(egui::Color32::WHITE), Some(theme::flt_cat(cat)));
                }
                ui.add(egui::Label::new(RichText::new(c.summary()).size(12.0)).wrap());
            });
            let from = if c.station != s.airport && s.offset_nm > 1.0 { format!(" from {} ({:.0} NM)", c.station, s.offset_nm) } else { String::new() };
            faint(ui, &t, format!("{}{from}", c.source.label()));
            if let Some((_, text)) = &c.tempo {
                faint(ui, &t, text.clone());
            }
        }
        None => {
            faint(ui, &t, "No forecast covers this time yet");
        }
    }
    ui.add_space(4.0);
}

pub fn panel(app: &mut App, ctx: &egui::Context) {
    let t = app.t;
    let ids: Vec<String> = app.saved.route.waypoints.iter().filter(|w| w.kind == crate::model::place::Kind::Airport).map(|w| w.wx_id()).collect();
    app.route_wx.update(ids, &app.be, false);
    let subtitle = app.wx.updated.map(|u| format!("Advisories updated {}", u.format("%H:%M"))).unwrap_or_else(|| "Loading…".into());
    let close = widgets::side_panel(ctx, &t, "panel-weather", "Weather", &subtitle, 340.0, app, |_, _| false, |ui, app| {
        section(ui, &t, "On the chart");
        let radar_hint = match &app.chart.radar_frame {
            Some(f) => f.label(),
            None => "Live NEXRAD now; as you move the timeline, HRRR forecast radar to about 48 h, then the WPC forecast chart (days 1-3) and WPC precipitation forecast (days 4-5)".into(),
        };
        widgets::toggle_line(ui, &t, &mut app.saved.wx.radar, "Radar", &radar_hint, Some(crate::wx::radar::dbz_color(30.0)));
        if app.saved.wx.radar {
            ui.horizontal(|ui| {
                ui.add_space(18.0);
                faint(ui, &t, "Opacity");
                ui.spacing_mut().slider_width = 150.0;
                ui.add(egui::Slider::new(&mut app.saved.radar_opacity, 0.2..=1.0).show_value(false));
            });
            ui.horizontal(|ui| {
                ui.add_space(18.0);
                for dbz in [10.0, 20.0, 30.0, 40.0, 50.0, 60.0] {
                    let (r, _) = ui.allocate_exact_size(egui::vec2(22.0, 6.0), egui::Sense::hover());
                    ui.painter().rect_filled(r, CornerRadius::same(2), crate::wx::radar::dbz_color(dbz));
                }
                faint(ui, &t, "light → heavy");
            });
        }
        widgets::toggle_line(ui, &t, &mut app.saved.wx.spc, "Convective outlook (SPC)", "The Storm Prediction Center's thunderstorm outlook for the timeline's day, days 1-8", Some(egui::Color32::from_rgb(0x55, 0xbb, 0x55)));
        widgets::toggle_line(ui, &t, &mut app.saved.wx.metars, "Flight category", "METAR dots: green VFR, blue MVFR, red IFR, purple LIFR", Some(theme::flt_cat("VFR")));
        widgets::toggle_line(ui, &t, &mut app.saved.wx.winds, "Winds aloft", "NWS winds-aloft forecast (FB) at its stations, at the level you pick, for the timeline's time (about 24 hours ahead)", Some(crate::ui::chart::speed_color(30.0)));
        if app.saved.wx.winds {
            ui.horizontal(|ui| {
                ui.add_space(18.0);
                egui::ComboBox::from_id_salt("wind-level").selected_text(WIND_LEVELS[app.saved.wind_level.min(WIND_LEVELS.len() - 1)].1).show_ui(ui, |ui| {
                    for (i, (_, label)) in WIND_LEVELS.iter().enumerate() {
                        ui.selectable_value(&mut app.saved.wind_level, i, *label);
                    }
                });
            });
        }
        let n_tfr = app.wx.tfrs["features"].as_array().map(|a| a.len()).unwrap_or(0);
        widgets::toggle_line(ui, &t, &mut app.saved.wx.tfr, "TFRs", &format!("{n_tfr} active shapes"), Some(egui::Color32::from_rgb(0xd2, 0x3a, 0x3a)));
        let active = app.wx.active_sigmets().len();
        widgets::toggle_line(ui, &t, &mut app.saved.wx.sigmet, "AIRMETs & SIGMETs", &format!("{active} in effect"), Some(egui::Color32::from_rgb(0xe0, 0x45, 0x2b)));
        widgets::toggle_line(ui, &t, &mut app.saved.wx.gairmet, "G-AIRMETs", "IFR, mountain obscuration, turbulence, icing, freezing level", Some(egui::Color32::from_rgb(0x7b, 0x4f, 0xc9)));
        let hours = app.wx.gairmet_hours();
        if app.saved.wx.gairmet && hours.len() > 1 {
            ui.horizontal_wrapped(|ui| {
                ui.add_space(18.0);
                for h in hours {
                    let on = app.wx.gairmet_hour == h;
                    if ui.selectable_label(on, format!("+{h} h")).clicked() {
                        app.wx.gairmet_hour = h;
                    }
                }
            });
        }
        widgets::toggle_line(ui, &t, &mut app.saved.wx.pireps, "PIREPs", "Last 3 hours; blue icing, orange turbulence, red severe", Some(egui::Color32::from_rgb(0xd4, 0x8a, 0x0b)));

        // ---- the route at the planned time ----
        if app.saved.route.waypoints.len() >= 2 {
            section(ui, &t, "Your flight");
            let dep = app.saved.route.departure_time();
            let eta = app.plan.log.as_ref().map(|l| dep + chrono::Duration::milliseconds((l.total_time * 60_000.0) as i64)).unwrap_or(dep);
            let fc = &app.plan.forecast;
            if fc.loading && fc.dep.is_none() {
                faint(ui, &t, "Fetching forecasts…");
            }
            at_your_time(ui, app, "Departure", fc.dep.as_ref(), dep);
            at_your_time(ui, app, "Arrival", fc.dest.as_ref(), eta);
        }

        let list = app.route_wx.list.clone();
        widgets::fold(ui, &t, "wx-fold-metars", &format!("METARs & TAFs ({})", list.len()), false, |ui| {
            if list.is_empty() {
                widgets::para(ui, "Add airports to your route to see their METARs and TAFs here.", t.text2, 12.5);
            }
            for (id, m, tf) in &list {
                station(ui, app, id, m.as_ref(), tf.as_ref());
                widgets::rule(ui, &t);
            }
        });

        let sigs: Vec<Value> = app.wx.active_sigmets().into_iter().cloned().collect();
        widgets::fold(ui, &t, "wx-fold-sigmets", &format!("Advisories in effect ({})", sigs.len()), false, |ui| {
            for s in &sigs {
                Frame::new().fill(t.surface2).corner_radius(CornerRadius::same(9)).inner_margin(Margin::same(10)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    if super::wxtext::show(ui, &t, super::wxtext::Kind::Sigmet(s), app.saved.raw_wx, true) {
                        app.saved.raw_wx = !app.saved.raw_wx;
                    }
                });
                ui.add_space(4.0);
            }
        });

        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if widgets::link(ui, &t, format!("{} Windy.com", ph::ARROW_SQUARE_OUT)).clicked() {
                let (lon, lat) = app.map.center_lonlat();
                let _ = open::that(format!("https://www.windy.com/?{lat:.3},{lon:.3},{}", app.map.zoom.round()));
            }
            if widgets::link(ui, &t, format!("{} 1800wxbrief", ph::ARROW_SQUARE_OUT)).clicked() {
                let _ = open::that("https://www.1800wxbrief.com/");
            }
        });
        widgets::para(ui, "Sources: aviationweather.gov, FAA TFR, Iowa Environmental Mesonet (NEXRAD, HRRR, NBM), NOAA WPC and SPC, NWS winds aloft (FB). Get an official briefing before flight.", t.text3, 10.5);
    });
    if close {
        app.panel = Panel::None;
    }
}
