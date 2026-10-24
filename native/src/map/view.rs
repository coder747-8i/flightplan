//! The chart widget: web-mercator view with pan / zoom, rendered tiles, and live symbols and
//! labels placed with collision detection (airports first, like the sectional).

use super::icons::{Atlas, Icon};
use super::render::Look;
use super::style::{lin, AltitudeFilter, Group, Layers, Palette};
use super::symbols::{Kind, Sym};
use super::tiles::{Source, Tiles};
use egui::{epaint::TextShape, pos2, vec2, Color32, FontFamily, FontId, Galley, Painter, Pos2, Rect, Shape, Stroke, TextureId};
use std::f64::consts::PI;
use std::sync::Arc;

pub fn lonlat_to_world(lon: f64, lat: f64) -> [f64; 2] {
    let lat = lat.clamp(-85.05, 85.05).to_radians();
    [(lon + 180.0) / 360.0, (1.0 - (lat.tan() + 1.0 / lat.cos()).ln() / PI) / 2.0]
}

pub fn world_to_lonlat(w: [f64; 2]) -> (f64, f64) {
    let lon = w[0] * 360.0 - 180.0;
    let lat = (PI * (1.0 - 2.0 * w[1])).sinh().atan().to_degrees();
    (lon, lat)
}

/// Screen transform for one frame.
#[derive(Clone, Copy, Debug)]
pub struct Proj {
    pub rect: Rect,
    pub center: [f64; 2],
    pub zoom: f64,
    /// world size in points
    pub size: f64,
}

impl Proj {
    pub fn world_to_screen(&self, w: [f64; 2]) -> Pos2 {
        let c = self.rect.center();
        pos2(c.x + ((w[0] - self.center[0]) * self.size) as f32, c.y + ((w[1] - self.center[1]) * self.size) as f32)
    }
    pub fn to_screen(&self, lon: f64, lat: f64) -> Pos2 {
        self.world_to_screen(lonlat_to_world(lon, lat))
    }
    pub fn screen_to_world(&self, p: Pos2) -> [f64; 2] {
        let c = self.rect.center();
        [self.center[0] + (p.x - c.x) as f64 / self.size, self.center[1] + (p.y - c.y) as f64 / self.size]
    }
    pub fn from_screen(&self, p: Pos2) -> (f64, f64) {
        world_to_lonlat(self.screen_to_world(p))
    }
    /// visible lon/lat box: (west, south, east, north)
    pub fn bounds(&self) -> (f64, f64, f64, f64) {
        let (w, n) = self.from_screen(self.rect.left_top());
        let (e, s) = self.from_screen(self.rect.right_bottom());
        (w, s, e, n)
    }
}

/// Something the user can click on the chart.
#[derive(Clone, Debug)]
pub struct Hit {
    pub rect: Rect,
    pub sym: Sym,
}

pub struct MapView {
    pub center: [f64; 2],
    pub zoom: f64,
    anim: Option<(Instant2, [f64; 2], f64, [f64; 2], f64)>,
    pub hits: Vec<Hit>,
    atlas: Option<(egui::TextureHandle, Arc<Atlas>, bool)>,
}

type Instant2 = std::time::Instant;

pub struct Style<'a> {
    pub palette: &'a Palette,
    pub layers: &'a Layers,
    pub altitude: AltitudeFilter,
    pub look: &'a Look,
    pub rev: u64,
}

impl MapView {
    pub fn new(lon: f64, lat: f64, zoom: f64) -> Self {
        MapView { center: lonlat_to_world(lon, lat), zoom, anim: None, hits: Vec::new(), atlas: None }
    }

    pub fn proj(&self, rect: Rect) -> Proj {
        Proj { rect, center: self.center, zoom: self.zoom, size: 256.0 * 2f64.powf(self.zoom) }
    }

    pub fn center_lonlat(&self) -> (f64, f64) {
        world_to_lonlat(self.center)
    }

    pub fn fly_to(&mut self, lon: f64, lat: f64, zoom: f64) {
        self.anim = Some((Instant2::now(), self.center, self.zoom, lonlat_to_world(lon, lat), zoom.clamp(2.0, 16.0)));
    }

