//! The chart area: tiles, weather overlays, symbols, the route (with editing by dragging) and
//! clicks that open airport / point details.

use crate::app::{App, Decode, PointItem, Selection};
use crate::backend::Task;
use crate::map::style::{lin, Group};
use crate::map::symbols::{Kind as SymKind, Sym};
use crate::map::view::{lonlat_to_world, single, Fonts, Proj, Style};
use crate::model::place::{Kind, Place};
use crate::nav::geo::{bearing, distance_nm, great_circle, interpolate, point_along};
use crate::nav::hazards::outer_rings;
use crate::nav::magvar::variation_now;
use crate::nav::navlog::deg;
use crate::wx::decode::str_of;
use crate::wx::state::{hazard_color, WIND_LEVELS};
use egui::{pos2, vec2, Color32, Painter, Pos2, Rect, Sense, Shape, Stroke};
use serde_json::Value;
use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug)]
enum RouteDrag {
    Move(usize),
    Insert(usize),
}

#[derive(Default)]
pub struct ChartUi {
    drag: Option<RouteDrag>,
    drag_from: Option<Pos2>,
    pub cursor: Option<(f64, f64)>,
    radar: HashMap<(String, i64, u8, u32, u32), RadarTile>,
    /// recently shown radar layers, newest last (tiles of older ones are dropped)
    radar_layers: Vec<(String, i64)>,
    pub runs: crate::wx::radar::Runs,
    pub model: crate::wx::radar::ModelGrid,
    /// what the radar layer is showing (for the timeline's label)
    pub radar_frame: Option<crate::wx::radar::Frame>,
    /// tiles of the current frame still loading
    pub radar_loading: bool,
}

enum RadarTile {
    Loading(Task<Option<egui::ColorImage>>),
    Ready(egui::TextureHandle),
    Missing,
}

fn halo_text(painter: &Painter, pos: Pos2, galley: std::sync::Arc<egui::Galley>, halo: Color32, w: f32, angle: f32) {
    for (dx, dy) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0), (-0.7, -0.7), (0.7, -0.7), (-0.7, 0.7), (0.7, 0.7)] {
        let mut t = egui::epaint::TextShape::new(pos + vec2(dx * w, dy * w), galley.clone(), halo);
        t.override_text_color = Some(halo);
        t.angle = angle;
        painter.add(t);
    }
    let mut t = egui::epaint::TextShape::new(pos, galley, Color32::PLACEHOLDER);
    t.angle = angle;
    painter.add(t);
}

/// Filled + outlined geographic polygon (rings of [lon, lat]).
fn polygon(painter: &Painter, proj: &Proj, rings: &[Vec<[f64; 2]>], fill: Color32, stroke: Stroke, dash: Option<(f32, f32)>) {
    let screen: Vec<Vec<Pos2>> = rings.iter().map(|r| r.iter().map(|p| proj.to_screen(p[0], p[1])).collect()).collect();
    if let Some(outer) = screen.first() {
        if outer.len() >= 3 && fill.a() > 0 {
            let mut flat: Vec<f64> = Vec::new();
            let mut holes = Vec::new();
            for (i, r) in screen.iter().enumerate() {
                if i > 0 {
                    holes.push(flat.len() / 2);
                }
                for p in r {
                    flat.push(p.x as f64);
                    flat.push(p.y as f64);
                }
            }
            if let Ok(idx) = earcutr::earcut(&flat, &holes, 2) {
                let mut mesh = egui::Mesh::default();
                for c in flat.chunks(2) {
                    mesh.colored_vertex(pos2(c[0] as f32, c[1] as f32), fill);
                }
                mesh.indices = idx.into_iter().map(|i| i as u32).collect();
                painter.add(Shape::mesh(mesh));
            }
        }
    }
    for r in &screen {
        if r.len() < 2 {
            continue;
        }
        let mut pts = r.clone();
        if pts.first() != pts.last() {
            pts.push(pts[0]);
        }
        match dash {
            Some((d, g)) => {
                painter.extend(Shape::dashed_line(&pts, stroke, d, g));
            }
            None => {
                painter.add(Shape::line(pts, stroke));
            }
        }
    }
}

fn ring_of(coords: &Value) -> Vec<[f64; 2]> {
    coords
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|c| Some([c["lon"].as_f64().or_else(|| str_of(&c["lon"]).parse().ok())?, c["lat"].as_f64().or_else(|| str_of(&c["lat"]).parse().ok())?]))
                .collect()
        })
        .unwrap_or_default()
}

fn hit_to_place(s: &Sym) -> Option<Place> {
    let (lon, lat) = crate::map::view::world_to_lonlat(s.world);
    Some(match &s.kind {
        SymKind::Airport(a) => Place { kind: Kind::Airport, id: a.ident().to_string(), name: a.name.to_string(), detail: String::new(), lat, lon },
        SymKind::Navaid { id, name, ty, freq, .. } => Place { kind: Kind::Navaid, id: id.to_string(), name: name.to_string(), detail: format!("{ty} {freq}"), lat, lon },
        SymKind::Fix { id, vfr } => Place { kind: Kind::Fix, id: id.to_string(), name: id.to_string(), detail: if *vfr { "VFR waypoint".into() } else { "Fix".into() }, lat, lon },
        _ => return None,
    })
}

