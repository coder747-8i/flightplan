//! Everything floating over the chart: the rail, search, altitude filter, data banner, the open
//! side panel, the details card and the vertical profile.

use super::tour::mark;
use super::widgets::{self, glass};
use crate::app::{App, Panel};
use crate::theme;
use egui::{pos2, vec2, Align2, Color32, CornerRadius, Frame, Margin, RichText, Sense, Stroke};
use egui_phosphor::regular as ph;

pub fn show(app: &mut App, ctx: &egui::Context) {
    rail(app, ctx);
    super::search::show(app, ctx);
    altitude(app, ctx);
    ruler_button(app, ctx);
    banner(app, ctx);
    match app.panel {
        Panel::Route => super::route::panel(app, ctx),
        Panel::Layers => super::layers::panel(app, ctx),
        Panel::Weather => super::weather::panel(app, ctx),
        Panel::Aircraft => super::aircraft::panel(app, ctx),
        Panel::Settings => super::settings::panel(app, ctx),
        Panel::Help => super::help::panel(app, ctx),
        Panel::None => {}
    }
    super::profile::show(app, ctx);
    super::timeline::show(app, ctx);
    super::info::show(app, ctx);
}

/// Left edge of the free chart area (after the rail and an open panel).
pub fn panel_offset(app: &App) -> f32 {
    match app.panel {
        Panel::None => 88.0,
        Panel::Route | Panel::Aircraft | Panel::Settings => 380.0 + 100.0,
        _ => 340.0 + 100.0,
    }
}

fn rail(app: &mut App, ctx: &egui::Context) {
    let t = app.t;
    let screen = ctx.content_rect();
    egui::Area::new("rail".into()).fixed_pos(screen.min + vec2(12.0, 12.0)).order(egui::Order::Middle).show(ctx, |ui| {
        glass(&t).inner_margin(Margin::symmetric(5, 10)).show(ui, |ui| {
            ui.set_width(54.0);
            ui.set_height(screen.height() - 44.0);
            ui.vertical_centered(|ui| {
                // brand mark: the route line
                let (r, _) = ui.allocate_exact_size(vec2(40.0, 40.0), Sense::hover());
                let c = t.accent;
                let p = |x: f32, y: f32| r.min + vec2(9.0 + x, 9.0 + y);
                ui.painter().add(egui::Shape::line(vec![p(0.0, 17.0), p(7.0, 11.0), p(12.0, 13.5), p(21.0, 5.0)], Stroke::new(2.6, c)));
                ui.painter().circle_filled(p(0.0, 17.0), 2.4, c);
                ui.painter().circle_filled(p(21.0, 5.0), 2.4, c);
                ui.add_space(6.0);
                let items = [(Panel::Route, ph::PATH, "Route", "rail-route"), (Panel::Layers, ph::STACK, "Layers", "rail-layers"), (Panel::Weather, ph::CLOUD_SUN, "Weather", "rail-weather"), (Panel::Aircraft, ph::AIRPLANE_TILT, "Aircraft", "rail-aircraft")];
                for (panel, icon, label, tid) in items {
                    let r = rail_button(ui, &t, icon, label, app.panel == panel);
                    // the go / no-go call as a dot on the Route button
                    if panel == Panel::Route {
                        if let Some(v) = app.plan.verdict.as_ref().filter(|_| app.saved.route.waypoints.len() >= 2) {
                            let c = super::verdict::color(&t, v.call);
                            let p = r.rect.center() + vec2(11.0, -15.0);
                            ui.painter().circle(p, 5.0, c, Stroke::new(2.0, t.surface_solid));
                        }
                    }
                    mark(ctx, tid, r.rect);
                    if r.clicked() {
                        app.panel = if app.panel == panel { Panel::None } else { panel };
                    }
                }
                let r = rail_button(ui, &t, ph::FILE_TEXT, "Briefing", app.briefing.is_some());
                mark(ctx, "rail-briefing", r.rect);
                if r.clicked() {
                    app.briefing = Some(super::briefing::Briefing::new(app));
                }
                let used = ui.min_rect().height();
                ui.add_space((screen.height() - 44.0 - used - 4.0 * 52.0).max(4.0));
                let r = rail_button(ui, &t, ph::GEAR_SIX, "Settings", app.panel == Panel::Settings);
                mark(ctx, "rail-settings", r.rect);
                if r.clicked() {
                    app.panel = if app.panel == Panel::Settings { Panel::None } else { Panel::Settings };
                }
                let locked = app.saved.locked;
                let r = rail_button(ui, &t, if locked { ph::LOCK_SIMPLE } else { ph::LOCK_SIMPLE_OPEN }, if locked { "Locked" } else { "Edit" }, locked).on_hover_text(if locked { "View only: the plan can't be changed. Click (or L) to edit." } else { "Editing. Click (or L) to lock the plan for viewing." });
                mark(ctx, "rail-lock", r.rect);
                if r.clicked() {
                    app.saved.locked = !locked;
                    app.toast(if app.saved.locked { "View only: the plan is locked (L to edit)" } else { "Editing on" });
                }
                let night = app.night();
                if rail_button(ui, &t, if night { ph::SUN } else { ph::MOON }, if night { "Day" } else { "Night" }, false).on_hover_text("Day / night chart").clicked() {
                    app.set_night(!night);
                }
                let r = rail_button(ui, &t, ph::QUESTION, "Help", app.panel == Panel::Help);
                mark(ctx, "rail-help", r.rect);
                if r.clicked() {
                    app.panel = if app.panel == Panel::Help { Panel::None } else { Panel::Help };
                }
            });
        });
    });
}

