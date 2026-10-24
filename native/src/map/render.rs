//! Rasterises one chart tile: terrain relief and hillshade from the elevation tiles, then the
//! basemap, airspace, airways and runways from vector tiles. Points and labels are drawn live
//! by the map view instead, so they stay crisp and don't collide across tile edges.

use super::mvt::{Feature, GeomType, Tile};
use super::style::{lin, zw, AltitudeFilter, Group, Layers, Palette, Rgba};
use std::sync::Arc;
use tiny_skia::{FillRule, LineCap, LineJoin, Mask, Paint, PathBuilder, Pixmap, Stroke, StrokeDash, Transform};

/// Elevation tile (Terrarium encoding) with MapLibre-style hillshade derivatives.
pub struct Dem {
    pub z: u8,
    /// metres, 256 x 256
    pub elev: Vec<f32>,
    /// slope derivatives per pixel, already scaled the way MapLibre's prepare pass does
    pub deriv: Vec<[f32; 2]>,
}

impl Dem {
    pub fn from_png(z: u8, png: &[u8]) -> anyhow::Result<Dem> {
        let img = image::load_from_memory(png)?.to_rgb8();
        let (w, h) = img.dimensions();
        anyhow::ensure!(w == 256 && h == 256, "unexpected DEM tile size {w}x{h}");
        let elev: Vec<f32> = img.pixels().map(|p| p[0] as f32 * 256.0 + p[1] as f32 + p[2] as f32 / 256.0 - 32768.0).collect();
        let at = |x: i32, y: i32| elev[(y.clamp(0, 255) * 256 + x.clamp(0, 255)) as usize];
        let zf = z as f32;
        let exaggeration = if zf < 2.0 { 0.4 } else if zf < 4.5 { 0.35 } else { 0.3 };
        let k = 2f32.powf(exaggeration + (19.2562 - zf));
        let mut deriv = vec![[0.0; 2]; 256 * 256];
        for y in 0..256 {
            for x in 0..256 {
                let (a, b, c) = (at(x - 1, y - 1), at(x, y - 1), at(x + 1, y - 1));
                let (d, f) = (at(x - 1, y), at(x + 1, y));
                let (g, hh, i) = (at(x - 1, y + 1), at(x, y + 1), at(x + 1, y + 1));
                let dx = ((c + f + f + i) - (a + d + d + g)) / k;
                let dy = ((g + hh + hh + i) - (a + b + b + c)) / k;
                deriv[(y * 256 + x) as usize] = [dx.clamp(-1.0, 1.0), dy.clamp(-1.0, 1.0)];
            }
        }
        Ok(Dem { z, elev, deriv })
    }
}

/// A source tile and where the rendered tile sits inside it (for overzoom).
#[derive(Clone)]
pub struct Src<T> {
    pub data: Arc<T>,
    pub z: u8,
    pub x: u32,
    pub y: u32,
}

pub struct Inputs {
    pub z: u8,
    pub x: u32,
    pub y: u32,
    pub base: Option<Src<Tile>>,
    pub aero: Option<Src<Tile>>,
    pub airspace: Option<Src<Tile>>,
    pub dem: Option<Src<Dem>>,
}

#[derive(Clone)]
pub struct Look {
    pub palette: Palette,
    pub layers: Layers,
    pub altitude: AltitudeFilter,
    /// texture edge in pixels
    pub size: u32,
}

/// Maps a source tile's coordinates onto the rendered tile's pixels.
fn transform_for<T>(src: &Src<T>, z: u8, x: u32, y: u32, extent: f32, size: f32) -> Transform {
    let dz = z - src.z;
    let n = (1u32 << dz) as f32;
    let sub_x = (x - (src.x << dz)) as f32;
    let sub_y = (y - (src.y << dz)) as f32;
    let s = size * n / extent;
    Transform::from_row(s, 0.0, 0.0, s, -sub_x * size, -sub_y * size)
}

fn path_of(f: &Feature, close: bool) -> Option<tiny_skia::Path> {
    let mut pb = PathBuilder::new();
    for part in &f.parts {
        if part.len() < 2 {
            continue;
        }
        pb.move_to(part[0][0], part[0][1]);
        for p in &part[1..] {
            pb.line_to(p[0], p[1]);
        }
        if close {
            pb.close();
        }
    }
    pb.finish()
}

