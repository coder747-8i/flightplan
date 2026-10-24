//! Navlog: wind triangle per leg, climb to TOC and descent from TOD, fuel and times.

use super::geo::{bearing, distance_nm, interpolate};
use super::magvar::variation;
use crate::model::aircraft::Aircraft;
use crate::model::place::Waypoint;
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;

/// Winds/temperatures aloft at one point, as the server returns them.
#[derive(Clone, Debug, Deserialize, Default)]
pub struct WindPoint {
    pub lat: f64,
    pub lon: f64,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub levels: Vec<WindLevel>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct WindLevel {
    pub p: f64,
    pub alt: f64,
    #[serde(default)]
    pub times: Vec<WindTime>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct WindTime {
    pub t: i64,
    pub dir: f64,
    pub spd: f64,
    #[serde(default)]
    pub temp: Option<f64>,
    #[serde(default)]
    pub alt: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Wind {
    pub dir: f64,
    pub spd: f64,
    pub temp: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Climb,
    Cruise,
    Descent,
}

impl Phase {
    pub fn label(self) -> &'static str {
        match self {
            Phase::Climb => "climb",
            Phase::Cruise => "cruise",
            Phase::Descent => "descent",
        }
    }
}

#[derive(Clone, Debug)]
pub struct NavRow {
    pub from: String,
    pub to: String,
    pub phase: Phase,
    pub alt: f64,
    pub tc: f64,
    pub mc: f64,
    pub variation: f64,
    pub wind: Option<Wind>,
    pub wca: f64,
    pub th: f64,
    pub mh: f64,
    pub tas: f64,
    pub gs: f64,
    pub dist: f64,
    /// minutes
    pub ete: f64,
    /// gallons
    pub fuel: f64,
    pub cum_dist: f64,
    pub cum_time: f64,
    pub eta: DateTime<Utc>,
    pub fuel_rem: f64,
    pub lat: f64,
    pub lon: f64,
}

#[derive(Clone, Debug, Default)]
pub struct NavLog {
    pub rows: Vec<NavRow>,
    pub total_dist: f64,
    pub total_time: f64,
    pub trip_fuel: f64,
    pub taxi_fuel: f64,
    pub reserve_fuel: f64,
    pub fuel_at_landing: f64,
    pub toc_dist: Option<f64>,
    pub tod_dist: Option<f64>,
    pub warnings: Vec<String>,
}

pub fn norm(d: f64) -> f64 {
    ((d % 360.0) + 360.0) % 360.0
}

/// Wind at altitude (ft) from a point's level data, at the hour nearest `when`.
pub fn wind_at(wp: Option<&WindPoint>, alt_ft: f64, when: DateTime<Utc>) -> Option<Wind> {
    let wp = wp?;
    if wp.error.is_some() || wp.levels.is_empty() {
        return None;
    }
    let t = when.timestamp();
    let mut per_level: Vec<(f64, Wind)> = wp
        .levels
        .iter()
        .filter_map(|l| {
            let best = l.times.iter().min_by_key(|x| (x.t - t).abs())?;
            Some((best.alt.unwrap_or(l.alt), Wind { dir: best.dir, spd: best.spd, temp: best.temp }))
        })
        .collect();
    per_level.sort_by(|a, b| a.0.total_cmp(&b.0));
    let first = per_level.first()?;
    if alt_ft <= first.0 {
        return Some(first.1);
    }
    for i in 1..per_level.len() {
        let (a_alt, a) = per_level[i - 1];
        let (b_alt, b) = per_level[i];
        if alt_ft <= b_alt {
            let f = (alt_ft - a_alt) / (b_alt - a_alt);
            // interpolate as vectors so 350 and 010 average to 000
            let (ax, ay) = (a.spd * a.dir.to_radians().sin(), a.spd * a.dir.to_radians().cos());
            let (bx, by) = (b.spd * b.dir.to_radians().sin(), b.spd * b.dir.to_radians().cos());
            let (x, y) = (ax + (bx - ax) * f, ay + (by - ay) * f);
            let temp = match (a.temp, b.temp) {
                (Some(p), Some(q)) => Some(p + (q - p) * f),
                _ => None,
            };
            return Some(Wind { dir: norm(x.atan2(y).to_degrees()).round(), spd: x.hypot(y).round(), temp });
        }
    }
    per_level.last().map(|p| p.1)
}

pub struct Triangle {
    pub wca: f64,
    pub th: f64,
    pub gs: f64,
}

/// Wind triangle. Wind direction is where it blows FROM (true).
pub fn triangle(tc: f64, tas: f64, w: Option<Wind>) -> Triangle {
    let Some(w) = w.filter(|w| w.spd != 0.0) else { return Triangle { wca: 0.0, th: tc, gs: tas } };
    let rel = (w.dir - tc).to_radians();
    let s = w.spd * rel.sin() / tas;
    if s.abs() >= 1.0 {
        return Triangle { wca: 0.0, th: tc, gs: (tas - w.spd).max(1.0) };
    }
    let wca = s.asin().to_degrees();
    let gs = tas * wca.to_radians().cos() - w.spd * rel.cos();
    Triangle { wca, th: norm(tc + wca), gs: gs.max(1.0) }
}

pub struct NavInput<'a> {
    pub waypoints: &'a [Waypoint],
    pub aircraft: &'a Aircraft,
    pub cruise: f64,
    pub departure: DateTime<Utc>,
    pub dep_elev: f64,
    pub dest_elev: f64,
    /// winds per waypoint index
    pub winds: &'a [Option<WindPoint>],
}

/// Builds the log. Climb starts at the departure field, descent ends 1,000 ft above the
/// destination; both are spread over legs as needed (TOC/TOD may fall mid-leg).
pub fn build(inp: &NavInput) -> NavLog {
    let w = inp.waypoints;
    let ac = inp.aircraft;
    let p = &ac.perf;
    let cruise = inp.cruise;
    let mut warnings = Vec::new();
    let reserve_fuel = p.gph * p.reserve_min / 60.0;
    if w.len() < 2 {
        return NavLog { taxi_fuel: p.taxi_gal, reserve_fuel, ..Default::default() };
    }
    struct Leg<'a> {
        a: &'a Waypoint,
        b: &'a Waypoint,
        dist: f64,
    }
    let legs: Vec<Leg> = w.windows(2).map(|x| Leg { a: &x[0], b: &x[1], dist: distance_nm(x[0].lat, x[0].lon, x[1].lat, x[1].lon) }).collect();
    let total: f64 = legs.iter().map(|l| l.dist).sum();

