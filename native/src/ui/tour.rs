//! Spotlight tour over the real screen: dims everything except one control and explains it.
//! Controls register where they are each frame with `mark`.

use super::widgets::{self, glass};
use crate::app::{App, Panel};
use crate::theme;
use egui::{pos2, vec2, Color32, CornerRadius, Margin, Rect, RichText, Stroke};
use egui_phosphor::regular as ph;
use std::collections::HashMap;

/// Remember where a tour target is drawn this frame.
pub fn mark(ctx: &egui::Context, name: &'static str, rect: Rect) {
    ctx.data_mut(|d| {
        let m: &mut HashMap<&'static str, Rect> = d.get_temp_mut_or_default(egui::Id::new("tour-targets"));
        m.insert(name, rect);
    });
}

fn target(ctx: &egui::Context, name: &str) -> Option<Rect> {
    ctx.data(|d| d.get_temp::<HashMap<&'static str, Rect>>(egui::Id::new("tour-targets")).and_then(|m| m.get(name).copied()))
}

struct Step {
    target: &'static str,
    title: &'static str,
    text: &'static str,
    panel: Option<Panel>,
}

const STEPS: [Step; 14] = [
    Step { target: "search", title: "Search", text: "Find any airport, navaid or fix. Press / from anywhere to jump here.", panel: None },
    Step { target: "rail-route", title: "Route", text: "Type a route or click the map to build one. Your aircraft, altitude and departure time live here too.", panel: Some(Panel::Route) },
    Step { target: "route-entry", title: "Route entry", text: "Enter identifiers and lat/longs separated by spaces and press Enter. The pointer button lets you click the map to add points instead.", panel: Some(Panel::Route) },
    Step { target: "route-verdict", title: "Go / no-go", text: "A green, yellow or red call for the flight at your departure time: weather at both ends against your personal minimums, wind and crosswind, thunderstorms and icing along the route, terrain, airspace, fuel and weight & balance. Set your minimums in Settings.", panel: Some(Panel::Route) },
    Step { target: "timeline", title: "Timeline", text: "Drag to plan up to five days ahead. Radar, winds, forecasts and the go / no-go call follow the time you pick; green stretches are good windows to fly. Press play to animate the forecast radar.", panel: None },
    Step { target: "route-export", title: "Export", text: "Send the route to FltPlan Go with a QR code (nothing gets filed), fill FltPlan.com's form, or save a Garmin .fpl, GPX or KML file. The folder button next to it opens .fpl/.gpx files.", panel: Some(Panel::Route) },
    Step { target: "rail-layers", title: "Layers", text: "Switch chart layers on and off: airspace classes, special use airspace, airports, navaids, airways, obstacles.", panel: Some(Panel::Layers) },
    Step { target: "altitude", title: "Altitude filter", text: "Show only the airspace that matters in your altitude band.", panel: None },
    Step { target: "rail-weather", title: "Weather", text: "Radar (live, then forecast), flight categories, winds aloft, TFRs, AIRMETs / SIGMETs, G-AIRMETs and PIREPs, plus the forecast at your departure and arrival.", panel: Some(Panel::Weather) },
    Step { target: "rail-aircraft", title: "Aircraft", text: "Load the airplane and check weight & balance. Create, edit, import and export airframes here.", panel: Some(Panel::Aircraft) },
    Step { target: "aircraft-new", title: "New aircraft", text: "The guided editor walks through performance, stations and the CG envelope.", panel: Some(Panel::Aircraft) },
    Step { target: "profile", title: "Profile", text: "Terrain, obstacles and airspace along the route at your planned altitude. Shows up once a route has two points.", panel: None },
    Step { target: "rail-briefing", title: "Briefing", text: "Everything for the flight on one page: save it as a PDF to print or take along.", panel: None },
    Step { target: "rail-help", title: "Help", text: "Bring back this tour or the setup screen, and update chart data, any time.", panel: None },
];

#[derive(Default)]
pub struct Tour {
    i: usize,
}

/// Returns the tour while it continues.
pub fn show(app: &mut App, ctx: &egui::Context, mut tour: Tour) -> Option<Tour> {
    let t = app.t;
    let st = &STEPS[tour.i];
    match st.panel {
        Some(p) if app.panel != p => app.panel = p,
        None if matches!(app.panel, Panel::Route | Panel::Layers | Panel::Aircraft | Panel::Weather) => app.panel = Panel::None,
        _ => {}
    }
    let screen = ctx.content_rect();
    let hole = target(ctx, st.target).map(|r| r.expand(6.0));
    let mut next = false;
    let mut back = false;
    let mut end = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    if ctx.input(|i| i.key_pressed(egui::Key::ArrowRight)) {
        next = true;
    }
    if ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft)) {
        back = true;
    }
    egui::Area::new("tour-dim".into()).fixed_pos(screen.min).order(egui::Order::Foreground).show(ctx, |ui| {
        ui.set_min_size(screen.size());
        let dim = Color32::from_black_alpha(115);
        let p = ui.painter();
        match hole {
            Some(h) => {
                // four rectangles around the spotlight
                p.rect_filled(Rect::from_min_max(screen.min, pos2(screen.right(), h.top())), 0.0, dim);
                p.rect_filled(Rect::from_min_max(pos2(screen.left(), h.bottom()), screen.max), 0.0, dim);
                p.rect_filled(Rect::from_min_max(pos2(screen.left(), h.top()), pos2(h.left(), h.bottom())), 0.0, dim);
                p.rect_filled(Rect::from_min_max(pos2(h.right(), h.top()), pos2(screen.right(), h.bottom())), 0.0, dim);
                p.rect_stroke(h, CornerRadius::same(12), Stroke::new(2.0, t.accent), egui::StrokeKind::Outside);
            }
            None => {
                p.rect_filled(screen, 0.0, dim);
            }
        }
        // swallow clicks on the dimmed area
        ui.allocate_rect(screen, egui::Sense::click());
    });
    let w = 300.0;
    let pos = match hole {
        None => pos2(screen.center().x - w / 2.0, screen.center().y - 80.0),
        Some(h) => {
            let mut x = h.right() + 16.0;
            let mut y = h.top();
            if x + w > screen.right() - 12.0 {
                x = (h.left()).clamp(12.0, screen.right() - w - 12.0);
                y = h.bottom() + 14.0;
                if y + 170.0 > screen.bottom() {
                    y = (h.top() - 184.0).max(12.0);
                }
            }
            pos2(x, y.clamp(12.0, screen.bottom() - 190.0))
        }
    };
    egui::Area::new("tour-card".into()).fixed_pos(pos).order(egui::Order::Tooltip).show(ctx, |ui| {
        glass(&t).inner_margin(Margin::same(14)).show(ui, |ui| {
            ui.set_width(w - 28.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{} of {}", tour.i + 1, STEPS.len())).size(11.0).color(t.text3));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::icon_button(ui, &t, ph::X, "End tour").clicked() {
                        end = true;
                    }
                });
            });
            ui.label(RichText::new(st.title).font(theme::bold(15.0)));
            widgets::para(ui, st.text, t.text2, 13.0);
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if tour.i > 0 && widgets::button(ui, ph::ARROW_LEFT).clicked() {
                    back = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let label = if tour.i + 1 == STEPS.len() { "Done".to_string() } else { format!("Next  {}", ph::ARROW_RIGHT) };
                    if widgets::primary(ui, &t, label).clicked() {
                        next = true;
                    }
                });
            });
        });
    });
    let _ = vec2(0.0, 0.0);
    if end {
        app.panel = Panel::None;
        return None;
    }
    if next {
        if tour.i + 1 >= STEPS.len() {
            app.panel = Panel::None;
            return None;
        }
        tour.i += 1;
    }
    if back {
        tour.i = tour.i.saturating_sub(1);
    }
    Some(tour)
}