/// Nearest chart feature (airport > navaid > fix) under a point, else a lat/long waypoint.
fn snap(app: &App, proj: &Proj, p: Pos2) -> Place {
    let rank = |s: &Sym| match s.kind {
        SymKind::Airport(_) => 0,
        SymKind::Navaid { .. } => 1,
        SymKind::Fix { .. } => 2,
        _ => 9,
    };
    let best = app
        .map
        .hits
        .iter()
        .filter(|h| rank(&h.sym) < 9 && h.rect.center().distance(p) < 14.0)
        .min_by(|a, b| rank(&a.sym).cmp(&rank(&b.sym)).then(a.rect.center().distance(p).total_cmp(&b.rect.center().distance(p))));
    if let Some(place) = best.and_then(|h| hit_to_place(&h.sym)) {
        return place;
    }
    let (lon, lat) = proj.from_screen(p);
    Place::coord(lat, lon)
}

fn dist_to_segment(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
    (a + ab * t).distance(p)
}

fn route_screen(app: &App, proj: &Proj) -> Vec<Pos2> {
    great_circle(&app.saved.route.path(), 5.0).iter().map(|p| proj.to_screen(p[0], p[1])).collect()
}

fn near_line(line: &[Pos2], p: Pos2, tol: f32) -> bool {
    line.windows(2).any(|w| dist_to_segment(p, w[0], w[1]) < tol)
}

/// Along-track NM of the profile sample nearest a screen point.
fn along_track(app: &App, proj: &Proj, p: Pos2) -> Option<f64> {
    let (lon, lat) = proj.from_screen(p);
    let k = lat.to_radians().cos();
    app.plan.profile.samples.iter().min_by(|a, b| ((a.lat - lat).powi(2) + ((a.lon - lon) * k).powi(2)).total_cmp(&((b.lat - lat).powi(2) + ((b.lon - lon) * k).powi(2)))).map(|s| s.d)
}

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let rect = ui.max_rect();
    let resp = ui.allocate_rect(rect, Sense::click_and_drag());
    let proj0 = app.map.proj(rect);

    // ---- route drag start: a waypoint (move) or the line (insert) ----
    if resp.drag_started_by(egui::PointerButton::Primary) {
        let origin = ui.input(|i| i.pointer.press_origin()).unwrap_or_default();
        let w = &app.saved.route.waypoints;
        let node = w.iter().position(|p| proj0.to_screen(p.lon, p.lat).distance(origin) < 11.0);
        app.chart.drag = match node {
            Some(i) => Some(RouteDrag::Move(i)),
            None if w.len() >= 2 && near_line(&route_screen(app, &proj0), origin, 7.0) => {
                let (lon, lat) = proj0.from_screen(origin);
                Some(RouteDrag::Insert(app.saved.route.leg_index_for(lat, lon)))
            }
            None => None,
        };
        app.chart.drag_from = Some(origin);
    }
    app.map.interact(ui, &resp, app.chart.drag.is_none());
    let proj = app.map.proj(rect);
    app.last_proj = Some(proj);
    let painter = ui.painter_at(rect);
    let style = Style { palette: &app.palette, layers: &app.saved.layers, altitude: app.saved.altitude, look: &app.look, rev: app.look_rev };
    app.map.draw_tiles(&painter, &proj, &mut app.tiles, &style);

    draw_radar(app, &painter, &proj);
    draw_advisories(app, &painter, &proj);

    let style = Style { palette: &app.palette, layers: &app.saved.layers, altitude: app.saved.altitude, look: &app.look, rev: app.look_rev };
    app.map.draw_symbols(ui, &painter, &proj, &mut app.tiles, &style);

    draw_picked(app, &painter, &proj);
    draw_winds(app, ui, &painter, &proj);
    draw_route(app, ui, &painter, &proj);
    draw_dots(app, ui, &painter, &proj);

    // ---- pointer ----
    let hover = resp.hover_pos();
    app.chart.cursor = hover.map(|p| {
        let (lon, lat) = proj.from_screen(p);
        (lat, lon)
    });
    if let Some(p) = hover {
        if app.chart.drag.is_none() && app.saved.route.waypoints.len() >= 2 {
            let line = route_screen(app, &proj);
            let on_node = app.saved.route.waypoints.iter().any(|w| proj.to_screen(w.lon, w.lat).distance(p) < 11.0);
            if on_node {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
            } else if near_line(&line, p, 7.0) {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Copy);
                if let Some(d) = along_track(app, &proj, p) {
                    app.plan.hover = Some((d, false));
                }
            } else if app.plan.hover.is_some_and(|h| !h.1) {
                app.plan.hover = None;
            }
        }
        if app.saved.route.editing {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        } else if app.map.hits.iter().any(|h| matches!(h.sym.kind, SymKind::Airport(_)) && h.rect.contains(p)) {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
    } else if app.plan.hover.is_some_and(|h| !h.1) {
        app.plan.hover = None;
    }

    // ---- route drag in progress / finished ----
    if let Some(drag) = app.chart.drag {
        let p = ui.input(|i| i.pointer.interact_pos()).unwrap_or_default();
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        let w = &app.saved.route.waypoints;
        let mut pts = Vec::new();
        match drag {
            RouteDrag::Move(i) => {
                if i > 0 {
                    pts.push(proj.to_screen(w[i - 1].lon, w[i - 1].lat));
                }
                pts.push(p);
                if i + 1 < w.len() {
                    pts.push(proj.to_screen(w[i + 1].lon, w[i + 1].lat));
                }
            }
            RouteDrag::Insert(i) => {
                pts.push(proj.to_screen(w[i - 1].lon, w[i - 1].lat));
                pts.push(p);
                pts.push(proj.to_screen(w[i].lon, w[i].lat));
            }
        }
        painter.extend(Shape::dashed_line(&pts, Stroke::new(3.0, app.palette.route.egui().gamma_multiply(0.85)), 8.0, 6.0));
        if resp.drag_stopped() {
            app.chart.drag = None;
            let place = snap(app, &proj, p);
            let moved = app.chart.drag_from.map(|o| o.distance(p)).unwrap_or(99.0);
            match drag {
                RouteDrag::Move(i) if moved >= 3.0 => app.saved.route.set(i, place),
                RouteDrag::Insert(i) => app.saved.route.add(place, Some(i)),
                _ => {}
            }
        }
    } else if resp.clicked() {
        if let Some(p) = resp.interact_pointer_pos() {
            click(app, &proj, p);
        }
    }

    // ---- coordinates ----
    if app.capturing {
        return;
    }
    if let Some((lat, lon)) = app.chart.cursor {
        let t = app.t;
        let text = format!("{:.4}°{} {:.4}°{}", lat.abs(), if lat >= 0.0 { 'N' } else { 'S' }, lon.abs(), if lon >= 0.0 { 'E' } else { 'W' });
        let galley = ui.fonts_mut(|f| f.layout_no_wrap(text, crate::theme::regular(11.0), t.text2));
        let r = Rect::from_min_size(pos2(rect.left() + 88.0, rect.bottom() - 30.0), galley.size() + vec2(20.0, 8.0));
        if !app.saved.profile_open || app.saved.route.waypoints.len() < 2 {
            painter.rect_filled(r, 99.0, t.surface);
            painter.galley(r.min + vec2(10.0, 4.0), galley, t.text2);
        }
    }
    // scale bar
    draw_scale(app, &painter, &proj);
}

