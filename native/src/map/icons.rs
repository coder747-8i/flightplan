//! Chart symbols drawn with tiny-skia into one atlas texture (per theme): airports, heliports,
//! VOR/VORTAC/DME/NDB, compass roses, waypoints, obstacles. Ported from the web app's canvas
//! icons so both look alike.

use super::style::{Palette, Rgba};
use std::collections::HashMap;
use std::f32::consts::PI;
use tiny_skia::{FillRule, LineCap, LineJoin, Paint, PathBuilder, Pixmap, Rect, Stroke, Transform};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Icon {
    /// colour key: 0 towered, 1 non-towered, 2 military
    Airport { color: u8, hard: bool, fuel: bool },
    Private { hard: bool },
    Heli { towered: bool },
    Vor(u8),
    Ndb,
    Rose,
    FixVfr,
    FixIfr,
    Obstacle { lit: bool },
}

impl Icon {
    /// size in points
    pub fn size(self) -> f32 {
        match self {
            Icon::Airport { .. } | Icon::Vor(_) | Icon::Ndb => 26.0,
            Icon::Private { .. } => 24.0,
            Icon::Heli { .. } => 22.0,
            Icon::Rose => 220.0,
            Icon::FixVfr | Icon::Obstacle { .. } => 18.0,
            Icon::FixIfr => 14.0,
        }
    }

    pub fn vor_kind(t: &str) -> Option<u8> {
        Some(match t {
            "VOR" => 0,
            "VORTAC" => 1,
            "VOR/DME" => 2,
            "DME" => 3,
            "TACAN" => 4,
            _ => return None,
        })
    }
}

pub struct Atlas {
    pub image: egui::ColorImage,
    /// uv rectangles (0..1)
    pub uv: HashMap<Icon, egui::Rect>,
}

const PR: f32 = 2.0; // drawn at 2x for crispness

struct Pen<'a> {
    pm: &'a mut Pixmap,
    ox: f32,
    oy: f32,
}

impl Pen<'_> {
    fn t(&self) -> Transform {
        Transform::from_row(PR, 0.0, 0.0, PR, self.ox, self.oy)
    }
    fn fill_circle(&mut self, x: f32, y: f32, r: f32, c: Rgba) {
        if let Some(p) = PathBuilder::from_circle(x, y, r) {
            self.pm.fill_path(&p, &paint(c), FillRule::Winding, self.t(), None);
        }
    }
    fn stroke_circle(&mut self, x: f32, y: f32, r: f32, c: Rgba, w: f32) {
        if let Some(p) = PathBuilder::from_circle(x, y, r) {
            self.pm.stroke_path(&p, &paint(c), &stroke(w), self.t(), None);
        }
    }
    fn line(&mut self, a: (f32, f32), b: (f32, f32), c: Rgba, w: f32) {
        let mut pb = PathBuilder::new();
        pb.move_to(a.0, a.1);
        pb.line_to(b.0, b.1);
        if let Some(p) = pb.finish() {
            self.pm.stroke_path(&p, &paint(c), &stroke(w), self.t(), None);
        }
    }
    fn poly(&mut self, pts: &[(f32, f32)], fill: Option<Rgba>, line: Option<(Rgba, f32)>) {
        let mut pb = PathBuilder::new();
        pb.move_to(pts[0].0, pts[0].1);
        for p in &pts[1..] {
            pb.line_to(p.0, p.1);
        }
        pb.close();
        if let Some(p) = pb.finish() {
            if let Some(c) = fill {
                self.pm.fill_path(&p, &paint(c), FillRule::Winding, self.t(), None);
            }
            if let Some((c, w)) = line {
                self.pm.stroke_path(&p, &paint(c), &stroke(w), self.t(), None);
            }
        }
    }
    fn polyline(&mut self, pts: &[(f32, f32)], c: Rgba, w: f32) {
        let mut pb = PathBuilder::new();
        pb.move_to(pts[0].0, pts[0].1);
        for p in &pts[1..] {
            pb.line_to(p.0, p.1);
        }
        if let Some(p) = pb.finish() {
            self.pm.stroke_path(&p, &paint(c), &stroke(w), self.t(), None);
        }
    }
}

fn paint(c: Rgba) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(c.skia());
    p.anti_alias = true;
    p
}

