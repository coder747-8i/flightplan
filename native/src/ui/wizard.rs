//! Step-by-step airframe editor: identity, performance, weights, loading stations, CG
//! envelope, then a review that runs the same checks as importing a file. Used to create
//! aircraft and to edit existing ones.

use super::widgets::{self, faint, Tone};
use crate::app::App;
use crate::io::aircraft_file::{self, guess_kind, parse_table};
use crate::model::aircraft::{self, compute_wb, Aircraft, Perf, StationKind, FUEL_TYPES};
use crate::theme;
use egui::{vec2, RichText};
use egui_phosphor::regular as ph;
use serde_json::{json, Value};

#[derive(Clone)]
struct Row {
    id: String,
    name: String,
    kind: StationKind,
    arm: Option<f64>,
    max: Option<f64>,
}

pub struct Wizard {
    edit: Option<Aircraft>,
    step: usize,
    started_from: String,
    name: String,
    tail: String,
    type_code: String,
    fuel_type: String,
    lb_per_gal: Option<f64>,
    usable: Option<f64>,
    perf: [Option<f64>; 11],
    empty_weight: Option<f64>,
    empty_arm: Option<f64>,
    max_ramp: Option<f64>,
    max_takeoff: Option<f64>,
    max_landing: Option<f64>,
    rows: Vec<Row>,
    env: Vec<(Option<f64>, Option<f64>)>,
    notes: String,
    moment: Option<f64>,
    per_1000: bool,
    paste_st: String,
    paste_env: String,
    show_paste_st: bool,
    show_paste_env: bool,
}

const PERF: [(&str, &str, &str, &str); 11] = [
    ("tas", "Cruise true airspeed", "kt", "At your usual cruise power and altitude"),
    ("gph", "Cruise fuel flow", "gph", ""),
    ("climbFpm", "Climb rate", "fpm", "Average to cruise altitude, not sea-level best"),
    ("climbTas", "Climb speed", "kt TAS", ""),
    ("climbGph", "Climb fuel flow", "gph", ""),
    ("descentFpm", "Descent rate", "fpm", ""),
    ("descentTas", "Descent speed", "kt TAS", ""),
    ("descentGph", "Descent fuel flow", "gph", ""),
    ("taxiGal", "Start, taxi & runup fuel", "gal", ""),
    ("reserveMin", "Fuel reserve", "min", "30 min day VFR, 45 min night VFR / IFR (FAR 91.151 / 91.167)"),
    ("maxXwind", "Max demonstrated crosswind", "kt", ""),
];

fn perf_array(p: &Perf) -> [Option<f64>; 11] {
    [p.tas, p.gph, p.climb_fpm, p.climb_tas, p.climb_gph, p.descent_fpm, p.descent_tas, p.descent_gph, p.taxi_gal, p.reserve_min, p.max_xwind].map(Some)
}

fn new_id() -> String {
    aircraft::uid()[..6].to_string()
}

impl Wizard {
    pub fn new(edit: Option<Aircraft>) -> Self {
        let mut w = Wizard {
            edit: None,
            step: 0,
            started_from: String::new(),
            name: String::new(),
            tail: String::new(),
            type_code: String::new(),
            fuel_type: "100LL".into(),
            lb_per_gal: Some(6.0),
            usable: None,
            perf: perf_array(&Perf::default()),
            empty_weight: None,
            empty_arm: None,
            max_ramp: None,
            max_takeoff: None,
            max_landing: None,
            rows: vec![],
            env: vec![],
            notes: String::new(),
            moment: None,
            per_1000: true,
            paste_st: String::new(),
            paste_env: String::new(),
            show_paste_st: false,
            show_paste_env: false,
        };
        if let Some(a) = edit {
            w.load(&a, false);
            w.tail = a.tail.clone();
            w.edit = Some(a);
        }
        w
    }

