//! Search airports, navaids and fixes; "/" focuses it from anywhere.

use super::tour::mark;
use super::widgets::glass;
use crate::app::{App, Panel, Selection};
use crate::model::place::{Kind, Place};
use crate::theme;
use egui::{vec2, Align2, CornerRadius, Key, Margin, RichText};
use egui_phosphor::regular as ph;

#[derive(Default)]
pub struct Search {
    q: String,
    last: String,
    results: Vec<Place>,
    active: usize,
    focus: bool,
    /// the results list stays open after the field loses focus (Enter or a click on a result
    /// both take focus away) until a pick, Escape or a click elsewhere
    open: bool,
}

pub fn pick(app: &mut App, p: &Place, add: bool) {
    if add {
        app.saved.route.add(p.clone(), None);
        app.panel = Panel::Route;
    }
    let z = app.map.zoom.max(if p.kind == Kind::Airport { 11.0 } else { 10.0 });
    app.map.fly_to(p.lon, p.lat, z);
    if p.kind == Kind::Airport {
        app.selection = Some(Selection::Airport(p.id.clone()));
    }
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let t = app.t;
    let left = super::shell::panel_offset(app);
    let screen = ctx.content_rect();
    let width = 440f32.min(screen.width() - left - 220.0).max(220.0);
    let x = (left + (screen.width() - left - 180.0) / 2.0 - width / 2.0).max(left);
    if ctx.input(|i| i.key_pressed(Key::Slash)) && !ctx.egui_wants_keyboard_input() {
        app.search.focus = true;
    }
    egui::Area::new("search".into()).fixed_pos(screen.min + vec2(x, 12.0)).order(egui::Order::Middle).show(ctx, |ui| {
        let box_resp = glass(&t).corner_radius(CornerRadius::same(12)).inner_margin(Margin::symmetric(12, 6)).show(ui, |ui| {
            ui.set_width(width - 24.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(ph::MAGNIFYING_GLASS).size(16.0).color(t.text2));
                let te = egui::TextEdit::singleline(&mut app.search.q).hint_text("Search airports, navaids, waypoints").frame(egui::Frame::NONE).desired_width(width - 90.0).font(theme::regular(13.5));
                let r = ui.add(te);
                if app.search.focus {
                    r.request_focus();
                    app.search.focus = false;
                }
                ui.label(RichText::new("/").font(theme::mono(11.0)).color(t.text3));
                (r.has_focus(), r.lost_focus())
            })
            .inner
        });
        mark(ctx, "search", box_resp.response.rect);
        let (focused, lost) = box_resp.inner;
        // a single-line field drops focus on the frame Enter is pressed
        let submitted = lost && ui.input(|i| i.key_pressed(Key::Enter));
        if focused {
            app.search.open = true;
        }
        let q = app.search.q.trim().to_string();
        if q != app.search.last {
            app.search.last = q.clone();
            app.search.active = 0;
            app.search.results = if q.len() >= 2 {
                app.be.server.try_data().ok().and_then(|d| d.db.search(&q).ok()).map(|v| v.iter().map(Place::from_server).collect()).unwrap_or_default()
            } else {
                vec![]
            };
        }
        if submitted && app.search.results.is_empty() && q.len() >= 2 {
            app.toast(format!("Nothing found for \"{q}\""));
        }
        if app.search.open && !app.search.results.is_empty() {
            let n = app.search.results.len();
            if ui.input(|i| i.key_pressed(Key::ArrowDown)) {
                app.search.active = (app.search.active + 1).min(n - 1);
            }
            if ui.input(|i| i.key_pressed(Key::ArrowUp)) {
                app.search.active = app.search.active.saturating_sub(1);
            }
            let enter = submitted || (focused && ui.input(|i| i.key_pressed(Key::Enter)));
            let shift = ui.input(|i| i.modifiers.shift);
            let mut chosen: Option<(Place, bool)> = None;
            if enter {
                chosen = Some((app.search.results[app.search.active].clone(), shift));
            }
            ui.add_space(6.0);
            let list = glass(&t).inner_margin(Margin::same(4)).show(ui, |ui| {
                ui.set_width(width - 8.0);
                for (i, r) in app.search.results.iter().enumerate().take(12) {
                    let icon = match r.kind {
                        Kind::Airport => ph::AIRPLANE,
                        Kind::Navaid => ph::BROADCAST,
                        _ => ph::TRIANGLE,
                    };
                    let resp = ui.add(
                        egui::Button::new(RichText::new(format!("{icon}  {}   {}   {}", r.id, r.name, r.detail)).size(12.5))
                            .fill(if i == app.search.active { t.surface2 } else { egui::Color32::TRANSPARENT })
                            .stroke(egui::Stroke::NONE)
                            .min_size(vec2(width - 16.0, 30.0)),
                    );
                    if resp.hovered() {
                        app.search.active = i;
                    }
                    if resp.clicked() {
                        chosen = Some((r.clone(), ui.input(|i| i.modifiers.shift)));
                    }
                }
                ui.label(RichText::new("Enter to open · Shift+Enter to add to the route").size(10.5).color(t.text3));
            });
            // a click anywhere else closes the list
            let pressed_elsewhere = ui.input(|i| i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|p| !list.response.rect.contains(p) && !box_resp.response.rect.contains(p)));
            if pressed_elsewhere {
                app.search.open = false;
            }
            if let Some((p, add)) = chosen {
                app.search.q.clear();
                app.search.last.clear();
                app.search.results.clear();
                app.search.open = false;
                ui.memory_mut(|m| m.surrender_focus(ui.id()));
                pick(app, &p, add);
            }
        }
        if ui.input(|i| i.key_pressed(Key::Escape)) {
            app.search.q.clear();
            app.search.open = false;
        }
    });
    let _ = Align2::CENTER_TOP;
}
