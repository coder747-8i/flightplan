//! Send the route elsewhere: FltPlan.com (each flight plan field ready to copy), Garmin .fpl for
//! avionics / Garmin Pilot / ForeFlight, or GPX.

use super::widgets::{self, faint};
use crate::app::App;
use crate::io::flightplan::{fltplan_fields, to_fpl, to_gpx};
use crate::theme;
use egui::{vec2, CornerRadius, Frame, Margin, RichText};
use egui_phosphor::regular as ph;

fn save(app: &mut App, name: &str, ext: &str, label: &str, body: String) {
    let file = format!("{}.{ext}", super::aircraft::slug(name));
    if let Some(path) = rfd::FileDialog::new().set_title("Export route").set_file_name(&file).add_filter(label, &[ext]).save_file() {
        match std::fs::write(&path, body) {
            Ok(()) => app.toast(format!("Saved {}", path.display())),
            Err(e) => app.toast(format!("Couldn't save: {e}")),
        }
    }
}

pub fn show(app: &mut App, ctx: &egui::Context) -> bool {
    let t = app.t;
    let places: Vec<crate::model::place::Place> = app.saved.route.waypoints.iter().map(|w| w.place.clone()).collect();
    let refs: Vec<&crate::model::place::Place> = places.iter().collect();
    let f = fltplan_fields(&refs, app.saved.route.cruise, app.fleet.active(), app.plan.log.as_ref().map(|l| l.total_time));
    let name = if f.destination.is_empty() { f.departure.clone() } else { format!("{} to {}", f.departure, f.destination) };
    let mut open = true;
    let mut action: Option<&str> = None;
    let modal = egui::Modal::new(egui::Id::new("export")).show(ctx, |ui| {
        ui.set_width(540.0);
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new("Export route").font(theme::bold(17.0)));
                faint(ui, &t, &name);
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                if widgets::icon_button(ui, &t, ph::X, "Close").clicked() {
                    open = false;
                }
            });
        });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("FltPlan.com").font(theme::bold(14.0)));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::primary(ui, &t, format!("{}  Open FltPlan.com", ph::ARROW_SQUARE_OUT)).clicked() {
                    let _ = open::that("https://www.fltplan.com/");
                }
            });
        });
        widgets::para(ui, "FltPlan.com files from what you type into its flight plan form. Copy each value across, or copy the whole route into its route box. Plans you save there sync to FltPlan Go.", t.text3, 11.5);
        ui.add_space(4.0);
        Frame::new().fill(t.surface2).corner_radius(CornerRadius::same(9)).inner_margin(Margin::symmetric(10, 8)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.add(egui::Label::new(RichText::new(&f.full).font(theme::mono(12.5))).wrap());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::small_button(ui, format!("{}  Copy route", ph::COPY)).clicked() {
                        widgets::copy(ui, &f.full);
                        action = Some("copied");
                    }
                });
            });
        });
        ui.add_space(4.0);
        egui::Grid::new("fltplan").num_columns(3).spacing(vec2(12.0, 2.0)).show(ui, |ui| {
            for (label, value) in f.rows() {
                if value.is_empty() {
                    continue;
                }
                ui.label(RichText::new(label).size(12.0).color(t.text3));
                ui.label(RichText::new(value).font(theme::mono(12.5)));
                if ui.add(egui::Button::new(RichText::new(ph::COPY).size(13.0)).frame(false)).on_hover_text(format!("Copy {}", label.to_lowercase())).clicked() {
                    widgets::copy(ui, value);
                    action = Some("copied");
                }
                ui.end_row();
            }
        });
        if widgets::ghost(ui, &t, format!("{}  Copy all as text", ph::COPY)).clicked() {
            let text = f.rows().iter().filter(|(_, v)| !v.is_empty()).map(|(k, v)| format!("{k}: {v}")).collect::<Vec<_>>().join("\n");
            widgets::copy(ui, &text);
            action = Some("copied");
        }
        widgets::rule(ui, &t);
        ui.label(RichText::new("Files").font(theme::bold(14.0)));
        ui.add_space(4.0);
        let w = (ui.available_width() - 10.0) / 2.0;
        ui.horizontal(|ui| {
            if widgets::card(ui, &t, ph::FILE_ARROW_DOWN, "Garmin .fpl", "Garmin avionics (SD card / Flight Stream), Garmin Pilot, ForeFlight, SkyVector", w).clicked() {
                action = Some("fpl");
            }
            if widgets::card(ui, &t, ph::FILE_ARROW_DOWN, "GPX", "Dynon, Avidyne, handheld GPS units and most mapping apps", w).clicked() {
                action = Some("gpx");
            }
        });
        ui.add_space(4.0);
        faint(ui, &t, format!("{} User waypoints are written as lat/long. Check the plan on the receiving device before flight.", ph::INFO));
    });
    match action {
        Some("copied") => app.toast("Copied"),
        Some("fpl") => save(app, &name, "fpl", "Garmin flight plan", to_fpl(&refs, &name)),
        Some("gpx") => save(app, &name, "gpx", "GPX route", to_gpx(&refs, &name)),
        _ => {}
    }
    if modal.should_close() {
        open = false;
    }
    open
}