/// Polygon outline without the edges vector-tile clipping adds along the tile buffer
/// (both ends past the same side of the tile), like MapLibre's boundary-edge check.
fn outline_of(f: &Feature, extent: f32) -> Option<tiny_skia::Path> {
    let outside = |a: [f32; 2], b: [f32; 2]| (a[0] < 0.0 && b[0] < 0.0) || (a[0] > extent && b[0] > extent) || (a[1] < 0.0 && b[1] < 0.0) || (a[1] > extent && b[1] > extent);
    let mut pb = PathBuilder::new();
    for ring in &f.parts {
        if ring.len() < 2 {
            continue;
        }
        let mut pen_down = false;
        for w in ring.windows(2) {
            if outside(w[0], w[1]) {
                pen_down = false;
                continue;
            }
            if !pen_down {
                pb.move_to(w[0][0], w[0][1]);
                pen_down = true;
            }
            pb.line_to(w[1][0], w[1][1]);
        }
    }
    pb.finish()
}

fn paint(c: Rgba) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(c.skia());
    p.anti_alias = true;
    p
}

struct Canvas<'a> {
    pm: &'a mut Pixmap,
    /// device px per CSS px
    k: f32,
    mask: Mask,
}

impl Canvas<'_> {
    fn fill(&mut self, path: &tiny_skia::Path, ts: Transform, c: Rgba) {
        if let Some(p) = path.clone().transform(ts) {
            self.pm.fill_path(&p, &paint(c), FillRule::EvenOdd, Transform::identity(), None);
        }
    }

    /// Stroke in CSS px (scaled to device px); `dash` in multiples of the line width like MapLibre.
    fn stroke(&mut self, path: &tiny_skia::Path, ts: Transform, c: Rgba, width: f32, dash: Option<&[f32]>, clip: bool) {
        let Some(p) = path.clone().transform(ts) else { return };
        let w = (width * self.k).max(0.3);
        let mut s = Stroke { width: w, line_cap: LineCap::Butt, line_join: LineJoin::Round, ..Default::default() };
        if let Some(d) = dash {
            s.dash = StrokeDash::new(d.iter().map(|v| (v * w).max(0.5)).collect(), 0.0);
        }
        let mask = if clip { Some(&self.mask) } else { None };
        self.pm.stroke_path(&p, &paint(c), &s, Transform::identity(), mask);
    }

    /// A band just inside a polygon's edge (MapLibre line-offset), `width` CSS px wide.
    fn inner_band(&mut self, shape: &Shape, ts: Transform, c: Rgba, width: f32, dash: Option<&[f32]>) {
        let (Some(fill), Some(p)) = (shape.fill.clone().transform(ts), shape.outline.as_ref().and_then(|o| o.clone().transform(ts))) else { return };
        self.mask.clear();
        self.mask.fill_path(&fill, FillRule::EvenOdd, true, Transform::identity());
        let w = width * 2.0 * self.k;
        let mut s = Stroke { width: w, line_cap: LineCap::Butt, line_join: LineJoin::Round, ..Default::default() };
        if let Some(d) = dash {
            s.dash = StrokeDash::new(d.iter().map(|v| (v * w / 2.0).max(0.5)).collect(), 0.0);
        }
        self.pm.stroke_path(&p, &paint(c), &s, Transform::identity(), Some(&self.mask));
    }
}

/// A polygon as a filled area and as its visible outline.
struct Shape {
    fill: tiny_skia::Path,
    outline: Option<tiny_skia::Path>,
}