fn click(app: &mut App, proj: &Proj, p: Pos2) {
    if app.saved.route.editing {
        let place = snap(app, proj, p);
        app.saved.route.add(place, None);
        return;
    }
    if app.saved.route.waypoints.len() >= 2 && near_line(&route_screen(app, proj), p, 9.0) && !app.map.hits.iter().any(|h| h.rect.contains(p)) {
        if let Some(d) = along_track(app, proj, p) {
            app.plan.hover = Some((d, false));
            app.saved.profile_open = true;
            return;
        }
    }
    let pal = app.palette.clone();
    if let Some(h) = app.map.hits.iter().find(|h| matches!(h.sym.kind, SymKind::Airport(_)) && h.rect.contains(p)) {
        if let SymKind::Airport(a) = &h.sym.kind {
            app.selection = Some(Selection::Airport(a.id.to_string()));
            return;
        }
    }
    let (lon, lat) = proj.from_screen(p);
    let mut items: Vec<PointItem> = Vec::new();
    for h in app.map.hits.iter().filter(|h| h.rect.expand(3.0).contains(p)) {
        match &h.sym.kind {
            SymKind::Navaid { id, name, ty, freq, .. } => items.push(PointItem { title: format!("{id} · {name}"), subtitle: format!("{ty} {freq}"), color: pal.navaid.egui(), airport: None, decode: None }),
            SymKind::Fix { id, vfr } => items.push(PointItem { title: id.to_string(), subtitle: if *vfr { "VFR waypoint".into() } else { "Fix".into() }, color: pal.nontowered.egui(), airport: None, decode: None }),
            SymKind::Obstacle { agl, msl, lit, ty } => items.push(PointItem {
                title: format!("{ty} {} ft MSL", msl.round()),
                subtitle: format!("{} ft AGL{}", agl.round(), if *lit { " · lighted" } else { "" }),
                color: pal.obstacle.egui(),
                airport: None,
                decode: None,
            }),
            _ => {}
        }
    }
    // weather dots and areas
    if app.saved.wx.metars {
        for m in &app.wx.metars {
            let (Some(mlon), Some(mlat)) = (m["lon"].as_f64(), m["lat"].as_f64()) else { continue };
            if proj.to_screen(mlon, mlat).distance(p) < 8.0 {
                let cat = str_of(&m["fltCat"]);
                items.push(PointItem { title: format!("{} {cat}", str_of(&m["icaoId"])), subtitle: String::new(), color: crate::theme::flt_cat(&cat), airport: None, decode: Some(Decode::Metar(str_of(&m["rawOb"]))) });
            }
        }
    }
    if app.saved.wx.pireps {
        for r in &app.wx.pireps {
            let (Some(rlon), Some(rlat)) = (r["lon"].as_f64(), r["lat"].as_f64()) else { continue };
            if proj.to_screen(rlon, rlat).distance(p) < 8.0 {
                items.push(PointItem { title: format!("PIREP FL{:03}", r["fltLvl"].as_i64().unwrap_or(0)), subtitle: String::new(), color: pirep_color(r), airport: None, decode: Some(Decode::Pirep(str_of(&r["rawOb"]))) });
            }
        }
    }
    if app.saved.wx.tfr {
        for f in app.wx.tfrs["features"].as_array().into_iter().flatten() {
            if outer_rings(&f["geometry"]).iter().any(|r| crate::nav::geo::in_ring(lon, lat, r)) {
                let pr = &f["properties"];
                let ty = if pr["type"].is_null() { str_of(&pr["legal"]) } else { str_of(&pr["type"]) };
                let sub = if pr["title"].is_null() { str_of(&pr["description"]) } else { str_of(&pr["title"]) };
                items.push(PointItem { title: format!("TFR {} · {ty}", str_of(&pr["notam"])), subtitle: sub, color: Color32::from_rgb(0xd2, 0x3a, 0x3a), airport: None, decode: None });
            }
        }
    }
    if app.saved.wx.sigmet {
        for s in app.wx.active_sigmets() {
            let ring = ring_of(&s["coords"]);
            if ring.len() >= 3 && crate::nav::geo::in_ring(lon, lat, &ring) {
                items.push(PointItem { title: format!("{} {}", str_of(&s["airSigmetType"]), str_of(&s["hazard"])), subtitle: String::new(), color: hazard_color(&str_of(&s["hazard"])), airport: None, decode: Some(Decode::Sigmet(s.clone())) });
            }
        }
    }
    if app.saved.wx.gairmet {
        let hours = app.wx.gairmet_hours();
        let hr = if hours.contains(&app.wx.gairmet_hour) { app.wx.gairmet_hour } else { hours.first().copied().unwrap_or(0) };
        for g in app.wx.gairmets.iter().filter(|g| g["forecastHour"].as_i64() == Some(hr)) {
            let ring = ring_of(&g["coords"]);
            if ring.len() >= 3 && str_of(&g["geometryType"]) != "LINE" && crate::nav::geo::in_ring(lon, lat, &ring) {
                items.push(PointItem { title: format!("G-AIRMET {}", str_of(&g["hazard"]).replace('_', " ")), subtitle: String::new(), color: hazard_color(&str_of(&g["hazard"])), airport: None, decode: Some(Decode::Gairmet(g.clone())) });
            }
        }
    }
    // airspace at the point (shown layers, altitude band)
    if let Ok(data) = app.be.server.try_data() {
        for v in data.airspace.at_point(lon, lat) {
            if !app.saved.layers.airspace_kind(&v.kind, v.lo_agl, v.lo as f64) || !app.saved.altitude.passes(v.lo as f64, v.hi as f64) {
                continue;
            }
            let color = if ["B", "D", "R", "P", "W"].contains(&v.kind.as_str()) { pal.class_b.egui() } else { pal.class_c.egui() };
            let range = format!("{} – {}", fmt_alt(v.lo as f64, v.lo_agl), fmt_alt(v.hi as f64, false));
            items.push(PointItem {
                title: if v.name.is_empty() { kind_name(&v.kind).into() } else { v.name.clone() },
                subtitle: format!("{} · {range}{}", kind_name(&v.kind), if v.times.is_empty() { String::new() } else { format!(" · {}", v.times) }),
                color,
                airport: None,
                decode: None,
            });
        }
    }
    let mut seen = std::collections::HashSet::new();
    items.retain(|i| seen.insert(format!("{}{}", i.title, i.subtitle)));
    app.selection = if items.is_empty() { None } else { Some(Selection::Point { lon, lat, items }) };
}