    /// Fit a lon/lat box inside `rect` minus padding (left, top, right, bottom).
    pub fn fit(&mut self, rect: Rect, west: f64, south: f64, east: f64, north: f64, pad: [f32; 4], max_zoom: f64, animate: bool) {
        let a = lonlat_to_world(west, north);
        let b = lonlat_to_world(east, south);
        let inner_w = (rect.width() - pad[0] - pad[2]).max(50.0) as f64;
        let inner_h = (rect.height() - pad[1] - pad[3]).max(50.0) as f64;
        let dx = (b[0] - a[0]).abs().max(1e-9);
        let dy = (b[1] - a[1]).abs().max(1e-9);
        let zoom = ((inner_w / (dx * 256.0)).min(inner_h / (dy * 256.0))).log2().min(max_zoom).clamp(2.0, 16.0);
        let size = 256.0 * 2f64.powf(zoom);
        // shift the centre so the box sits in the padded area
        let off_x = (pad[0] - pad[2]) as f64 / 2.0 / size;
        let off_y = (pad[1] - pad[3]) as f64 / 2.0 / size;
        let center = [(a[0] + b[0]) / 2.0 - off_x, (a[1] + b[1]) / 2.0 - off_y];
        if animate {
            self.anim = Some((Instant2::now(), self.center, self.zoom, center, zoom));
        } else {
            self.center = center;
            self.zoom = zoom;
        }
    }

    pub fn animating(&self) -> bool {
        self.anim.is_some()
    }

    /// Pan / zoom from input. `pan` false while something else owns the drag.
    pub fn interact(&mut self, ui: &egui::Ui, resp: &egui::Response, pan: bool) {
        if let Some((t0, c0, z0, c1, z1)) = self.anim {
            let t = (t0.elapsed().as_secs_f64() / 0.55).min(1.0);
            let e = 1.0 - (1.0 - t).powi(3);
            self.center = [c0[0] + (c1[0] - c0[0]) * e, c0[1] + (c1[1] - c0[1]) * e];
            self.zoom = z0 + (z1 - z0) * e;
            if t >= 1.0 {
                self.anim = None;
            }
            ui.ctx().request_repaint();
        }
        let size = 256.0 * 2f64.powf(self.zoom);
        if pan && resp.dragged_by(egui::PointerButton::Primary) {
            let d = resp.drag_delta();
            self.center[0] -= d.x as f64 / size;
            self.center[1] -= d.y as f64 / size;
            self.anim = None;
        }
        if resp.hovered() {
            let (scroll, pinch) = ui.input(|i| (i.smooth_scroll_delta.y + i.smooth_scroll_delta.x * 0.0, i.zoom_delta()));
            let dz = scroll as f64 / 280.0 + pinch.ln() as f64 / std::f64::consts::LN_2;
            if dz.abs() > 1e-6 {
                if let Some(p) = resp.hover_pos() {
                    self.zoom_at(resp.rect, p, dz);
                }
            }
        }
        if resp.double_clicked() {
            if let Some(p) = resp.interact_pointer_pos() {
                let target = (self.zoom + 1.0).min(16.0);
                let proj = self.proj(resp.rect);
                let (lon, lat) = proj.from_screen(p);
                // keep the clicked point under the pointer: animate toward the zoomed centre
                let w = lonlat_to_world(lon, lat);
                let s1 = 256.0 * 2f64.powf(target);
                let c = resp.rect.center();
                let center = [w[0] - (p.x - c.x) as f64 / s1, w[1] - (p.y - c.y) as f64 / s1];
                self.anim = Some((Instant2::now(), self.center, self.zoom, center, target));
            }
        }
        self.center[0] = self.center[0].clamp(0.0, 1.0);
        self.center[1] = self.center[1].clamp(0.02, 0.98);
    }

    pub fn zoom_at(&mut self, rect: Rect, p: Pos2, dz: f64) {
        let before = self.proj(rect).screen_to_world(p);
        self.zoom = (self.zoom + dz).clamp(2.0, 16.0);
        let after = self.proj(rect).screen_to_world(p);
        self.center[0] += before[0] - after[0];
        self.center[1] += before[1] - after[1];
        self.anim = None;
    }