fn rail_button(ui: &mut egui::Ui, t: &theme::Tokens, icon: &str, label: &str, active: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(54.0, 50.0), Sense::click());
    let hovered = resp.hovered();
    if active || hovered {
        ui.painter().rect_filled(rect, CornerRadius::same(10), if active { t.accent_soft } else { t.surface2 });
    }
    let color = if active { t.accent } else if hovered { t.text } else { t.text2 };
    ui.painter().text(rect.center() - vec2(0.0, 7.0), Align2::CENTER_CENTER, icon, egui::FontId::proportional(19.0), color);
    ui.painter().text(rect.center() + vec2(0.0, 13.0), Align2::CENTER_CENTER, label, theme::semibold(10.0), color);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn fmt_ft(ft: f64) -> String {
    if ft <= 0.0 {
        "SFC".into()
    } else if ft >= 18000.0 {
        "FL180+".into()
    } else if ft >= 10000.0 {
        format!("{:.1}k", ft / 1000.0)
    } else {
        crate::nav::navlog::thousands(ft)
    }
}

/// Garmin-style altitude band filter, top right.
fn altitude(app: &mut App, ctx: &egui::Context) {
    let t = app.t;
    let screen = ctx.content_rect();
    let a = app.saved.altitude;
    let text = if a.enabled { format!("{} – {}", fmt_ft(a.lo), fmt_ft(a.hi)) } else { "All altitudes".into() };
    egui::Area::new("altitude".into()).anchor(Align2::RIGHT_TOP, vec2(-12.0, 12.0)).order(egui::Order::Middle).show(ctx, |ui| {
        ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
            let pill = glass(&t).corner_radius(CornerRadius::same(99)).inner_margin(Margin::symmetric(12, 8)).show(ui, |ui| {
                ui.label(RichText::new(format!("{}  {text}", ph::MOUNTAINS)).font(theme::semibold(13.0)).color(if a.enabled { t.accent } else { t.text2 }));
            });
            let r = pill.response.interact(Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("Filter airspace by altitude");
            mark(ctx, "altitude", r.rect);
            if r.clicked() {
                app.altitude_open = !app.altitude_open;
            }
            if app.altitude_open {
                ui.add_space(8.0);
                glass(&t).inner_margin(Margin::same(14)).show(ui, |ui| {
                    ui.set_width(250.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Airspace altitude filter").font(theme::semibold(13.0)));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            widgets::switch(ui, &t, &mut app.saved.altitude.enabled);
                        });
                    });
                    widgets::faint(ui, &t, "Shows only airspace that overlaps this band.");
                    ui.add_space(6.0);
                    band_slider(ui, &t, &mut app.saved.altitude);
                    ui.add_space(6.0);
                    ui.horizontal_wrapped(|ui| {
                        for (label, lo, hi) in [("Below 3k", 0.0, 3000.0), ("Below 10k", 0.0, 10000.0), ("4.5k – 12.5k", 4500.0, 12500.0)] {
                            if widgets::small_button(ui, label).clicked() {
                                app.saved.altitude = crate::map::style::AltitudeFilter { enabled: true, lo, hi };
                            }
                        }
                    });
                });
            }
        });
    });
    let _ = screen;
}

