//! The planning timeline along the bottom of the chart: now to five days out. Dragging it sets
//! the departure time, so the radar (observed, HRRR forecast, then model precipitation), winds
//! aloft, forecasts and the go / no-go call all follow. With a route, the track is coloured by
//! the call for departures every 3 hours: the green stretches are your windows to fly.

use super::widgets::glass;
use crate::app::App;
use crate::nav::verdict::Call;
use crate::theme;
use chrono::{DateTime, Duration, Local, TimeZone, Utc};
use egui::{pos2, vec2, Align2, Color32, CornerRadius, Margin, Rect, RichText, Sense, Stroke};
use egui_phosphor::regular as ph;
use std::time::Instant;

const HOURS: i64 = 120;

#[derive(Default)]
pub struct Timeline {
    playing: bool,
    last_step: Option<Instant>,
}

fn start() -> DateTime<Utc> {
    let now = Utc::now().timestamp();
    Utc.timestamp_opt(now / 900 * 900, 0).single().unwrap_or_else(Utc::now)
}

/// Snap to 15 minutes within the HRRR's 18 hours, whole hours after that.
fn snap(t: DateTime<Utc>) -> DateTime<Utc> {
    let step = if (t - Utc::now()).num_hours() < 18 { 900 } else { 3600 };
    let s = (t.timestamp() as f64 / step as f64).round() as i64 * step;
    Utc.timestamp_opt(s, 0).single().unwrap_or(t)
}

