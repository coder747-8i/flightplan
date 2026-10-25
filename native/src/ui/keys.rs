//! API keys for Windy and the FAA NOTAM API, stored by the backend in settings.json.

use super::widgets::{self, faint};
use crate::app::App;
use crate::backend::Task;
use crate::theme;
use egui::{CornerRadius, Frame, Margin, RichText};
use egui_phosphor::regular as ph;

#[derive(Default)]
pub struct KeysUi {
    windy_map: String,
    windy_point: String,
    notam_id: String,
    notam_secret: String,
    reveal: bool,
    test: Option<(&'static str, Task<(bool, String)>)>,
    results: std::collections::HashMap<&'static str, (bool, String)>,
    error: String,
}

fn hint(v: &Option<String>) -> String {
    match v {
        Some(k) => format!("saved …{}", &k[k.len().saturating_sub(4)..]),
        None => "not set".into(),
    }
}

fn save(app: &mut App, which: &str, clear: bool) {
    let mut k = app.be.server.settings.get();
    let ui = &mut app.settings_ui;
    let take = |s: &mut String| {
        let v = s.trim().to_string();
        s.clear();
        v
    };
    let set = |slot: &mut Option<String>, v: String, clear: bool| {
        if clear {
            *slot = None;
        } else if !v.is_empty() {
            *slot = Some(v);
        }
    };
    match which {
        "windyMap" => set(&mut k.windy_map, take(&mut ui.windy_map), clear),
        "windyPoint" => set(&mut k.windy_point, take(&mut ui.windy_point), clear),
        _ => {
            let (a, b) = (take(&mut ui.notam_id), take(&mut ui.notam_secret));
            set(&mut k.notam_id, a, clear);
            set(&mut k.notam_secret, b, clear);
        }
    }
    match app.be.server.settings.replace(k) {
        Ok(()) => {
            app.be.server.cache.forget_prefix("windy:");
            app.be.server.cache.forget_prefix("notam:");
            app.settings_ui.error.clear();
            app.refresh_keys();
        }
        Err(e) => app.settings_ui.error = format!("{e:#}"),
    }
}

pub fn form(app: &mut App, ui: &mut egui::Ui) {
    let t = app.t;
    if let Some((which, task)) = &mut app.settings_ui.test {
        if let Some(r) = task.poll() {
            app.settings_ui.results.insert(which, r);
            app.settings_ui.test = None;
        }
    }
    Frame::new().fill(t.surface2).corner_radius(CornerRadius::same(9)).inner_margin(Margin::same(10)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        widgets::para(ui, format!("{}  All optional. Keys are saved in settings.json in Flightplan's data folder (readable only by your user) and never shown again after saving.", ph::KEY), t.text2, 11.5);
    });
    if !app.settings_ui.error.is_empty() {
        widgets::para(ui, &app.settings_ui.error, t.bad, 12.0);
    }
    let k = app.be.server.settings.get();
    // winds aloft come from the NWS forecast; no key needed
    let sections: [(&str, &str, &str, &'static str, Vec<(&str, &str, Option<String>)>); 2] = [
        ("Windy map (web app)", "Used by the browser version's Windy map. The desktop app draws winds natively, so this key only matters if you also use the web version.", "https://api.windy.com/keys", "windyMap", vec![("windyMap", "Map Forecast API key", k.windy_map.clone())]),
        ("FAA NOTAMs", "Airport, en route, FDC and TFR NOTAMs along your route in the briefing. Register a free app on the FAA API portal and copy its client ID and secret.", "https://api.faa.gov/s/", "notam", vec![("notamId", "Client ID", k.notam_id.clone()), ("notamSecret", "Client secret", k.notam_secret.clone())]),
    ];
    for (title, blurb, link, test, fields) in sections {
        ui.add_space(10.0);
        let all_set = fields.iter().all(|f| f.2.is_some());
        let any_set = fields.iter().any(|f| f.2.is_some());
        ui.horizontal(|ui| {
            ui.label(RichText::new(title).font(theme::semibold(13.5)));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let (label, fg) = if all_set { (format!("{} Connected", ph::CHECK), Some(t.ok)) } else if any_set { ("Incomplete".to_string(), None) } else { ("Not set".to_string(), None) };
                widgets::chip(ui, &t, &label, fg, None);
            });
        });
        widgets::para(ui, blurb, t.text3, 11.5);
        if widgets::link(ui, &t, format!("{} {}", link.trim_start_matches("https://").trim_end_matches('/'), ph::ARROW_SQUARE_OUT)).clicked() {
            let _ = open::that(link);
        }
        for (id, label, saved) in &fields {
            faint(ui, &t, format!("{label} · {}", hint(saved)));
            let reveal = app.settings_ui.reveal;
            let buf = match *id {
                "windyMap" => &mut app.settings_ui.windy_map,
                "windyPoint" => &mut app.settings_ui.windy_point,
                "notamId" => &mut app.settings_ui.notam_id,
                _ => &mut app.settings_ui.notam_secret,
            };
            ui.add(egui::TextEdit::singleline(buf).password(!reveal).font(theme::mono(12.0)).desired_width(f32::INFINITY).hint_text(if saved.is_some() { "Enter a new value to replace" } else { "Paste the key" }));
        }
        ui.horizontal(|ui| {
            let ids: Vec<&str> = fields.iter().map(|f| f.0).collect();
            let has_draft = ids.iter().any(|id| match *id {
                "windyMap" => !app.settings_ui.windy_map.trim().is_empty(),
                "windyPoint" => !app.settings_ui.windy_point.trim().is_empty(),
                "notamId" => !app.settings_ui.notam_id.trim().is_empty(),
                _ => !app.settings_ui.notam_secret.trim().is_empty(),
            });
            if ui.add_enabled(has_draft, egui::Button::new(RichText::new("Save").color(egui::Color32::WHITE)).fill(t.accent)).clicked() {
                save(app, test, false);
            }
            let running = app.settings_ui.test.as_ref().is_some_and(|x| x.0 == test);
            if ui.add_enabled(any_set && !running, egui::Button::new(if running { "Testing…" } else { "Test" })).clicked() {
                let st = app.be.server.clone();
                let which = test;
                let task = app.be.spawn(async move { flightplan_server::settings::test_key(&st, which).await });
                app.settings_ui.test = Some((test, task));
            }
            if any_set && widgets::ghost(ui, &t, format!("{} Remove", ph::X)).clicked() {
                save(app, test, true);
            }
        });
        if let Some((ok, msg)) = app.settings_ui.results.get(test) {
            widgets::para(ui, msg, if *ok { t.ok } else { t.bad }, 12.0);
        }
        widgets::rule(ui, &t);
    }
    ui.checkbox(&mut app.settings_ui.reveal, "Show what I type");
}