pub fn kind_name(k: &str) -> &'static str {
    match k {
        "B" => "Class B",
        "C" => "Class C",
        "D" => "Class D",
        "E2" => "Class E surface",
        "E3" | "E4" => "Class E extension",
        "E5" => "Class E",
        "E6" => "Class E (en route)",
        "MODEC" => "Mode C veil",
        "R" => "Restricted",
        "P" => "Prohibited",
        "W" => "Warning",
        "A" => "Alert",
        "MOA" => "MOA",
        _ => "Airspace",
    }
}

pub fn fmt_alt(v: f64, agl: bool) -> String {
    if v >= 99999.0 {
        "unlimited".into()
    } else if v == 0.0 {
        "SFC".into()
    } else if v >= 17999.0 {
        format!("FL{}", ((v + 1.0) / 100.0).round())
    } else if agl {
        format!("{} AGL", crate::nav::navlog::thousands(v))
    } else {
        crate::nav::navlog::thousands(v)
    }
}

fn pirep_color(p: &Value) -> Color32 {
    let ice = str_of(&p["icgInt1"]);
    let turb = str_of(&p["tbInt1"]);
    let severe = format!("{ice} {turb}").contains("SEV") || format!("{ice} {turb}").contains("EXTRM");
    if severe {
        Color32::from_rgb(0xd2, 0x3a, 0x3a)
    } else if !ice.is_empty() && ice != "NEG" {
        Color32::from_rgb(0x2f, 0x8f, 0xd6)
    } else if !turb.is_empty() && turb != "NEG" {
        Color32::from_rgb(0xd4, 0x8a, 0x0b)
    } else {
        Color32::from_rgb(0x5c, 0x63, 0x70)
    }
}

