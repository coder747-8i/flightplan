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

/// A pasted key, cleaned: no whitespace or line breaks anywhere, no surrounding quotes, and no
/// "client_id:" style label copied along with it.
pub fn clean_key(s: &str) -> String {
    let mut v: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    for label in ["client_id:", "clientid:", "client_secret:", "clientsecret:", "client_id=", "client_secret=", "key:", "id:", "secret:"] {
        if v.to_lowercase().starts_with(label) {
            v = v[label.len()..].to_string();
        }
    }
    v.trim_matches(|c| c == '"' || c == '\'' || c == ',' || c == ';').to_string()
}

fn save(app: &mut App, which: &str, clear: bool) {
    let mut k = app.be.server.settings.get();
    let ui = &mut app.settings_ui;
    let take = |s: &mut String| {
        let v = clean_key(s);
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
            // both halves are needed: don't save one and leave the other missing
            let (a, b) = (clean_key(&ui.notam_id), clean_key(&ui.notam_secret));
            let have_a = !a.is_empty() || k.notam_id.is_some();
            let have_b = !b.is_empty() || k.notam_secret.is_some();
            if !clear && !(have_a && have_b) {
                ui.error = "Enter both the client ID and the client secret from your FAA app.".into();
                return;
            }
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
        ("FAA NOTAMs", "NOTAMs for your route, the airport cards, the briefing and the go / no-go call. The FAA's NOTAM Management Service (NMS) isn't self-serve: request API access by email to notams@faa.gov, and paste the client ID and client secret they send. Older api.faa.gov NOTAM keys work too.", "https://nms.aim.faa.gov/", "notam", vec![("notamId", "Client ID", k.notam_id.clone()), ("notamSecret", "Client secret", k.notam_secret.clone())]),
    ];
    for (title, blurb, link, test, fields) in sections {
        ui.add_space(10.0);
        let all_set = fields.iter().all(|f| f.2.is_some());
        let any_set = fields.iter().any(|f| f.2.is_some());
        ui.horizontal(|ui| {
            ui.label(RichText::new(title).font(theme::semibold(13.5)));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // "Connected" only once a test has passed
                let tested = app.settings_ui.results.get(test).map(|r| r.0);
                let (label, fg) = match (all_set, any_set, tested) {
                    (true, _, Some(true)) => (format!("{} Connected", ph::CHECK), Some(t.ok)),
                    (true, _, Some(false)) => (format!("{} Not accepted", ph::X), Some(t.bad)),
                    (true, _, None) => ("Saved".to_string(), None),
                    (false, true, _) => ("Incomplete".to_string(), None),
                    _ => ("Not set".to_string(), None),
                };
                widgets::chip(ui, &t, &label, fg, None);
            });
        });
        widgets::para(ui, blurb, t.text3, 11.5);
        ui.horizontal(|ui| {
            if widgets::link(ui, &t, format!("{} {}", link.trim_start_matches("https://").trim_end_matches('/'), ph::ARROW_SQUARE_OUT)).clicked() {
                let _ = open::that(link);
            }
            if test == "notam" && widgets::link(ui, &t, format!("{} Email the FAA for access", ph::ENVELOPE_SIMPLE)).clicked() {
                let _ = open::that(super::notams::REQUEST_MAILTO);
            }
        });
        for (id, label, saved) in &fields {
            faint(ui, &t, format!("{label} · {}", hint(saved)));
            // the client ID isn't secret: show it, so a paste that didn't land is obvious
            let reveal = app.settings_ui.reveal || *id == "notamId";
            let buf = match *id {
                "windyMap" => &mut app.settings_ui.windy_map,
                "windyPoint" => &mut app.settings_ui.windy_point,
                "notamId" => &mut app.settings_ui.notam_id,
                _ => &mut app.settings_ui.notam_secret,
            };
            ui.add(egui::TextEdit::singleline(buf).password(!reveal).font(theme::mono(12.0)).desired_width(f32::INFINITY).hint_text(if saved.is_some() { "Enter a new value to replace" } else { "Paste here (Ctrl+V)" }));
            let n = clean_key(buf).len();
            if n > 0 {
                faint(ui, &t, format!("{n} characters entered (spaces and line breaks are removed)"));
            }
        }
        ui.horizontal(|ui| {
            let ids: Vec<&str> = fields.iter().map(|f| f.0).collect();
            let has_draft = ids.iter().any(|id| match *id {
                "windyMap" => !app.settings_ui.windy_map.trim().is_empty(),
                "windyPoint" => !app.settings_ui.windy_point.trim().is_empty(),
                "notamId" => !app.settings_ui.notam_id.trim().is_empty(),
                _ => !app.settings_ui.notam_secret.trim().is_empty(),
            });
            if ui.add_enabled(has_draft, egui::Button::new(RichText::new(if test == "notam" { "Save and test" } else { "Save" }).color(egui::Color32::WHITE)).fill(t.accent)).clicked() {
                save(app, test, false);
                // check it straight away against the service
                if app.settings_ui.error.is_empty() && test != "windyMap" {
                    let st = app.be.server.clone();
                    let which = test;
                    app.settings_ui.results.remove(test);
                    app.settings_ui.test = Some((test, app.be.spawn(async move { flightplan_server::settings::test_key(&st, which).await })));
                }
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

#[cfg(test)]
mod tests {
    #[test]
    fn cleans_pasted_keys() {
        assert_eq!(super::clean_key("  abc123 \n"), "abc123");
        assert_eq!(super::clean_key("\"abc 123\""), "abc123");
        assert_eq!(super::clean_key("client_id: abc123"), "abc123");
        assert_eq!(super::clean_key("Client_Secret:XyZ"), "XyZ");
    }
}
