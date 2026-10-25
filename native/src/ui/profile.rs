//! Vertical profile: terrain (centerline + corridor high point), obstacles, airspace volumes the
//! route passes through, planned climb / cruise / descent, freezing level, and a strip of
//! warnings. Hover syncs with the map; drag the magenta cruise line to change altitude; click
//! a volume to see that airspace.

use super::tour::mark;
use super::widgets::{self, faint, glass};
use crate::app::App;
use crate::nav::geo::distance_nm;
use crate::nav::profile::{Crossing, Level};
use crate::theme;
use egui::{pos2, vec2, Align2, Color32, CornerRadius, Frame, Margin, Pos2, Rect, RichText, Sense, Shape, Stroke};
use egui_phosphor::regular as ph;

const H: f32 = 230.0;
const STRIP: f32 = 20.0;

struct KindStyle {
    color: Color32,
    dash: bool,
    fill: f32,
    label: &'static str,
}

fn kind_style(app: &App, k: &str) -> Option<KindStyle> {
    let (blue, mag) = (app.t.blue, app.t.magenta);
    let sua = Color32::from_rgb(0x2c, 0x63, 0xb8);
    Some(match k {
        "B" => KindStyle { color: blue, dash: false, fill: 0.16, label: "B" },
        "C" => KindStyle { color: mag, dash: false, fill: 0.14, label: "C" },
        "D" => KindStyle { color: blue, dash: true, fill: 0.08, label: "D" },
        "E2" => KindStyle { color: mag, dash: true, fill: 0.05, label: "E" },
        "E3" | "E4" => KindStyle { color: mag, dash: true, fill: 0.04, label: "E" },
        "E5" => KindStyle { color: mag, dash: true, fill: 0.03, label: "E" },
        "R" => KindStyle { color: sua, dash: false, fill: 0.18, label: "R" },
        "P" => KindStyle { color: sua, dash: false, fill: 0.28, label: "P" },
        "W" => KindStyle { color: sua, dash: false, fill: 0.10, label: "W" },
        "A" => KindStyle { color: mag, dash: false, fill: 0.08, label: "A" },
        "MOA" => KindStyle { color: mag, dash: false, fill: 0.10, label: "MOA" },
        _ => return None,
    })
}

fn fmt_alt(v: f64) -> String {
    if v >= 99999.0 {
        "UNL".into()
    } else if v >= 17999.0 {
        format!("FL{}", ((v + 1.0) / 100.0).round())
    } else if v == 0.0 {
        "SFC".into()
    } else {
        format!("{}", (v / 100.0).round())
    }
}

pub fn range_text(c: &Crossing) -> String {
    let lo = if c.lo_agl { format!("{} AGL", crate::nav::navlog::thousands(c.lo)) } else if c.lo == 0.0 { "SFC".into() } else { crate::nav::navlog::thousands(c.lo) };
    let hi = if c.hi >= 99999.0 { "unlimited".to_string() } else if c.hi == 17999.0 { "FL180".into() } else { crate::nav::navlog::thousands(c.hi) };
    format!("{lo} – {hi}")
}