    fn atlas(&mut self, ctx: &egui::Context, p: &Palette) -> (TextureId, Arc<Atlas>) {
        let stale = self.atlas.as_ref().is_none_or(|a| a.2 != p.night);
        if stale {
            let atlas = Arc::new(super::icons::build(p));
            let tex = ctx.load_texture("chart-icons", atlas.image.clone(), egui::TextureOptions::LINEAR);
            self.atlas = Some((tex, atlas, p.night));
        }
        let a = self.atlas.as_ref().unwrap();
        (a.0.id(), a.1.clone())
    }

    /// Draw the rendered tiles.
    pub fn draw_tiles(&mut self, painter: &Painter, proj: &Proj, tiles: &mut Tiles, style: &Style) {
        painter.rect_filled(proj.rect, 0.0, style.palette.background.egui());
        let zt = (proj.zoom.floor() as i32).clamp(1, 16) as u8;
        let n = 1u32 << zt;
        let tile_pts = proj.size / n as f64;
        let tl = proj.screen_to_world(proj.rect.left_top());
        let br = proj.screen_to_world(proj.rect.right_bottom());
        let x0 = ((tl[0] * n as f64).floor().max(0.0)) as u32;
        let y0 = ((tl[1] * n as f64).floor().max(0.0)) as u32;
        let x1 = ((br[0] * n as f64).floor() as i64).clamp(0, n as i64 - 1) as u32;
        let y1 = ((br[1] * n as f64).floor() as i64).clamp(0, n as i64 - 1) as u32;
        // nearest-to-centre first, so the middle fills in first
        let mut keys: Vec<(u32, u32)> = (y0..=y1).flat_map(|y| (x0..=x1).map(move |x| (x, y))).collect();
        let cx = proj.center[0] * n as f64;
        let cy = proj.center[1] * n as f64;
        keys.sort_by(|a, b| {
            let da = (a.0 as f64 + 0.5 - cx).powi(2) + (a.1 as f64 + 0.5 - cy).powi(2);
            let db = (b.0 as f64 + 0.5 - cx).powi(2) + (b.1 as f64 + 0.5 - cy).powi(2);
            da.total_cmp(&db)
        });
        let white = Color32::WHITE;
        for (x, y) in keys {
            let min = proj.world_to_screen([x as f64 / n as f64, y as f64 / n as f64]);
            let rect = Rect::from_min_size(min, vec2(tile_pts as f32 + 0.5, tile_pts as f32 + 0.5));
            match tiles.tile((zt, x, y), style.look, style.rev) {
                Some(tex) => {
                    painter.image(tex, rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), white);
                }
                None => {
                    // a parent's texture, cropped, until this tile is ready
                    for up in 1..=4u8 {
                        if up > zt {
                            break;
                        }
                        let pk = (zt - up, x >> up, y >> up);
                        if let Some(tex) = tiles.existing(pk) {
                            let k = (1u32 << up) as f32;
                            let u0 = (x - (pk.1 << up)) as f32 / k;
                            let v0 = (y - (pk.2 << up)) as f32 / k;
                            painter.image(tex, rect, Rect::from_min_size(pos2(u0, v0), vec2(1.0 / k, 1.0 / k)), white);
                            break;
                        }
                    }
                }
            }
        }
    }

    /// Symbols visible in the view for one source, from tiles at the right zoom.
    fn collect(&self, proj: &Proj, tiles: &mut Tiles, src: Source, out: &mut Vec<Sym>) {
        let z = (proj.zoom.floor() as i32).clamp(src.min_zoom() as i32, src.max_zoom() as i32) as u8;
        if (proj.zoom.floor() as i32) < src.min_zoom() as i32 {
            return;
        }
        let n = 1u32 << z;
        let pad = 40.0 / proj.size; // symbols just outside still label into view
        let tl = proj.screen_to_world(proj.rect.left_top());
        let br = proj.screen_to_world(proj.rect.right_bottom());
        let x0 = (((tl[0] - pad) * n as f64).floor().max(0.0)) as u32;
        let y0 = (((tl[1] - pad) * n as f64).floor().max(0.0)) as u32;
        let x1 = (((br[0] + pad) * n as f64).floor() as i64).clamp(0, n as i64 - 1) as u32;
        let y1 = (((br[1] + pad) * n as f64).floor() as i64).clamp(0, n as i64 - 1) as u32;
        for y in y0..=y1 {
            for x in x0..=x1 {
                if let Some(Some(t)) = tiles.vector(src, z, x, y) {
                    out.extend(t.syms.iter().filter(|s| s.world[0] >= tl[0] - pad && s.world[0] <= br[0] + pad && s.world[1] >= tl[1] - pad && s.world[1] <= br[1] + pad).cloned());
                }
            }
        }
    }

    /// Place and draw chart symbols and labels. Fills `self.hits` for clicks.
    pub fn draw_symbols(&mut self, ui: &egui::Ui, painter: &Painter, proj: &Proj, tiles: &mut Tiles, style: &Style) {
        self.hits.clear();
        let z = proj.zoom as f32;
        let p = style.palette;
        let lay = style.layers;
        let (atlas_tex, atlas) = self.atlas(ui.ctx(), p);
        let mut syms = Vec::new();
        self.collect(proj, tiles, Source::Aero, &mut syms);
        if lay.on(Group::Obstacles) && z >= 9.0 {
            self.collect(proj, tiles, Source::Obstacles, &mut syms);
        }
        if z >= 7.0 && lay.on(Group::AirspaceLabels) {
            self.collect(proj, tiles, Source::Airspace, &mut syms);
        }
        if lay.on(Group::Basemap) || z >= 6.0 {
            self.collect(proj, tiles, Source::Base, &mut syms);
        }

        let halo = p.label_halo.egui();
        let fonts = Fonts::default();
        let mut grid = Grid::new(proj.rect);
        // queued drawing, in placement order (highest priority first); drawn back to front
        let mut draws: Vec<Draw> = Vec::new();
        let mut roses: Vec<Draw> = Vec::new();
        let icon_size = |ic: Icon, scale: f32| ic.size() * scale;

        // ---- airports (public & military first, by rank) ----
        let mut apts: Vec<(&Sym, &super::symbols::Airport)> = syms.iter().filter_map(|s| if let Kind::Airport(a) = &s.kind { Some((s, a)) } else { None }).collect();
        apts.sort_by(|a, b| a.1.rank.total_cmp(&b.1.rank).then(b.1.rwy.total_cmp(&a.1.rwy)));
        let apt_scale = lin(z, &[(4.0, 0.65), (9.0, 0.9), (12.0, 1.05)]);
        for (s, a) in &apts {
            let heli = &*a.ty == "H";
            let public = !a.pvt || a.mil;
            let pos = proj.world_to_screen(s.world);
            if heli {
                if !lay.on(Group::Heliports) || z < 10.0 {
                    continue;
                }
                let ic = Icon::Heli { towered: a.twr };
                let r = Rect::from_center_size(pos, egui::Vec2::splat(icon_size(ic, 1.0)));
                if grid.place(r, false) {
                    draws.push(Draw::Icon { ic, pos, size: r.width(), angle: 0.0 });
                    draws.push(Draw::Letter { pos, text: "H", color: if a.twr { p.towered.egui() } else { p.nontowered.egui() }, size: 8.0 });
                    self.hits.push(Hit { rect: r, sym: (*s).clone() });
                }
                continue;
            }
            let (on, scale, ic) = if public {
                let color = if a.mil { 2 } else if a.twr { 0 } else { 1 };
                (lay.on(Group::AirportsPublic), apt_scale, Icon::Airport { color, hard: a.hard, fuel: a.fuel })
            } else {
                (lay.on(Group::AirportsPrivate) && z >= 9.0, 0.8, Icon::Private { hard: a.hard })
            };
            if !on {
                continue;
            }
            let sz = icon_size(ic, scale);
            let r = Rect::from_center_size(pos, egui::Vec2::splat(sz * 0.7));
            let allow = public && z >= 8.0;
            if !grid.place(r, allow) {
                continue;
            }
            let angle = if a.hdg >= 0.0 { (a.hdg as f32).to_radians() } else { 0.0 };
            draws.push(Draw::Icon { ic, pos, size: sz, angle });
            if !public {
                draws.push(Draw::Letter { pos, text: "R", color: if a.hard { p.nontowered.egui() } else { p.symbol_halo.egui() }, size: 7.5 });
            }
            self.hits.push(Hit { rect: r.expand(3.0), sym: (*s).clone() });
            // label below
            let color = if a.mil { p.military } else if a.twr { p.towered } else { p.nontowered }.egui();
            if public && z >= 6.0 {
                let size = lin(z, &[(6.0, 10.0), (10.0, 12.0)]);
                let job = if z < 9.0 {
                    single(a.ident(), fonts.regular(size), color)
                } else {
                    let mut j = egui::text::LayoutJob::default();
                    j.halign = egui::Align::Center;
                    push(&mut j, &a.name.to_uppercase(), fonts.regular(size * 0.82), color);
                    push(&mut j, "\n", fonts.regular(size), color);
                    push(&mut j, a.ident(), fonts.bold(size), color);
                    if !a.ctaf.is_empty() {
                        push(&mut j, &format!("  {}", a.ctaf), fonts.regular(size), color);
                    }
                    push(&mut j, &format!("\n{} {}{}", a.elev.round(), if a.lit { "L " } else { "" }, (a.rwy / 100.0).round()), fonts.regular(size * 0.85), color);
                    j
                };
                let g = ui.fonts_mut(|f| f.layout_job(job));
                let tl = pos2(pos.x - g.size().x / 2.0, pos.y + size * 1.15);
                if grid.place(Rect::from_min_size(tl, g.size()), false) {
                    draws.push(Draw::Text { galley: g, pos: tl, halo, width: 1.6, angle: 0.0 });
                }
            } else if !public && z >= 10.0 {
                let g = ui.fonts_mut(|f| f.layout_job(single(&a.id, fonts.regular(10.0), p.nontowered.egui())));
                let tl = pos2(pos.x - g.size().x / 2.0, pos.y + 12.0);
                if grid.place(Rect::from_min_size(tl, g.size()), false) {
                    draws.push(Draw::Text { galley: g, pos: tl, halo, width: 1.3, angle: 0.0 });
                }
            }
        }

        // ---- places and water ----
        if lay.on(Group::Basemap) {
            for s in &syms {
                match &s.kind {
                    Kind::Place { city, name } if !name.is_empty() && ((*city && z >= 5.0) || (!city && z >= 9.0)) => {
                        let pos = proj.world_to_screen(s.world);
                        let (text, size, op) = if *city { (spaced(&name.to_uppercase()), lin(z, &[(5.0, 11.0), (10.0, 15.0)]), 0.7) } else { (name.to_string(), 11.0, 0.75) };
                        let g = ui.fonts_mut(|f| f.layout_job(single(&text, fonts.regular(size), p.label.alpha(op).egui())));
                        let tl = pos - g.size() / 2.0;
                        if grid.place(Rect::from_min_size(tl, g.size()), false) {
                            draws.push(Draw::Text { galley: g, pos: tl, halo, width: 1.3, angle: 0.0 });
                        }
                    }
                    Kind::Water { name } if !name.is_empty() && z >= 6.0 => {
                        let pos = proj.world_to_screen(s.world);
                        let g = ui.fonts_mut(|f| f.layout_job(single(name, fonts.italic(11.0), p.water_line.egui())));
                        let tl = pos - g.size() / 2.0;
                        if grid.place(Rect::from_min_size(tl, g.size()), false) {
                            draws.push(Draw::Text { galley: g, pos: tl, halo, width: 1.0, angle: 0.0 });
                        }
                    }
                    _ => {}
                }
            }
        }

        // ---- airway labels ----
        for s in &syms {
            if let Kind::Airway { id, high, angle } = &s.kind {
                let (on, minz, color) = if *high { (lay.on(Group::AirwaysHigh), 7.0, p.military) } else { (lay.on(Group::AirwaysLow), 8.0, p.airway) };
                if !on || z < minz {
                    continue;
                }
                let pos = proj.world_to_screen(s.world);
                let g = ui.fonts_mut(|f| f.layout_job(single(id, fonts.bold(10.0), color.egui())));
                let mut a = *angle;
                if a > std::f32::consts::FRAC_PI_2 {
                    a -= std::f32::consts::PI;
                }
                if a < -std::f32::consts::FRAC_PI_2 {
                    a += std::f32::consts::PI;
                }
                let r = Rect::from_center_size(pos, g.size());
                if grid.place(r, false) {
                    // rotate about the centre
                    let half = g.size() / 2.0;
                    let rot = egui::emath::Rot2::from_angle(a);
                    let tl = pos - rot * half;
                    draws.push(Draw::Text { galley: g, pos: tl, halo, width: 1.5, angle: a });
                }
            }
        }

        // ---- navaids ----
        if lay.on(Group::Navaids) {
            let nav_scale = lin(z, &[(6.0, 0.65), (10.0, 1.0)]);
            for s in &syms {
                let Kind::Navaid { id, name, ty, freq, var } = &s.kind else { continue };
                let pos = proj.world_to_screen(s.world);
                let ic = match Icon::vor_kind(ty) {
                    Some(k) => Icon::Vor(k),
                    None if ty.starts_with("NDB") => Icon::Ndb,
                    None => continue,
                };
                let sz = icon_size(ic, nav_scale);
                let r = Rect::from_center_size(pos, egui::Vec2::splat(sz * 0.7));
                grid.place(r, true);
                draws.push(Draw::Icon { ic, pos, size: sz, angle: 0.0 });
                self.hits.push(Hit { rect: r.expand(3.0), sym: s.clone() });
                if z >= 9.0 && matches!(&**ty, "VOR" | "VORTAC" | "VOR/DME") {
                    let scale = super::style::interp_exp(z, 2.0, &[(9.0, 0.55), (11.0, 1.4), (13.0, 3.0)]);
                    let rose_angle = (-*var as f32).to_radians();
                    roses.push(Draw::Icon { ic: Icon::Rose, pos, size: Icon::Rose.size() * scale, angle: rose_angle });
                    roses.push(Draw::RoseNumbers { pos, radius: (Icon::Rose.size() * scale / 2.0 - 2.0 * scale) - 16.0 * scale, angle: rose_angle, color: p.navaid.alpha(0.75).egui(), size: 9.0 * scale });
                }
                if z >= 7.0 {
                    let color = if ty.starts_with("NDB") { p.ndb } else { p.navaid }.egui();
                    let mut j = egui::text::LayoutJob::default();
                    j.halign = egui::Align::Center;
                    push(&mut j, &name.to_uppercase(), fonts.regular(10.5 * 0.9), color);
                    push(&mut j, "\n", fonts.regular(10.5), color);
                    push(&mut j, &format!("{freq} {id}"), fonts.bold(10.5), color);
                    let g = ui.fonts_mut(|f| f.layout_job(j));
                    let tl = pos2(pos.x - g.size().x / 2.0, pos.y + 10.5 * 1.3);
                    if grid.place(Rect::from_min_size(tl, g.size()), false) {
                        draws.push(Draw::Text { galley: g, pos: tl, halo, width: 1.6, angle: 0.0 });
                    }
                }
            }
        }

        // ---- fixes ----
        for s in &syms {
            let Kind::Fix { id, vfr } = &s.kind else { continue };
            let on = if *vfr { lay.on(Group::VfrWaypoints) } else { lay.on(Group::IfrFixes) };
            if !on || z < 9.0 {
                continue;
            }
            let pos = proj.world_to_screen(s.world);
            let ic = if *vfr { Icon::FixVfr } else { Icon::FixIfr };
            let r = Rect::from_center_size(pos, egui::Vec2::splat(ic.size() * 0.8));
            if !grid.place(r, false) {
                continue;
            }
            draws.push(Draw::Icon { ic, pos, size: ic.size(), angle: 0.0 });
            self.hits.push(Hit { rect: r.expand(2.0), sym: s.clone() });
            if z >= 10.0 {
                let (font, color) = if *vfr { (fonts.bold(10.0), p.nontowered) } else { (fonts.regular(10.0), p.navaid) };
                let g = ui.fonts_mut(|f| f.layout_job(single(id, font, color.egui())));
                let tl = pos2(pos.x - g.size().x / 2.0, pos.y + 11.0);
                if grid.place(Rect::from_min_size(tl, g.size()), false) {
                    draws.push(Draw::Text { galley: g, pos: tl, halo, width: 1.4, angle: 0.0 });
                }
            }
        }

        // ---- obstacles ----
        if lay.on(Group::Obstacles) && z >= 9.0 {
            for s in &syms {
                let Kind::Obstacle { agl, msl, lit, .. } = &s.kind else { continue };
                let pos = proj.world_to_screen(s.world);
                let scale = lin(*agl as f32, &[(200.0, 0.75), (1000.0, 1.1)]);
                let ic = Icon::Obstacle { lit: *lit };
                let sz = ic.size() * scale;
                let r = Rect::from_center_size(pos, egui::Vec2::splat(sz * 0.6));
                grid.place(r, true);
                draws.push(Draw::Icon { ic, pos, size: sz, angle: 0.0 });
                self.hits.push(Hit { rect: r.expand(2.0), sym: s.clone() });
                if z >= 11.0 {
                    let mut j = egui::text::LayoutJob::default();
                    j.halign = egui::Align::Center;
                    push(&mut j, &format!("{}\n({})", msl.round(), agl.round()), fonts.regular(10.0), p.obstacle.egui());
                    let g = ui.fonts_mut(|f| f.layout_job(j));
                    let tl = pos2(pos.x - g.size().x / 2.0, pos.y + 10.0 * 1.4 - 4.0);
                    if grid.place(Rect::from_min_size(tl, g.size()), false) {
                        draws.push(Draw::Text { galley: g, pos: tl, halo, width: 1.3, angle: 0.0 });
                    }
                }
            }
        }

        // ---- airspace labels ----
        if lay.on(Group::AirspaceLabels) {
            for s in &syms {
                let Kind::AirspaceLabel { kind, label, range, lo_agl, lo, hi, .. } = &s.kind else { continue };
                if !style.altitude.passes(*lo, *hi) || !lay.airspace_kind(kind, *lo_agl, *lo) {
                    continue;
                }
                let pos = proj.world_to_screen(s.world);
                let job = match &**kind {
                    "B" | "C" if z >= 7.0 => {
                        let size = lin(z, &[(7.0, 11.0), (11.0, 15.0)]);
                        let color = if &**kind == "B" { p.class_b } else { p.class_c }.egui();
                        let text = match label.split_once('/') {
                            Some((a, b)) => format!("{a}\n{b}"),
                            None => label.to_string(),
                        };
                        let mut j = egui::text::LayoutJob::default();
                        j.halign = egui::Align::Center;
                        push(&mut j, &text, fonts.bold(size), color);
                        Some((j, 2.0))
                    }
                    "D" if z >= 8.0 => Some((single(label, fonts.bold(12.0), p.class_d.egui()), 1.8)),
                    "R" | "P" | "MOA" | "W" | "A" if z >= 7.0 => {
                        let color = p.sua(kind).egui();
                        let mut j = egui::text::LayoutJob::default();
                        j.halign = egui::Align::Center;
                        push(&mut j, label, fonts.italic(11.0), color);
                        if !range.is_empty() {
                            push(&mut j, &format!("\n{range}"), fonts.italic(11.0 * 0.85), color);
                        }
                        Some((j, 1.5))
                    }
                    "E5" if z >= 9.0 && !lo_agl && !label.is_empty() => Some((single(label, fonts.regular(11.0), p.class_b.egui()), 1.5)),
                    _ => None,
                };
                let Some((job, w)) = job else { continue };
                let g = ui.fonts_mut(|f| f.layout_job(job));
                if g.size().x < 1.0 {
                    continue;
                }
                let tl = pos - g.size() / 2.0;
                if grid.place(Rect::from_min_size(tl, g.size()), false) {
                    draws.push(Draw::Text { galley: g, pos: tl, halo, width: w, angle: 0.0 });
                }
            }
        }

        // back to front: roses under everything, then lowest priority first
        for d in roses.iter().chain(draws.iter().rev()) {
            d.paint(painter, atlas_tex, &atlas);
        }
    }
}