fn draw_radar(app: &mut App, painter: &Painter, proj: &Proj) {
    if !app.saved.wx.radar {
        app.chart.radar_frame = None;
        return;
    }
    use crate::wx::radar::{frame_for, Kind as RadarKind};
    app.chart.runs.update(&app.be);
    let now = chrono::Utc::now();
    let frame = frame_for(app.saved.route.departure_time(), now, &app.chart.runs);
    let tint = Color32::from_white_alpha((app.saved.radar_opacity * 255.0) as u8);
    app.chart.radar_loading = false;
    if frame.kind == RadarKind::Model {
        let (w, s, e, n) = proj.bounds();
        if let Some((tex, b)) = app.chart.model.update(&app.be, painter.ctx(), (w, s, e, n), proj.zoom, frame.valid) {
            let r = Rect::from_min_max(proj.to_screen(b[0], b[3]), proj.to_screen(b[2], b[1]));
            painter.image(tex, r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), tint);
        }
        app.chart.radar_loading = app.chart.model.loading;
        if app.chart.model.loading {
            painter.ctx().request_repaint_after(Duration::from_millis(300));
        }
        app.chart.radar_frame = Some(frame);
        return;
    }
    let layer = frame.layer.clone().unwrap_or_default();
    let stamp = frame.valid.timestamp();
    if app.chart.radar_layers.last() != Some(&(layer.clone(), stamp)) {
        app.chart.radar_layers.retain(|l| l != &(layer.clone(), stamp));
        app.chart.radar_layers.push((layer.clone(), stamp));
        // keep a few frames so scrubbing back and forth doesn't refetch
        if app.chart.radar_layers.len() > 8 {
            app.chart.radar_layers.remove(0);
        }
        let keep = app.chart.radar_layers.clone();
        app.chart.radar.retain(|k, _| keep.iter().any(|l| l.0 == k.0 && l.1 == k.1));
    }
    // HRRR tiles exist to zoom 8 on IEM's cache; NEXRAD to 8 as well
    let z = (proj.zoom.floor() as i32).clamp(3, 8) as u8;
    let n = 1u32 << z;
    let tl = proj.screen_to_world(proj.rect.left_top());
    let br = proj.screen_to_world(proj.rect.right_bottom());
    for y in ((tl[1] * n as f64).floor().max(0.0) as u32)..=((br[1] * n as f64).floor().min(n as f64 - 1.0) as u32) {
        for x in ((tl[0] * n as f64).floor().max(0.0) as u32)..=((br[0] * n as f64).floor().min(n as f64 - 1.0) as u32) {
            let key = (layer.clone(), stamp, z, x, y);
            if !app.chart.radar.contains_key(&key) {
                let http = app.be.http.clone();
                let url = frame.tile_url(z, x, y).unwrap_or_default();
                let task = app.be.spawn(async move {
                    let b = http.get(url).send().await.ok()?.error_for_status().ok()?.bytes().await.ok()?;
                    let img = image::load_from_memory(&b).ok()?.to_rgba8();
                    Some(egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw()))
                });
                app.chart.radar.insert(key.clone(), RadarTile::Loading(task));
            }
            let entry = app.chart.radar.get_mut(&key).unwrap();
            if let RadarTile::Loading(t) = entry {
                if let Some(img) = t.poll() {
                    *entry = match img {
                        Some(img) => RadarTile::Ready(painter.ctx().load_texture(format!("radar-{}-{z}-{x}-{y}", key.0), img, egui::TextureOptions::LINEAR)),
                        None => RadarTile::Missing,
                    };
                }
            }
            match entry {
                RadarTile::Ready(tex) => {
                    let min = proj.world_to_screen([x as f64 / n as f64, y as f64 / n as f64]);
                    let size = (proj.size / n as f64) as f32;
                    painter.image(tex.id(), Rect::from_min_size(min, vec2(size, size)), Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), tint);
                }
                RadarTile::Loading(_) => app.chart.radar_loading = true,
                RadarTile::Missing => {}
            }
        }
    }
    app.chart.radar_frame = Some(frame);
}

