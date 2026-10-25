//! Shared building blocks: floating "glass" panels, section titles, toggles, cards, status
//! boxes, number fields and buttons, styled like the web app.

use crate::theme::{self, Tokens};
use egui::{vec2, Align, Color32, CornerRadius, Frame, Layout, Margin, Response, RichText, Sense, Stroke, Ui};
use egui_phosphor::regular as ph;

pub fn glass(t: &Tokens) -> Frame {
    Frame::new()
        .fill(t.surface_solid)
        .stroke(Stroke::new(1.0, t.line_strong))
        .corner_radius(CornerRadius::same(14))
        .shadow(egui::epaint::Shadow { offset: [0, 6], blur: 24, spread: 0, color: Color32::from_black_alpha(if t.night { 90 } else { 30 }) })
}

/// A floating panel docked to the left of the screen, beside the rail.
/// `state` is handed to `actions` (header buttons; return true to close) and then to `body`.
#[allow(clippy::too_many_arguments)]
pub fn side_panel<S>(ctx: &egui::Context, t: &Tokens, id: &str, title: &str, subtitle: &str, width: f32, state: &mut S, actions: impl FnOnce(&mut Ui, &mut S) -> bool, body: impl FnOnce(&mut Ui, &mut S)) -> bool {
    let screen = ctx.content_rect();
    let mut close = false;
    let height = screen.height() - 24.0;
    egui::Area::new(egui::Id::new(id)).fixed_pos(screen.min + vec2(88.0, 12.0)).order(egui::Order::Middle).show(ctx, |ui| {
        glass(t).show(ui, |ui| {
            ui.set_width(width);
            ui.set_height(height);
            ui.vertical(|ui| {
                Frame::new().inner_margin(Margin { left: 18, right: 12, top: 14, bottom: 6 }).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(RichText::new(title).font(theme::bold(16.0)));
                            if !subtitle.is_empty() {
                                ui.label(RichText::new(subtitle).size(12.0).color(t.text3));
                            }
                        });
                        ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                            if icon_button(ui, t, ph::X, "Close").clicked() {
                                close = true;
                            }
                            if actions(ui, state) {
                                close = true;
                            }
                        });
                    });
                });
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    Frame::new().inner_margin(Margin { left: 18, right: 18, top: 2, bottom: 18 }).show(ui, |ui| {
                        ui.set_width(width - 36.0);
                        body(ui, state);
                    });
                });
            });
        });
    });
    close
}

pub fn section(ui: &mut Ui, t: &Tokens, text: &str) {
    ui.add_space(12.0);
    ui.label(RichText::new(text.to_uppercase()).font(theme::semibold(11.0)).color(t.text3).extra_letter_spacing(0.6));
    ui.add_space(2.0);
}

pub fn faint(ui: &mut Ui, t: &Tokens, text: impl Into<String>) -> Response {
    ui.label(RichText::new(text.into()).size(11.5).color(t.text3))
}

pub fn muted(ui: &mut Ui, t: &Tokens, text: impl Into<String>) -> Response {
    ui.label(RichText::new(text.into()).color(t.text2))
}

/// A wrapped paragraph in a given colour.
pub fn para(ui: &mut Ui, text: impl Into<String>, color: Color32, size: f32) {
    ui.add(egui::Label::new(RichText::new(text.into()).color(color).size(size)).wrap());
}

pub fn icon_button(ui: &mut Ui, t: &Tokens, icon: &str, tip: &str) -> Response {
    let r = ui.add(egui::Button::new(RichText::new(icon).size(16.0).color(t.text2)).frame(false).min_size(vec2(30.0, 30.0)));
    r.on_hover_text(tip)
}

pub fn primary(ui: &mut Ui, t: &Tokens, text: impl Into<String>) -> Response {
    ui.add(egui::Button::new(RichText::new(text.into()).color(Color32::WHITE).font(theme::semibold(13.0))).fill(t.accent).stroke(Stroke::NONE).min_size(vec2(0.0, 32.0)))
}

pub fn button(ui: &mut Ui, text: impl Into<String>) -> Response {
    ui.add(egui::Button::new(text.into()).min_size(vec2(0.0, 32.0)))
}

pub fn small_button(ui: &mut Ui, text: impl Into<String>) -> Response {
    ui.add(egui::Button::new(RichText::new(text.into()).size(12.0)).min_size(vec2(0.0, 26.0)))
}

