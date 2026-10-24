//! CG envelope chart with the loading's takeoff -> landing path (and zero fuel).

use crate::model::aircraft::{Aircraft, WbResult};
use crate::theme::{self, Tokens};
use egui::{pos2, vec2, Align2, Color32, Pos2, Sense, Shape, Stroke};

pub fn show(ui: &mut egui::Ui, t: &Tokens, ac: &Aircraft, wb: &WbResult, height: f32) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let p = ui.painter_at(rect);
    let (l, r, top, b) = (44.0, 12.0, 10.0, 28.0);
    let mut pts: Vec<[f64; 2]> = ac.wb.envelope.clone();
    if let Some(u) = &ac.wb.utility {
        pts.extend(u.iter().copied());
    }
    let mut arms: Vec<f64> = pts.iter().map(|p| p[0]).collect();
    let mut wts: Vec<f64> = pts.iter().map(|p| p[1]).collect();
    for pt in [&wb.takeoff, &wb.landing, &wb.zero_fuel] {
        if pt.cg > 0.0 {
            arms.push(pt.cg);
        }
        if pt.weight > 0.0 {
            wts.push(pt.weight);
        }
    }
    if arms.is_empty() || wts.is_empty() {
        return;
    }
    let x0 = (arms.iter().cloned().fold(f64::MAX, f64::min) - 1.0).floor();
    let x1 = (arms.iter().cloned().fold(f64::MIN, f64::max) + 1.0).ceil();
    let y0 = ((wts.iter().cloned().fold(f64::MAX, f64::min) - 100.0) / 100.0).floor() * 100.0;
    let y1 = ((wts.iter().cloned().fold(f64::MIN, f64::max) + 100.0) / 100.0).ceil() * 100.0;
    let x = |a: f64| rect.left() + l + ((a - x0) / (x1 - x0).max(1e-6)) as f32 * (width - l - r);
    let y = |w: f64| rect.top() + top + (1.0 - ((w - y0) / (y1 - y0).max(1e-6)) as f32) * (height - top - b);
    let grid = Stroke::new(1.0, t.line);
    let mut tick = y0;
    while tick <= y1 + 0.1 {
        p.line_segment([pos2(rect.left() + l, y(tick)), pos2(rect.right() - r, y(tick))], grid);
        p.text(pos2(rect.left() + l - 6.0, y(tick)), Align2::RIGHT_CENTER, format!("{}", tick as i64), theme::regular(10.0), t.text3);
        tick += 200.0;
    }
    let step = if x1 - x0 > 12.0 { 2.0 } else { 1.0 };
    let mut a = x0;
    while a <= x1 + 0.01 {
        p.text(pos2(x(a), rect.bottom() - b + 6.0), Align2::CENTER_TOP, format!("{}", a as i64), theme::regular(10.0), t.text3);
        a += step;
    }
    p.text(pos2(rect.left() + l + (width - l - r) / 2.0, rect.bottom() - 4.0), Align2::CENTER_BOTTOM, "CG (inches aft of datum) · weight (lb)", theme::regular(10.0), t.text3);
    let ok = !wb.problems.iter().any(|p| p.contains("envelope"));
    let col = if ok { t.ok } else { t.bad };
    let poly = |pts: &[[f64; 2]]| pts.iter().map(|q| pos2(x(q[0]), y(q[1]))).collect::<Vec<Pos2>>();
    if let Some(u) = &ac.wb.utility {
        let mut v = poly(u);
        v.push(v[0]);
        p.extend(Shape::dashed_line(&v, Stroke::new(1.2, t.text3), 4.0, 3.0));
    }
    let env = poly(&ac.wb.envelope);
    if env.len() >= 3 {
        // fill via triangulation (envelopes are usually convex but not always)
        let flat: Vec<f64> = env.iter().flat_map(|q| [q.x as f64, q.y as f64]).collect();
        if let Ok(idx) = earcutr::earcut(&flat, &[], 2) {
            let mut mesh = egui::Mesh::default();
            for q in &env {
                mesh.colored_vertex(*q, col.gamma_multiply(0.1));
            }
            mesh.indices = idx.into_iter().map(|i| i as u32).collect();
            p.add(Shape::mesh(mesh));
        }
        let mut closed = env.clone();
        closed.push(env[0]);
        p.add(Shape::line(closed, Stroke::new(1.6, col)));
    }
    let to = pos2(x(wb.takeoff.cg), y(wb.takeoff.weight));
    let ld = pos2(x(wb.landing.cg), y(wb.landing.weight));
    let zf = pos2(x(wb.zero_fuel.cg), y(wb.zero_fuel.weight));
    let accent = t.accent;
    p.extend(Shape::dashed_line(&[to, ld], Stroke::new(1.8, accent), 4.0, 3.0));
    for (pt, label) in [(to, "TO"), (ld, "LDG"), (zf, "ZF")] {
        p.circle(pt, 4.5, accent, Stroke::new(1.5, Color32::WHITE));
        p.text(pt + vec2(7.0, 0.0), Align2::LEFT_CENTER, label, theme::bold(10.0), t.text2);
    }
}