/// Letter-spaced uppercase for city names.
fn spaced(s: &str) -> String {
    s.chars().map(|c| c.to_string()).collect::<Vec<_>>().join("\u{200A}")
}

#[derive(Default)]
pub struct Fonts;

impl Fonts {
    pub fn regular(&self, size: f32) -> FontId {
        FontId::new(size, FontFamily::Proportional)
    }
    pub fn bold(&self, size: f32) -> FontId {
        FontId::new(size, FontFamily::Name("bold".into()))
    }
    pub fn italic(&self, size: f32) -> FontId {
        FontId::new(size, FontFamily::Name("italic".into()))
    }
}

pub fn single(text: &str, font: FontId, color: Color32) -> egui::text::LayoutJob {
    let mut j = egui::text::LayoutJob::default();
    j.halign = egui::Align::LEFT;
    push(&mut j, text, font, color);
    j
}

pub fn push(j: &mut egui::text::LayoutJob, text: &str, font: FontId, color: Color32) {
    j.append(text, 0.0, egui::TextFormat { font_id: font, color, ..Default::default() });
}

enum Draw {
    Icon { ic: Icon, pos: Pos2, size: f32, angle: f32 },
    Letter { pos: Pos2, text: &'static str, color: Color32, size: f32 },
    Text { galley: Arc<Galley>, pos: Pos2, halo: Color32, width: f32, angle: f32 },
    RoseNumbers { pos: Pos2, radius: f32, angle: f32, color: Color32, size: f32 },
}

impl Draw {
    fn paint(&self, painter: &Painter, tex: TextureId, atlas: &Atlas) {
        match self {
            Draw::Icon { ic, pos, size, angle } => {
                let Some(uv) = atlas.uv.get(ic) else { return };
                let mut mesh = egui::Mesh::with_texture(tex);
                let h = size / 2.0;
                let rot = egui::emath::Rot2::from_angle(*angle);
                let corners = [vec2(-h, -h), vec2(h, -h), vec2(h, h), vec2(-h, h)];
                let uvs = [uv.left_top(), uv.right_top(), uv.right_bottom(), uv.left_bottom()];
                for (c, t) in corners.iter().zip(uvs) {
                    mesh.vertices.push(egui::epaint::Vertex { pos: *pos + rot * *c, uv: t, color: Color32::WHITE });
                }
                mesh.indices.extend([0, 1, 2, 0, 2, 3]);
                painter.add(Shape::mesh(mesh));
            }
            Draw::Letter { pos, text, color, size } => {
                painter.text(*pos + vec2(0.0, 0.5), egui::Align2::CENTER_CENTER, *text, FontId::new(*size, FontFamily::Name("bold".into())), *color);
            }
            Draw::Text { galley, pos, halo, width, angle } => {
                let w = width.min(2.0) * 0.75;
                for (dx, dy) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0), (-0.7, -0.7), (0.7, -0.7), (-0.7, 0.7), (0.7, 0.7)] {
                    let mut t = TextShape::new(*pos + vec2(dx * w, dy * w), galley.clone(), *halo);
                    t.override_text_color = Some(*halo);
                    t.angle = *angle;
                    painter.add(t);
                }
                let mut t = TextShape::new(*pos, galley.clone(), Color32::PLACEHOLDER);
                t.angle = *angle;
                painter.add(t);
            }
            Draw::RoseNumbers { pos, radius, angle, color, size } => {
                if *size < 6.0 {
                    return;
                }
                for (deg, label) in [(90.0f32, "9"), (180.0, "18"), (270.0, "27")] {
                    let a = (deg - 90.0).to_radians() + angle;
                    let c = *pos + vec2(a.cos(), a.sin()) * *radius;
                    painter.text(c, egui::Align2::CENTER_CENTER, label, FontId::new(*size, FontFamily::Name("bold".into())), *color);
                }
            }
        }
    }
}