/// Height of the profile panel, open or collapsed.
pub fn height(open: bool) -> f32 {
    if open { H + 50.0 } else { 44.0 }
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    if app.saved.route.waypoints.len() < 2 {
        return;
    }
    let t = app.t;
    let screen = ctx.content_rect();
    let left = super::shell::panel_offset(app);
    let open = app.saved.profile_open;
    let width = if open { screen.width() - left - 12.0 } else { 220.0 };
    let show_e_id = egui::Id::new("profile-show-e");
    let mut show_e: bool = ctx.data(|d| d.get_temp(show_e_id)).unwrap_or(false);
    let height = height(open);
    let r = egui::Area::new("profile".into()).fixed_pos(pos2(screen.left() + left, screen.bottom() - 12.0 - height)).order(egui::Order::Middle).show(ctx, |ui| {
        glass(&t).inner_margin(Margin { left: 12, right: 12, top: 6, bottom: 8 }).show(ui, |ui| {
            ui.set_width(width - 24.0);
            ui.horizontal(|ui| {
                let label = format!("{}  Vertical profile  {}", ph::MOUNTAINS, if open { ph::CARET_DOWN } else { ph::CARET_UP });
                if ui.add(egui::Button::new(RichText::new(label).font(theme::semibold(13.0))).frame(false)).clicked() {
                    app.saved.profile_open = !open;
                }
                if open {
                    let legend = |ui: &mut egui::Ui, c: Color32, text: &str| {
                        let (r, _) = ui.allocate_exact_size(vec2(12.0, 8.0), Sense::hover());
                        ui.painter().rect_filled(r, CornerRadius::same(2), c);
                        ui.label(RichText::new(text).size(11.0).color(t.text2));
                    };
                    legend(ui, Color32::from_rgb(0xc8, 0xb0, 0x8c), "Terrain");
                    legend(ui, Color32::from_rgb(0x8a, 0x6d, 0x48), "Highest within 1 NM");
                    legend(ui, t.accent, "Planned (drag)");
                    if ui.checkbox(&mut show_e, RichText::new("Class E").size(11.0)).changed() {
                        ctx.data_mut(|d| d.insert_temp(show_e_id, show_e));
                    }
                    let warnings = app.plan.conflicts.iter().filter(|c| c.bad).count() + app.plan.incursions.iter().filter(|i| i.level == Level::Warning).count();
                    let cautions = app.plan.conflicts.iter().filter(|c| !c.bad).count() + app.plan.incursions.iter().filter(|i| i.level == Level::Caution).count();
                    if warnings > 0 {
                        widgets::chip(ui, &t, &format!("{} {warnings} warning{}", ph::OCTAGON, if warnings > 1 { "s" } else { "" }), Some(t.bad), Some(t.bad.gamma_multiply(0.1)));
                    } else if cautions > 0 {
                        widgets::chip(ui, &t, &format!("{} {cautions} caution{}", ph::WARNING, if cautions > 1 { "s" } else { "" }), Some(t.warn), Some(t.warn.gamma_multiply(0.1)));
                    }
                    if app.plan.loading {
                        faint(ui, &t, "Updating…");
                    }
                }
            });
            if open {
                chart(app, ui, show_e);
            }
        });
    });
    mark(ctx, "profile", r.response.rect);
}

