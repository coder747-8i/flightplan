//! First-run screen: welcome, a walk through what the app does, chart data, optional API keys,
//! aircraft, then finish (optionally into the spotlight tour of the real screen). It comes
//! back when chart data is missing.

use super::widgets::{self, faint, Tone};
use crate::app::{App, Dialog};
use crate::theme;
use egui::{pos2, vec2, Align2, CornerRadius, Frame, Margin, Rect, RichText, Sense, Stroke};
use egui_phosphor::regular as ph;

pub struct Onboarding {
    pub open: bool,
    pub step: usize,
    slide: usize,
}

impl Onboarding {
    pub fn new(onboarded: bool, installed: bool) -> Self {
        // returning users with missing data go straight to the data step
        let open = !onboarded || !installed;
        Onboarding { open, step: if onboarded { 2 } else { 0 }, slide: 0 }
    }
    pub fn reopen(&mut self, step: usize) {
        self.open = true;
        self.step = step;
        self.slide = 0;
    }
}

const STEPS: [&str; 6] = ["Welcome", "How it works", "Chart data", "Weather keys", "Your aircraft", "Finish"];

struct Slide {
    icon: &'static str,
    title: &'static str,
    points: [&'static str; 3],
    tip: &'static str,
}

const SLIDES: [Slide; 6] = [
    Slide {
        icon: ph::MAP_TRIFOLD,
        title: "A sectional you can shape",
        points: [
            "Vector chart with airspace, airports, navaids, obstacles and terrain.",
            "Layers turns groups on and off; the altitude filter shows only airspace in your altitude band, like a Garmin.",
            "Click anything on the chart for details: airports open a card with runways, frequencies, weather and FBOs.",
        ],
        tip: "Press / to search for an airport, navaid or fix.",
    },
    Slide {
        icon: ph::PATH,
        title: "Build the route",
        points: [
            "Type a route (KNYL BZA 3300N11400W KPHX) or click the map to add waypoints.",
            "Drag the magenta line to rubber-band a new waypoint in; drag a waypoint to move it.",
            "Pick your aircraft, cruise altitude and departure time. The navlog uses winds aloft for that time.",
        ],
        tip: "The suggested VFR altitude follows the hemispheric rule and clears terrain by 1,000 ft.",
    },
    Slide {
        icon: ph::CHART_LINE,
        title: "See it from the side",
        points: [
            "The profile shows terrain, obstacles and airspace along the route at your planned altitude.",
            "Hover the profile and the map follows, and the other way round.",
            "Climb and descent points (TOC/TOD) come from your aircraft's performance.",
        ],
        tip: "Click an airspace band in the profile to highlight it on the map.",
    },
    Slide {
        icon: ph::CLOUD_SUN,
        title: "Weather and go / no-go",
        points: [
            "Drag the timeline under the chart to plan up to five days ahead: live radar becomes HRRR forecast radar, then the official WPC forecast charts; winds and TAFs follow.",
            "With a route, you get a green, yellow or red call for that time against your personal minimums, with the reasons.",
            "The timeline shows the call for every 3 hours, so the green stretches are your windows to fly.",
        ],
        tip: "Set your personal minimums under Settings.",
    },
    Slide {
        icon: ph::AIRPLANE_TILT,
        title: "Aircraft, weight & balance",
        points: [
            "Create airframes step by step, or import files from another pilot or your club.",
            "Load seats, bags and fuel; the CG chart shows takeoff and landing against the envelope.",
            "Performance numbers drive the navlog, fuel burn and reserves.",
        ],
        tip: "Export your fleet to a file to back it up or share it.",
    },
    Slide {
        icon: ph::SHARE_NETWORK,
        title: "Brief and export",
        points: [
            "The briefing gathers the go / no-go call, route, navlog, weather, NOTAMs and W&B on one page you can save as PDF and print.",
            "Send the route to FltPlan Go with a QR code (nothing gets filed), or export Garmin .fpl, GPX or KML.",
            "For planning only: always get an official briefing before flight.",
        ],
        tip: "Everything you plan stays on this computer.",
    },
];

fn finish(app: &mut App, tour: bool) {
    app.prefs.onboarded = true;
    app.store.save_prefs(&app.prefs);
    app.onboarding.open = false;
    if tour {
        app.tour = Some(super::tour::Tour::default());
    }
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let t = app.t;
    let screen = ctx.content_rect();
    let installed = app.data_info.is_some();
    let step = app.onboarding.step;
    let mut go: Option<usize> = None;
    let mut done: Option<bool> = None;
    let mut open_wizard = false;
    let mut open_import = false;
    egui::Area::new("onboarding".into()).fixed_pos(screen.min).order(egui::Order::Foreground).show(ctx, |ui| {
        ui.set_min_size(screen.size());
        let painter = ui.painter();
        painter.rect_filled(screen, 0.0, t.bg);
        // ---- sidebar ----
        let side = Rect::from_min_size(screen.min, vec2(260.0, screen.height()));
        painter.rect_filled(side, 0.0, t.surface_solid);
        painter.vline(side.right(), side.y_range(), Stroke::new(1.0, t.line));
        let mut side_ui = ui.new_child(egui::UiBuilder::new().max_rect(side.shrink2(vec2(20.0, 28.0))));
        side_ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::hover());
            let c = t.accent;
            let p = |x: f32, y: f32| r.min + vec2(1.0 + x, 1.0 + y);
            ui.painter().add(egui::Shape::line(vec![p(0.0, 17.0), p(8.0, 11.0), p(13.0, 14.0), p(22.0, 5.0)], Stroke::new(2.6, c)));
            ui.painter().circle_filled(p(0.0, 17.0), 2.4, c);
            ui.painter().circle_filled(p(22.0, 5.0), 2.4, c);
            ui.label(RichText::new("Flightplan").font(theme::bold(17.0)));
        });
        side_ui.add_space(24.0);
        for (i, s) in STEPS.iter().enumerate() {
            let (icon, color, fill) = if i == step { (ph::CIRCLE, t.accent, t.accent_soft) } else if i < step { (ph::CHECK_CIRCLE, t.text2, egui::Color32::TRANSPARENT) } else { (ph::CIRCLE, t.text3, egui::Color32::TRANSPARENT) };
            let b = egui::Button::new(RichText::new(format!("{icon}   {s}")).font(theme::semibold(13.0)).color(color)).fill(fill).stroke(Stroke::NONE).min_size(vec2(220.0, 36.0)).corner_radius(9);
            if side_ui.add(b).clicked() {
                go = Some(i);
            }
        }
        let foot = Rect::from_min_max(pos2(side.left() + 20.0, side.bottom() - 70.0), pos2(side.right() - 20.0, side.bottom() - 16.0));
        let mut fui = ui.new_child(egui::UiBuilder::new().max_rect(foot));
        widgets::para(&mut fui, "For planning only. Not a substitute for an official briefing, current charts, NOTAMs or the POH.", t.text3, 11.0);

