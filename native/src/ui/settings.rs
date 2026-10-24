//! Settings: chart data (install / update), API keys, and re-running setup.

use super::widgets::{self, section};
use crate::app::{App, Panel};
use egui_phosphor::regular as ph;

pub fn panel(app: &mut App, ctx: &egui::Context) {
    let t = app.t;
    let close = widgets::side_panel(ctx, &t, "panel-settings", "Settings", "Chart data and API keys", 380.0, app, |_, _| false, |ui, app| {
        section(ui, &t, "Chart data");
        super::data::show(app, ui, true);
        section(ui, &t, "API keys");
        super::keys::form(app, ui);
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
