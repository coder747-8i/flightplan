//! Import aircraft from files (or pasted text): shows what each entry contains, what was filled
//! in with defaults and what's missing, before anything is added. An aircraft with a tail
//! number already in the hangar updates that one instead of adding a copy.

use super::widgets::{self, faint, Tone};
use crate::app::App;
use crate::io::aircraft_file::{self, ImportResult};
use crate::theme;
use egui::{vec2, CornerRadius, Frame, Margin, RichText, Stroke};
use egui_phosphor::regular as ph;

#[derive(Default)]
pub struct Import {
    results: Vec<ImportResult>,
    pasted: String,
    show_paste: bool,
    added: usize,
}

fn existing<'a>(app: &'a App, r: &ImportResult) -> Option<&'a crate::model::aircraft::Aircraft> {
    let tail = r.aircraft.as_ref()?.tail.trim().to_uppercase();
    if tail.is_empty() {
        return None;
    }
    app.fleet.list.iter().find(|a| a.tail.trim().to_uppercase() == tail)
}

fn read_paths(imp: &mut Import, paths: Vec<std::path::PathBuf>) {
    for p in paths {
        let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        match std::fs::read_to_string(&p) {
            Ok(text) => imp.results.extend(aircraft_file::parse(&text, &name)),
            Err(e) => imp.results.push(ImportResult { label: name, aircraft: None, errors: vec![e.to_string()], warnings: vec![] }),
        }
    }
}

pub fn show(app: &mut App, ctx: &egui::Context, imp: &mut Import) -> bool {
    let t = app.t;
    let mut open = true;
    // files dropped on the window
    let dropped: Vec<std::path::PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
    if !dropped.is_empty() {
        read_paths(imp, dropped);
    }
    let hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());
    let modal = egui::Modal::new(egui::Id::new("import")).show(ctx, |ui| {
        ui.set_width(540.0);
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new("Import aircraft").font(theme::bold(17.0)));
                faint(ui, &t, "Flightplan aircraft files (.fpaircraft.json) or JSON in the same format");
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                if widgets::icon_button(ui, &t, ph::X, "Close").clicked() {
                    open = false;
                }
            });
        });
        ui.add_space(8.0);
        let drop = Frame::new().stroke(Stroke::new(1.5, if hovering { t.accent } else { t.line_strong })).fill(if hovering { t.accent_soft } else { egui::Color32::TRANSPARENT }).corner_radius(CornerRadius::same(12)).inner_margin(Margin::same(18)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(ph::FILE_ARROW_UP).size(24.0).color(t.text2));
                ui.horizontal(|ui| {
                    ui.add_space((ui.available_width() - 230.0).max(0.0) / 2.0);
                    ui.label("Drop files on the window or");
                    if widgets::link(ui, &t, "choose files").clicked() {
                        if let Some(paths) = rfd::FileDialog::new().set_title("Import aircraft").add_filter("Aircraft files", &["json"]).pick_files() {
                            read_paths(imp, paths);
                        }
                    }
                });
                faint(ui, &t, "You can pick several at once, or a file with a whole fleet.");
            });
        });
        let _ = drop;
        ui.add_space(6.0);
        if widgets::ghost(ui, &t, format!("{}  Paste aircraft JSON instead", ph::CLIPBOARD_TEXT)).clicked() {
            imp.show_paste = !imp.show_paste;
        }
        if imp.show_paste {
            ui.add(egui::TextEdit::multiline(&mut imp.pasted).font(theme::mono(12.0)).desired_rows(5).desired_width(f32::INFINITY).hint_text("{ \"format\": \"flightplan-aircraft\", \"aircraft\": [ … ] }"));
            if ui.add_enabled(!imp.pasted.trim().is_empty(), egui::Button::new("Read pasted text")).clicked() {
                imp.results.extend(aircraft_file::parse(&imp.pasted, "pasted text"));
                imp.pasted.clear();
            }
        }
        if imp.added > 0 {
            widgets::status(ui, &t, Tone::Ok, ph::CHECK_CIRCLE, &[format!("Added or updated {} aircraft", imp.added)]);
        }
        egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
            let mut remove = None;
            for (i, r) in imp.results.iter().enumerate() {
                let bad = r.aircraft.is_none();
                Frame::new().stroke(Stroke::new(1.0, if bad { t.bad.gamma_multiply(0.4) } else { t.line_strong })).corner_radius(CornerRadius::same(10)).inner_margin(Margin::same(10)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(if bad { ph::OCTAGON } else { ph::CHECK_CIRCLE }).color(if bad { t.bad } else { t.ok }));
                        ui.label(RichText::new(&r.label).font(theme::semibold(13.0)));
                        if let Some(a) = &r.aircraft {
                            faint(ui, &t, format!("{} · {} kt · {} stations", a.type_code, a.perf.tas, a.wb.stations.len()));
                        }
                        if existing(app, r).is_some() {
                            widgets::chip(ui, &t, "updates yours", None, None);
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if widgets::ghost(ui, &t, "Remove").clicked() {
                                remove = Some(i);
                            }
                        });
                    });
                    for e in &r.errors {
                        widgets::para(ui, e, t.bad, 12.0);
                    }
                    for w in &r.warnings {
                        widgets::para(ui, format!("{}  {w}", ph::WARNING), t.warn, 12.0);
                    }
                });
                ui.add_space(4.0);
            }
            if let Some(i) = remove {
                imp.results.remove(i);
            }
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            faint(ui, &t, "Need to type an airframe up by hand?");
            if widgets::link(ui, &t, format!("{} Get a template file", ph::DOWNLOAD_SIMPLE)).clicked() {
                if let Some(path) = rfd::FileDialog::new().set_file_name(format!("aircraft-template.{}", aircraft_file::EXT)).save_file() {
                    let _ = std::fs::write(path, aircraft_file::template_file());
                }
            }
        });
        faint(ui, &t, "The format is described in docs/aircraft-format.md.");
        widgets::rule(ui, &t);
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let ok: Vec<usize> = imp.results.iter().enumerate().filter(|(_, r)| r.aircraft.is_some()).map(|(i, _)| i).collect();
                let updates = ok.iter().filter(|&&i| existing(app, &imp.results[i]).is_some()).count();
                let label = if updates == ok.len() && updates > 0 {
                    format!("Update {updates} aircraft")
                } else if updates > 0 {
                    format!("Add {} aircraft ({updates} updated)", ok.len())
                } else {
                    format!("Add {} aircraft", ok.len())
                };
                if ui.add_enabled(!ok.is_empty(), egui::Button::new(RichText::new(label).color(egui::Color32::WHITE).font(theme::semibold(13.0))).fill(t.accent).min_size(vec2(0.0, 32.0))).clicked() {
                    for &i in &ok {
                        let r = &imp.results[i];
                        let mut a = r.aircraft.clone().unwrap();
                        if let Some(old) = existing(app, r) {
                            a.id = old.id.clone();
                            for s in &a.wb.stations {
                                if let Some(v) = old.load.get(&s.id) {
                                    a.load.insert(s.id.clone(), *v);
                                }
                            }
                        }
                        app.fleet.upsert(a);
                    }
                    imp.added = ok.len();
                    imp.results.retain(|r| r.aircraft.is_none());
                    if imp.results.is_empty() {
                        open = false;
                        app.toast(format!("Added or updated {} aircraft", imp.added));
                    }
                }
                if widgets::button(ui, "Close").clicked() {
                    open = false;
                }
            });
        });
    });
    if modal.should_close() {
        open = false;
    }
    open
}