pub fn render(inp: &Inputs, look: &Look) -> Pixmap {
    let size = look.size;
    let mut pm = Pixmap::new(size, size).expect("pixmap");
    let p = &look.palette;
    let z = inp.z as f32;
    pm.fill(p.background.skia());

    if look.layers.on(Group::Terrain) {
        if let Some(dem) = &inp.dem {
            terrain(&mut pm, dem, inp, p);
        }
    }

    let mut cv = Canvas { k: size as f32 / 256.0, mask: Mask::new(size, size).expect("mask"), pm: &mut pm };

    // ---- basemap ----
    if let Some(src) = &inp.base {
        let ts = |ext: f32| transform_for(src, inp.z, inp.x, inp.y, ext, size as f32);
        let basemap = look.layers.on(Group::Basemap);
        if basemap && z >= 6.0 {
            if let Some(l) = src.data.layer("landuse") {
                let op = lin(z, &[(6.0, 0.55), (12.0, 0.35)]);
                for f in &l.features {
                    if f.kind == GeomType::Polygon && matches!(f.str("class"), "residential" | "suburb" | "neighbourhood" | "commercial" | "industrial" | "retail") {
                        if let Some(path) = path_of(f, true) {
                            cv.fill(&path, ts(l.extent), p.urban.alpha(op));
                        }
                    }
                }
            }
        }
        if let Some(l) = src.data.layer("water") {
            for f in l.features.iter().filter(|f| f.kind == GeomType::Polygon) {
                if let Some(path) = path_of(f, true) {
                    cv.fill(&path, ts(l.extent), p.water);
                }
            }
        }
        if z >= 7.0 {
            if let Some(l) = src.data.layer("waterway") {
                let w = zw(z, &[(7.0, 0.5), (12.0, 1.6)]);
                for f in l.features.iter().filter(|f| matches!(f.str("class"), "river" | "canal")) {
                    if let Some(path) = path_of(f, false) {
                        cv.stroke(&path, ts(l.extent), p.water_line, w, None, false);
                    }
                }
            }
        }
        if basemap {
            if let Some(l) = src.data.layer("transportation") {
                for f in &l.features {
                    let class = f.str("class");
                    let (color, w) = match class {
                        "primary" | "secondary" | "tertiary" if z >= 10.0 => (p.road, zw(z, &[(10.0, 0.6), (14.0, 2.5)])),
                        "motorway" | "trunk" if z >= 6.0 => (p.road_major, zw(z, &[(6.0, 0.6), (12.0, 2.2)])),
                        _ => continue,
                    };
                    if let Some(path) = path_of(f, false) {
                        cv.stroke(&path, ts(l.extent), color, w, None, false);
                    }
                }
            }
            if let Some(l) = src.data.layer("boundary") {
                let w = zw(z, &[(3.0, 0.6), (10.0, 1.4)]);
                for f in l.features.iter().filter(|f| f.num("admin_level") <= 4.0 && f.num("maritime") != 1.0) {
                    if let Some(path) = path_of(f, false) {
                        cv.stroke(&path, ts(l.extent), p.border.alpha(0.6), w, Some(&[3.0, 2.0]), false);
                    }
                }
            }
        }
    }

    // ---- airspace ----
    if let Some(src) = &inp.airspace {
        if let Some(l) = src.data.layer("airspace") {
            let ts = transform_for(src, inp.z, inp.x, inp.y, l.extent, size as f32);
            airspace(&mut cv, l, ts, z, look);
        }
    }

    // ---- airways and runways ----
    if let Some(src) = &inp.aero {
        if let Some(l) = src.data.layer("airways") {
            let ts = transform_for(src, inp.z, inp.x, inp.y, l.extent, size as f32);
            for f in &l.features {
                let (on, color, w, op) = if f.str("level") == "low" {
                    (look.layers.on(Group::AirwaysLow), p.airway, zw(z, &[(7.0, 0.8), (12.0, 2.0)]), 0.55)
                } else {
                    (look.layers.on(Group::AirwaysHigh), p.military, zw(z, &[(6.0, 0.7), (12.0, 1.6)]), 0.5)
                };
                if on {
                    if let Some(path) = path_of(f, false) {
                        cv.stroke(&path, ts, color.alpha(op), w, None, false);
                    }
                }
            }
        }
        if z >= 11.0 && look.layers.on(Group::Runways) {
            if let Some(l) = src.data.layer("runways") {
                let ts = transform_for(src, inp.z, inp.x, inp.y, l.extent, size as f32);
                for f in &l.features {
                    let hard = f.num("hard") == 1.0;
                    let (color, w) = if hard { (p.runway, zw(z, &[(11.0, 2.5), (14.0, 10.0), (16.0, 34.0)])) } else { (p.runway_soft, zw(z, &[(11.0, 2.0), (14.0, 8.0), (16.0, 26.0)])) };
                    if let Some(path) = path_of(f, false) {
                        cv.stroke(&path, ts, color, w, None, false);
                    }
                }
            }
        }
    }
    pm
}