fn chart(app: &mut App, ui: &mut egui::Ui, show_e: bool) {
    let t = app.t;
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), H), Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    let pad = (52.0, 16.0, 14.0 + STRIP, 26.0); // l, r, t, b
    let samples = &app.plan.profile.samples;
    let total = app.plan.total().max(0.01);
    let terrain_max = samples.iter().map(|s| s.max).fold(0.0, f64::max);
    let cruise = app.saved.route.cruise;
    let y_max = ((cruise + 2500.0).max(terrain_max + 3000.0).max(4000.0) / 1000.0).ceil() * 1000.0;
    let plot_w = (rect.width() - pad.0 - pad.1).max(1.0);
    let x = |d: f64| rect.left() + pad.0 + (d / total) as f32 * plot_w;
    let y = |ft: f64| rect.top() + pad.2 + (1.0 - (ft.clamp(0.0, y_max) / y_max) as f32) * (H - pad.2 - pad.3);
    let d_at = |px: f32| (((px - rect.left() - pad.0) / plot_w) as f64 * total).clamp(0.0, total);
    let ft_at = |py: f32| (1.0 - (py - rect.top() - pad.2) / (H - pad.2 - pad.3)) as f64 * y_max;

    // grid
    let mut tick = 0.0;
    while tick <= y_max {
        painter.line_segment([pos2(rect.left() + pad.0, y(tick)), pos2(rect.right() - pad.1, y(tick))], Stroke::new(1.0, t.line));
        painter.text(pos2(rect.left() + pad.0 - 8.0, y(tick)), Align2::RIGHT_CENTER, if tick == 0.0 { "SFC".to_string() } else { format!("{}k", tick / 1000.0) }, theme::regular(10.5), t.text3);
        tick += 2000.0;
    }
    // thinned samples for drawing
    let step = (samples.len() as f32 / (plot_w / 2.0)).floor().max(1.0) as usize;
    let draw: Vec<&crate::nav::profile::Sample> = samples.iter().enumerate().filter(|(i, _)| i % step == 0 || *i + 1 == samples.len()).map(|(_, s)| s).collect();

    // airspace volumes
    let picked = app.plan.picked;
    let mut volumes: Vec<(usize, Vec<Pos2>, KindStyle)> = Vec::new();
    for (i, c) in app.plan.profile.airspace.iter().enumerate() {
        let Some(st) = kind_style(app, &c.kind) else { continue };
        if (!show_e && c.kind.starts_with('E')) || c.to - c.from <= 0.05 {
            continue;
        }
        let hi = c.hi.min(y_max);
        let pts: Vec<Pos2> = if c.lo_agl {
            let mut v = vec![pos2(x(c.from), y(hi)), pos2(x(c.to), y(hi))];
            v.extend(draw.iter().rev().filter(|s| s.d >= c.from && s.d <= c.to).map(|s| pos2(x(s.d), y(s.elev + c.lo))));
            v
        } else {
            vec![pos2(x(c.from), y(hi)), pos2(x(c.to), y(hi)), pos2(x(c.to), y(c.lo)), pos2(x(c.from), y(c.lo))]
        };
        volumes.push((i, pts, st));
    }
    for (i, pts, st) in &volumes {
        let sel = picked == Some(*i);
        fill_poly(&painter, pts, st.color.gamma_multiply(if sel { st.fill + 0.2 } else { st.fill }));
        let mut closed = pts.clone();
        closed.push(pts[0]);
        let stroke = Stroke::new(if sel { 2.6 } else { 1.3 }, st.color);
        if st.dash {
            painter.extend(Shape::dashed_line(&closed, stroke, 5.0, 3.0));
        } else {
            painter.add(Shape::line(closed, stroke));
        }
        let c = &app.plan.profile.airspace[*i];
        if x(c.to) - x(c.from) > 46.0 {
            let label = format!("{} {}/{}", st.label, fmt_alt(c.hi), if c.lo_agl { format!("{}AGL", c.lo) } else { fmt_alt(c.lo) });
            painter.text(pos2((x(c.from) + x(c.to)) / 2.0, (y(c.hi.min(y_max)) + 13.0).max(rect.top() + pad.2 + 12.0)), Align2::CENTER_CENTER, label, theme::bold(10.5), st.color);
        }
    }

    // terrain
    if !draw.is_empty() {
        let mut terr = vec![pos2(x(0.0), y(0.0))];
        terr.extend(draw.iter().map(|s| pos2(x(s.d), y(s.elev))));
        terr.push(pos2(x(total), y(0.0)));
        // columns from the ground up (terrain is not convex)
        let base = y(0.0);
        let mut mesh = egui::Mesh::default();
        let top_c = Color32::from_rgb(0xb5, 0x9a, 0x76);
        let bot_c = Color32::from_rgb(0xd9, 0xc8, 0xa9);
        for w in draw.windows(2) {
            let (a, b) = (pos2(x(w[0].d), y(w[0].elev)), pos2(x(w[1].d), y(w[1].elev)));
            let i0 = mesh.vertices.len() as u32;
            mesh.colored_vertex(a, top_c);
            mesh.colored_vertex(b, top_c);
            mesh.colored_vertex(pos2(b.x, base), bot_c);
            mesh.colored_vertex(pos2(a.x, base), bot_c);
            mesh.add_triangle(i0, i0 + 1, i0 + 2);
            mesh.add_triangle(i0, i0 + 2, i0 + 3);
        }
        painter.add(Shape::mesh(mesh));
        let corridor: Vec<Pos2> = draw.iter().map(|s| pos2(x(s.d), y(s.max))).collect();
        painter.extend(Shape::dashed_line(&corridor, Stroke::new(1.0, Color32::from_rgb(0x8a, 0x6d, 0x48)), 2.0, 2.0));
    }
    // obstacles
    for o in &app.plan.profile.obstacles {
        let (ox, oy) = (x(o.d), y(o.msl));
        painter.add(Shape::line(vec![pos2(ox - 4.0, oy + 9.0), pos2(ox, oy), pos2(ox + 4.0, oy + 9.0)], Stroke::new(1.4, t.text)));
    }
    // freezing level per waypoint from the temperatures aloft
    let mut ticks = Vec::new();
    let mut acc = 0.0;
    for (i, w) in app.saved.route.waypoints.iter().enumerate() {
        if i > 0 {
            let p = &app.saved.route.waypoints[i - 1];
            acc += distance_nm(p.lat, p.lon, w.lat, w.lon);
        }
        ticks.push((w.id.clone(), acc));
    }
    let dep = app.saved.route.departure_time().timestamp();
    let mut freeze = Vec::new();
    for (i, (_, d)) in ticks.iter().enumerate() {
        let Some(Some(wp)) = app.plan.winds.get(i) else { continue };
        let mut lv: Vec<(f64, f64)> = wp
            .levels
            .iter()
            .filter_map(|l| {
                let n = l.times.iter().min_by_key(|x| (x.t - dep).abs())?;
                Some((n.alt.unwrap_or(l.alt), n.temp?))
            })
            .collect();
        lv.sort_by(|a, b| a.0.total_cmp(&b.0));
        if lv.first().is_some_and(|l| l.1 <= 0.0) {
            freeze.push(pos2(x(*d), y(0.0)));
            continue;
        }
        if let Some(w) = lv.windows(2).find(|w| w[0].1 > 0.0 && w[1].1 <= 0.0) {
            let (lo, hi) = (w[0], w[1]);
            freeze.push(pos2(x(*d), y(lo.0 + (hi.0 - lo.0) * lo.1 / (lo.1 - hi.1))));
        }
    }
    if freeze.len() >= 2 {
        painter.extend(Shape::dashed_line(&freeze, Stroke::new(1.6, Color32::from_rgb(0x3a, 0x8f, 0xd6)), 6.0, 4.0));
        if let Some(p) = freeze.last() {
            painter.text(*p + vec2(-4.0, -8.0), Align2::RIGHT_BOTTOM, format!("{} 0°C", ph::SNOWFLAKE), theme::regular(10.0), Color32::from_rgb(0x3a, 0x8f, 0xd6));
        }
    }

    // warning strip and highlighted stretches
    let strip = Rect::from_min_size(pos2(rect.left() + pad.0, rect.top() + 4.0), vec2(plot_w, STRIP - 6.0));
    painter.rect_filled(strip, CornerRadius::same(3), t.surface2);
    struct Mark {
        from: f64,
        to: f64,
        level: Level,
        crossing: Option<usize>,
        terrain: bool,
    }
    let mut marks: Vec<Mark> = app.plan.conflicts.iter().map(|c| Mark { from: c.from, to: c.to, level: if c.bad { Level::Warning } else { Level::Caution }, crossing: None, terrain: true }).collect();
    marks.extend(app.plan.incursions.iter().map(|i| Mark { from: i.from, to: i.to, level: i.level, crossing: Some(i.crossing), terrain: false }));
    for m in &marks {
        let c = match m.level {
            Level::Warning => t.bad,
            Level::Caution => t.warn,
            Level::Info => t.blue,
        };
        let r = Rect::from_min_max(pos2(x(m.from) - 2.0, strip.top()), pos2((x(m.to) + 2.0).max(x(m.from) + 4.0), strip.bottom()));
        painter.rect_filled(r, CornerRadius::same(3), c.gamma_multiply(if m.crossing.is_some() && m.crossing == picked { 1.0 } else { 0.8 }));
        if m.terrain {
            painter.rect_filled(Rect::from_min_max(pos2(x(m.from) - 1.0, rect.top() + pad.2), pos2((x(m.to) + 1.0).max(x(m.from) + 3.0), rect.bottom() - pad.3)), 0.0, c.gamma_multiply(0.1));
        } else {
            let a = y(app.plan.alt_at((m.from + m.to) / 2.0));
            painter.line_segment([pos2(x(m.from), a), pos2(x(m.to), a)], Stroke::new(9.0, c.gamma_multiply(0.35)));
        }
    }

    // planned altitude
    let alt: Vec<Pos2> = app.plan.alt_path.iter().map(|p| pos2(x(p[0]), y(p[1]))).collect();
    if alt.len() >= 2 {
        painter.add(Shape::line(alt.clone(), Stroke::new(6.0, Color32::from_white_alpha(230))));
        painter.add(Shape::line(alt, Stroke::new(3.0, t.accent)));
    }
    let (toc, tod) = app.plan.log.as_ref().map(|l| (l.toc_dist.unwrap_or(0.0), l.tod_dist.unwrap_or(total))).unwrap_or((0.0, total));
    painter.text(pos2(x((toc + tod) / 2.0), y(cruise) - 8.0), Align2::CENTER_BOTTOM, format!("{} ft", crate::nav::navlog::thousands(cruise)), theme::bold(11.0), t.accent);
    // waypoint ticks
    for (id, d) in &ticks {
        painter.line_segment([pos2(x(*d), rect.top() + pad.2), pos2(x(*d), rect.bottom() - pad.3)], Stroke::new(1.0, t.line_strong));
        painter.text(pos2(x(*d), rect.bottom() - 8.0), Align2::CENTER_CENTER, id, theme::bold(10.5), t.text2);
    }

    // ---- interaction ----
    let drag_id = egui::Id::new("profile-drag");
    let mut dragging: bool = ui.data(|d| d.get_temp(drag_id)).unwrap_or(false);
    let locked = app.saved.locked;
    if resp.drag_started() && !locked {
        if let Some(p) = ui.input(|i| i.pointer.press_origin()) {
            dragging = (p.y - y(cruise)).abs() < 12.0;
            ui.data_mut(|d| d.insert_temp(drag_id, dragging));
        }
    }
    if let Some(p) = resp.hover_pos().or(resp.interact_pointer_pos()) {
        if p.x >= rect.left() + pad.0 && p.x <= rect.right() - pad.1 {
            app.plan.hover = Some((d_at(p.x), true));
        }
        if !locked && ((p.y - y(cruise)).abs() < 12.0 || dragging) {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
        }
        if dragging && resp.dragged() {
            app.saved.route.cruise = ((ft_at(p.y) / 500.0).round() * 500.0).clamp(500.0, 45000.0);
        }
    } else if app.plan.hover.is_some_and(|h| h.1) {
        app.plan.hover = None;
    }
    if resp.drag_stopped() {
        ui.data_mut(|d| d.insert_temp(drag_id, false));
    }
    let before = app.plan.picked;
    if resp.clicked() {
        if let Some(p) = resp.interact_pointer_pos() {
            let d = d_at(p.x);
            if p.y < rect.top() + pad.2 {
                app.plan.picked = marks.iter().find(|m| d >= m.from - total * 0.01 && d <= m.to + total * 0.01).and_then(|m| m.crossing);
            } else {
                let ft = ft_at(p.y);
                let s = samples.iter().find(|s| s.d >= d).or(samples.last());
                let hit = volumes.iter().rev().find(|(i, ..)| {
                    let c = &app.plan.profile.airspace[*i];
                    let floor = if c.lo_agl { s.map(|s| s.elev).unwrap_or(0.0) + c.lo } else { c.lo };
                    d >= c.from && d <= c.to && ft >= floor && ft <= c.hi
                });
                app.plan.picked = hit.map(|h| h.0);
            }
        }
    }
    let pick_changed = app.plan.picked != before;

    // hover crosshair + tooltip
    if let Some((hd, _)) = app.plan.hover {
        if let Some(s) = samples.iter().min_by(|a, b| (a.d - hd).abs().total_cmp(&(b.d - hd).abs())) {
            let a = app.plan.alt_at(s.d);
            painter.line_segment([pos2(x(s.d), rect.top() + pad.2), pos2(x(s.d), rect.bottom() - pad.3)], Stroke::new(1.0, t.text2));
            painter.circle(pos2(x(s.d), y(a)), 5.0, t.accent, Stroke::new(2.0, Color32::WHITE));
            if app.plan.picked.is_none() {
                let clr = a - s.max;
                let inside: Vec<&Crossing> = app.plan.profile.airspace.iter().filter(|c| s.d >= c.from && s.d <= c.to && (show_e || !c.kind.starts_with('E'))).collect();
                let tip_x = (x(s.d) + 12.0).min(rect.right() - 250.0);
                egui::Area::new("profile-tip".into()).fixed_pos(pos2(tip_x, rect.top() + pad.2)).order(egui::Order::Tooltip).interactable(false).show(ui.ctx(), |ui| {
                    glass(&t).inner_margin(Margin::symmetric(10, 7)).show(ui, |ui| {
                        ui.set_max_width(240.0);
                        ui.label(RichText::new(format!("{:.1} NM · planned {} ft", s.d, crate::nav::navlog::thousands(a))).font(theme::semibold(12.0)));
                        ui.label(RichText::new(format!("Terrain {} ft · highest nearby {} ft", crate::nav::navlog::thousands(s.elev), crate::nav::navlog::thousands(s.max))).size(11.5));
                        ui.label(RichText::new(format!("Clearance {} ft", crate::nav::navlog::thousands(clr))).size(11.5).color(if clr < 500.0 { t.bad } else if clr < 1000.0 { t.warn } else { t.text }));
                        for c in inside {
                            let floor = if c.lo_agl { s.elev + c.lo } else { c.lo };
                            let at = a >= floor && a <= c.hi;
                            ui.label(RichText::new(format!("{}{} · {}", if at { "● " } else { "" }, c.name, range_text(c))).size(11.0).color(if at { t.text } else { t.text3 }));
                        }
                    });
                });
            }
        }
    }
    // picked airspace card
    if let Some(i) = app.plan.picked {
        if let Some(c) = app.plan.profile.airspace.get(i).cloned() {
            let inc: Vec<_> = app.plan.incursions.iter().filter(|x| x.crossing == i).collect();
            let px = x(c.from).max(rect.left() + pad.0).min(rect.right() - 300.0);
            let mut close = false;
            egui::Area::new("profile-pick".into()).fixed_pos(pos2(px, rect.top() + pad.2)).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
                glass(&t).inner_margin(Margin::same(10)).show(ui, |ui| {
                    ui.set_max_width(280.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&c.name).font(theme::semibold(12.5)));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                            if widgets::icon_button(ui, &t, ph::X, "Close").clicked() {
                                close = true;
                            }
                        });
                    });
                    ui.label(format!("{}{}", range_text(&c), if c.lo_agl { "" } else { " MSL" }));
                    faint(ui, &t, format!("Route inside from {:.1} to {:.1} NM", c.from, c.to));
                    if !c.times.is_empty() {
                        faint(ui, &t, format!("Times: {}", c.times));
                    }
                    if let (Some(a), Some(b)) = (inc.first(), inc.last()) {
                        widgets::para(ui, format!("Your planned altitude is inside it from {:.1} to {:.1} NM", a.from, b.to), if a.level == Level::Warning { t.bad } else { t.warn }, 12.0);
                    } else {
                        widgets::para(ui, "Your planned altitude stays outside it", t.ok, 12.0);
                    }
                    faint(ui, &t, "Highlighted on the map");
                });
            });
            if close {
                app.plan.picked = None;
            }
        }
    }
    if pick_changed {
        focus_picked(app);
    }
    let _ = Frame::NONE;
}