fn draw_advisories(app: &App, painter: &Painter, proj: &Proj) {
    let fonts = Fonts;
    if app.saved.wx.gairmet {
        let hours = app.wx.gairmet_hours();
        let hr = if hours.contains(&app.wx.gairmet_hour) { app.wx.gairmet_hour } else { hours.first().copied().unwrap_or(0) };
        for g in app.wx.gairmets.iter().filter(|g| g["forecastHour"].as_i64() == Some(hr)) {
            let ring = ring_of(&g["coords"]);
            if ring.len() < 2 {
                continue;
            }
            let c = hazard_color(&str_of(&g["hazard"]));
            if str_of(&g["geometryType"]) == "LINE" {
                let pts: Vec<Pos2> = ring.iter().map(|p| proj.to_screen(p[0], p[1])).collect();
                painter.extend(Shape::dashed_line(&pts, Stroke::new(1.6, c), 6.0, 4.0));
            } else {
                polygon(painter, proj, &[ring], c.gamma_multiply(0.12), Stroke::new(1.6, c), Some((6.0, 4.0)));
            }
        }
    }
    if app.saved.wx.sigmet {
        for s in app.wx.active_sigmets() {
            let ring = ring_of(&s["coords"]);
            if ring.len() < 3 {
                continue;
            }
            let c = hazard_color(&str_of(&s["hazard"]));
            polygon(painter, proj, std::slice::from_ref(&ring), c.gamma_multiply(0.14), Stroke::new(2.0, c), None);
            if proj.zoom >= 5.0 {
                let cx = ring.iter().map(|p| p[0]).sum::<f64>() / ring.len() as f64;
                let cy = ring.iter().map(|p| p[1]).sum::<f64>() / ring.len() as f64;
                let label = format!("{} {}", str_of(&s["airSigmetType"]), str_of(&s["hazard"]));
                let g = painter.ctx().fonts_mut(|f| f.layout_job(single(&label, fonts.bold(11.0), c)));
                let p = proj.to_screen(cx, cy) - g.size() / 2.0;
                halo_text(painter, p, g, Color32::WHITE, 1.2, 0.0);
            }
        }
    }
    if app.saved.wx.tfr {
        let red = Color32::from_rgb(0xd2, 0x3a, 0x3a);
        for f in app.wx.tfrs["features"].as_array().into_iter().flatten() {
            for ring in outer_rings(&f["geometry"]) {
                polygon(painter, proj, std::slice::from_ref(&ring), red.gamma_multiply(0.16), Stroke::new(2.2, red), Some((5.0, 3.0)));
                if proj.zoom >= 6.0 && !ring.is_empty() {
                    let cx = ring.iter().map(|p| p[0]).sum::<f64>() / ring.len() as f64;
                    let cy = ring.iter().map(|p| p[1]).sum::<f64>() / ring.len() as f64;
                    let g = painter.ctx().fonts_mut(|fo| fo.layout_job(single(&format!("TFR {}", str_of(&f["properties"]["notam"])), fonts.bold(11.0), red)));
                    halo_text(painter, proj.to_screen(cx, cy) - g.size() / 2.0, g, Color32::WHITE, 1.2, 0.0);
                }
            }
        }
    }
}

fn draw_picked(app: &App, painter: &Painter, proj: &Proj) {
    let Some(i) = app.plan.picked else { return };
    let Some(c) = app.plan.profile.airspace.get(i) else { return };
    let route = app.palette.route.egui();
    for poly in &c.geom {
        polygon(painter, proj, poly, route.gamma_multiply(0.12), Stroke::new(3.0, route), Some((6.0, 3.0)));
    }
}

/// Wind barbs at the chosen pressure level (the winds-aloft layer).
fn draw_winds(app: &App, ui: &egui::Ui, painter: &Painter, proj: &Proj) {
    if !app.saved.wx.winds {
        return;
    }
    let (hpa, _) = WIND_LEVELS[app.saved.wind_level.min(WIND_LEVELS.len() - 1)];
    let ink = if app.night() { Color32::from_rgb(220, 226, 234) } else { Color32::from_rgb(30, 36, 46) };
    let halo = app.palette.label_halo.egui();
    for c in app.wx.wind_cells(hpa, app.saved.route.departure_time()) {
        let p = proj.to_screen(c.lon, c.lat);
        if !proj.rect.contains(p) {
            continue;
        }
        let color = speed_color(c.spd);
        // shaft points toward where the wind comes from; barbs on the left (northern hemisphere)
        let a = (c.dir as f32).to_radians();
        let dir = vec2(a.sin(), -a.cos());
        let tip = p + dir * 28.0;
        let side = vec2(-dir.y, dir.x) * -1.0;
        let mut shapes = vec![Shape::line_segment([p, tip], Stroke::new(2.0, color))];
        let mut kt = (c.spd / 5.0).round() as i32 * 5;
        let mut at = 0.0;
        while kt >= 50 {
            let b = tip - dir * at;
            shapes.push(Shape::convex_polygon(vec![b, b - dir * 6.0, b + side * 11.0], color, Stroke::NONE));
            at += 8.0;
            kt -= 50;
        }
        while kt >= 10 {
            let b = tip - dir * at;
            shapes.push(Shape::line_segment([b, b + side * 11.0 + dir * 3.0], Stroke::new(2.0, color)));
            at += 4.5;
            kt -= 10;
        }
        if kt >= 5 {
            let b = tip - dir * at.max(4.0);
            shapes.push(Shape::line_segment([b, b + side * 6.0 + dir * 1.5], Stroke::new(2.0, color)));
        }
        if c.spd < 3.0 {
            shapes = vec![Shape::circle_stroke(p, 5.0, Stroke::new(2.0, color))];
        }
        painter.circle_filled(p, 2.5, color);
        painter.extend(shapes);
        let label = match c.temp {
            Some(t) => format!("{:03}/{} {:+.0}°", c.dir.round() as i64, c.spd.round(), t),
            None => format!("{:03}/{}", c.dir.round() as i64, c.spd.round()),
        };
        let g = ui.fonts_mut(|f| f.layout_job(single(&label, crate::theme::bold(10.0), ink)));
        halo_text(painter, p + vec2(6.0, 4.0), g, halo, 1.2, 0.0);
    }
}

