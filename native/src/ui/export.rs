//! Send the route elsewhere without filing anything: FltPlan Go (scan a QR code of the route
//! string, plus a KML of user waypoints to import as custom waypoints), FltPlan.com (each flight
//! plan field ready to copy; saved, not filed, plans sync to FltPlan Go), and files: Garmin .fpl
//! for avionics / Garmin Pilot / ForeFlight, GPX and KML.

use super::widgets::{self, faint};
use crate::app::App;
use crate::io::flightplan::{fltplan_fields, fltplan_go, to_fpl, to_gpx, to_kml};
use crate::theme::{self, Tokens};
use egui::{vec2, Color32, CornerRadius, Frame, Margin, RichText, Sense};
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

/// A QR code drawn with the painter (dark modules on a white quiet zone).
fn qr(ui: &mut egui::Ui, text: &str, size: f32) {
    let Ok(code) = qrcode::QrCode::new(text.as_bytes()) else { return };
    let n = code.width();
    let colors = code.to_colors();
    let (rect, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(8), Color32::WHITE);
    let quiet = 2.0;
    let cell = size / (n as f32 + quiet * 2.0);
    for (i, c) in colors.iter().enumerate() {
        if *c == qrcode::Color::Dark {
            let (x, y) = ((i % n) as f32, (i / n) as f32);
            let r = egui::Rect::from_min_size(rect.min + vec2((x + quiet) * cell, (y + quiet) * cell), vec2(cell + 0.3, cell + 0.3));
            p.rect_filled(r, CornerRadius::ZERO, Color32::from_rgb(20, 22, 26));
        }
    }
}

fn step(ui: &mut egui::Ui, t: &Tokens, n: usize, text: &str) {
    ui.horizontal_top(|ui| {
        ui.label(RichText::new(format!("{n}")).font(theme::bold(12.0)).color(t.accent));
        ui.add(egui::Label::new(RichText::new(text).size(12.0).color(t.text2)).wrap());
    });
}

pub fn show(app: &mut App, ctx: &egui::Context) -> bool {
    let t = app.t;
    let places: Vec<crate::model::place::Place> = app.saved.route.waypoints.iter().map(|w| w.place.clone()).collect();
    let refs: Vec<&crate::model::place::Place> = places.iter().collect();
    let ete = app.plan.log.as_ref().map(|l| l.total_time);
    let f = fltplan_fields(&refs, app.saved.route.cruise, app.fleet.active(), ete);
    let go = fltplan_go(&refs);
    let name = if f.destination.is_empty() { f.departure.clone() } else { format!("{} to {}", f.departure, f.destination) };
    let mut open = true;
    let mut action: Option<&str> = None;
    let modal = egui::Modal::new(egui::Id::new("export")).show(ctx, |ui| {
        ui.set_width(560.0);
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new("Export route").font(theme::bold(17.0)));
                faint(ui, &t, format!("{name} · nothing is filed with the FAA"));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                if widgets::icon_button(ui, &t, ph::X, "Close").clicked() {
                    open = false;
                }
            });
        });
        ui.add_space(8.0);

        // ---- FltPlan Go ----
        Frame::new().fill(t.surface2).corner_radius(CornerRadius::same(12)).inner_margin(Margin::same(14)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                qr(ui, &go.route, 150.0);
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new(format!("{}  FltPlan Go", ph::DEVICE_TABLET)).font(theme::bold(14.0)));
                    ui.add_space(2.0);
                    step(ui, &t, 1, "Point your iPad or iPhone camera at the code and tap Copy.");
                    step(ui, &t, 2, "In FltPlan Go, start a new flight plan and paste the route. Save it: there's no need to file.");
                    ui.add_space(4.0);
                    Frame::new().fill(t.surface_solid).corner_radius(CornerRadius::same(8)).inner_margin(Margin::symmetric(10, 6)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.add(egui::Label::new(RichText::new(&go.route).font(theme::mono(12.5))).wrap());
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.add(egui::Button::new(RichText::new(ph::COPY).size(14.0)).frame(false)).on_hover_text("Copy the route").clicked() {
                                    widgets::copy(ui, &go.route);
                                    action = Some("copied");
                                }
                            });
                        });
                    });
                    if !go.custom.is_empty() {
                        faint(ui, &t, "Lat/long points are written as coordinates (3430N11230W).");
                    }
                });
            });
        });

        // ---- FltPlan.com ----
        widgets::fold(ui, &t, "export-fltplan", "FltPlan.com form fields", false, |ui| {
            widgets::para(ui, "Fill FltPlan.com's flight plan form from these values and press Save, not File: saved plans sync to FltPlan Go too.", t.text3, 11.5);
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
            ui.horizontal(|ui| {
                if widgets::ghost(ui, &t, format!("{}  Copy all", ph::COPY)).clicked() {
                    let text = f.rows().iter().filter(|(_, v)| !v.is_empty()).map(|(k, v)| format!("{k}: {v}")).collect::<Vec<_>>().join("\n");
                    widgets::copy(ui, &text);
                    action = Some("copied");
                }
                if widgets::ghost(ui, &t, format!("{}  Open FltPlan.com", ph::ARROW_SQUARE_OUT)).clicked() {
                    let _ = open::that("https://www.fltplan.com/");
                }
            });
        });

        // ---- files ----
        ui.add_space(10.0);
        ui.label(RichText::new("FILES").font(theme::semibold(11.0)).color(t.text3).extra_letter_spacing(0.6));
        ui.add_space(4.0);
        let w = (ui.available_width() - 20.0) / 3.0;
        ui.horizontal(|ui| {
            if widgets::card(ui, &t, ph::FILE_ARROW_DOWN, "Garmin .fpl", "Avionics SD card / Flight Stream, Garmin Pilot, ForeFlight, SkyVector", w).clicked() {
                action = Some("fpl");
            }
            if widgets::card(ui, &t, ph::FILE_ARROW_DOWN, "GPX", "Dynon, Avidyne, handheld GPS and mapping apps", w).clicked() {
                action = Some("gpx");
            }
            if widgets::card(ui, &t, ph::GLOBE_HEMISPHERE_WEST, "KML", "Google Earth; also FltPlan Go custom waypoints", w).clicked() {
                action = Some("kml");
            }
        });
        ui.add_space(4.0);
        faint(ui, &t, format!("{} Check the plan on the receiving device before flight.", ph::INFO));
    });
    let all: Vec<(String, f64, f64)> = refs.iter().zip(go.route.split(' ')).map(|(p, id)| (id.to_string(), p.lat, p.lon)).collect();
    match action {
        Some("copied") => app.toast("Copied"),
        Some("fpl") => save(app, &name, "fpl", "Garmin flight plan", to_fpl(&refs, &name)),
        Some("gpx") => save(app, &name, "gpx", "GPX route", to_gpx(&refs, &name)),
        Some("kml") => save(app, &name, "kml", "KML", to_kml(&all, &refs, &name)),
        _ => {}
    }
    if modal.should_close() {
        open = false;
    }
    open
}