fn airspace(cv: &mut Canvas, l: &super::mvt::Layer, ts: Transform, z: f32, look: &Look) {
    let p = &look.palette;
    let lay = &look.layers;
    let night = p.night;
    // features grouped in draw order, each with its parsed properties
    let feats: Vec<(&Feature, &str, bool, f64, f64)> = l
        .features
        .iter()
        .filter(|f| f.kind == GeomType::Polygon)
        .map(|f| (f, f.str("kind"), f.num("lo_agl") == 1.0, f.num("lo"), f.num("hi")))
        .filter(|(_, _, _, lo, hi)| look.altitude.passes(*lo, *hi))
        .collect();
    let shapes: Vec<Option<Shape>> = feats.iter().map(|(f, ..)| Some(Shape { fill: path_of(f, true)?, outline: outline_of(f, l.extent) })).collect();
    let each = |pred: &dyn Fn(&str, bool, f64) -> bool| -> Vec<usize> { (0..feats.len()).filter(|&i| pred(feats[i].1, feats[i].2, feats[i].3)).collect() };

    let e_band_w = zw(z, &[(7.0, 4.0), (10.0, 10.0), (13.0, 22.0)]);
    let e_off = zw(z, &[(7.0, 2.0), (10.0, 5.0), (13.0, 11.0)]);
    let e_w = e_band_w / 2.0 + e_off;

    if lay.on(Group::ClassEhigh) {
        for i in each(&|k, agl, lo| (k == "E5" || k == "E6") && agl && lo > 700.0) {
            if let Some(shape) = &shapes[i] {
                if let Some(o) = &shape.outline { cv.stroke(o, ts, p.class_b.alpha(0.35), 1.0, Some(&[1.0, 2.0]), false); }
            }
        }
    }
    if lay.on(Group::ClassE) && z >= 5.0 {
        // soft vignette inside 700 ft AGL floors (magenta) and MSL floors (blue)
        for i in each(&|k, agl, lo| k == "E5" && (!agl || lo <= 700.0)) {
            let (_, _, agl, ..) = feats[i];
            let color = if agl { p.class_e.alpha(if night { 0.22 } else { 0.18 }) } else { p.class_b.alpha(0.22) };
            if let Some(shape) = &shapes[i] {
                for (wk, a) in [(1.0, 0.35), (0.7, 0.35), (0.4, 0.4)] {
                    cv.inner_band(shape, ts, color.alpha(a), e_w * wk, None);
                }
            }
        }
        for i in each(&|k, _, _| matches!(k, "E2" | "E3" | "E4")) {
            if let Some(shape) = &shapes[i] {
                if let Some(o) = &shape.outline { cv.stroke(o, ts, p.class_e.alpha(0.85), zw(z, &[(7.0, 1.0), (12.0, 2.0)]), Some(&[4.0, 2.5]), false); }
            }
        }
    }
    if lay.on(Group::ModeC) {
        for i in each(&|k, _, _| k == "MODEC") {
            if let Some(shape) = &shapes[i] {
                if let Some(o) = &shape.outline { cv.stroke(o, ts, p.class_c.alpha(0.5), 1.0, None, false); }
            }
        }
    }
    for (group, kinds) in [(Group::WarningAlert, &["W", "A"][..]), (Group::Moa, &["MOA"][..]), (Group::Restricted, &["R", "P"][..])] {
        if !lay.on(group) {
            continue;
        }
        let idx = each(&|k, _, _| kinds.contains(&k));
        for &i in &idx {
            let kind = feats[i].1;
            let op = match kind {
                "P" => 0.12,
                "R" => 0.07,
                _ => 0.04,
            };
            if let Some(shape) = &shapes[i] {
                cv.fill(&shape.fill, ts, p.sua(kind).alpha(op));
            }
        }
        if group == Group::Restricted {
            let w = zw(z, &[(5.0, 3.0), (10.0, 9.0), (13.0, 16.0)]);
            let off = zw(z, &[(5.0, 1.5), (10.0, 4.5), (13.0, 8.0)]);
            for &i in &idx {
                if let Some(shape) = &shapes[i] {
                    cv.inner_band(shape, ts, p.restricted.alpha(0.35), w / 2.0 + off, Some(&[0.25, 0.25]));
                }
            }
        }
        for &i in &idx {
            if let Some(shape) = &shapes[i] {
                if let Some(o) = &shape.outline { cv.stroke(o, ts, p.sua(feats[i].1).alpha(0.9), zw(z, &[(5.0, 1.0), (12.0, 2.0)]), None, false); }
            }
        }
    }
    if lay.on(Group::ClassD) {
        for i in each(&|k, _, _| k == "D") {
            if let Some(shape) = &shapes[i] {
                if let Some(o) = &shape.outline { cv.stroke(o, ts, p.class_d, zw(z, &[(6.0, 1.0), (12.0, 2.2)]), Some(&[3.0, 2.0]), false); }
            }
        }
    }
    for (group, kind, color, band, band_off, line) in [
        (Group::ClassC, "C", p.class_c, [(5.0, 3.0), (10.0, 10.0), (13.0, 20.0)], [(5.0, 1.5), (10.0, 5.0), (13.0, 10.0)], [(5.0, 1.2), (12.0, 3.0)]),
        (Group::ClassB, "B", p.class_b, [(4.0, 3.0), (10.0, 12.0), (13.0, 24.0)], [(4.0, 1.5), (10.0, 6.0), (13.0, 12.0)], [(4.0, 1.2), (12.0, 3.2)]),
    ] {
        if !lay.on(group) {
            continue;
        }
        let idx = each(&|k, _, _| k == kind);
        let w = zw(z, &band) / 2.0 + zw(z, &band_off);
        for &i in &idx {
            if let Some(shape) = &shapes[i] {
                cv.inner_band(shape, ts, color.alpha(0.14), w, None);
            }
        }
        for &i in &idx {
            if let Some(shape) = &shapes[i] {
                if let Some(o) = &shape.outline { cv.stroke(o, ts, color, zw(z, &line), None, false); }
            }
        }
    }
}