pub fn ghost(ui: &mut Ui, t: &Tokens, text: impl Into<String>) -> Response {
    ui.add(egui::Button::new(RichText::new(text.into()).color(t.text2)).frame(false).min_size(vec2(0.0, 30.0)))
}

/// Link-styled text button.
pub fn link(ui: &mut Ui, t: &Tokens, text: impl Into<String>) -> Response {
    ui.add(egui::Button::new(RichText::new(text.into()).color(t.accent).font(theme::semibold(12.0))).frame(false)).on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// iOS-style switch.
pub fn switch(ui: &mut Ui, t: &Tokens, on: &mut bool) -> Response {
    let size = vec2(34.0, 20.0);
    let (rect, mut resp) = ui.allocate_exact_size(size, Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let k = ui.ctx().animate_bool_responsive(resp.id, *on);
    let bg = if *on { t.accent } else { t.line_strong };
    ui.painter().rect_filled(rect, CornerRadius::same(10), bg);
    let x = egui::lerp(rect.left() + 10.0..=rect.right() - 10.0, k);
    ui.painter().circle_filled(egui::pos2(x, rect.center().y), 8.0, Color32::WHITE);
    resp
}

/// Labelled switch row with an optional hint and colour swatch.
pub fn toggle_row(ui: &mut Ui, t: &Tokens, on: &mut bool, label: &str, hint: &str, swatch: Option<Color32>) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        if let Some(c) = swatch {
            let (r, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
            ui.painter().rect_filled(r, CornerRadius::same(3), c);
        }
        let w = (ui.available_width() - 48.0).max(60.0);
        ui.vertical(|ui| {
            ui.set_width(w);
            ui.label(RichText::new(label).size(13.0));
            if !hint.is_empty() {
                ui.add(egui::Label::new(RichText::new(hint).size(11.0).color(t.text3)).wrap());
            }
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if switch(ui, t, on).changed() {
                changed = true;
            }
        });
    });
    ui.add_space(4.0);
    changed
}

/// One-line switch row: swatch, label, a hint on hover, the switch on the right.
pub fn toggle_line(ui: &mut Ui, t: &Tokens, on: &mut bool, label: &str, hint: &str, swatch: Option<Color32>) -> bool {
    let mut changed = false;
    let r = ui.horizontal(|ui| {
        ui.set_min_height(26.0);
        match swatch {
            Some(c) => {
                let (r, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
                ui.painter().rect_filled(r, CornerRadius::same(3), c);
            }
            None => {
                ui.add_space(10.0);
            }
        }
        ui.label(RichText::new(label).size(13.0));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if switch(ui, t, on).changed() {
                changed = true;
            }
        });
    });
    if !hint.is_empty() {
        r.response.on_hover_text(hint);
    }
    changed
}

/// A section header that folds its body; remembers the state by `id`.
pub fn fold(ui: &mut Ui, t: &Tokens, id: &str, title: &str, default_open: bool, body: impl FnOnce(&mut Ui)) {
    let key = egui::Id::new(id);
    let mut open: bool = ui.data(|d| d.get_temp(key)).unwrap_or(default_open);
    ui.add_space(10.0);
    let label = format!("{}  {}", if open { ph::CARET_DOWN } else { ph::CARET_RIGHT }, title.to_uppercase());
    if ui.add(egui::Button::new(RichText::new(label).font(theme::semibold(11.0)).color(t.text3).extra_letter_spacing(0.6)).frame(false)).clicked() {
        open = !open;
        ui.data_mut(|d| d.insert_temp(key, open));
    }
    if open {
        body(ui);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Ok,
    Warn,
    Bad,
    Info,
    Accent,
}

pub fn tone_color(t: &Tokens, tone: Tone) -> Color32 {
    match tone {
        Tone::Ok => t.ok,
        Tone::Warn => t.warn,
        Tone::Bad => t.bad,
        Tone::Info => t.blue,
        Tone::Accent => t.accent,
    }
}

/// Tinted box with an icon and lines of text.
pub fn status(ui: &mut Ui, t: &Tokens, tone: Tone, icon: &str, lines: &[String]) {
    let c = tone_color(t, tone);
    Frame::new().fill(c.gamma_multiply(0.1)).corner_radius(CornerRadius::same(9)).inner_margin(Margin::symmetric(10, 8)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_top(|ui| {
            ui.label(RichText::new(icon).color(c).size(15.0));
            ui.vertical(|ui| {
                for l in lines {
                    ui.add(egui::Label::new(RichText::new(l).color(c).font(theme::semibold(12.0))).wrap());
                }
            });
        });
    });
}