pub fn set_departure(app: &mut App, t: DateTime<Utc>) {
    app.saved.route.departure = if (t - Utc::now()).num_minutes() < 10 { String::new() } else { t.with_timezone(&Local).format("%Y-%m-%dT%H:%M").to_string() };
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    // only when there's something time-dependent on screen
    if app.saved.route.waypoints.len() < 2 && !app.saved.wx.radar && !app.saved.wx.winds {
        return;
    }
    let t = app.t;
    let screen = ctx.content_rect();
    let left = screen.left() + super::shell::panel_offset(app);
    let right = screen.right() - 12.0;
    let has_route = app.saved.route.waypoints.len() >= 2;
    let bottom = if has_route {
        screen.bottom() - 12.0 - if app.saved.profile_open { super::profile::height(true) + 10.0 } else { 0.0 }
    } else {
        screen.bottom() - 12.0
    };
    let avail = right - left;
    let width = avail.min(760.0).max(320.0);
    // with the profile collapsed it sits bottom-left: keep clear of it
    let x = if has_route && !app.saved.profile_open { (left + 232.0).max(left + (avail - width) / 2.0).min(right - width) } else { left + (avail - width) / 2.0 };
    let t0 = start();
    let t1 = t0 + Duration::hours(HOURS);
    let dep = app.saved.route.departure_time().max(t0);
    let mut new_time: Option<DateTime<Utc>> = None;

    // play: step through time (an hour at a time, 3 h beyond two days)
    if app.timeline.playing {
        let due = app.timeline.last_step.is_none_or(|s| s.elapsed().as_millis() > 650);
        if due && !app.chart.radar_loading {
            app.timeline.last_step = Some(Instant::now());
            let step = if (dep - Utc::now()).num_hours() < 48 { 1 } else { 3 };
            let next = snap(dep + Duration::hours(step));
            new_time = Some(if next > t1 { t0 } else { next });
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(120));
    }

    let height = 64.0;
    super::tour::mark(ctx, "timeline", egui::Rect::from_min_size(pos2(x, bottom - height), vec2(width, height)));
    egui::Area::new("timeline".into()).fixed_pos(pos2(x, bottom - height)).order(egui::Order::Middle).show(ctx, |ui| {
        glass(&t).corner_radius(CornerRadius::same(16)).inner_margin(Margin { left: 10, right: 14, top: 7, bottom: 6 }).show(ui, |ui| {
            ui.set_width(width - 24.0);
            ui.horizontal(|ui| {
                let play = if app.timeline.playing { ph::PAUSE } else { ph::PLAY };
                if ui.add(egui::Button::new(RichText::new(play).size(15.0).color(t.accent)).frame(false).min_size(vec2(24.0, 22.0))).on_hover_text("Play through the forecast").clicked() {
                    app.timeline.playing = !app.timeline.playing;
                    app.timeline.last_step = None;
                    if app.timeline.playing && !app.saved.wx.radar {
                        app.saved.wx.radar = true;
                    }
                }
                let local = dep.with_timezone(&Local);
                let is_now = app.saved.route.departure.is_empty();
                let title = if is_now { format!("Now · {}", local.format("%a %H:%M")) } else { local.format("%a %d %b · %H:%M").to_string() };
                ui.label(RichText::new(title).font(theme::semibold(13.0)));
                if !is_now {
                    ui.label(RichText::new(crate::wx::forecast::lead_text(dep)).size(11.5).color(t.text3));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if !is_now && ui.add(egui::Button::new(RichText::new("Now").size(11.5)).min_size(vec2(0.0, 22.0))).clicked() {
                        new_time = Some(Utc::now());
                        app.timeline.playing = false;
                    }
                    if app.chart.radar_loading {
                        ui.add(egui::Spinner::new().size(11.0));
                    }
                    match &app.chart.radar_frame {
                        Some(f) => {
                            ui.add(egui::Label::new(RichText::new(f.label()).size(11.0).color(t.text3)).truncate());
                        }
                        None if app.saved.wx.radar => {
                            ui.label(RichText::new("No official forecast this far out").size(11.0).color(t.text3));
                        }
                        None => {}
                    }
                });
            });
            // ---- the track ----
            let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::click_and_drag());
            let track = Rect::from_min_max(pos2(rect.left() + 6.0, rect.top() + 2.0), pos2(rect.right() - 6.0, rect.top() + 10.0));
            let x_of = |when: DateTime<Utc>| track.left() + ((when - t0).num_seconds() as f32 / (t1 - t0).num_seconds() as f32).clamp(0.0, 1.0) * track.width();
            let t_of = |px: f32| t0 + Duration::seconds((((px - track.left()) / track.width()).clamp(0.0, 1.0) * (t1 - t0).num_seconds() as f32) as i64);
            let p = ui.painter();
            p.rect_filled(track, CornerRadius::same(4), t.surface2);
            // go / no-go windows
            let windows = &app.plan.windows;
            if has_route && !windows.is_empty() {
                for (i, (ts, call)) in windows.iter().enumerate() {
                    let a = Utc.timestamp_opt(*ts - 5400, 0).single().unwrap_or(t0).max(t0);
                    let b = Utc.timestamp_opt(windows.get(i + 1).map(|w| w.0 - 5400).unwrap_or(ts + 5400), 0).single().unwrap_or(t1).min(t1);
                    let c = match call {
                        Call::Go => t.ok,
                        Call::Caution => t.warn,
                        Call::NoGo => t.bad,
                    };
                    let r = Rect::from_min_max(pos2(x_of(a), track.top()), pos2(x_of(b), track.bottom()));
                    p.rect_filled(r, CornerRadius::ZERO, c.gamma_multiply(0.8));
                }
            }
            // nights, as a faint shade, and day labels at local midnight
            let mut day = Local.from_local_datetime(&t0.with_timezone(&Local).date_naive().and_hms_opt(0, 0, 0).unwrap_or_default()).single().map(|d| d.with_timezone(&Utc)).unwrap_or(t0);
            while day < t1 {
                let next = day + Duration::days(1);
                if day > t0 {
                    let xd = x_of(day);
                    p.line_segment([pos2(xd, track.top() - 2.0), pos2(xd, track.bottom() + 2.0)], Stroke::new(1.0, t.line_strong));
                }
                let mid = x_of((day + Duration::hours(12)).clamp(t0, t1));
                let label = day.with_timezone(&Local).format("%a %d").to_string();
                if mid > track.left() + 12.0 && mid < track.right() - 12.0 {
                    p.text(pos2(mid, track.bottom() + 7.0), Align2::CENTER_CENTER, label, theme::regular(10.5), t.text3);
                }
                day = next;
            }
            // HRRR's reach: beyond it the picture is the WPC forecast chart
            let reach = app.chart.runs.reach().unwrap_or_else(|| Utc::now() + Duration::hours(18));
            let hrrr = x_of(reach);
            p.line_segment([pos2(hrrr, track.top()), pos2(hrrr, track.bottom())], Stroke::new(1.0, t.text3.gamma_multiply(0.6)));
            // hover readout
            if let Some(h) = resp.hover_pos() {
                let ht = snap(t_of(h.x));
                let call = windows.iter().min_by_key(|w| (w.0 - ht.timestamp()).abs()).filter(|_| has_route).map(|w| format!(" · {}", w.1.word())).unwrap_or_default();
                resp.clone().on_hover_text_at_pointer(format!("{}{call}", ht.with_timezone(&Local).format("%a %d %b %H:%M")));
            }
            // the handle
            let hx = x_of(dep);
            p.line_segment([pos2(hx, track.top() - 3.0), pos2(hx, track.bottom() + 3.0)], Stroke::new(2.0, t.accent));
            p.circle(pos2(hx, track.center().y), 7.0, Color32::WHITE, Stroke::new(2.5, t.accent));
            if let Some(pos) = resp.interact_pointer_pos() {
                if resp.dragged() || resp.clicked() {
                    new_time = Some(snap(t_of(pos.x)));
                    app.timeline.playing = false;
                }
            }
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
        });
    });
    if let Some(nt) = new_time {
        set_departure(app, nt);
    }
}
