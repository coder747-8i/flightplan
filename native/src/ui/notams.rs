//! NOTAM lists: the route's in the Route panel and briefing, one airport's on its card. Most
//! important first (closures, fuel, navaids, GPS, airspace), each with when it's in effect and
//! plain English; the raw text on request. Without FAA credentials: how to get them.

use super::widgets::{self, faint};
use crate::app::{App, Panel};
use crate::theme::{self, Tokens};
use crate::wx::notams::{Effect, Notam};
use egui::{CornerRadius, Frame, Margin, RichText};
use egui_phosphor::regular as ph;

/// A ready-to-send request for NMS API access.
pub const REQUEST_MAILTO: &str = "mailto:notams@faa.gov?subject=NMS%20API%20access%20request&body=Hello%2C%0A%0AI%20would%20like%20to%20request%20access%20to%20the%20NOTAM%20Management%20Service%20(NMS)%20API%20for%20a%20personal%20flight-planning%20application%20(pre-flight%20NOTAM%20review%20for%20my%20own%20flights%2C%20GeoJSON%2C%20low%20volume).%0A%0AName%3A%20%0APilot%20certificate%20(optional)%3A%20%0A%0AThank%20you.";

fn icon(t: &Tokens, n: &Notam) -> (&'static str, egui::Color32) {
    match n.effect {
        Effect::AerodromeClosed | Effect::RunwayClosed(_) => (ph::PROHIBIT, t.bad),
        Effect::FuelUnavailable => (ph::GAS_PUMP, t.warn),
        Effect::NavaidOut | Effect::Gps => (ph::BROADCAST, t.warn),
        Effect::Airspace => (ph::POLYGON, t.blue),
        Effect::Obstacle => (ph::WARNING_DIAMOND, t.text3),
        Effect::Other => (ph::NOTE, t.text3),
    }
}

/// One NOTAM row.
pub fn row(ui: &mut egui::Ui, t: &Tokens, n: &Notam, raw: bool) {
    let (ic, c) = icon(t, n);
    Frame::new().fill(t.surface2).corner_radius(CornerRadius::same(8)).inner_margin(Margin::symmetric(9, 6)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_top(|ui| {
            ui.label(RichText::new(ic).color(c).size(14.0));
            ui.vertical(|ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(if n.icao.is_empty() { &n.location } else { &n.icao }).font(theme::bold(12.0)));
                    ui.label(RichText::new(format!("{} · {}", n.category, n.number)).size(10.5).color(t.text3));
                });
                let text = if raw { &n.raw } else { &n.plain };
                ui.add(egui::Label::new(RichText::new(text).size(11.5).font(if raw { theme::mono(11.0) } else { theme::regular(11.5) })).wrap());
                let w = n.window();
                if !w.is_empty() {
                    ui.label(RichText::new(w).size(10.5).color(t.text3));
                }
            });
        });
    });
    ui.add_space(3.0);
}

/// Prompt to set up NOTAMs (free FAA credentials).
pub fn setup_hint(ui: &mut egui::Ui, app: &mut App) {
    let t = app.t;
    widgets::para(ui, "NOTAMs come from the FAA NOTAM Management Service. Access is free but by request: email notams@faa.gov, then paste the client ID and secret they send in Settings.", t.text2, 11.5);
    ui.horizontal(|ui| {
        if widgets::link(ui, &t, format!("{} Email the FAA", ph::ENVELOPE_SIMPLE)).clicked() {
            let _ = open::that(REQUEST_MAILTO);
        }
        if widgets::link(ui, &t, format!("{} Add it in Settings", ph::GEAR_SIX)).clicked() {
            app.panel = Panel::Settings;
        }
    });
}

/// The route's NOTAMs, folded: count by importance in the title.
pub fn route_section(ui: &mut egui::Ui, app: &mut App) {
    let t = app.t;
    if app.saved.route.waypoints.len() < 2 {
        return;
    }
    let rn = &app.plan.notams;
    let important = rn.items.iter().filter(|n| matches!(n.effect, Effect::AerodromeClosed | Effect::RunwayClosed(_) | Effect::FuelUnavailable | Effect::NavaidOut | Effect::Gps)).count();
    let title = if !app.keys.notams {
        "NOTAMs".to_string()
    } else if rn.loading && !rn.done {
        "NOTAMs (loading…)".to_string()
    } else if important > 0 {
        format!("NOTAMs ({} · {important} important)", rn.items.len())
    } else {
        format!("NOTAMs ({})", rn.items.len())
    };
    let items: Vec<Notam> = rn.items.clone();
    let errors = rn.errors.clone();
    let configured = app.keys.notams;
    widgets::fold(ui, &t, "route-notams", &title, false, |ui| {
        if !configured {
            setup_hint(ui, app);
            return;
        }
        if !errors.is_empty() && items.is_empty() {
            widgets::para(ui, format!("Couldn't get NOTAMs: {}", errors[0]), t.bad, 11.5);
        }
        let raw = app.saved.raw_wx;
        let all_id = egui::Id::new("route-notams-all");
        let all: bool = ui.data(|d| d.get_temp(all_id)).unwrap_or(false);
        let shown = if all { items.len() } else { items.len().min(12) };
        for n in &items[..shown] {
            row(ui, &t, n, raw);
        }
        ui.horizontal(|ui| {
            if items.len() > 12 && widgets::link(ui, &t, if all { "Show fewer".to_string() } else { format!("Show all {}", items.len()) }).clicked() {
                ui.data_mut(|d| d.insert_temp(all_id, !all));
            }
            if widgets::link(ui, &t, if raw { "Plain English" } else { "Raw text" }).clicked() {
                app.saved.raw_wx = !raw;
            }
        });
        faint(ui, &t, "Airports and navaids on the route, a 10 NM corridor and the ARTCCs crossed.");
    });
}

/// One airport's NOTAMs (the airport card). Returns (toggle raw text, open Settings).
pub fn airport_tab(ui: &mut egui::Ui, t: &Tokens, configured: bool, raw: bool, v: Option<&serde_json::Value>) -> (bool, bool) {
    if !configured {
        widgets::para(ui, "NOTAMs come from the FAA NOTAM Management Service. Access is free but by request: email notams@faa.gov, then paste the client ID and secret they send in Settings.", t.text2, 11.5);
        let mut settings = false;
        ui.horizontal(|ui| {
            if widgets::link(ui, t, format!("{} Email the FAA", ph::ENVELOPE_SIMPLE)).clicked() {
                let _ = open::that(REQUEST_MAILTO);
            }
            settings = widgets::link(ui, t, format!("{} Add it in Settings", ph::GEAR_SIX)).clicked();
        });
        return (false, settings);
    }
    let Some(v) = v else {
        faint(ui, t, "Loading NOTAMs…");
        return (false, false);
    };
    let (_, items, errors) = crate::wx::notams::parse(v);
    if let Some(e) = errors.first().filter(|_| items.is_empty()) {
        widgets::para(ui, format!("Couldn't get NOTAMs: {e}"), t.bad, 11.5);
    }
    if items.is_empty() && errors.is_empty() {
        faint(ui, t, "No NOTAMs in effect.");
    }
    for n in &items {
        row(ui, t, n, raw);
    }
    let toggle = !items.is_empty() && widgets::link(ui, t, if raw { "Plain English" } else { "Raw text" }).clicked();
    (toggle, false)
}