fn fill_poly(painter: &egui::Painter, pts: &[Pos2], c: Color32) {
    if pts.len() < 3 {
        return;
    }
    let flat: Vec<f64> = pts.iter().flat_map(|p| [p.x as f64, p.y as f64]).collect();
    if let Ok(idx) = earcutr::earcut(&flat, &[], 2) {
        let mut mesh = egui::Mesh::default();
        for p in pts {
            mesh.colored_vertex(*p, c);
        }
        mesh.indices = idx.into_iter().map(|i| i as u32).collect();
        painter.add(Shape::mesh(mesh));
    }
}

/// Bring the airspace picked in the profile into view (above the profile drawer).
fn focus_picked(app: &mut App) {
    let (Some(i), Some(proj)) = (app.plan.picked, app.last_proj) else { return };
    let Some(c) = app.plan.profile.airspace.get(i) else { return };
    let pts: Vec<[f64; 2]> = c.geom.iter().flat_map(|poly| poly.first().cloned().unwrap_or_default()).collect();
    if pts.is_empty() {
        return;
    }
    let b = crate::nav::geo::BBox::of(pts);
    let left = super::shell::panel_offset(app) + 30.0;
    app.map.fit(proj.rect, b.min_x, b.min_y, b.max_x, b.max_y, [left, 80.0, 80.0, 320.0], 11.0, true);
}