    let climb_ft = (cruise - inp.dep_elev).max(0.0);
    let desc_ft = (cruise - (inp.dest_elev + 1000.0)).max(0.0);
    let climb_min = climb_ft / p.climb_fpm.max(1.0);
    let desc_min = desc_ft / p.descent_fpm.max(1.0);
    let mut toc_d = climb_min / 60.0 * p.climb_tas;
    let mut tod_from_end = desc_min / 60.0 * p.descent_tas;
    if toc_d + tod_from_end > total {
        warnings.push(format!("Route is too short to reach {} ft; climb and descent overlap", thousands(cruise)));
        let k = total / (toc_d + tod_from_end);
        toc_d *= k;
        tod_from_end *= k;
    }
    let tod_d = total - tod_from_end;

    let fuel_station = ac.wb.stations.iter().find(|s| s.kind == crate::model::aircraft::StationKind::Fuel);
    let loaded = fuel_station.and_then(|s| ac.load.get(&s.id).copied()).unwrap_or(ac.usable_fuel);
    let fuel_start = loaded - p.taxi_gal;
    let mut rows: Vec<NavRow> = Vec::new();
    let mut cum_d = 0.0;
    let mut cum_t = 0.0;
    let mut fuel = fuel_start;
    let wind_for = |i: usize, alt: f64, when: DateTime<Utc>| -> Option<Wind> {
        // average the winds at both ends of the leg
        let a = wind_at(inp.winds.get(i).and_then(|x| x.as_ref()), alt, when);
        let b = wind_at(inp.winds.get(i + 1).and_then(|x| x.as_ref()), alt, when);
        match (a, b) {
            (None, b) => b,
            (a, None) => a,
            (Some(a), Some(b)) => {
                let ax = a.spd * a.dir.to_radians().sin() + b.spd * b.dir.to_radians().sin();
                let ay = a.spd * a.dir.to_radians().cos() + b.spd * b.dir.to_radians().cos();
                let temp = match (a.temp, b.temp) {
                    (Some(p), Some(q)) => Some((p + q) / 2.0),
                    (t, _) => t,
                };
                Some(Wind { dir: norm(ax.atan2(ay).to_degrees()).round(), spd: (ax.hypot(ay) / 2.0).round(), temp })
            }
        }
    };

