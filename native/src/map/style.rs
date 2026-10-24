//! Chart colours and the user-switchable layer groups. Day follows the sectional's conventions
//! (blue = towered / Class B & D, magenta = non-towered / Class C & E) in a calmer, flatter
//! tone; Night is the same map inverted.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba(pub u8, pub u8, pub u8, pub u8);

const fn hex(v: u32) -> Rgba {
    Rgba((v >> 16) as u8, (v >> 8) as u8, v as u8, 255)
}

impl Rgba {
    pub fn alpha(self, a: f32) -> Rgba {
        Rgba(self.0, self.1, self.2, (self.3 as f32 * a.clamp(0.0, 1.0)).round() as u8)
    }
    pub fn egui(self) -> egui::Color32 {
        egui::Color32::from_rgba_unmultiplied(self.0, self.1, self.2, self.3)
    }
    pub fn skia(self) -> tiny_skia::Color {
        tiny_skia::Color::from_rgba8(self.0, self.1, self.2, self.3)
    }
}

#[derive(Clone, Debug)]
pub struct Palette {
    pub night: bool,
    pub background: Rgba,
    pub water: Rgba,
    pub water_line: Rgba,
    pub urban: Rgba,
    pub road: Rgba,
    pub road_major: Rgba,
    pub border: Rgba,
    pub label: Rgba,
    pub label_halo: Rgba,
    pub hill_shadow: Rgba,
    pub hill_highlight: Rgba,
    /// metres -> colour
    pub relief: Vec<(f32, Rgba)>,
    pub towered: Rgba,
    pub nontowered: Rgba,
    pub military: Rgba,
    pub navaid: Rgba,
    pub ndb: Rgba,
    pub obstacle: Rgba,
    pub class_b: Rgba,
    pub class_c: Rgba,
    pub class_d: Rgba,
    pub class_e: Rgba,
    pub restricted: Rgba,
    pub moa: Rgba,
    pub warning: Rgba,
    pub alert: Rgba,
    pub airway: Rgba,
    pub runway: Rgba,
    pub runway_soft: Rgba,
    pub symbol_halo: Rgba,
    pub route: Rgba,
}

pub fn day() -> Palette {
    Palette {
        night: false,
        background: hex(0xf4f2ea),
        water: hex(0xbcd8ec),
        water_line: hex(0x8fbbdb),
        urban: hex(0xf5e7a8),
        road: hex(0xd9d1c4),
        road_major: hex(0xc4b5a5),
        border: hex(0x9a8f9e),
        label: hex(0x3d3a36),
        label_halo: Rgba(250, 248, 242, 230),
        hill_shadow: hex(0x5c5246),
        hill_highlight: hex(0xffffff),
        relief: vec![
            (-50.0, hex(0xeaf0dd)),
            (0.0, hex(0xe8efd9)),
            (300.0, hex(0xeef1d6)),
            (700.0, hex(0xf2eed2)),
            (1200.0, hex(0xefe4c4)),
            (1800.0, hex(0xe8d7b4)),
            (2500.0, hex(0xdcc6a1)),
            (3200.0, hex(0xd2b994)),
            (4200.0, hex(0xe6ddd2)),
        ],
        towered: hex(0x2c63b8),
        nontowered: hex(0xa33a86),
        military: hex(0x5b5f66),
        navaid: hex(0x2c63b8),
        ndb: hex(0xa33a86),
        obstacle: hex(0x2b2b2b),
        class_b: hex(0x2c63b8),
        class_c: hex(0xa33a86),
        class_d: hex(0x2c63b8),
        class_e: hex(0xa33a86),
        restricted: hex(0x2c63b8),
        moa: hex(0xa33a86),
        warning: hex(0x2c63b8),
        alert: hex(0xa33a86),
        airway: hex(0x6f8fbf),
        runway: hex(0x4b4b4b),
        runway_soft: hex(0x8a7a5c),
        symbol_halo: hex(0xffffff),
        route: hex(0xd4148c),
    }
}

pub fn night() -> Palette {
    Palette {
        night: true,
        background: hex(0x151a21),
        water: hex(0x1d3247),
        water_line: hex(0x2a4a68),
        urban: hex(0x2c2a22),
        road: hex(0x2b3139),
        road_major: hex(0x3b434e),
        border: hex(0x56506a),
        label: hex(0xc9ced6),
        label_halo: Rgba(16, 20, 26, 230),
        hill_shadow: hex(0x000000),
        hill_highlight: hex(0x6d7a8a),
        relief: vec![(-50.0, hex(0x161c22)), (0.0, hex(0x171e24)), (700.0, hex(0x1b2126)), (1500.0, hex(0x211f1d)), (2500.0, hex(0x272320)), (4200.0, hex(0x2e2c2b))],
        towered: hex(0x6ea3ff),
        nontowered: hex(0xe77fd0),
        military: hex(0xa3a8b0),
        navaid: hex(0x6ea3ff),
        ndb: hex(0xe77fd0),
        obstacle: hex(0xe8e8e8),
        class_b: hex(0x6ea3ff),
        class_c: hex(0xe77fd0),
        class_d: hex(0x6ea3ff),
        class_e: hex(0xe77fd0),
        restricted: hex(0x6ea3ff),
        moa: hex(0xe77fd0),
        warning: hex(0x6ea3ff),
        alert: hex(0xe77fd0),
        airway: hex(0x5a77a3),
        runway: hex(0xc9c9c9),
        runway_soft: hex(0x9c8d6e),
        symbol_halo: hex(0x151a21),
        route: hex(0xff3fb4),
    }
}