/// Relief colouring and hillshade, after MapLibre's color-relief and hillshade layers.
fn terrain(pm: &mut Pixmap, dem: &Src<Dem>, inp: &Inputs, p: &Palette) {
    let size = pm.width() as usize;
    let dz = inp.z.saturating_sub(dem.z);
    let n = (1u32 << dz) as f32;
    let sub_x = (inp.x - (dem.x << dz)) as f32;
    let sub_y = (inp.y - (dem.y << dz)) as f32;
    let d = &dem.data;
    let night = p.night;
    let intensity: f32 = if night { 0.5 } else { 0.32 };
    let base = 1.875 - intensity * 1.75;
    let max_v = std::f32::consts::FRAC_PI_2;
    let azimuth = 315f32.to_radians() + std::f32::consts::PI;
    let shadow = p.hill_shadow;
    let high = p.hill_highlight;
    let ic = (intensity * 2.0).clamp(0.0, 1.0);
    // latitude range of the DEM tile for the mercator slope correction
    let tile_lat = |ty: f32, zz: u8| {
        let nn = (1u32 << zz) as f32;
        (std::f32::consts::PI * (1.0 - 2.0 * ty / nn)).sinh().atan().to_degrees()
    };
    let lat_top = tile_lat(dem.y as f32, dem.z);
    let lat_bot = tile_lat(dem.y as f32 + 1.0, dem.z);
    let data = pm.data_mut();
    for py in 0..size {
        let v = ((py as f32 + 0.5) / size as f32 + sub_y) / n; // 0..1 within the DEM tile
        let fy = (v * 256.0 - 0.5).clamp(0.0, 255.0);
        let y0 = fy.floor() as usize;
        let y1 = (y0 + 1).min(255);
        let ty = fy - y0 as f32;
        let scale = (lat_top + (lat_bot - lat_top) * v).to_radians().cos().max(0.05);
        for px in 0..size {
            let u = ((px as f32 + 0.5) / size as f32 + sub_x) / n;
            let fx = (u * 256.0 - 0.5).clamp(0.0, 255.0);
            let x0 = fx.floor() as usize;
            let x1 = (x0 + 1).min(255);
            let tx = fx - x0 as f32;
            let bl = |a: f32, b: f32, c: f32, e: f32| (a * (1.0 - tx) + b * tx) * (1.0 - ty) + (c * (1.0 - tx) + e * tx) * ty;
            let i00 = y0 * 256 + x0;
            let i01 = y0 * 256 + x1;
            let i10 = y1 * 256 + x0;
            let i11 = y1 * 256 + x1;
            let elev = bl(d.elev[i00], d.elev[i01], d.elev[i10], d.elev[i11]);
            let dx = bl(d.deriv[i00][0], d.deriv[i01][0], d.deriv[i10][0], d.deriv[i11][0]);
            let dy = bl(d.deriv[i00][1], d.deriv[i01][1], d.deriv[i10][1], d.deriv[i11][1]);
            let relief = p.relief_at(elev);
            // hillshade (premultiplied), composited over the relief colour
            let slope = (1.25 * (dx * dx + dy * dy).sqrt() / scale).atan();
            let aspect = if dx != 0.0 { dy.atan2(-dx) } else { std::f32::consts::FRAC_PI_2 * if dy > 0.0 { 1.0 } else { -1.0 } };
            let scaled = if (intensity - 0.5).abs() > 1e-6 { (base.powf(slope) - 1.0) / (base.powf(max_v) - 1.0) * max_v } else { slope };
            let accent = 1.0 - scaled.cos();
            let shade = (((aspect + azimuth) / std::f32::consts::PI + 0.5).rem_euclid(2.0) - 1.0).abs();
            let sa = scaled.sin() * ic;
            let mix = |a: u8, b: u8| a as f32 / 255.0 * (1.0 - shade) + b as f32 / 255.0 * shade;
            let shade_rgb = [mix(shadow.0, high.0) * sa, mix(shadow.1, high.1) * sa, mix(shadow.2, high.2) * sa];
            let shade_a = sa;
            let acc_a = accent * ic;
            let acc_rgb = [shadow.0 as f32 / 255.0 * acc_a, shadow.1 as f32 / 255.0 * acc_a, shadow.2 as f32 / 255.0 * acc_a];
            let out_rgb = [acc_rgb[0] * (1.0 - shade_a) + shade_rgb[0], acc_rgb[1] * (1.0 - shade_a) + shade_rgb[1], acc_rgb[2] * (1.0 - shade_a) + shade_rgb[2]];
            let out_a = (acc_a * (1.0 - shade_a) + shade_a).min(1.0);
            let base_rgb = [relief.0 as f32 / 255.0, relief.1 as f32 / 255.0, relief.2 as f32 / 255.0];
            let o = (py * size + px) * 4;
            for c in 0..3 {
                data[o + c] = ((out_rgb[c] + base_rgb[c] * (1.0 - out_a)).clamp(0.0, 1.0) * 255.0).round() as u8;
            }
            data[o + 3] = 255;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Renders a Phoenix tile from the repository's data build plus the live basemap and
    /// terrain to target/tile-*.png for a visual check (skipped without data or network).
    #[test]
    fn render_phoenix() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let Ok(aero) = crate::map::pmtiles::PmTiles::open(&root.join("../tiles/aero.pmtiles")) else { return };
        let airspace = crate::map::pmtiles::PmTiles::open(&root.join("../tiles/airspace.pmtiles")).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let get = |url: String| rt.block_on(async move { reqwest::get(url).await.ok()?.bytes().await.ok().map(|b| b.to_vec()) });
        for (z, x, y) in [(9u8, 96u32, 205u32), (11, 386, 821), (9, 98, 206)] {
            let dz = z.min(12);
            let dem = get(format!("https://s3.amazonaws.com/elevation-tiles-prod/terrarium/{dz}/{}/{}.png", x >> (z - dz), y >> (z - dz))).and_then(|b| Dem::from_png(dz, &b).ok());
            let base = get(format!("https://tiles.openfreemap.org/planet/20261018_080001_pt/{z}/{x}/{y}.pbf")).map(|b| crate::map::mvt::decode(&b));
            let a = aero.get(z.min(13), x, y).unwrap().map(|b| crate::map::mvt::decode(&b));
            let s = airspace.get(z.min(11), x >> (z - z.min(11)), y >> (z - z.min(11))).unwrap().map(|b| crate::map::mvt::decode(&b));
            let inp = Inputs {
                z,
                x,
                y,
                base: base.map(|t| Src { data: Arc::new(t), z, x, y }),
                aero: a.map(|t| Src { data: Arc::new(t), z, x, y }),
                airspace: s.map(|t| Src { data: Arc::new(t), z: z.min(11), x: x >> (z - z.min(11)), y: y >> (z - z.min(11)) }),
                dem: dem.map(|d| Src { data: Arc::new(d), z: dz, x: x >> (z - dz), y: y >> (z - dz) }),
            };
            let look = Look { palette: crate::map::style::day(), layers: Default::default(), altitude: Default::default(), size: 512 };
            let t = std::time::Instant::now();
            let pm = render(&inp, &look);
            eprintln!("z{z} rendered in {:?}", t.elapsed());
            pm.save_png(root.join(format!("target/tile-{z}-{x}-{y}.png"))).unwrap();
        }
    }
}