pub fn speed_color(kt: f64) -> Color32 {
    let stops = [(0.0, (90, 160, 210)), (15.0, (60, 180, 120)), (30.0, (230, 180, 40)), (50.0, (230, 110, 40)), (80.0, (200, 40, 80))];
    for w in stops.windows(2) {
        let (a, ca) = w[0];
        let (b, cb) = w[1];
        if kt <= b {
            let f = ((kt - a) / (b - a)).clamp(0.0, 1.0);
            let l = |x: u8, y: u8| (x as f64 + (y as f64 - x as f64) * f) as u8;
            return Color32::from_rgb(l(ca.0, cb.0), l(ca.1, cb.1), l(ca.2, cb.2));
        }
    }
    Color32::from_rgb(200, 40, 80)
}

fn draw_route(app: &App, ui: &egui::Ui, painter: &Painter, proj: &Proj) {
    let route = &app.saved.route;
    let w = &route.waypoints;
    let rc = app.palette.route.egui();
    let fonts = Fonts;
    if w.len() >= 2 {
        let line = route_screen(app, proj);
        painter.add(Shape::line(line.clone(), Stroke::new(8.0, Color32::from_white_alpha(215))));
        painter.add(Shape::line(line, Stroke::new(4.0, rc)));
        // leg labels: magnetic course and distance, upright along the line
        for i in 1..w.len() {
            let (a, b) = (&w[i - 1], &w[i]);
            let d = distance_nm(a.lat, a.lon, b.lat, b.lon);
            let pa = proj.to_screen(a.lon, a.lat);
            let pb = proj.to_screen(b.lon, b.lat);
            if d < 3.0 || pa.distance(pb) < 90.0 {
                continue;
            }
            let (mla, mlo) = interpolate(a.lat, a.lon, b.lat, b.lon, 0.5);
            let tc = bearing(mla, mlo, b.lat, b.lon);
            let mc = tc - variation_now(mla, mlo);
            let text = format!("{}°  {} NM", deg(mc), d.round());
            let g = ui.fonts_mut(|f| f.layout_job(single(&text, fonts.bold(11.5), rc)));
            let mut angle = (pb.y - pa.y).atan2(pb.x - pa.x);
            if angle > std::f32::consts::FRAC_PI_2 {
                angle -= std::f32::consts::PI;
            }
            if angle < -std::f32::consts::FRAC_PI_2 {
                angle += std::f32::consts::PI;
            }
            let mid = proj.to_screen(mlo, mla);
            let rot = egui::emath::Rot2::from_angle(angle);
            let up = rot * vec2(0.0, -14.0);
            let tl = mid + up - rot * (g.size() / 2.0);
            halo_text(painter, tl, g, Color32::WHITE, 1.6, angle);
        }
        // TOC / TOD
        if let Some(log) = &app.plan.log {
            for (d, label) in [(log.toc_dist, "TOC"), (log.tod_dist, "TOD")] {
                let Some(d) = d.filter(|d| *d > 0.5 && *d < log.total_dist - 0.5) else { continue };
                let p = point_along(&route.path(), d);
                let s = proj.to_screen(p[0], p[1]);
                painter.circle(s, 5.0, Color32::WHITE, Stroke::new(2.5, rc));
                let g = ui.fonts_mut(|f| f.layout_job(single(label, fonts.bold(10.0), rc)));
                halo_text(painter, s + vec2(-g.size().x / 2.0, 9.0), g, Color32::WHITE, 1.6, 0.0);
            }
        }
    }
    for p in w.iter() {
        let s = proj.to_screen(p.lon, p.lat);
        painter.circle(s, 7.0, rc, Stroke::new(2.5, Color32::WHITE));
        let g = ui.fonts_mut(|f| f.layout_job(single(&p.id, fonts.bold(12.0), Color32::WHITE)));
        halo_text(painter, s + vec2(11.0, -g.size().y / 2.0), g, rc, 2.2, 0.0);
    }
    // hover marker shared with the profile
    if let Some((d, _)) = app.plan.hover {
        if let Some(s) = app.plan.profile.samples.iter().min_by(|a, b| (a.d - d).abs().total_cmp(&(b.d - d).abs())) {
            let p = proj.to_screen(s.lon, s.lat);
            painter.circle(p, 8.0, Color32::WHITE, Stroke::new(3.5, rc));
        }
    }
}

