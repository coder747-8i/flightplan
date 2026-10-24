//! Aircraft profiles: performance for the navlog, weight & balance data for the loading sheet.
//! The JSON shape matches the web app's (camelCase), so aircraft files move between them.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StationKind {
    Seat,
    Baggage,
    Fuel,
    Other,
}

impl StationKind {
    pub const ALL: [StationKind; 4] = [StationKind::Seat, StationKind::Baggage, StationKind::Fuel, StationKind::Other];
    pub fn label(self) -> &'static str {
        match self {
            StationKind::Seat => "Seat",
            StationKind::Baggage => "Baggage",
            StationKind::Fuel => "Fuel",
            StationKind::Other => "Other",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Station {
    pub id: String,
    pub name: String,
    pub arm: f64,
    pub kind: StationKind,
    /// lb, or gallons for fuel
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Perf {
    pub tas: f64,
    pub gph: f64,
    pub climb_fpm: f64,
    pub climb_tas: f64,
    pub climb_gph: f64,
    pub descent_fpm: f64,
    pub descent_tas: f64,
    pub descent_gph: f64,
    pub taxi_gal: f64,
    pub reserve_min: f64,
    pub max_xwind: f64,
}

impl Default for Perf {
    fn default() -> Self {
        Perf { tas: 110.0, gph: 8.0, climb_fpm: 600.0, climb_tas: 75.0, climb_gph: 9.0, descent_fpm: 500.0, descent_tas: 110.0, descent_gph: 6.0, taxi_gal: 1.0, reserve_min: 30.0, max_xwind: 15.0 }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WeightBalance {
    pub empty_weight: f64,
    pub empty_arm: f64,
    pub max_ramp: f64,
    pub max_takeoff: f64,
    pub max_landing: f64,
    pub stations: Vec<Station>,
    /// CG envelope polygon, [arm, weight] points in order
    pub envelope: Vec<[f64; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub utility: Option<Vec<[f64; 2]>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Aircraft {
    pub id: String,
    pub name: String,
    pub tail: String,
    #[serde(rename = "type")]
    pub type_code: String,
    pub fuel_type: String,
    pub lb_per_gal: f64,
    pub perf: Perf,
    pub usable_fuel: f64,
    pub wb: WeightBalance,
    /// loading for the current flight: station id -> lb (fuel station: gallons)
    #[serde(default)]
    pub load: BTreeMap<String, f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

pub const FUEL_TYPES: [&str; 4] = ["100LL", "UL94", "Mogas", "Jet A"];

pub fn uid() -> String {
    format!("{:x}", crate::model::place::new_uid() & 0xffff_ffff_ffff)
}

fn st(id: &str, name: &str, arm: f64, kind: StationKind, max: Option<f64>) -> Station {
    Station { id: id.into(), name: name.into(), arm, kind, max }
}

/// Typical POH figures. Every aircraft differs: always enter your own weighing data.
pub fn presets() -> Vec<Aircraft> {
    use StationKind::*;
    let mk = |name: &str, type_code: &str, perf: Perf, usable: f64, wb: WeightBalance| Aircraft {
        id: String::new(),
        name: name.into(),
        tail: String::new(),
        type_code: type_code.into(),
        fuel_type: "100LL".into(),
        lb_per_gal: 6.0,
        perf,
        usable_fuel: usable,
        wb,
        load: BTreeMap::new(),
        notes: None,
    };
    vec![
        mk(
            "Cessna 172S Skyhawk",
            "C172",
            Perf { tas: 120.0, gph: 8.5, climb_fpm: 700.0, climb_tas: 75.0, climb_gph: 10.5, descent_fpm: 500.0, descent_tas: 120.0, descent_gph: 7.0, taxi_gal: 1.1, reserve_min: 30.0, max_xwind: 15.0 },
            53.0,
            WeightBalance {
                empty_weight: 1663.0,
                empty_arm: 41.0,
                max_ramp: 2558.0,
                max_takeoff: 2550.0,
                max_landing: 2550.0,
                stations: vec![
                    st("front", "Pilot & front passenger", 37.0, Seat, None),
                    st("rear", "Rear passengers", 73.0, Seat, None),
                    st("bagA", "Baggage area A", 95.0, Baggage, Some(120.0)),
                    st("bagB", "Baggage area B", 123.0, Baggage, Some(50.0)),
                    st("fuel", "Fuel (usable)", 48.0, Fuel, Some(53.0)),
                ],
                envelope: vec![[35.0, 1500.0], [35.0, 1950.0], [41.0, 2550.0], [47.3, 2550.0], [47.3, 1500.0]],
                utility: Some(vec![[35.0, 1500.0], [35.0, 1950.0], [37.5, 2200.0], [40.5, 2200.0], [40.5, 1500.0]]),
            },
        ),
        mk(
            "Piper PA-28-181 Archer",
            "P28A",
            Perf { tas: 125.0, gph: 9.5, climb_fpm: 650.0, climb_tas: 76.0, climb_gph: 11.0, descent_fpm: 500.0, descent_tas: 125.0, descent_gph: 7.5, taxi_gal: 1.0, reserve_min: 30.0, max_xwind: 17.0 },
            48.0,
            WeightBalance {
                empty_weight: 1650.0,
                empty_arm: 86.0,
                max_ramp: 2558.0,
                max_takeoff: 2550.0,
                max_landing: 2550.0,
                stations: vec![
                    st("front", "Pilot & front passenger", 80.5, Seat, None),
                    st("rear", "Rear passengers", 118.1, Seat, None),
                    st("bag", "Baggage", 142.8, Baggage, Some(200.0)),
                    st("fuel", "Fuel (usable)", 95.0, Fuel, Some(48.0)),
                ],
                envelope: vec![[82.0, 1200.0], [82.0, 2050.0], [88.6, 2550.0], [93.0, 2550.0], [93.0, 1200.0]],
                utility: None,
            },
        ),
        mk(
            "Cessna 152",
            "C152",
            Perf { tas: 95.0, gph: 6.1, climb_fpm: 715.0, climb_tas: 67.0, climb_gph: 7.5, descent_fpm: 500.0, descent_tas: 95.0, descent_gph: 5.0, taxi_gal: 0.8, reserve_min: 30.0, max_xwind: 12.0 },
            24.5,
            WeightBalance {
                empty_weight: 1129.0,
                empty_arm: 30.7,
                max_ramp: 1675.0,
                max_takeoff: 1670.0,
                max_landing: 1670.0,
                stations: vec![
                    st("front", "Pilot & passenger", 39.0, Seat, None),
                    st("bag1", "Baggage area 1", 64.0, Baggage, Some(120.0)),
                    st("bag2", "Baggage area 2", 84.0, Baggage, Some(40.0)),
                    st("fuel", "Fuel (usable)", 42.0, Fuel, Some(24.5)),
                ],
                envelope: vec![[31.0, 1100.0], [31.0, 1350.0], [32.65, 1670.0], [36.5, 1670.0], [36.5, 1100.0]],
                utility: None,
            },
        ),
    ]
}

/// A template made ready to fly: new id, tail, and a default loading (pilot + full fuel).
pub fn from_preset(mut a: Aircraft, tail: &str) -> Aircraft {
    a.id = uid();
    a.tail = tail.into();
    a.load = default_load(&a);
    a
}

pub fn default_load(a: &Aircraft) -> BTreeMap<String, f64> {
    let mut load = BTreeMap::new();
    let first_seat = a.wb.stations.iter().find(|s| s.kind == StationKind::Seat).map(|s| s.id.clone());
    for s in &a.wb.stations {
        let v = match s.kind {
            StationKind::Fuel => s.max.unwrap_or(a.usable_fuel),
            _ if Some(&s.id) == first_seat.as_ref() => 170.0,
            _ => 0.0,
        };
        load.insert(s.id.clone(), v);
    }
    load
}

pub fn in_polygon(x: f64, y: f64, poly: &[[f64; 2]]) -> bool {
    crate::nav::geo::in_ring(x, y, poly)
}

#[derive(Clone, Debug, Default)]
pub struct WbPoint {
    pub weight: f64,
    pub cg: f64,
}

#[derive(Clone, Debug)]
pub struct WbRow {
    pub name: String,
    pub weight: f64,
    pub arm: f64,
    pub moment: f64,
}

#[derive(Clone, Debug)]
pub struct WbResult {
    pub rows: Vec<WbRow>,
    pub ramp: WbPoint,
    pub takeoff: WbPoint,
    pub landing: WbPoint,
    pub zero_fuel: WbPoint,
    pub problems: Vec<String>,
}

/// Weight & balance for ramp, takeoff (after taxi fuel), zero-fuel and landing (after trip fuel).
pub fn compute_wb(a: &Aircraft, trip_gal: f64) -> WbResult {
    let mut rows = vec![WbRow { name: "Basic empty weight".into(), weight: a.wb.empty_weight, arm: a.wb.empty_arm, moment: a.wb.empty_weight * a.wb.empty_arm }];
    let mut fuel_arm = 0.0;
    let mut fuel_gal = 0.0;
    let mut problems = Vec::new();
    for s in &a.wb.stations {
        let v = a.load.get(&s.id).copied().unwrap_or(0.0);
        if s.kind == StationKind::Fuel {
            fuel_arm = s.arm;
            fuel_gal += v;
            if let Some(m) = s.max.filter(|m| v > *m) {
                problems.push(format!("{}: {} gal exceeds {} gal capacity", s.name, fmt_num(v), fmt_num(m)));
            }
            continue;
        }
        if let Some(m) = s.max.filter(|m| v > *m) {
            problems.push(format!("{}: {} lb exceeds the {} lb limit", s.name, fmt_num(v), fmt_num(m)));
        }
        if v != 0.0 {
            rows.push(WbRow { name: s.name.clone(), weight: v, arm: s.arm, moment: v * s.arm });
        }
    }
    let sum = |r: &[WbRow]| {
        let w: f64 = r.iter().map(|x| x.weight).sum();
        let m: f64 = r.iter().map(|x| x.moment).sum();
        WbPoint { weight: w, cg: if w != 0.0 { m / w } else { 0.0 } }
    };
    let zf = sum(&rows);
    let with_fuel = |gal: f64| {
        let w = gal * a.lb_per_gal;
        let mut r = rows.clone();
        r.push(WbRow { name: "Fuel".into(), weight: w, arm: fuel_arm, moment: w * fuel_arm });
        sum(&r)
    };
    let ramp = with_fuel(fuel_gal);
    let takeoff = with_fuel((fuel_gal - a.perf.taxi_gal).max(0.0));
    let landing = with_fuel((fuel_gal - a.perf.taxi_gal - trip_gal).max(0.0));
    rows.push(WbRow { name: format!("Fuel {} gal", fmt_num(fuel_gal)), weight: fuel_gal * a.lb_per_gal, arm: fuel_arm, moment: fuel_gal * a.lb_per_gal * fuel_arm });
    if ramp.weight > a.wb.max_ramp {
        problems.push(format!("Ramp weight {} lb is over the {} lb maximum", ramp.weight.round(), fmt_num(a.wb.max_ramp)));
    }
    if takeoff.weight > a.wb.max_takeoff {
        problems.push(format!("Takeoff weight {} lb is over the {} lb maximum", takeoff.weight.round(), fmt_num(a.wb.max_takeoff)));
    }
    if landing.weight > a.wb.max_landing {
        problems.push(format!("Landing weight {} lb is over the {} lb maximum", landing.weight.round(), fmt_num(a.wb.max_landing)));
    }
    for (label, pt) in [("Takeoff", &takeoff), ("Landing", &landing), ("Zero fuel", &zf)] {
        if !in_polygon(pt.cg, pt.weight, &a.wb.envelope) {
            problems.push(format!("{label} CG {:.2} in at {} lb is outside the envelope", pt.cg, pt.weight.round()));
        }
    }
    WbResult { rows, ramp, takeoff, landing, zero_fuel: zf, problems }
}

/// Numbers without a trailing ".0".
pub fn fmt_num(v: f64) -> String {
    if (v - v.round()).abs() < 1e-9 { format!("{}", v.round() as i64) } else { format!("{}", (v * 100.0).round() / 100.0) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c172_default_loading_is_in_limits() {
        let a = from_preset(presets()[0].clone(), "N1");
        let wb = compute_wb(&a, 10.0);
        assert!(wb.problems.is_empty(), "{:?}", wb.problems);
        assert!((wb.takeoff.weight - (1663.0 + 170.0 + (53.0 - 1.1) * 6.0)).abs() < 0.01);
    }

    #[test]
    fn web_json_roundtrip() {
        let a = from_preset(presets()[2].clone(), "N4821B");
        let v = serde_json::to_value(&a).unwrap();
        assert_eq!(v["type"], "C152");
        assert!(v["wb"]["emptyWeight"].is_number());
        let back: Aircraft = serde_json::from_value(v).unwrap();
        assert_eq!(back, a);
    }
}
