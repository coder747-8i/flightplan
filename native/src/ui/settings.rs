//! Settings: personal minimums for the go / no-go call, chart data (install / update), API keys,
//! and re-running setup.

use super::widgets::{self, faint, section};
use crate::app::{App, Panel};
use crate::nav::verdict::Minimums;
use crate::theme;
use egui::RichText;
use egui_phosphor::regular as ph;

fn row(ui: &mut egui::Ui, t: &theme::Tokens, label: &str, hint: &str, v: &mut f64, unit: &str, range: std::ops::RangeInclusive<f64>, speed: f64) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).size(12.5)).on_hover_text(hint);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(unit).size(11.0).color(t.text3));
            ui.add(egui::DragValue::new(v).range(range).speed(speed).max_decimals(1));
        });
    });
}

/// The pilot's personal minimums: what turns the call yellow or red.
pub fn minimums(app: &mut App, ui: &mut egui::Ui) {
    let t = app.t;
    let m = &mut app.saved.minimums;
    faint(ui, &t, "Used for the go / no-go call. Below a minimum is a caution; past a hard limit is a no-go.");
    ui.add_space(4.0);
    row(ui, &t, "Ceiling", "Caution below this ceiling at departure or destination (ft AGL)", &mut m.ceiling_ft, "ft", 500.0..=10000.0, 100.0);
    row(ui, &t, "Visibility", "Caution below this visibility (statute miles)", &mut m.vis_sm, "SM", 1.0..=10.0, 0.5);
    row(ui, &t, "Wind", "No-go above this steady surface wind (kt)", &mut m.wind_kt, "kt", 5.0..=50.0, 1.0);
    row(ui, &t, "Gusts", "No-go above this gust (kt)", &mut m.gust_kt, "kt", 5.0..=60.0, 1.0);
    row(ui, &t, "Gust spread", "Caution when gusts exceed the steady wind by more than this (kt)", &mut m.gust_spread_kt, "kt", 3.0..=30.0, 1.0);
    row(ui, &t, "Crosswind", "Caution above this share of the aircraft's demonstrated crosswind; above 100% is a no-go", &mut m.xwind_pct, "%", 30.0..=100.0, 1.0);
    row(ui, &t, "Thunder: caution", "Caution at or above this probability of thunderstorms", &mut m.tstm_caution, "%", 5.0..=80.0, 1.0);
    row(ui, &t, "Thunder: no-go", "No-go at or above this probability of thunderstorms", &mut m.tstm_nogo, "%", 5.0..=100.0, 1.0);
    row(ui, &t, "Density altitude", "Caution above this density altitude at departure (ft)", &mut m.density_alt_ft, "ft", 2000.0..=12000.0, 100.0);
    ui.add_space(2.0);
    widgets::toggle_line(ui, &t, &mut m.night_ok, "Night flight OK", "Off: a night arrival is a no-go instead of a caution", None);
    widgets::toggle_line(ui, &t, &mut m.ifr, "Instrument rated and current", "On: IFR or LIFR at the ends is a caution instead of a no-go", None);
    if m.tstm_nogo < m.tstm_caution {
        m.tstm_nogo = m.tstm_caution;
    }
    if *m != Minimums::default() && widgets::link(ui, &t, "Reset to defaults").clicked() {
        *m = Minimums::default();
    }
}

pub fn panel(app: &mut App, ctx: &egui::Context) {
    let t = app.t;
    let close = widgets::side_panel(ctx, &t, "panel-settings", "Settings", "Minimums, chart data and API keys", 380.0, app, |_, _| false, |ui, app| {
        widgets::fold(ui, &t, "settings-minimums", "Personal minimums", true, |ui| minimums(app, ui));
        widgets::fold(ui, &t, "settings-data", "Chart data", true, |ui| super::data::show(app, ui, true));
        let keys_open = !app.keys.notams;
        widgets::fold(ui, &t, "settings-keys", "API keys (FAA NOTAMs)", keys_open, |ui| super::keys::form(app, ui));
        section(ui, &t, "Setup");
        if widgets::button(ui, format!("{}  Run the setup screen again", ph::SPARKLE)).clicked() {
            app.onboarding.reopen(0);
        }
        widgets::faint(ui, &t, format!("Data folder: {}", app.be.data_dir.display()));
    });
    if close {
        app.panel = Panel::None;
    }
}
