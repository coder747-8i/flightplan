//! The go / no-go card: a green, yellow or red call with the reasons behind it, and the
//! checks that passed. Used by the route panel and the briefing.

use super::widgets;
use crate::app::App;
use crate::nav::verdict::{Call, Factor, Verdict};
use crate::theme::{self, Tokens};
use egui::{vec2, Color32, CornerRadius, Frame, Margin, RichText, Stroke};
use egui_phosphor::regular as ph;

pub fn color(t: &Tokens, c: Call) -> Color32 {
    match c {
        Call::Go => t.ok,
        Call::Caution => t.warn,
        Call::NoGo => t.bad,
    }
}

pub fn icon(c: Call) -> &'static str {
    match c {
        Call::Go => ph::CHECK_CIRCLE,
        Call::Caution => ph::WARNING,
        Call::NoGo => ph::PROHIBIT,
    }
}

/// Solid pill: "● GO".
pub fn badge(ui: &mut egui::Ui, t: &Tokens, c: Call, size: f32) -> egui::Response {
    Frame::new()
        .fill(color(t, c))
        .corner_radius(CornerRadius::same(99))
        .inner_margin(Margin::symmetric((size * 0.7) as i8, (size * 0.25) as i8))
        .show(ui, |ui| {
            ui.label(RichText::new(format!("{}  {}", icon(c), c.word())).font(theme::bold(size)).color(Color32::WHITE));
        })
        .response
}

fn factor_row(ui: &mut egui::Ui, t: &Tokens, f: &Factor, detail: bool) -> egui::Response {
    let c = color(t, f.call);
    let r = ui.horizontal_top(|ui| {
        ui.add_space(2.0);
        ui.label(RichText::new(icon(f.call)).color(c).size(14.0));
        ui.vertical(|ui| {
            let at = f.at.filter(|d| *d > 0.5 && !f.title.contains(" NM")).map(|d| format!("  ·  {d:.0} NM")).unwrap_or_default();
            ui.add(egui::Label::new(RichText::new(format!("{}{at}", f.title)).font(theme::semibold(12.5))).wrap());
            if detail && !f.detail.is_empty() {
                ui.add(egui::Label::new(RichText::new(&f.detail).size(11.5).color(t.text2)).wrap());
            }
        });
    });
    ui.add_space(3.0);
    r.response
}