    fn steps(&self) -> Vec<&'static str> {
        let mut v = vec!["Identity", "Performance", "Weights", "Stations", "Envelope", "Review"];
        if self.edit.is_none() {
            v.insert(0, "Start");
        }
        v
    }

    fn load(&mut self, a: &Aircraft, keep_identity: bool) {
        if !keep_identity {
            self.name = a.name.clone();
            self.type_code = a.type_code.clone();
        }
        self.fuel_type = a.fuel_type.clone();
        self.lb_per_gal = Some(a.lb_per_gal);
        self.usable = Some(a.usable_fuel);
        self.perf = perf_array(&a.perf);
        self.empty_weight = Some(a.wb.empty_weight);
        self.empty_arm = Some(a.wb.empty_arm);
        self.max_ramp = Some(a.wb.max_ramp);
        self.max_takeoff = Some(a.wb.max_takeoff);
        self.max_landing = Some(a.wb.max_landing);
        self.rows = a.wb.stations.iter().map(|s| Row { id: s.id.clone(), name: s.name.clone(), kind: s.kind, arm: Some(s.arm), max: s.max }).collect();
        self.env = a.wb.envelope.iter().map(|p| (Some(p[0]), Some(p[1]))).collect();
        self.notes = a.notes.clone().unwrap_or_default();
    }

    fn start_from(&mut self, preset: Option<Aircraft>) {
        match preset {
            None => {
                self.started_from = "blank".into();
                self.name.clear();
                self.type_code.clear();
                self.perf = perf_array(&Perf::default());
                self.empty_weight = None;
                self.empty_arm = None;
                self.max_ramp = None;
                self.max_takeoff = None;
                self.max_landing = None;
                self.usable = None;
                self.rows = [("front", "Front seats", StationKind::Seat), ("rear", "Rear seats", StationKind::Seat), ("bag", "Baggage", StationKind::Baggage), ("fuel", "Fuel (usable)", StationKind::Fuel)]
                    .into_iter()
                    .map(|(id, n, k)| Row { id: id.into(), name: n.into(), kind: k, arm: None, max: None })
                    .collect();
                self.env = vec![(None, None); 4];
            }
            Some(p) => {
                self.started_from = p.name.clone();
                self.load(&p, false);
                // a template's empty weight is never yours: make the user enter their own
                self.empty_weight = None;
                self.empty_arm = None;
            }
        }
        self.step = 1;
    }

    /// The draft as an aircraft-file object, for the shared validator.
    fn draft(&self) -> Value {
        let mut perf = serde_json::Map::new();
        for (i, (k, ..)) in PERF.iter().enumerate() {
            perf.insert(k.to_string(), self.perf[i].map(|v| json!(v)).unwrap_or(Value::Null));
        }
        json!({
            "name": self.name, "tail": self.tail, "type": self.type_code, "fuelType": self.fuel_type, "lbPerGal": self.lb_per_gal,
            "usableFuel": self.usable, "perf": perf, "notes": self.notes,
            "wb": {
                "emptyWeight": self.empty_weight, "emptyArm": self.empty_arm,
                "maxRamp": self.max_ramp.or(self.max_takeoff), "maxTakeoff": self.max_takeoff, "maxLanding": self.max_landing.or(self.max_takeoff),
                "stations": self.rows.iter().filter(|r| !r.name.trim().is_empty() || r.arm.is_some()).map(|r| json!({"id": r.id, "name": r.name, "kind": format!("{:?}", r.kind).to_lowercase(), "arm": r.arm, "max": r.max})).collect::<Vec<_>>(),
                "envelope": self.env.iter().filter_map(|(a, w)| Some(json!([(*a)?, (*w)?]))).collect::<Vec<_>>(),
            }
        })
    }

    /// Preview aircraft with a sample loading (170 lb in the first seat, full fuel).
    fn preview(&self) -> Option<Aircraft> {
        let env: Vec<[f64; 2]> = self.env.iter().filter_map(|(a, w)| Some([(*a)?, (*w)?])).collect();
        if env.len() < 3 {
            return None;
        }
        let mut a = aircraft::presets()[0].clone();
        a.wb.envelope = env;
        a.wb.utility = None;
        a.wb.empty_weight = self.empty_weight.unwrap_or(0.0);
        a.wb.empty_arm = self.empty_arm.unwrap_or(0.0);
        a.wb.max_takeoff = self.max_takeoff.unwrap_or(0.0);
        a.wb.max_ramp = self.max_ramp.or(self.max_takeoff).unwrap_or(0.0);
        a.wb.max_landing = self.max_landing.or(self.max_takeoff).unwrap_or(0.0);
        a.wb.stations = self.rows.iter().filter_map(|r| Some(crate::model::aircraft::Station { id: r.id.clone(), name: r.name.clone(), kind: r.kind, arm: r.arm?, max: r.max })).collect();
        a.usable_fuel = self.usable.unwrap_or(0.0);
        a.lb_per_gal = self.lb_per_gal.unwrap_or(6.0);
        a.load = aircraft::default_load(&a);
        Some(a)
    }

    fn step_issue(&self, step: &str) -> Option<&'static str> {
        match step {
            "Identity" if self.name.trim().is_empty() => Some("Give the aircraft a name"),
            "Identity" if !self.usable.is_some_and(|v| v > 0.0) => Some("Enter the usable fuel"),
            "Performance" if !self.perf[0].is_some_and(|v| v > 0.0) => Some("Enter the cruise true airspeed"),
            "Weights" if self.empty_weight.is_none() => Some("Enter the basic empty weight from your weight & balance record"),
            "Weights" if self.empty_arm.is_none() => Some("Enter the empty-weight arm (or its moment)"),
            "Weights" if self.max_takeoff.is_none() => Some("Enter the maximum takeoff weight"),
            "Stations" if !self.rows.iter().any(|r| r.kind == StationKind::Fuel && r.arm.is_some()) => Some("Add a fuel station with its arm"),
            "Stations" if self.rows.iter().any(|r| !r.name.trim().is_empty() && r.arm.is_none()) => Some("Every station needs an arm"),
            "Envelope" if self.env.iter().filter(|(a, w)| a.is_some() && w.is_some()).count() < 3 => Some("The envelope needs at least 3 points"),
            _ => None,
        }
    }
}