/// A big clickable option card: icon, title, description.
pub fn card(ui: &mut Ui, t: &Tokens, icon: &str, title: &str, desc: &str, width: f32) -> Response {
    let resp = Frame::new()
        .fill(t.surface_solid)
        .stroke(Stroke::new(1.0, t.line_strong))
        .corner_radius(CornerRadius::same(11))
        .inner_margin(Margin::same(12))
        .show(ui, |ui| {
            ui.set_width(width - 26.0);
            ui.vertical(|ui| {
                ui.label(RichText::new(icon).size(20.0).color(t.accent));
                ui.label(RichText::new(title).font(theme::semibold(13.5)));
                ui.add(egui::Label::new(RichText::new(desc).size(11.5).color(t.text3)).wrap());
            });
        })
        .response;
    let resp = resp.interact(Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, title));
    if resp.hovered() {
        ui.painter().rect_stroke(resp.rect, CornerRadius::same(11), Stroke::new(1.5, t.accent), egui::StrokeKind::Inside);
    }
    resp
}

pub fn chip(ui: &mut Ui, t: &Tokens, text: &str, fg: Option<Color32>, bg: Option<Color32>) {
    Frame::new()
        .fill(bg.unwrap_or(t.surface2))
        .corner_radius(CornerRadius::same(99))
        .inner_margin(Margin::symmetric(8, 2))
        .show(ui, |ui| {
            ui.label(RichText::new(text).font(theme::semibold(11.0)).color(fg.unwrap_or(t.text2)));
        });
}

/// Number field with a unit, e.g. "lb", "kt".
pub fn num(ui: &mut Ui, v: &mut f64, unit: &str, speed: f64, range: std::ops::RangeInclusive<f64>, decimals: usize) -> Response {
    ui.horizontal(|ui| {
        let r = ui.add(egui::DragValue::new(v).speed(speed).range(range).max_decimals(decimals).min_decimals(0));
        if !unit.is_empty() {
            ui.label(RichText::new(unit).size(11.0).weak());
        }
        r
    })
    .inner
}

/// Optional number (None = empty) as a text field, for forms where "missing" matters.
pub fn opt_num(ui: &mut Ui, v: &mut Option<f64>, width: f32, hint: &str) -> Response {
    let id = ui.next_auto_id();
    let mut text: String = ui.data_mut(|d| d.get_temp::<String>(id)).unwrap_or_else(|| v.map(crate::model::aircraft::fmt_num).unwrap_or_default());
    let r = ui.add(egui::TextEdit::singleline(&mut text).desired_width(width).hint_text(hint).horizontal_align(Align::RIGHT));
    if r.changed() {
        let clean = text.replace(',', "");
        *v = clean.trim().parse().ok();
    }
    if r.has_focus() {
        ui.data_mut(|d| d.insert_temp(id, text));
    } else {
        ui.data_mut(|d| d.remove::<String>(id));
    }
    r
}

pub fn toast(ctx: &egui::Context, t: &Tokens, msg: &str) {
    let screen = ctx.content_rect();
    egui::Area::new(egui::Id::new("toast")).order(egui::Order::Tooltip).anchor(egui::Align2::CENTER_BOTTOM, vec2(0.0, -24.0)).show(ctx, |ui| {
        Frame::new().fill(if t.night { t.surface2 } else { Color32::from_rgb(30, 34, 41) }).corner_radius(CornerRadius::same(99)).inner_margin(Margin::symmetric(16, 9)).show(ui, |ui| {
            ui.set_max_width(screen.width() * 0.6);
            ui.label(RichText::new(msg).color(Color32::WHITE).font(theme::semibold(12.5)));
        });
    });
}

/// Copy text to the clipboard with a short confirmation.
pub fn copy(ui: &Ui, text: &str) {
    ui.ctx().copy_text(text.to_string());
}

/// A two-column label/value grid row.
pub fn kv(ui: &mut Ui, t: &Tokens, k: &str, v: impl Into<String>) {
    ui.label(RichText::new(k).size(12.0).color(t.text3));
    ui.add(egui::Label::new(RichText::new(v.into()).size(12.5)).wrap());
    ui.end_row();
}

/// Horizontal rule.
pub fn rule(ui: &mut Ui, t: &Tokens) {
    let w = ui.available_width();
    let (r, _) = ui.allocate_exact_size(vec2(w, 9.0), Sense::hover());
    ui.painter().hline(r.x_range(), r.center().y, Stroke::new(1.0, t.line));
}