        // ---- content ----
        let content_w = 760f32.min(screen.width() - 260.0 - 64.0);
        let cx = side.right() + ((screen.width() - 260.0) - content_w) / 2.0;
        let area = Rect::from_min_size(pos2(cx, screen.top() + 44.0), vec2(content_w, screen.height() - 44.0 - 80.0));
        let mut cui = ui.new_child(egui::UiBuilder::new().max_rect(area));
        egui::ScrollArea::vertical().id_salt("ob-scroll").show(&mut cui, |ui| {
            ui.set_width(content_w);
            match step {
                0 => {
                    ui.label(RichText::new(ph::SPARKLE).size(28.0).color(t.accent));
                    ui.label(RichText::new("Welcome to Flightplan").font(theme::bold(28.0)));
                    ui.add_space(6.0);
                    widgets::para(ui, "A flight planner for the US: a sectional-style chart, route planning with a vertical profile, winds-aloft navlog, weather, weight & balance and a printable briefing.", t.text2, 14.5);
                    ui.add_space(12.0);
                    widgets::muted(ui, &t, "Setup takes a couple of minutes:");
                    ui.add_space(6.0);
                    for (icon, text) in [(ph::STACK, "A quick look at how the app works"), (ph::MAP_TRIFOLD, "Download the FAA chart data"), (ph::CLOUD_SUN, "Optionally add Windy and FAA NOTAM keys"), (ph::AIRPLANE_TILT, "Set up your aircraft")] {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(icon).size(16.0).color(t.accent));
                            ui.label(RichText::new(text).size(14.0));
                        });
                        ui.add_space(4.0);
                    }
                }
                1 => {
                    let sl = &SLIDES[app.onboarding.slide];
                    ui.label(RichText::new("How Flightplan works").font(theme::bold(22.0)));
                    ui.add_space(18.0);
                    ui.horizontal_top(|ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(140.0, 140.0), Sense::hover());
                        ui.painter().rect_filled(r, CornerRadius::same(28), t.accent_soft);
                        ui.painter().text(r.center(), Align2::CENTER_CENTER, sl.icon, egui::FontId::proportional(48.0), t.accent);
                        ui.add_space(24.0);
                        ui.vertical(|ui| {
                            ui.set_width(content_w - 170.0);
                            ui.label(RichText::new(sl.title).font(theme::bold(18.0)));
                            ui.add_space(8.0);
                            for p in sl.points {
                                ui.horizontal_top(|ui| {
                                    ui.label(RichText::new("•").color(t.text3));
                                    widgets::para(ui, p, t.text2, 14.0);
                                });
                            }
                            ui.add_space(8.0);
                            Frame::new().fill(t.surface2).corner_radius(CornerRadius::same(9)).inner_margin(Margin::symmetric(10, 8)).show(ui, |ui| {
                                ui.label(RichText::new(format!("{}  {}", ph::SPARKLE, sl.tip)).size(12.5));
                            });
                        });
                    });
                    ui.add_space(18.0);
                    ui.horizontal(|ui| {
                        ui.add_space(content_w / 2.0 - 60.0);
                        for i in 0..SLIDES.len() {
                            let (r, resp) = ui.allocate_exact_size(vec2(if i == app.onboarding.slide { 22.0 } else { 8.0 }, 8.0), Sense::click());
                            ui.painter().rect_filled(r, CornerRadius::same(4), if i == app.onboarding.slide { t.accent } else { t.line_strong });
                            if resp.clicked() {
                                app.onboarding.slide = i;
                            }
                        }
                    });
                }
                2 => {
                    ui.label(RichText::new("Chart data").font(theme::bold(22.0)));
                    widgets::para(ui, "The chart, airport database, airspace and obstacles come from the FAA's 28-day cycle, packaged for Flightplan.", t.text2, 14.5);
                    ui.add_space(12.0);
                }
                3 => {
                    ui.label(RichText::new("Weather & NOTAM keys").font(theme::bold(22.0)));
                    widgets::para(ui, "Optional. Everything else works without them. You can add or change keys later in Settings.", t.text2, 14.5);
                    ui.add_space(12.0);
                }
                4 => {
                    ui.label(RichText::new("Your aircraft").font(theme::bold(22.0)));
                    widgets::para(ui, "Performance drives the navlog and fuel; weight & balance data drives the loading chart.", t.text2, 14.5);
                    ui.add_space(12.0);
                }
                _ => {
                    ui.label(RichText::new(ph::CHECK_CIRCLE).size(28.0).color(t.accent));
                    ui.label(RichText::new("You're set").font(theme::bold(28.0)));
                    ui.add_space(12.0);
                }
            }
        });
        // steps that need the whole app
        if matches!(step, 2..=5) {
            let mut cui = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_size(pos2(cx, screen.top() + 140.0), vec2(content_w, screen.height() - 230.0))));
            egui::ScrollArea::vertical().id_salt("ob-body").show(&mut cui, |ui| {
                ui.set_width(content_w);
                match step {
                    2 => super::data::show(app, ui, false),
                    3 => super::keys::form(app, ui),
                    4 => {
                        let sample = app.fleet.list.len() == 1 && app.fleet.list[0].tail == "N12345";
                        if sample {
                            widgets::status(ui, &t, Tone::Warn, ph::WARNING, &["N12345 is a sample Cessna 172S with typical numbers. Add your own airplane, then delete the sample in the Aircraft panel.".into()]);
                            ui.add_space(8.0);
                        }
                        let w = (content_w - 10.0) / 2.0;
                        ui.horizontal(|ui| {
                            if widgets::card(ui, &t, ph::PLUS, "Create an aircraft", "Step by step: start from a template or from scratch, with checks along the way", w).clicked() {
                                open_wizard = true;
                            }
                            if widgets::card(ui, &t, ph::UPLOAD_SIMPLE, "Import aircraft files", "From another pilot, your club, or a backup (.fpaircraft.json)", w).clicked() {
                                open_import = true;
                            }
                        });
                        widgets::section(ui, &t, "In your hangar");
                        for a in &app.fleet.list {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(ph::AIRPLANE).color(t.text3));
                                ui.label(RichText::new(if a.tail.is_empty() { "No tail" } else { &a.tail }).font(theme::semibold(13.0)));
                                faint(ui, &t, format!("{} · {} kt · {} gal", a.name, a.perf.tas, a.usable_fuel));
                            });
                        }
                        ui.add_space(6.0);
                        faint(ui, &t, format!("Templates available: {}.", crate::model::aircraft::presets().iter().map(|p| p.name.clone()).collect::<Vec<_>>().join(", ")));
                    }
                    _ => {
                        let keys = app.keys.clone();
                        let any_key = keys.windy_point || keys.windy_map || keys.notams;
                        let sample = app.fleet.list.len() == 1 && app.fleet.list[0].tail == "N12345";
                        let line = |ui: &mut egui::Ui, ok: bool, text: String| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(if ok { ph::CHECK_CIRCLE } else { ph::CIRCLE }).color(if ok { t.ok } else { t.text3 }));
                                ui.label(RichText::new(text).size(14.0).color(t.text2));
                            });
                        };
                        let data_line = match &app.data_info {
                            Some(i) => format!("Chart data installed{}", i.cycle.as_ref().map(|c| format!(" (cycle {c})")).unwrap_or_default()),
                            None => "No chart data yet: the chart and search stay empty until you install it".into(),
                        };
                        line(ui, installed, data_line);
                        if !installed && widgets::link(ui, &t, "Install chart data").clicked() {
                            go = Some(2);
                        }
                        line(ui, any_key, if any_key { "Weather / NOTAM keys added".into() } else { "No API keys (optional)".into() });
                        line(ui, !sample, format!("{} aircraft{}", app.fleet.list.len(), if sample { " (the sample only)" } else { "" }));
                        ui.add_space(12.0);
                        widgets::muted(ui, &t, "Take a 1-minute tour of the screen, or jump straight in. Help (bottom left) brings this setup and the tour back any time.");
                        ui.add_space(14.0);
                        ui.horizontal(|ui| {
                            if ui.add(egui::Button::new(RichText::new(format!("{}  Take the tour", ph::SPARKLE)).color(egui::Color32::WHITE).font(theme::semibold(14.0))).fill(t.accent).min_size(vec2(0.0, 40.0))).clicked() {
                                done = Some(true);
                            }
                            if ui.add(egui::Button::new(RichText::new("Start planning").font(theme::semibold(14.0))).min_size(vec2(0.0, 40.0))).clicked() {
                                done = Some(false);
                            }
                        });
                    }
                }
            });
        }
        // ---- footer ----
        if step < STEPS.len() - 1 {
            let footer = Rect::from_min_size(pos2(cx, screen.bottom() - 70.0), vec2(content_w, 44.0));
            let mut fui = ui.new_child(egui::UiBuilder::new().max_rect(footer));
            fui.horizontal(|ui| {
                let ob = &mut app.onboarding;
                if step > 0 && widgets::button(ui, format!("{}  Back", ph::ARROW_LEFT)).clicked() {
                    if step == 1 && ob.slide > 0 {
                        ob.slide -= 1;
                    } else {
                        go = Some(step - 1);
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let label = if step == 0 { format!("Get started  {}", ph::ARROW_RIGHT) } else { format!("Next  {}", ph::ARROW_RIGHT) };
                    let blocked = step == 2 && !installed;
                    if ui.add_enabled(!blocked, egui::Button::new(RichText::new(label).color(egui::Color32::WHITE).font(theme::semibold(13.0))).fill(t.accent).min_size(vec2(0.0, 34.0))).clicked() {
                        if step == 1 && ob.slide < SLIDES.len() - 1 {
                            ob.slide += 1;
                        } else {
                            go = Some(step + 1);
                        }
                    }
                    if step == 2 && !installed && widgets::ghost(ui, &t, "Skip for now").clicked() {
                        go = Some(3);
                    }
                    if step == 3 && widgets::ghost(ui, &t, "Skip").clicked() {
                        go = Some(4);
                    }
                });
            });
        }
    });
    if let Some(s) = go {
        app.onboarding.step = s.min(STEPS.len() - 1);
    }
    if open_wizard {
        app.dialog = Some(Dialog::Wizard(Box::new(super::wizard::Wizard::new(None))));
    }
    if open_import {
        app.dialog = Some(Dialog::Import(Box::default()));
    }
    if let Some(tour) = done {
        finish(app, tour);
    }
}