fn draw_dots(app: &App, ui: &egui::Ui, painter: &Painter, proj: &Proj) {
    let fonts = Fonts;
    let z = proj.zoom as f32;
    if app.saved.wx.pireps {
        for r in &app.wx.pireps {
            let (Some(lon), Some(lat)) = (r["lon"].as_f64(), r["lat"].as_f64()) else { continue };
            let p = proj.to_screen(lon, lat);
            if !proj.rect.contains(p) {
                continue;
            }
            let c = pirep_color(r);
            painter.circle(p, 5.0, c, Stroke::new(1.5, Color32::WHITE));
            if z >= 6.0 {
                let ice = str_of(&r["icgInt1"]);
                let turb = str_of(&r["tbInt1"]);
                let mut what = Vec::new();
                if !turb.is_empty() && turb != "NEG" {
                    what.push(format!("TB {turb}"));
                }
                if !ice.is_empty() && ice != "NEG" {
                    what.push(format!("IC {ice}"));
                }
                let label = format!("FL{:03} {}", r["fltLvl"].as_i64().unwrap_or(0), what.join(" "));
                let g = ui.fonts_mut(|f| f.layout_job(single(label.trim(), fonts.bold(10.0), c)));
                halo_text(painter, p + vec2(-g.size().x / 2.0, 8.0), g, Color32::WHITE, 1.2, 0.0);
            }
        }
    }
    if app.saved.wx.metars && z >= 4.5 {
        let r = lin(z, &[(4.0, 3.5), (9.0, 7.0)]);
        for m in &app.wx.metars {
            let (Some(lon), Some(lat)) = (m["lon"].as_f64(), m["lat"].as_f64()) else { continue };
            let p = proj.to_screen(lon, lat);
            if !proj.rect.contains(p) {
                continue;
            }
            let c = crate::theme::flt_cat(&str_of(&m["fltCat"]));
            painter.circle(p, r, c, Stroke::new(1.5, Color32::WHITE));
            if z >= 9.5 {
                let wind = match (m["wspd"].as_f64(), &m["wdir"]) {
                    (Some(s), d) => {
                        let dir = if d.as_str() == Some("VRB") { "VRB".to_string() } else { format!("{:03}", d.as_f64().unwrap_or(0.0) as i64) };
                        let g = m["wgst"].as_f64().map(|g| format!("G{g}")).unwrap_or_default();
                        format!(" {dir}@{s}{g}")
                    }
                    _ => String::new(),
                };
                let gal = ui.fonts_mut(|f| f.layout_job(single(&format!("{}{wind}", str_of(&m["icaoId"])), fonts.bold(10.0), c)));
                halo_text(painter, p + vec2(r + 2.0, -r - gal.size().y + 2.0), gal, Color32::WHITE, 1.2, 0.0);
            }
        }
    }
}

fn draw_scale(app: &App, painter: &Painter, proj: &Proj) {
    // nautical-mile scale bar, bottom right
    let (lon, lat) = proj.from_screen(proj.rect.center());
    let a = lonlat_to_world(lon, lat);
    let nm_per_pt = distance_nm(lat, lon, lat, lon + 1.0) / ((lonlat_to_world(lon + 1.0, lat)[0] - a[0]) * proj.size);
    let target = 90.0 * nm_per_pt;
    let nice = [1.0, 2.0, 5.0, 10.0, 20.0, 25.0, 50.0, 100.0, 200.0, 500.0].into_iter().filter(|v| *v <= target).last().unwrap_or(1.0);
    let len = (nice / nm_per_pt) as f32;
    let y = proj.rect.bottom() - 14.0;
    let x1 = proj.rect.right() - 16.0;
    let x0 = x1 - len;
    let t = &app.t;
    let profile_up = app.saved.profile_open && app.saved.route.waypoints.len() >= 2;
    if profile_up {
        return;
    }
    painter.line_segment([pos2(x0, y), pos2(x1, y)], Stroke::new(1.5, t.text2));
    painter.line_segment([pos2(x0, y - 4.0), pos2(x0, y)], Stroke::new(1.5, t.text2));
    painter.line_segment([pos2(x1, y - 4.0), pos2(x1, y)], Stroke::new(1.5, t.text2));
    painter.text(pos2((x0 + x1) / 2.0, y - 3.0), egui::Align2::CENTER_BOTTOM, format!("{nice} NM"), crate::theme::regular(10.0), t.text2);
    let _ = (Group::Terrain, Duration::ZERO, Instant::now());
}