/// Draw the wizard; returns false when it should close.
pub fn show(app: &mut App, ctx: &egui::Context, w: &mut Wizard) -> bool {
    let t = app.t;
    let steps = w.steps();
    let current = steps[w.step.min(steps.len() - 1)];
    let title = match &w.edit {
        Some(a) => format!("Edit {}", if a.tail.is_empty() { &a.name } else { &a.tail }),
        None => "New aircraft".into(),
    };
    let subtitle = if !w.started_from.is_empty() && w.started_from != "blank" { format!("Starting from the {} template", w.started_from) } else { "Performance and weight & balance, step by step".into() };
    let mut open = true;
    let mut save = None;
    let modal = egui::Modal::new(egui::Id::new("wizard")).show(ctx, |ui| {
        ui.set_width(620.0);
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new(&title).font(theme::bold(17.0)));
                faint(ui, &t, &subtitle);
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                if widgets::icon_button(ui, &t, ph::X, "Close").clicked() {
                    open = false;
                }
            });
        });
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            for (i, s) in steps.iter().enumerate() {
                let reachable = i <= w.step || w.edit.is_some() || !w.started_from.is_empty();
                let txt = RichText::new(*s).font(theme::semibold(11.0)).color(if i == w.step { t.accent } else if i < w.step { t.text2 } else { t.text3 });
                if ui.add_enabled(reachable, egui::Button::new(txt).fill(if i == w.step { t.accent_soft } else { t.surface2 }).corner_radius(99)).clicked() {
                    w.step = i;
                }
            }
        });
        ui.add_space(10.0);
        egui::ScrollArea::vertical().max_height(ctx.content_rect().height() * 0.65).show(ui, |ui| match current {
            "Start" => {
                widgets::muted(ui, &t, "Pick the closest template. You'll replace its numbers with your own airplane's in the next steps.");
                ui.add_space(6.0);
                let width = (ui.available_width() - 16.0) / 3.0;
                ui.horizontal_wrapped(|ui| {
                    for p in aircraft::presets() {
                        if widgets::card(ui, &t, ph::AIRPLANE, &p.name, &format!("{} · {} kt · {} gal · MTOW {} lb", p.type_code, p.perf.tas, p.usable_fuel, p.wb.max_takeoff), width).clicked() {
                            w.start_from(Some(p));
                        }
                    }
                    if widgets::card(ui, &t, ph::PLUS, "Blank", "Enter everything from the POH", width).clicked() {
                        w.start_from(None);
                    }
                    if widgets::card(ui, &t, ph::UPLOAD_SIMPLE, "Import a file", "A Flightplan aircraft file from another pilot, club or backup", width).clicked() {
                        open = false;
                        app.dialog = None;
                        app.dialog = Some(crate::app::Dialog::Import(Box::default()));
                    }
                });
            }
            "Identity" => {
                egui::Grid::new("wz-id").num_columns(2).spacing(vec2(12.0, 8.0)).show(ui, |ui| {
                    faint(ui, &t, "Tail number");
                    ui.add(egui::TextEdit::singleline(&mut w.tail).hint_text("N12345").desired_width(160.0));
                    ui.end_row();
                    faint(ui, &t, "ICAO type");
                    ui.add(egui::TextEdit::singleline(&mut w.type_code).hint_text("C172").desired_width(160.0));
                    ui.end_row();
                    faint(ui, &t, "Name");
                    ui.add(egui::TextEdit::singleline(&mut w.name).hint_text("Cessna 172S Skyhawk").desired_width(320.0));
                    ui.end_row();
                    faint(ui, &t, "Fuel");
                    egui::ComboBox::from_id_salt("wz-fuel").selected_text(w.fuel_type.clone()).show_ui(ui, |ui| {
                        for f in FUEL_TYPES {
                            if ui.selectable_value(&mut w.fuel_type, f.to_string(), f).clicked() {
                                w.lb_per_gal = Some(if f == "Jet A" { 6.7 } else { 6.0 });
                            }
                        }
                    });
                    ui.end_row();
                    faint(ui, &t, "Fuel weight (lb/gal)");
                    widgets::opt_num(ui, &mut w.lb_per_gal, 90.0, "6");
                    ui.end_row();
                    faint(ui, &t, "Usable fuel (gal)");
                    widgets::opt_num(ui, &mut w.usable, 90.0, "gal");
                    ui.end_row();
                    faint(ui, &t, "Notes");
                    ui.add(egui::TextEdit::multiline(&mut w.notes).desired_rows(2).desired_width(320.0).hint_text("Equipment, speeds, anything you want on hand"));
                    ui.end_row();
                });
            }
            "Performance" => {
                widgets::muted(ui, &t, "Numbers for the navlog. Use what your airplane actually does at the power settings you fly.");
                ui.add_space(6.0);
                egui::Grid::new("wz-perf").num_columns(2).spacing(vec2(12.0, 6.0)).show(ui, |ui| {
                    for (i, (_, label, unit, hint)) in PERF.iter().enumerate() {
                        ui.vertical(|ui| {
                            ui.label(RichText::new(format!("{label} ({unit})")).size(12.5));
                            if !hint.is_empty() {
                                ui.label(RichText::new(*hint).size(10.5).color(t.text3));
                            }
                        });
                        widgets::opt_num(ui, &mut w.perf[i], 90.0, unit);
                        ui.end_row();
                    }
                });
            }
            "Weights" => {
                widgets::muted(ui, &t, "From the airplane's current weight & balance record (not the POH sample). Arms are inches aft of the datum.");
                ui.add_space(6.0);
                egui::Grid::new("wz-w").num_columns(2).spacing(vec2(12.0, 8.0)).show(ui, |ui| {
                    faint(ui, &t, "Basic empty weight (lb)");
                    widgets::opt_num(ui, &mut w.empty_weight, 110.0, "lb");
                    ui.end_row();
                    faint(ui, &t, "Empty-weight arm (in)");
                    widgets::opt_num(ui, &mut w.empty_arm, 110.0, "in");
                    ui.end_row();
                    faint(ui, &t, "…or its moment");
                    ui.horizontal(|ui| {
                        widgets::opt_num(ui, &mut w.moment, 110.0, "moment");
                        ui.checkbox(&mut w.per_1000, "÷1000");
                        if ui.add_enabled(w.moment.is_some() && w.empty_weight.is_some(), egui::Button::new("Work out arm")).clicked() {
                            if let (Some(m), Some(ew)) = (w.moment, w.empty_weight) {
                                w.empty_arm = Some(((m * if w.per_1000 { 1000.0 } else { 1.0 }) / ew * 100.0).round() / 100.0);
                            }
                        }
                    });
                    ui.end_row();
                    faint(ui, &t, "Max ramp (lb)");
                    widgets::opt_num(ui, &mut w.max_ramp, 110.0, &w.max_takeoff.map(aircraft::fmt_num).unwrap_or_default());
                    ui.end_row();
                    faint(ui, &t, "Max takeoff (lb)");
                    widgets::opt_num(ui, &mut w.max_takeoff, 110.0, "lb");
                    ui.end_row();
                    faint(ui, &t, "Max landing (lb)");
                    widgets::opt_num(ui, &mut w.max_landing, 110.0, &w.max_takeoff.map(aircraft::fmt_num).unwrap_or_default());
                    ui.end_row();
                });
            }
            "Stations" => {
                widgets::muted(ui, &t, "Everything you load: seats, baggage areas and fuel. Max is pounds, or gallons for fuel.");
                ui.add_space(6.0);
                let mut remove = None;
                egui::Grid::new("wz-st").num_columns(5).spacing(vec2(6.0, 5.0)).show(ui, |ui| {
                    for h in ["Station", "Kind", "Arm (in)", "Max", ""] {
                        ui.label(RichText::new(h).size(11.0).color(t.text3));
                    }
                    ui.end_row();
                    for (i, r) in w.rows.iter_mut().enumerate() {
                        ui.add(egui::TextEdit::singleline(&mut r.name).desired_width(190.0).hint_text("Station name"));
                        egui::ComboBox::from_id_salt(("wz-kind", i)).selected_text(r.kind.label()).width(90.0).show_ui(ui, |ui| {
                            for k in StationKind::ALL {
                                ui.selectable_value(&mut r.kind, k, k.label());
                            }
                        });
                        widgets::opt_num(ui, &mut r.arm, 70.0, "in");
                        widgets::opt_num(ui, &mut r.max, 70.0, if r.kind == StationKind::Fuel { "gal" } else { "lb" });
                        if ui.add(egui::Button::new(RichText::new(ph::TRASH).size(13.0)).frame(false)).clicked() {
                            remove = Some(i);
                        }
                        ui.end_row();
                    }
                });
                if let Some(i) = remove {
                    w.rows.remove(i);
                }
                ui.horizontal(|ui| {
                    if widgets::small_button(ui, format!("{}  Add station", ph::PLUS)).clicked() {
                        w.rows.push(Row { id: new_id(), name: String::new(), kind: StationKind::Baggage, arm: None, max: None });
                    }
                    if widgets::ghost(ui, &t, format!("{}  Paste from a spreadsheet", ph::CLIPBOARD_TEXT)).clicked() {
                        w.show_paste_st = !w.show_paste_st;
                    }
                });
                if w.show_paste_st {
                    ui.add(egui::TextEdit::multiline(&mut w.paste_st).font(theme::mono(12.0)).desired_rows(5).desired_width(f32::INFINITY).hint_text("Front seats\t37\nRear seats\t73\nBaggage A\t95\t120\nFuel\t48\t53"));
                    ui.horizontal(|ui| {
                        faint(ui, &t, "One station per line: name, arm, max (optional). Replaces the list above.");
                        if ui.add_enabled(!w.paste_st.trim().is_empty(), egui::Button::new("Use these")).clicked() {
                            let mut out = Vec::new();
                            for cells in parse_table(&w.paste_st) {
                                let nums: Vec<Option<f64>> = cells.iter().map(|c| c.replace(',', "").parse::<f64>().ok()).collect();
                                let label = cells.iter().zip(&nums).find(|(_, n)| n.is_none()).map(|(c, _)| c.clone()).unwrap_or_else(|| format!("Station {}", out.len() + 1));
                                let values: Vec<f64> = nums.into_iter().flatten().collect();
                                if values.is_empty() {
                                    continue;
                                }
                                out.push(Row { id: new_id(), kind: guess_kind("", &label), name: label, arm: Some(values[0]), max: values.get(1).copied() });
                            }
                            if !out.is_empty() {
                                w.rows = out;
                                w.paste_st.clear();
                                w.show_paste_st = false;
                            }
                        }
                    });
                }
            }
            "Envelope" => {
                widgets::muted(ui, &t, "Corner points of the normal-category CG envelope, in order around the outline (POH section 6).");
                ui.add_space(6.0);
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(230.0);
                        let mut remove = None;
                        egui::Grid::new("wz-env").num_columns(3).spacing(vec2(6.0, 5.0)).show(ui, |ui| {
                            ui.label(RichText::new("Arm (in)").size(11.0).color(t.text3));
                            ui.label(RichText::new("Weight (lb)").size(11.0).color(t.text3));
                            ui.label("");
                            ui.end_row();
                            for (i, p) in w.env.iter_mut().enumerate() {
                                widgets::opt_num(ui, &mut p.0, 80.0, "arm");
                                widgets::opt_num(ui, &mut p.1, 80.0, "lb");
                                if ui.add(egui::Button::new(RichText::new(ph::TRASH).size(13.0)).frame(false)).clicked() {
                                    remove = Some(i);
                                }
                                ui.end_row();
                            }
                        });
                        if let Some(i) = remove {
                            w.env.remove(i);
                        }
                        ui.horizontal(|ui| {
                            if widgets::small_button(ui, format!("{}  Add point", ph::PLUS)).clicked() {
                                w.env.push((None, None));
                            }
                            if widgets::ghost(ui, &t, format!("{}  Paste", ph::CLIPBOARD_TEXT)).clicked() {
                                w.show_paste_env = !w.show_paste_env;
                            }
                        });
                        if w.show_paste_env {
                            ui.add(egui::TextEdit::multiline(&mut w.paste_env).font(theme::mono(12.0)).desired_rows(5).desired_width(220.0).hint_text("35.0, 1500\n35.0, 1950\n41.0, 2550\n47.3, 2550\n47.3, 1500"));
                            if ui.add_enabled(!w.paste_env.trim().is_empty(), egui::Button::new("Use these")).clicked() {
                                let mut out = Vec::new();
                                for cells in parse_table(&w.paste_env) {
                                    let v: Vec<f64> = cells.iter().filter_map(|c| c.replace(',', "").parse().ok()).collect();
                                    if v.len() >= 2 {
                                        // arms are tens of inches, weights hundreds+ of pounds: accept either order
                                        let (a, b) = if v[0] > v[1] { (v[1], v[0]) } else { (v[0], v[1]) };
                                        out.push((Some(a), Some(b)));
                                    }
                                }
                                if out.len() >= 3 {
                                    w.env = out;
                                    w.paste_env.clear();
                                    w.show_paste_env = false;
                                }
                            }
                        }
                    });
                    ui.vertical(|ui| match w.preview() {
                        Some(a) => {
                            let wb = compute_wb(&a, a.usable_fuel / 2.0);
                            super::wbchart::show(ui, &t, &a, &wb, 240.0);
                            faint(ui, &t, "Dots: a sample loading (170 lb pilot, full tanks) at takeoff, and landing after burning half the fuel.");
                        }
                        None => {
                            faint(ui, &t, "The envelope appears here once it has 3 points.");
                        }
                    });
                });
            }
            _ => {
                let check = aircraft_file::normalize(&w.draft(), "aircraft");
                if check.errors.is_empty() {
                    widgets::status(ui, &t, Tone::Ok, ph::CHECK_CIRCLE, &["Ready to save".into()]);
                } else {
                    widgets::status(ui, &t, Tone::Bad, ph::OCTAGON, &check.errors);
                }
                if !check.warnings.is_empty() {
                    ui.add_space(4.0);
                    widgets::status(ui, &t, Tone::Warn, ph::WARNING, &check.warnings);
                }
                ui.add_space(8.0);
                egui::Grid::new("wz-sum").num_columns(2).spacing(vec2(12.0, 4.0)).show(ui, |ui| {
                    let n = |v: Option<f64>| v.map(aircraft::fmt_num).unwrap_or_else(|| "?".into());
                    widgets::kv(ui, &t, "Aircraft", format!("{} · {} ({})", if w.tail.is_empty() { "No tail" } else { &w.tail }, w.name, if w.type_code.is_empty() { "no type" } else { &w.type_code }));
                    let endurance = match (w.usable, w.perf[1]) {
                        (Some(u), Some(g)) if g > 0.0 => format!("{:.1} h", u / g),
                        _ => "?".into(),
                    };
                    widgets::kv(ui, &t, "Cruise", format!("{} kt at {} gph · {} gal usable ({endurance} endurance)", n(w.perf[0]), n(w.perf[1]), n(w.usable)));
                    widgets::kv(ui, &t, "Empty", format!("{} lb at {} in", n(w.empty_weight), n(w.empty_arm)));
                    widgets::kv(ui, &t, "Max weights", format!("ramp {} · takeoff {} · landing {} lb", n(w.max_ramp.or(w.max_takeoff)), n(w.max_takeoff), n(w.max_landing.or(w.max_takeoff))));
                    widgets::kv(ui, &t, "Stations", w.rows.iter().map(|r| r.name.clone()).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(", "));
                    widgets::kv(ui, &t, "Envelope", format!("{} points", w.env.iter().filter(|(a, b)| a.is_some() && b.is_some()).count()));
                });
                if let Some(a) = w.preview() {
                    let wb = compute_wb(&a, a.usable_fuel / 2.0);
                    super::wbchart::show(ui, &t, &a, &wb, 200.0);
                }
                widgets::para(ui, "Your aircraft's weighing record and POH govern. Double-check every number before relying on it.", t.text3, 11.0);
                if let Some(a) = check.aircraft {
                    save = Some(a);
                }
            }
        });
        ui.add_space(8.0);
        widgets::rule(ui, &t);
        ui.horizontal(|ui| {
            if current != "Start" && current != "Review" {
                if let Some(issue) = w.step_issue(current) {
                    faint(ui, &t, issue);
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if current == "Review" {
                    let label = if w.edit.is_some() { "Save changes" } else { "Add aircraft" };
                    if ui.add_enabled(save.is_some(), egui::Button::new(RichText::new(format!("{}  {label}", ph::CHECK_CIRCLE)).color(egui::Color32::WHITE).font(theme::semibold(13.0))).fill(t.accent).min_size(vec2(0.0, 32.0))).clicked() {
                        if let Some(mut a) = save.take() {
                            if let Some(old) = &w.edit {
                                a.id = old.id.clone();
                                // keep this flight's loading for stations that still exist
                                for s in &a.wb.stations {
                                    if let Some(v) = old.load.get(&s.id) {
                                        a.load.insert(s.id.clone(), *v);
                                    }
                                }
                            }
                            app.fleet.upsert(a);
                            open = false;
                        }
                    }
                } else if current != "Start" && widgets::primary(ui, &t, format!("Next  {}", ph::ARROW_RIGHT)).clicked() {
                    w.step += 1;
                }
                if w.step > 0 && widgets::button(ui, format!("{}  Back", ph::ARROW_LEFT)).clicked() {
                    w.step -= 1;
                }
            });
        });
    });
    if modal.should_close() {
        open = false;
    }
    open
}
