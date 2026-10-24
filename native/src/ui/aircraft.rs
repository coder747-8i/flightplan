//! Aircraft panel: pick an airframe, load it for this flight, weight & balance, performance and
//! W&B data; create / edit (guided), duplicate, import and export.

use super::tour::mark;
use super::widgets::{self, faint, section};
use crate::app::{App, Dialog, Panel};
use crate::io::aircraft_file;
use crate::model::aircraft::{compute_wb, Station, StationKind, FUEL_TYPES};
use crate::theme;
use egui::RichText;
use egui_phosphor::regular as ph;

pub fn export(app: &mut App, all: bool) {
    let list: Vec<_> = if all { app.fleet.list.iter().collect() } else { app.fleet.active().into_iter().collect() };
    if list.is_empty() {
        return;
    }
    let name = if all {
        "flightplan-fleet".to_string()
    } else if !list[0].tail.is_empty() {
        list[0].tail.clone()
    } else {
        list[0].name.clone()
    };
    let file = format!("{}.{}", slug(&name), aircraft_file::EXT);
    let text = aircraft_file::to_file(&list);
    if let Some(path) = rfd::FileDialog::new().set_title("Export aircraft").set_file_name(&file).add_filter("Flightplan aircraft", &["json"]).save_file() {
        match std::fs::write(&path, text) {
            Ok(()) => app.toast(format!("Saved {}", path.display())),
            Err(e) => app.toast(format!("Export failed: {e}")),
        }
    }
}

pub fn slug(s: &str) -> String {
    let v: String = s.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '.' { c } else { '-' }).collect();
    let v = v.trim_matches('-').to_string();
    if v.is_empty() { "untitled".into() } else { v }
}

pub fn panel(app: &mut App, ctx: &egui::Context) {
    let t = app.t;
    let act: std::cell::Cell<Option<&str>> = std::cell::Cell::new(None);
    let close = widgets::side_panel(
        ctx,
        &t,
        "panel-aircraft",
        "Aircraft",
        "Performance and weight & balance",
        380.0,
        app,
        |ui, app| {
            ui.menu_button(RichText::new(ph::DOWNLOAD_SIMPLE).size(15.0), |ui| {
                if ui.button("Export this aircraft…").clicked() {
                    act.set(Some("export1"));
                    ui.close();
                }
                if ui.button(format!("Export all {} aircraft…", app.fleet.list.len())).clicked() {
                    act.set(Some("exportall"));
                    ui.close();
                }
            })
            .response
            .on_hover_text("Export aircraft");
            if ui.add(egui::Button::new(RichText::new(ph::UPLOAD_SIMPLE).size(15.0)).frame(false)).on_hover_text("Import aircraft files").clicked() {
                act.set(Some("import"));
            }
            let r = ui.add(egui::Button::new(RichText::new(ph::PLUS).size(16.0)).frame(false)).on_hover_text("New aircraft (guided)");
            mark(ui.ctx(), "aircraft-new", r.rect);
            if r.clicked() {
                act.set(Some("new"));
            }
            false
        },
        |ui, app| body(app, ui, &act),
    );
    match act.get() {
        Some("export1") => export(app, false),
        Some("exportall") => export(app, true),
        Some("import") => app.dialog = Some(Dialog::Import(Box::default())),
        Some("new") => app.dialog = Some(Dialog::Wizard(Box::new(super::wizard::Wizard::new(None)))),
        Some("edit") => {
            let a = app.fleet.active().cloned();
            app.dialog = Some(Dialog::Wizard(Box::new(super::wizard::Wizard::new(a))));
        }
        _ => {}
    }
    if close {
        app.panel = Panel::None;
    }
}

