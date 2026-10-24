//! Help: the tour, the setup screen, shortcuts and what's installed.

use super::widgets::{self, section};
use crate::app::{App, Panel};
use egui::{vec2, RichText};
use egui_phosphor::regular as ph;

pub fn panel(app: &mut App, ctx: &egui::Context) {
    let t = app.t;
    let tour = std::cell::Cell::new(false);
    let setup = std::cell::Cell::new(false);
    let close = widgets::side_panel(ctx, &t, "panel-help", "Help", "Tour, setup and shortcuts", 340.0, app, |_, _| false, |ui, app| {
        let w = (ui.available_width() - 8.0) / 2.0;
        ui.horizontal(|ui| {
            if widgets::card(ui, &t, ph::SPARKLE, "Tour the screen", "A minute, one control at a time", w).clicked() {
                tour.set(true);
            }
            if widgets::card(ui, &t, ph::ROCKET_LAUNCH, "Setup screen", "Walkthrough, chart data, keys, aircraft", w).clicked() {
                setup.set(true);
            }
        });
        section(ui, &t, "Shortcuts");
        egui::Grid::new("keys").num_columns(2).spacing(vec2(12.0, 6.0)).show(ui, |ui| {
            for (k, v) in [("/", "Search airports, navaids and fixes"), ("Enter", "Load the route typed in the Route panel"), ("Shift+Enter", "Add a search result to the route"), ("Scroll / pinch", "Zoom the chart"), ("Double-click", "Zoom in"), ("Esc", "Close dialogs and the tour")] {
                ui.label(RichText::new(k).font(crate::theme::mono(11.5)));
                ui.label(RichText::new(v).size(12.5));
                ui.end_row();
            }
        });
        section(ui, &t, "About");
        egui::Grid::new("about").num_columns(2).spacing(vec2(12.0, 6.0)).show(ui, |ui| {
            widgets::kv(ui, &t, "Version", format!("{} (desktop)", env!("CARGO_PKG_VERSION")));
            let data = match &app.data_info {
                Some(i) => format!("{}{}", i.cycle.as_ref().map(|c| format!("FAA cycle {c}")).unwrap_or_else(|| "built locally".into()), if app.data_expired() { " · out of date".to_string() } else { i.expires.as_ref().map(|e| format!(" · until {e}")).unwrap_or_default() }),
                None => "not installed".into(),
            };
            widgets::kv(ui, &t, "Chart data", data);
        });
        ui.add_space(12.0);
        widgets::para(ui, "For planning only. Flightplan is not a substitute for an official briefing (1800wxbrief.com), current charts, NOTAMs or the aircraft's POH. Data: FAA NASR, FAA AIS, FAA Digital Obstacle File, aviationweather.gov, OpenStreetMap / OpenFreeMap, Mapzen terrain, Overture Maps, Open-Meteo.", t.text3, 11.0);
    });
    if tour.get() {
        app.panel = Panel::None;
        app.tour = Some(super::tour::Tour::default());
    } else if setup.get() {
        app.panel = Panel::None;
        app.onboarding.reopen(0);
    } else if close {
        app.panel = Panel::None;
    }
}