    for (i, leg) in legs.iter().enumerate() {
        let start = cum_d;
        let end = cum_d + leg.dist;
        let mut cuts = vec![start];
        cuts.extend([toc_d, tod_d].into_iter().filter(|d| *d > start + 0.05 && *d < end - 0.05));
        cuts.push(end);
        cuts.sort_by(f64::total_cmp);
        for k in 1..cuts.len() {
            let (d0, d1) = (cuts[k - 1], cuts[k]);
            let mid = (d0 + d1) / 2.0;
            let phase = if mid < toc_d { Phase::Climb } else if mid > tod_d { Phase::Descent } else { Phase::Cruise };
            let (tas, gph) = match phase {
                Phase::Climb => (p.climb_tas, p.climb_gph),
                Phase::Descent => (p.descent_tas, p.descent_gph),
                Phase::Cruise => (p.tas, p.gph),
            };
            let alt_mid = match phase {
                Phase::Climb => inp.dep_elev + climb_ft * (mid / toc_d.max(0.01)),
                Phase::Descent => cruise - desc_ft * ((mid - tod_d) / tod_from_end.max(0.01)),
                Phase::Cruise => cruise,
            };
            let f = (mid - start) / leg.dist.max(0.001);
            let (mlat, mlon) = interpolate(leg.a.lat, leg.a.lon, leg.b.lat, leg.b.lon, f);
            let when = inp.departure + Duration::milliseconds((cum_t * 60_000.0) as i64);
            let wind = wind_for(i, alt_mid, when);
            let tc = bearing(mlat, mlon, leg.b.lat, leg.b.lon);
            let tri = triangle(tc, tas, wind);
            let var = variation(mlat, mlon, inp.departure);
            let dist = d1 - d0;
            let ete = dist / tri.gs * 60.0;
            let burn = ete / 60.0 * gph;
            cum_t += ete;
            fuel -= burn;
            let fe = (d1 - start) / leg.dist.max(0.001);
            let (elat, elon) = interpolate(leg.a.lat, leg.a.lon, leg.b.lat, leg.b.lon, fe);
            let to = if (d1 - toc_d).abs() < 0.06 && d1 < end - 0.05 {
                "TOC".to_string()
            } else if (d1 - tod_d).abs() < 0.06 && d1 < end - 0.05 {
                "TOD".to_string()
            } else {
                leg.b.id.clone()
            };
            let from = match rows.last() {
                Some(last) if last.to != leg.a.id && k > 1 => last.to.clone(),
                _ => leg.a.id.clone(),
            };
            rows.push(NavRow {
                from,
                to,
                phase,
                alt: if phase == Phase::Cruise { cruise } else { alt_mid.round() },
                tc,
                mc: norm(tc - var),
                variation: var,
                wind,
                wca: tri.wca,
                th: tri.th,
                mh: norm(tri.th - var),
                tas,
                gs: tri.gs,
                dist,
                ete,
                fuel: burn,
                cum_dist: d1,
                cum_time: cum_t,
                eta: inp.departure + Duration::milliseconds((cum_t * 60_000.0) as i64),
                fuel_rem: fuel,
                lat: elat,
                lon: elon,
            });
        }
        cum_d = end;
    }

    let trip_fuel: f64 = rows.iter().map(|r| r.fuel).sum();
    let fuel_at_landing = fuel_start - trip_fuel;
    if fuel_at_landing < reserve_fuel {
        warnings.push(format!("Fuel at landing {fuel_at_landing:.1} gal is below the {}-minute reserve ({reserve_fuel:.1} gal)", p.reserve_min));
    }
    if let Some(r) = rows.iter().find(|r| r.tas - r.gs > 30.0) {
        warnings.push(format!("Headwind over 30 kt on {}–{}", r.from, r.to));
    }
    NavLog {
        rows,
        total_dist: total,
        total_time: cum_t,
        trip_fuel,
        taxi_fuel: p.taxi_gal,
        reserve_fuel,
        fuel_at_landing,
        toc_dist: (climb_ft > 0.0).then_some(toc_d),
        tod_dist: (desc_ft > 0.0).then_some(tod_d),
        warnings,
    }
}

/// "1:05" from minutes.
pub fn fmt_time(min: f64) -> String {
    let h = (min / 60.0).floor() as i64;
    let mut m = (min % 60.0).round() as i64;
    let mut h2 = h;
    if m == 60 {
        h2 += 1;
        m = 0;
    }
    format!("{h2}:{m:02}")
}

/// Course as three digits, 360 for north.
pub fn deg(d: f64) -> String {
    let v = norm(d).round() as i64;
    format!("{:03}", if v == 0 { 360 } else { v })
}

/// 6,500 style thousands separators.
pub fn thousands(v: f64) -> String {
    let n = v.round() as i64;
    let s = n.abs().to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0 { format!("-{out}") } else { out }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{aircraft, place::Place};

    fn wp(id: &str, lat: f64, lon: f64) -> Waypoint {
        Waypoint { place: Place { kind: crate::model::place::Kind::Airport, id: id.into(), name: id.into(), detail: String::new(), lat, lon }, uid: 0 }
    }

    #[test]
    fn simple_leg() {
        let ac = aircraft::from_preset(aircraft::presets()[0].clone(), "N1");
        let w = vec![wp("KSDL", 33.6229, -111.9105), wp("KPRC", 34.6548, -112.4192)];
        let log = build(&NavInput { waypoints: &w, aircraft: &ac, cruise: 7500.0, departure: Utc::now(), dep_elev: 1510.0, dest_elev: 5045.0, winds: &[] });
        assert!((log.total_dist - 66.0).abs() < 3.0, "{}", log.total_dist);
        assert!(log.rows.iter().any(|r| r.phase == Phase::Climb));
        assert!(log.trip_fuel > 4.0 && log.trip_fuel < 10.0);
        assert_eq!(deg(0.2), "360");
        assert_eq!(thousands(12500.0), "12,500");
        assert_eq!(fmt_time(65.0), "1:05");
    }

    #[test]
    fn wind_triangle() {
        let t = triangle(90.0, 100.0, Some(Wind { dir: 90.0, spd: 20.0, temp: None }));
        assert!((t.gs - 80.0).abs() < 0.01);
        let t = triangle(0.0, 100.0, Some(Wind { dir: 90.0, spd: 20.0, temp: None }));
        assert!(t.wca > 11.0 && t.wca < 12.0);
    }
}