/// The measuring tool's switch, under the altitude filter.
fn ruler_button(app: &mut App, ctx: &egui::Context) {
    let t = app.t;
    let on = app.chart.ruler.on;
    let y = if app.altitude_open { 360.0 } else { 58.0 };
    egui::Area::new("ruler-btn".into()).anchor(Align2::RIGHT_TOP, vec2(-12.0, y)).order(egui::Order::Middle).show(ctx, |ui| {
        let pill = glass(&t).fill(if on { t.accent } else { t.surface_solid }).corner_radius(CornerRadius::same(99)).inner_margin(Margin::symmetric(12, 7)).show(ui, |ui| {
            let label = if on { format!("{}  Measuring · Space new line · Esc clear", ph::RULER) } else { format!("{}  Measure", ph::RULER) };
            ui.add(egui::Label::new(RichText::new(label).font(theme::semibold(12.5)).color(if on { Color32::WHITE } else { t.text2 })).wrap_mode(egui::TextWrapMode::Extend));
        });
        let r = pill.response.interact(Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("Measure distance and course (R). Click points along a line; Space starts a new line (up to 3); Backspace removes the last point; Esc clears. Points snap to airports and navaids.");
        r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Measure"));
        mark(ctx, "ruler", r.rect);
        if r.clicked() {
            app.chart.ruler = Default::default();
            app.chart.ruler.on = !on;
        }
    });
}

/// Vertical two-handle slider, SFC to FL180.
fn band_slider(ui: &mut egui::Ui, t: &theme::Tokens, f: &mut crate::map::style::AltitudeFilter) {
    const MAX: f64 = 18000.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 210.0), Sense::click_and_drag());
    let track = egui::Rect::from_min_max(pos2(rect.left() + 60.0, rect.top() + 8.0), pos2(rect.left() + 74.0, rect.bottom() - 8.0));
    let y_of = |ft: f64| track.bottom() - (ft.min(MAX) / MAX) as f32 * track.height();
    let ft_of = |y: f32| (((track.bottom() - y) / track.height()).clamp(0.0, 1.0) as f64 * MAX / 500.0).round() * 500.0;
    let p = ui.painter();
    for tick in [0.0, 3000.0, 6000.0, 9000.0, 12000.0, 15000.0, 18000.0] {
        let label = if tick == 0.0 { "SFC".to_string() } else if tick >= MAX { "FL180".to_string() } else { format!("{}k", tick / 1000.0) };
        p.text(pos2(track.left() - 12.0, y_of(tick)), Align2::RIGHT_CENTER, label, theme::regular(11.0), t.text3);
    }
    p.rect_filled(track, CornerRadius::same(7), t.surface2);
    let band = egui::Rect::from_min_max(pos2(track.left(), y_of(f.hi)), pos2(track.right(), y_of(f.lo)));
    p.rect_filled(band, CornerRadius::same(7), if f.enabled { t.accent.gamma_multiply(0.35) } else { t.line_strong });
    for v in [f.hi, f.lo] {
        let c = pos2(track.center().x, y_of(v));
        p.circle(c, 8.0, Color32::WHITE, Stroke::new(2.0, if f.enabled { t.accent } else { t.text3 }));
        p.text(c + vec2(16.0, 0.0), Align2::LEFT_CENTER, fmt_ft(v), theme::semibold(12.0), t.text);
    }
    if let Some(pos) = resp.interact_pointer_pos() {
        if resp.dragged() || resp.clicked() {
            let v = ft_of(pos.y);
            f.enabled = true;
            if (v - f.lo).abs() < (v - f.hi).abs() {
                f.lo = v.min(f.hi - 500.0).max(0.0);
            } else {
                let hi = v.max(f.lo + 500.0);
                f.hi = if hi >= MAX { 99999.0 } else { hi };
            }
        }
    }
}

fn banner(app: &mut App, ctx: &egui::Context) {
    if app.onboarding.open {
        return;
    }
    let msg = match &app.data_info {
        None => "No chart data installed: open Settings to install it".to_string(),
        Some(i) if app.data_expired() => format!("Chart data expired {}: update it in Settings", i.expires.clone().unwrap_or_default()),
        _ => return,
    };
    let t = app.t;
    egui::Area::new("banner".into()).anchor(Align2::CENTER_TOP, vec2(0.0, 64.0)).order(egui::Order::Middle).show(ctx, |ui| {
        let r = glass(&t)
            .corner_radius(CornerRadius::same(99))
            .inner_margin(Margin::symmetric(14, 8))
            .show(ui, |ui| {
                ui.label(RichText::new(format!("{}  {msg}", ph::WARNING)).color(t.warn).font(theme::semibold(12.0)));
            })
            .response
            .interact(Sense::click());
        if r.clicked() {
            app.panel = Panel::Settings;
        }
    });
}

/// Small header bar used by full-screen pages (briefing).
pub fn page_frame(t: &theme::Tokens) -> Frame {
    Frame::new().fill(t.bg).inner_margin(Margin::ZERO)
}