impl Palette {
    pub fn relief_at(&self, m: f32) -> Rgba {
        let r = &self.relief;
        if m <= r[0].0 {
            return r[0].1;
        }
        for w in r.windows(2) {
            let (a, ca) = w[0];
            let (b, cb) = w[1];
            if m <= b {
                let f = (m - a) / (b - a);
                let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * f).round() as u8;
                return Rgba(l(ca.0, cb.0), l(ca.1, cb.1), l(ca.2, cb.2), 255);
            }
        }
        r[r.len() - 1].1
    }

    /// special-use airspace colour by kind
    pub fn sua(&self, kind: &str) -> Rgba {
        match kind {
            "MOA" => self.moa,
            "A" => self.alert,
            "W" => self.warning,
            _ => self.restricted,
        }
    }
}

/// Layer groups the user can toggle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum Group {
    Terrain,
    Basemap,
    ClassB,
    ClassC,
    ClassD,
    ClassE,
    ClassEhigh,
    ModeC,
    Restricted,
    Moa,
    WarningAlert,
    AirspaceLabels,
    AirportsPublic,
    AirportsPrivate,
    Heliports,
    Runways,
    Navaids,
    VfrWaypoints,
    IfrFixes,
    AirwaysLow,
    AirwaysHigh,
    Obstacles,
}

impl Group {
    pub fn default_on(self) -> bool {
        !matches!(self, Group::ClassEhigh | Group::ModeC | Group::Heliports | Group::IfrFixes | Group::AirwaysLow | Group::AirwaysHigh)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Layers(pub std::collections::BTreeMap<Group, bool>);

impl Default for Layers {
    fn default() -> Self {
        use Group::*;
        Layers(
            [Terrain, Basemap, ClassB, ClassC, ClassD, ClassE, ClassEhigh, ModeC, Restricted, Moa, WarningAlert, AirspaceLabels, AirportsPublic, AirportsPrivate, Heliports, Runways, Navaids, VfrWaypoints, IfrFixes, AirwaysLow, AirwaysHigh, Obstacles]
                .into_iter()
                .map(|g| (g, g.default_on()))
                .collect(),
        )
    }
}

impl Layers {
    pub fn on(&self, g: Group) -> bool {
        self.0.get(&g).copied().unwrap_or_else(|| g.default_on())
    }
    pub fn set(&mut self, g: Group, v: bool) {
        self.0.insert(g, v);
    }

    /// Is an airspace volume of this kind shown?
    pub fn airspace_kind(&self, kind: &str, lo_agl: bool, lo: f64) -> bool {
        match kind {
            "B" => self.on(Group::ClassB),
            "C" => self.on(Group::ClassC),
            "D" => self.on(Group::ClassD),
            "MODEC" => self.on(Group::ModeC),
            "E6" => self.on(Group::ClassEhigh),
            "E5" if lo_agl && lo > 700.0 => self.on(Group::ClassEhigh),
            k if k.starts_with('E') => self.on(Group::ClassE),
            "MOA" => self.on(Group::Moa),
            "W" | "A" => self.on(Group::WarningAlert),
            _ => self.on(Group::Restricted),
        }
    }
}

/// Garmin-style altitude band; when enabled only airspace overlapping it is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct AltitudeFilter {
    pub enabled: bool,
    pub lo: f64,
    pub hi: f64,
}

impl Default for AltitudeFilter {
    fn default() -> Self {
        AltitudeFilter { enabled: false, lo: 0.0, hi: 12500.0 }
    }
}

impl AltitudeFilter {
    pub fn passes(&self, lo: f64, hi: f64) -> bool {
        !self.enabled || (lo <= self.hi && hi >= self.lo)
    }
}

/// MapLibre-style exponential zoom interpolation over (zoom, value) stops.
pub fn interp_exp(z: f32, base: f32, stops: &[(f32, f32)]) -> f32 {
    if z <= stops[0].0 {
        return stops[0].1;
    }
    for w in stops.windows(2) {
        let (z0, v0) = w[0];
        let (z1, v1) = w[1];
        if z <= z1 {
            let t = if (base - 1.0).abs() < 1e-6 { (z - z0) / (z1 - z0) } else { (base.powf(z - z0) - 1.0) / (base.powf(z1 - z0) - 1.0) };
            return v0 + (v1 - v0) * t;
        }
    }
    stops[stops.len() - 1].1
}

pub fn zw(z: f32, stops: &[(f32, f32)]) -> f32 {
    interp_exp(z, 1.5, stops)
}

pub fn lin(z: f32, stops: &[(f32, f32)]) -> f32 {
    interp_exp(z, 1.0, stops)
}