/// The full card. `id` keeps its expanded state.
pub fn card(ui: &mut egui::Ui, app: &mut App, v: &Verdict, id: &str) {
    let t = app.t;
    let c = color(&t, v.call);
    let open_id = egui::Id::new(id).with("open");
    let checks_id = egui::Id::new(id).with("checks");
    let mut open: bool = ui.data(|d| d.get_temp(open_id)).unwrap_or(v.call == Call::NoGo);
    let mut checks: bool = ui.data(|d| d.get_temp(checks_id)).unwrap_or(false);
    let dep = app.saved.route.departure_time();
    let when = dep.with_timezone(&chrono::Local);
    let eta = app.plan.log.as_ref().map(|l| (dep + chrono::Duration::milliseconds((l.total_time * 60_000.0) as i64)).with_timezone(&chrono::Local));
    let mut hover: Option<f64> = None;
    Frame::new().fill(c.gamma_multiply(if t.night { 0.16 } else { 0.09 })).stroke(Stroke::new(1.0, c.gamma_multiply(0.45))).corner_radius(CornerRadius::same(12)).inner_margin(Margin::same(12)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            badge(ui, &t, v.call, 13.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let lead = crate::wx::forecast::lead_text(dep);
                ui.label(RichText::new(format!("{} · {lead}", when.format("%a %d %b %H:%M"))).size(11.5).color(t.text2));
            });
        });
        ui.add_space(4.0);
        ui.add(egui::Label::new(RichText::new(&v.headline).font(theme::semibold(14.0)).color(t.text)).wrap());
        if let Some(eta) = eta {
            ui.label(RichText::new(format!("Departs {} · arrives {}", when.format("%H:%M"), eta.format("%H:%M"))).size(11.5).color(t.text3));
        }
        let problems: Vec<&Factor> = v.problems().collect();
        if problems.len() == 1 && problems[0].title == v.headline {
            // the headline is the one problem: just add its detail
            if !problems[0].detail.is_empty() {
                ui.add(egui::Label::new(RichText::new(&problems[0].detail).size(11.5).color(t.text2)).wrap());
            }
            if ui.rect_contains_pointer(ui.min_rect()) {
                hover = problems[0].at;
            }
        } else if !problems.is_empty() {
            ui.add_space(6.0);
            let shown = if open { problems.len() } else { problems.len().min(3) };
            for f in &problems[..shown] {
                if factor_row(ui, &t, f, open).hovered() {
                    hover = f.at;
                }
            }
            if problems.len() > 3 || problems.iter().any(|f| !f.detail.is_empty()) {
                let label = if open { "Less detail".to_string() } else if problems.len() > 3 { format!("All {} reasons and details", problems.len()) } else { "Details".to_string() };
                if widgets::link(ui, &t, label).clicked() {
                    open = !open;
                }
            }
        }
        // not green: point at the next window that is
        if v.call != Call::Go {
            if let Some((a, b)) = next_green(&app.plan.windows, dep.timestamp()) {
                let fmt = |ts: i64| chrono::DateTime::from_timestamp(ts, 0).map(|d| d.with_timezone(&chrono::Local).format("%a %H:%M").to_string()).unwrap_or_default();
                let label = format!("{}  Next green window: {} – {}", ph::CALENDAR_CHECK, fmt(a), fmt(b));
                ui.add_space(2.0);
                if widgets::link(ui, &t, label).on_hover_text("Move the departure to the start of it").clicked() {
                    if let Some(d) = chrono::DateTime::from_timestamp(a, 0) {
                        super::timeline::set_departure(app, d);
                    }
                }
            }
        }
        let n_checks = v.checks().count();
        if n_checks > 0 {
            ui.add_space(2.0);
            let label = format!("{} {} check{} passed", if checks { ph::CARET_DOWN } else { ph::CARET_RIGHT }, n_checks, if n_checks == 1 { "" } else { "s" });
            if ui.add(egui::Button::new(RichText::new(label).size(11.5).color(t.text2)).frame(false)).clicked() {
                checks = !checks;
            }
            if checks {
                for f in v.checks() {
                    factor_row(ui, &t, f, true);
                }
            }
        }
        ui.add_space(2.0);
        ui.label(RichText::new("Advisory only. You are the pilot in command.").size(10.5).color(t.text3));
    });
    ui.data_mut(|d| {
        d.insert_temp(open_id, open);
        d.insert_temp(checks_id, checks);
    });
    if let Some(d) = hover {
        app.plan.hover = Some((d, true));
    }
}

/// The next run of "go" departures after `from` in the 3-hourly windows: (start, end) unix times.
pub fn next_green(windows: &[(i64, Call)], from: i64) -> Option<(i64, i64)> {
    let start = windows.iter().position(|w| w.0 > from && w.1 == Call::Go)?;
    let mut end = start;
    while end + 1 < windows.len() && windows[end + 1].1 == Call::Go {
        end += 1;
    }
    Some((windows[start].0, windows[end].0))
}

/// One-line summary for places with no room for the card.
pub fn line(ui: &mut egui::Ui, t: &Tokens, v: &Verdict) {
    ui.horizontal(|ui| {
        badge(ui, t, v.call, 11.0);
        ui.add(egui::Label::new(RichText::new(&v.headline).size(12.0).color(t.text2)).truncate());
    });
    let _ = vec2(0.0, 0.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_next_green_run() {
        let w = [(0, Call::Go), (10, Call::NoGo), (20, Call::Caution), (30, Call::Go), (40, Call::Go), (50, Call::NoGo)];
        assert_eq!(next_green(&w, 5), Some((30, 40)));
        assert_eq!(next_green(&w, 45), None);
    }
}