fn stroke(w: f32) -> Stroke {
    Stroke { width: w, line_cap: LineCap::Round, line_join: LineJoin::Round, ..Default::default() }
}

fn hexagon(m: f32, r: f32) -> Vec<(f32, f32)> {
    (0..6).map(|i| {
        let a = PI / 3.0 * i as f32;
        (m + a.cos() * r, m + a.sin() * r)
    })
    .collect()
}

fn draw(pen: &mut Pen, icon: Icon, p: &Palette) {
    let s = icon.size();
    let m = s / 2.0;
    let halo = p.symbol_halo;
    match icon {
        Icon::Airport { color, hard, fuel } => airport(pen, s, [p.towered, p.nontowered, p.military][color as usize], halo, hard, fuel, false, color == 2),
        Icon::Private { hard } => airport(pen, s, p.nontowered, halo, hard, false, true, false),
        Icon::Heli { towered } => {
            pen.fill_circle(m, m, s * 0.3, halo);
            pen.stroke_circle(m, m, s * 0.3, if towered { p.towered } else { p.nontowered }, 1.6);
        }
        Icon::Vor(kind) => {
            let r = s * 0.24;
            let c = p.navaid;
            if kind == 1 || kind == 4 {
                // three filled "ears" on alternate hexagon sides
                for i in 0..3 {
                    let a = PI * 2.0 * i as f32 / 3.0 + PI / 2.0;
                    let (cx, cy) = (m + a.cos() * r * 1.05, m + a.sin() * r * 1.05);
                    let rot = a + PI / 2.0;
                    let (ux, uy) = (rot.cos(), rot.sin());
                    let (vx, vy) = (-uy, ux);
                    let (hw, hh) = (r * 0.5, 2.5);
                    let corner = |sx: f32, sy: f32| (cx + ux * hw * sx + vx * hh * sy, cy + uy * hw * sx + vy * hh * sy);
                    pen.poly(&[corner(-1.0, -1.0), corner(1.0, -1.0), corner(1.0, 1.0), corner(-1.0, 1.0)], Some(c), None);
                }
            }
            if kind == 2 || kind == 3 {
                let q = r * 1.15;
                pen.poly(&[(m - q, m - q), (m + q, m - q), (m + q, m + q), (m - q, m + q)], None, Some((c, 1.6)));
            }
            if kind != 3 {
                pen.poly(&hexagon(m, r), Some(halo), Some((c, 1.6)));
            }
            pen.fill_circle(m, m, 1.6, c);
        }
        Icon::Ndb => {
            for (ring, n, dot) in [(s * 0.18, 10, 0.9), (s * 0.3, 16, 0.8), (s * 0.42, 22, 0.7)] {
                for i in 0..n {
                    let a = PI * 2.0 * i as f32 / n as f32;
                    pen.fill_circle(m + a.cos() * ring, m + a.sin() * ring, dot, p.ndb);
                }
            }
            pen.fill_circle(m, m, 2.4, p.ndb);
        }
        Icon::Rose => {
            let r = s / 2.0 - 2.0;
            let c = p.navaid.alpha(0.75);
            pen.stroke_circle(m, m, r, c, 1.0);
            for deg in (0..360).step_by(5) {
                let a = ((deg - 90) as f32).to_radians();
                let len = if deg % 30 == 0 { 9.0 } else if deg % 10 == 0 { 6.0 } else { 3.0 };
                pen.line((m + a.cos() * r, m + a.sin() * r), (m + a.cos() * (r - len), m + a.sin() * (r - len)), c, if deg % 30 == 0 { 1.4 } else { 1.0 });
            }
            // magnetic north arrow
            pen.poly(&[(m, m - r - 1.0), (m - 4.0, m - r + 9.0), (m + 4.0, m - r + 9.0)], Some(c), None);
        }
        Icon::FixVfr => {
            let c = p.nontowered;
            pen.fill_circle(m, m, s * 0.36, halo);
            pen.stroke_circle(m, m, s * 0.36, c, 1.4);
            let (big, small) = (s * 0.32, s * 0.09);
            let pts: Vec<(f32, f32)> = (0..8)
                .map(|i| {
                    let a = PI / 4.0 * i as f32 - PI / 2.0;
                    let rad = if i % 2 == 1 { small } else { big };
                    (m + a.cos() * rad, m + a.sin() * rad)
                })
                .collect();
            pen.poly(&pts, Some(c), None);
        }
        Icon::FixIfr => pen.poly(&[(m, m - s * 0.3), (m + s * 0.28, m + s * 0.22), (m - s * 0.28, m + s * 0.22)], Some(halo), Some((p.navaid, 1.4))),
        Icon::Obstacle { lit } => {
            let c = p.obstacle;
            let pts = [(m - s * 0.22, m + s * 0.3), (m, m - s * 0.25), (m + s * 0.22, m + s * 0.3)];
            pen.polyline(&pts, halo, 4.0);
            pen.polyline(&pts, c, 1.6);
            pen.fill_circle(m, m + s * 0.3, 1.4, c);
            if lit {
                for a in [-60.0f32, -30.0, 0.0, 30.0, 60.0] {
                    let rad = (a - 90.0).to_radians();
                    pen.line(
                        (m + rad.cos() * s * 0.33, m - s * 0.25 + 2.0 + rad.sin() * s * 0.18),
                        (m + rad.cos() * s * 0.45, m - s * 0.25 + 2.0 + rad.sin() * s * 0.28),
                        c,
                        1.1,
                    );
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn airport(pen: &mut Pen, s: f32, color: Rgba, halo: Rgba, hard: bool, fuel: bool, pvt: bool, mil: bool) {
    let m = s / 2.0;
    let r = s * 0.27;
    // service ticks (fuel available), like the sectional's "cogwheel"
    if fuel {
        for i in 0..4 {
            let a = PI / 2.0 * i as f32 + PI / 4.0;
            pen.line((m + a.cos() * (r + 1.5), m + a.sin() * (r + 1.5)), (m + a.cos() * (r + 4.5), m + a.sin() * (r + 4.5)), color, 2.2);
        }
    }
    pen.fill_circle(m, m, r + 1.5, halo);
    if hard {
        pen.stroke_circle(m, m, r, color, 2.0);
        // runway bar pointing "up"; the map rotates it to the runway's true heading
        pen.line((m, m - r - 3.5), (m, m + r + 3.5), color, 3.0);
    } else {
        pen.fill_circle(m, m, r, color);
    }
    if mil {
        pen.stroke_circle(m, m, r - 3.0, color, 1.4);
    }
    let _ = pvt; // the "R" is drawn as text by the map view
}

pub fn all_icons() -> Vec<Icon> {
    let mut v = Vec::new();
    for color in 0..3 {
        for hard in [true, false] {
            for fuel in [true, false] {
                v.push(Icon::Airport { color, hard, fuel });
            }
        }
    }
    v.extend([Icon::Private { hard: true }, Icon::Private { hard: false }, Icon::Heli { towered: true }, Icon::Heli { towered: false }]);
    v.extend((0..5).map(Icon::Vor));
    v.extend([Icon::Ndb, Icon::Rose, Icon::FixVfr, Icon::FixIfr, Icon::Obstacle { lit: false }, Icon::Obstacle { lit: true }]);
    v
}

/// Lay every icon out in rows and draw them.
pub fn build(p: &Palette) -> Atlas {
    let icons = all_icons();
    let width = 1024u32;
    let mut placed = Vec::new();
    let (mut x, mut y, mut row_h) = (0f32, 0f32, 0f32);
    for ic in &icons {
        let px = (ic.size() * PR).ceil() + 2.0;
        if x + px > width as f32 {
            x = 0.0;
            y += row_h;
            row_h = 0.0;
        }
        placed.push((*ic, x + 1.0, y + 1.0, px - 2.0));
        x += px;
        row_h = row_h.max(px);
    }
    let height = (y + row_h).ceil() as u32;
    let mut pm = Pixmap::new(width, height).expect("atlas");
    let mut uv = HashMap::new();
    for (ic, ox, oy, px) in placed {
        let mut pen = Pen { pm: &mut pm, ox, oy };
        draw(&mut pen, ic, p);
        let _ = Rect::from_xywh(ox, oy, px, px);
        uv.insert(ic, egui::Rect::from_min_size(egui::pos2(ox / width as f32, oy / height as f32), egui::vec2(px / width as f32, px / height as f32)));
    }
    let image = egui::ColorImage::from_rgba_premultiplied([width as usize, height as usize], pm.data());
    Atlas { image, uv }
}