fn body(app: &mut App, ui: &mut egui::Ui, act: &std::cell::Cell<Option<&str>>) {
    let t = app.t;
    let label = |a: &crate::model::aircraft::Aircraft| format!("{} · {}", if a.tail.is_empty() { "No tail" } else { &a.tail }, a.name);
    let current = app.fleet.active().map(label).unwrap_or_default();
    egui::ComboBox::from_id_salt("ac-pick").selected_text(current).width(ui.available_width()).show_ui(ui, |ui| {
        for a in &app.fleet.list {
            ui.selectable_value(&mut app.fleet.active, a.id.clone(), label(a));
        }
    });
    let id = app.fleet.active().map(|a| a.id.clone()).unwrap_or_default();
    ui.horizontal(|ui| {
        if widgets::small_button(ui, format!("{}  Edit step by step", ph::PENCIL_SIMPLE)).clicked() {
            act.set(Some("edit"));
        }
        if widgets::small_button(ui, format!("{}  Duplicate", ph::COPY)).clicked() {
            app.fleet.duplicate(&id);
        }
        if app.fleet.list.len() > 1 && ui.add(egui::Button::new(RichText::new(ph::TRASH).color(t.bad)).frame(false)).on_hover_text("Delete this aircraft").clicked() {
            ui.memory_mut(|m| m.data.insert_temp(egui::Id::new("confirm-delete"), id.clone()));
        }
    });
    let confirm: Option<String> = ui.memory(|m| m.data.get_temp(egui::Id::new("confirm-delete")));
    if let Some(cid) = confirm {
        widgets::status(ui, &t, widgets::Tone::Bad, ph::TRASH, &["Delete this aircraft?".into()]);
        ui.horizontal(|ui| {
            if widgets::button(ui, "Delete").clicked() {
                app.fleet.remove(&cid);
                ui.memory_mut(|m| m.data.remove::<String>(egui::Id::new("confirm-delete")));
            }
            if widgets::ghost(ui, &t, "Keep it").clicked() {
                ui.memory_mut(|m| m.data.remove::<String>(egui::Id::new("confirm-delete")));
            }
        });
    }
    let trip = app.plan.log.as_ref().map(|l| l.trip_fuel).unwrap_or(0.0);
    let Some(ac) = app.fleet.active_mut() else { return };

    ui.add_space(6.0);
    egui::Grid::new("ac-id").num_columns(2).spacing(egui::vec2(10.0, 6.0)).show(ui, |ui| {
        faint(ui, &t, "Tail number");
        ui.add(egui::TextEdit::singleline(&mut ac.tail).desired_width(140.0));
        ui.end_row();
        faint(ui, &t, "Type (ICAO)");
        ui.add(egui::TextEdit::singleline(&mut ac.type_code).desired_width(140.0));
        ui.end_row();
        faint(ui, &t, "Name");
        ui.add(egui::TextEdit::singleline(&mut ac.name).desired_width(220.0));
        ui.end_row();
    });

    section(ui, &t, "Loading for this flight");
    for s in ac.wb.stations.clone() {
        let v = ac.load.entry(s.id.clone()).or_insert(0.0);
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new(&s.name).size(12.5));
                if let Some(m) = s.max {
                    ui.label(RichText::new(format!("max {} {}", crate::model::aircraft::fmt_num(m), if s.kind == StationKind::Fuel { "gal" } else { "lb" })).size(10.5).color(t.text3));
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(if s.kind == StationKind::Fuel { "gal" } else { "lb" }).size(11.0).color(t.text3));
                ui.add(egui::DragValue::new(v).speed(if s.kind == StationKind::Fuel { 0.5 } else { 5.0 }).range(0.0..=5000.0));
            });
        });
    }
    let wb = compute_wb(ac, trip);
    ui.add_space(6.0);
    if wb.problems.is_empty() {
        widgets::status(ui, &t, widgets::Tone::Ok, ph::CHECK_CIRCLE, &["Within limits at takeoff and landing".into()]);
    } else {
        widgets::status(ui, &t, widgets::Tone::Bad, ph::OCTAGON, &wb.problems);
    }
    ui.add_space(6.0);
    super::wbchart::show(ui, &t, ac, &wb, 220.0);
    egui::Grid::new("wb-sum").num_columns(3).striped(false).spacing(egui::vec2(18.0, 4.0)).show(ui, |ui| {
        ui.label("");
        ui.label(RichText::new("Weight").size(11.0).color(t.text3));
        ui.label(RichText::new("CG").size(11.0).color(t.text3));
        ui.end_row();
        for (label, pt, bold) in [("Ramp", &wb.ramp, false), ("Takeoff", &wb.takeoff, true), (&*format!("Landing (−{trip:.1} gal)"), &wb.landing, false), ("Zero fuel", &wb.zero_fuel, false)] {
            ui.label(RichText::new(label).size(12.0));
            let w = if bold { format!("{} / {}", pt.weight.round(), ac.wb.max_takeoff) } else { format!("{}", pt.weight.round()) };
            ui.label(RichText::new(w).font(if bold { theme::bold(12.0) } else { theme::regular(12.0) }));
            ui.label(RichText::new(format!("{:.2}", pt.cg)).font(if bold { theme::bold(12.0) } else { theme::regular(12.0) }));
            ui.end_row();
        }
    });

    ui.add_space(6.0);
    egui::CollapsingHeader::new(RichText::new("Performance").font(theme::semibold(13.0))).id_salt("perf").show(ui, |ui| {
        let p = &mut ac.perf;
        egui::Grid::new("perf-grid").num_columns(2).spacing(egui::vec2(10.0, 4.0)).show(ui, |ui| {
            for (label, v, unit, speed) in [
                ("Cruise TAS", &mut p.tas, "kt", 1.0),
                ("Cruise fuel flow", &mut p.gph, "gph", 0.1),
                ("Climb rate", &mut p.climb_fpm, "fpm", 10.0),
                ("Climb speed", &mut p.climb_tas, "kt", 1.0),
                ("Climb fuel flow", &mut p.climb_gph, "gph", 0.1),
                ("Descent rate", &mut p.descent_fpm, "fpm", 10.0),
                ("Descent speed", &mut p.descent_tas, "kt", 1.0),
                ("Descent fuel flow", &mut p.descent_gph, "gph", 0.1),
                ("Taxi fuel", &mut p.taxi_gal, "gal", 0.1),
                ("Reserve", &mut p.reserve_min, "min", 5.0),
                ("Max crosswind", &mut p.max_xwind, "kt", 1.0),
            ] {
                faint(ui, &t, label);
                widgets::num(ui, v, unit, speed, 0.0..=1000.0, 1);
                ui.end_row();
            }
            faint(ui, &t, "Usable fuel");
            widgets::num(ui, &mut ac.usable_fuel, "gal", 0.5, 0.0..=1000.0, 1);
            ui.end_row();
            faint(ui, &t, "Fuel");
            egui::ComboBox::from_id_salt("fuel-type").selected_text(ac.fuel_type.clone()).show_ui(ui, |ui| {
                for f in FUEL_TYPES {
                    if ui.selectable_value(&mut ac.fuel_type, f.to_string(), f).clicked() {
                        ac.lb_per_gal = if f == "Jet A" { 6.7 } else { 6.0 };
                    }
                }
            });
            ui.end_row();
            faint(ui, &t, "Fuel weight");
            widgets::num(ui, &mut ac.lb_per_gal, "lb/gal", 0.05, 4.0..=8.0, 2);
            ui.end_row();
        });
    });
    egui::CollapsingHeader::new(RichText::new("Weight & balance data").font(theme::semibold(13.0))).id_salt("wbdata").show(ui, |ui| {
        egui::Grid::new("wb-grid").num_columns(2).spacing(egui::vec2(10.0, 4.0)).show(ui, |ui| {
            for (label, v, unit, speed) in [
                ("Empty weight", &mut ac.wb.empty_weight, "lb", 1.0),
                ("Empty arm", &mut ac.wb.empty_arm, "in", 0.01),
                ("Max ramp", &mut ac.wb.max_ramp, "lb", 1.0),
                ("Max takeoff", &mut ac.wb.max_takeoff, "lb", 1.0),
                ("Max landing", &mut ac.wb.max_landing, "lb", 1.0),
            ] {
                faint(ui, &t, label);
                widgets::num(ui, v, unit, speed, 0.0..=100000.0, 2);
                ui.end_row();
            }
        });
        section(ui, &t, "Stations");
        let mut remove = None;
        for (i, s) in ac.wb.stations.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut s.name).desired_width(120.0));
                ui.add(egui::DragValue::new(&mut s.arm).speed(0.1).suffix(" in"));
                let mut m = s.max.unwrap_or(0.0);
                if ui.add(egui::DragValue::new(&mut m).speed(1.0).prefix("max ")).changed() {
                    s.max = if m > 0.0 { Some(m) } else { None };
                }
                if ui.add(egui::Button::new(RichText::new(ph::TRASH).size(12.0)).frame(false)).clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            ac.wb.stations.remove(i);
        }
        if widgets::small_button(ui, format!("{}  Add station", ph::PLUS)).clicked() {
            ac.wb.stations.push(Station { id: crate::model::aircraft::uid(), name: "New station".into(), arm: 0.0, kind: StationKind::Baggage, max: None });
        }
        faint(ui, &t, "The envelope is edited in the step-by-step editor.");
    });
    ui.add_space(6.0);
    widgets::para(ui, "Your aircraft's actual weighing record and POH govern. Templates are typical values only.", t.text3, 11.0);
}