/// Spatial hash of placed label / icon boxes.
struct Grid {
    cells: std::collections::HashMap<(i32, i32), Vec<Rect>>,
    origin: Pos2,
}

impl Grid {
    const CELL: f32 = 64.0;
    fn new(rect: Rect) -> Self {
        Grid { cells: Default::default(), origin: rect.min }
    }
    fn range(&self, r: Rect) -> (i32, i32, i32, i32) {
        let f = |v: f32, o: f32| ((v - o) / Self::CELL).floor() as i32;
        (f(r.min.x, self.origin.x), f(r.min.y, self.origin.y), f(r.max.x, self.origin.x), f(r.max.y, self.origin.y))
    }
    /// Reserve `r` unless it collides; `force` places it regardless (still blocking others).
    fn place(&mut self, r: Rect, force: bool) -> bool {
        let (x0, y0, x1, y1) = self.range(r);
        if !force {
            for y in y0..=y1 {
                for x in x0..=x1 {
                    if self.cells.get(&(x, y)).is_some_and(|v| v.iter().any(|o| o.intersects(r))) {
                        return false;
                    }
                }
            }
        }
        for y in y0..=y1 {
            for x in x0..=x1 {
                self.cells.entry((x, y)).or_default().push(r);
            }
        }
        true
    }
}

#[allow(dead_code)]
pub fn stroke(c: Color32, w: f32) -> Stroke {
    Stroke::new(w, c)
}
