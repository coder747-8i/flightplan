//! Hazard advisor: scans the planned flight for terrain, obstacles, airspace, TFRs, weather
//! advisories, fuel, weight & balance and night arrival. Departure / destination weather is
//! judged at ETD / ETA by the go / no-go verdict (nav/verdict.rs).

use super::geo::{in_ring, BBox};
use super::navlog::{thousands, NavLog};
use super::profile::{alt_at, Level, Profile};
use super::sun::sun_times;
use crate::model::aircraft::{compute_wb, Aircraft};
use crate::model::place::Waypoint;
use crate::wx::decode::{describe_sigmet, str_of};
use chrono::{DateTime, Duration, Local, Utc};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};

#[derive(Clone, Debug)]
pub struct Hazard {
    pub level: Level,
    pub category: &'static str,
    pub title: String,
    pub detail: String,
    /// along-track NM
    pub at: Option<f64>,
}

pub struct HazardInput<'a> {
    pub waypoints: &'a [Waypoint],
    pub profile: &'a Profile,
    pub altitude_path: &'a [[f64; 2]],
    pub log: Option<&'a NavLog>,
    pub aircraft: Option<&'a Aircraft>,
    pub departure: DateTime<Utc>,
    pub tfrs: &'a Value,
    pub sigmets: &'a [Value],
    pub gairmets: &'a [Value],
}

fn fmt_alt(v: f64) -> String {
    if v >= 99999.0 {
        "unlimited".into()
    } else if v == 17999.0 {
        "FL180".into()
    } else if v >= 18000.0 {
        format!("FL{}", (v / 100.0).round())
    } else if v == 0.0 {
        "surface".into()
    } else {
        format!("{} ft", thousands(v))
    }
}

fn cap(t: &str) -> String {
    let mut c = t.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

fn ring_of(coords: &Value) -> Vec<[f64; 2]> {
    coords
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|c| {
                    let lon = c["lon"].as_f64().or_else(|| str_of(&c["lon"]).parse().ok())?;
                    let lat = c["lat"].as_f64().or_else(|| str_of(&c["lat"]).parse().ok())?;
                    Some([lon, lat])
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Outer rings of a GeoJSON Polygon / MultiPolygon.
pub fn outer_rings(g: &Value) -> Vec<Vec<[f64; 2]>> {
    let ring = |r: &Value| -> Vec<[f64; 2]> { r.as_array().map(|a| a.iter().filter_map(|p| Some([p[0].as_f64()?, p[1].as_f64()?])).collect()).unwrap_or_default() };
    match g["type"].as_str() {
        Some("Polygon") => g["coordinates"].as_array().and_then(|a| a.first()).map(|r| vec![ring(r)]).unwrap_or_default(),
        Some("MultiPolygon") => g["coordinates"].as_array().map(|ps| ps.iter().filter_map(|p| p.as_array()?.first().map(ring)).collect()).unwrap_or_default(),
        _ => vec![],
    }
}

/// The days a TFR covers, from FAA titles like "City, ST, Friday, October 23, 2026 through
/// Sunday, October 25, 2026 Local". None when the title has no dates (permanent TFRs, or ones
/// whose times are only in the NOTAM text). A lone date already in the past is treated as
/// unknown: some long-standing TFRs carry their issue date.
pub fn tfr_days(title: &str, now: DateTime<Utc>) -> Option<(chrono::NaiveDate, chrono::NaiveDate)> {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r"(?:Monday|Tuesday|Wednesday|Thursday|Friday|Saturday|Sunday), ([A-Z][a-z]+) (\d{1,2}), (\d{4})").unwrap());
    let dates: Vec<chrono::NaiveDate> = re.captures_iter(title).filter_map(|c| chrono::NaiveDate::parse_from_str(&format!("{} {} {}", &c[1], &c[2], &c[3]), "%B %d %Y").ok()).collect();
    match dates.as_slice() {
        [] => None,
        [d] if *d < now.date_naive() - Duration::days(2) => None,
        [d] => Some((*d, *d)),
        [a, .., b] => Some((*a, *b)),
    }
}

/// Whether a TFR with these days could be in effect at `t` (12 h slack covers US time zones
/// and "Local" vs "UTC" titles).
pub fn tfr_covers(days: Option<(chrono::NaiveDate, chrono::NaiveDate)>, t: DateTime<Utc>) -> bool {
    let Some((a, b)) = days else { return true };
    let start = a.and_hms_opt(0, 0, 0).map(|d| d.and_utc() - Duration::hours(12));
    let end = b.and_hms_opt(23, 59, 59).map(|d| d.and_utc() + Duration::hours(12));
    start.is_none_or(|s| t >= s) && end.is_none_or(|e| t <= e)
}

fn lateral(h: &HazardInput, total: f64) -> Vec<Hazard> {
    let mut out = Vec::new();
    let samples = &h.profile.samples;
    let route_box = BBox::of(samples.iter().map(|s| [s.lon, s.lat]));
    // ---- TFRs (lateral check; altitudes are in the NOTAM text) ----
    let mut tfr_hits: Vec<(String, &Value, f64)> = Vec::new();
    let mut seen = HashSet::new();
    for f in h.tfrs["features"].as_array().into_iter().flatten() {
        let rings = outer_rings(&f["geometry"]);
        for ring in &rings {
            if ring.len() < 3 {
                continue;
            }
            let b = BBox::of(ring.iter().copied());
            if !b.overlaps(&route_box) {
                continue;
            }
            if let Some(s) = samples.iter().find(|s| b.contains(s.lon, s.lat) && in_ring(s.lon, s.lat, ring)) {
                let id = str_of(&f["properties"]["notam"]);
                if seen.insert(id.clone()) {
                    tfr_hits.push((id, &f["properties"], s.d));
                }
                break;
            }
        }
    }
    let total_min = h.log.map(|l| l.total_time).unwrap_or(0.0);
    for (id, p, d) in tfr_hits {
        // dated TFRs only count if they cover the time you'd be there
        let when = h.departure + Duration::milliseconds((total_min * 60_000.0 * d / total.max(1.0)) as i64);
        let days = tfr_days(&str_of(&p["title"]), Utc::now()).or_else(|| tfr_days(&str_of(&p["description"]), Utc::now()));
        if !tfr_covers(days, when) {
            continue;
        }
        let ty = str_of(&p["type"]);
        out.push(Hazard {
            level: Level::Warning,
            category: "TFR",
            title: format!("Route crosses TFR {id}{}", if ty.is_empty() { String::new() } else { format!(" ({})", ty.to_lowercase()) }),
            detail: format!("{} Read the NOTAM for altitudes and times.", str_of(&p["title"])).trim().to_string(),
            at: Some(d),
        });
    }

    // ---- AIRMET / SIGMET / G-AIRMET ----
    let now = Utc::now().timestamp() as f64;
    let eta_at = |d: f64| h.departure.timestamp() as f64 + total_min * 60.0 * d / total.max(1.0);
    for sg in h.sigmets {
        let ring = ring_of(&sg["coords"]);
        if ring.len() < 3 {
            continue;
        }
        let b = BBox::of(ring.iter().copied());
        if !b.overlaps(&route_box) {
            continue;
        }
        let Some(s) = samples.iter().find(|x| b.contains(x.lon, x.lat) && in_ring(x.lon, x.lat, &ring)) else { continue };
        // it has to be in force when you get there (and not already over)
        let t = eta_at(s.d).max(now);
        if sg["validTimeTo"].as_f64().is_some_and(|to| to < t) || sg["validTimeFrom"].as_f64().is_some_and(|from| from > t + 3600.0) {
            continue;
        }
        let hazard = str_of(&sg["hazard"]);
        let ty = str_of(&sg["airSigmetType"]);
        let severe = ty == "SIGMET" || hazard == "CONVECTIVE";
        let (summary, _) = describe_sigmet(sg);
        out.push(Hazard {
            level: if severe { Level::Warning } else { Level::Caution },
            category: "Weather",
            title: if hazard == "CONVECTIVE" { "Convective SIGMET on your route".into() } else { format!("{ty} for {} on your route", hazard.to_lowercase()) },
            detail: format!("{}.", summary[1..].join(". ")),
            at: Some(s.d),
        });
    }
    let mut g_hits: BTreeMap<String, f64> = BTreeMap::new();
    for g in h.gairmets {
        let hz = str_of(&g["hazard"]);
        if str_of(&g["geometryType"]) == "LINE" || hz == "FZLVL" || g_hits.contains_key(&hz) {
            continue;
        }
        let ring = ring_of(&g["coords"]);
        if ring.len() < 3 {
            continue;
        }
        let b = BBox::of(ring.iter().copied());
        if !b.overlaps(&route_box) {
            continue;
        }
        // each G-AIRMET is a snapshot valid around `validTime` until the product expires
        let valid = g["validTime"].as_str().and_then(|v| DateTime::parse_from_rfc3339(v).ok()).map(|d| d.timestamp() as f64);
        let expires = g["expireTime"].as_f64();
        if let Some(s) = samples.iter().find(|x| b.contains(x.lon, x.lat) && in_ring(x.lon, x.lat, &ring)) {
            let t = eta_at(s.d).max(now);
            if expires.is_some_and(|e| t > e) || valid.is_some_and(|v| t < v - 3.0 * 3600.0) {
                continue;
            }
            g_hits.insert(hz, s.d);
        }
    }
    let max_alt = h.altitude_path.iter().map(|p| p[1]).fold(0.0, f64::max);
    for (hz, d) in g_hits {
        if hz == "TURB-HI" && max_alt < 18000.0 {
            continue;
        }
        let name = match hz.as_str() {
            "IFR" => "IFR conditions",
            "MT_OBSC" => "mountain obscuration",
            "TURB-HI" => "high-altitude turbulence",
            "TURB-LO" => "turbulence below FL180",
            "ICE" => "icing",
            "SFC_WND" => "strong surface wind",
            "LLWS" => "low-level wind shear",
            "M_FZLVL" => "multiple freezing levels",
            x => x,
        };
        out.push(Hazard {
            level: if matches!(hz.as_str(), "IFR" | "MT_OBSC" | "ICE") { Level::Caution } else { Level::Info },
            category: "Weather",
            title: format!("G-AIRMET: {name} along route"),
            detail: String::new(),
            at: Some(d),
        });
    }
    out
}

pub fn analyze(h: &HazardInput) -> Vec<Hazard> {
    let mut out: Vec<Hazard> = Vec::new();
    let samples = &h.profile.samples;
    let total = samples.last().map(|s| s.d).unwrap_or(0.0);
    let Some(log) = h.log else { return out };
    if h.waypoints.len() < 2 {
        return out;
    }
    let path = h.altitude_path;
    let push = |out: &mut Vec<Hazard>, level, category, title: String, detail: String, at| out.push(Hazard { level, category, title, detail, at });

    // ---- terrain ----
    let worst = samples
        .iter()
        .filter(|s| s.d >= 5.0 && s.d <= total - 5.0)
        .map(|s| (alt_at(path, s.d) - s.max, s.d, s.max))
        .min_by(|a, b| a.0.total_cmp(&b.0));
    if let Some((clr, d, elev)) = worst {
        if clr < 0.0 {
            push(&mut out, Level::Warning, "Terrain", format!("Planned altitude is below terrain at {d:.0} NM"), format!("Terrain within 1 NM rises to {} ft.", thousands(elev)), Some(d));
        } else if clr < 500.0 {
            push(&mut out, Level::Warning, "Terrain", format!("Only {} ft above terrain at {d:.0} NM", clr.round()), format!("Highest terrain within 1 NM: {} ft.", thousands(elev)), Some(d));
        } else if clr < 1000.0 {
            push(&mut out, Level::Caution, "Terrain", format!("Less than 1,000 ft terrain clearance at {d:.0} NM"), format!("{} ft above the highest terrain within 1 NM.", clr.round()), Some(d));
        }
    }

    // ---- obstacles ----
    let tall: Vec<_> = h.profile.obstacles.iter().filter(|o| o.d > 2.0 && o.d < total - 2.0 && alt_at(path, o.d) - o.msl < 500.0).collect();
    if let Some(o) = tall.iter().max_by(|a, b| a.msl.total_cmp(&b.msl)) {
        push(
            &mut out,
            Level::Warning,
            "Obstacle",
            format!("{} obstacle{} within 500 ft of your altitude", tall.len(), if tall.len() > 1 { "s" } else { "" }),
            format!("Tallest: {} {} ft MSL ({} AGL) at {:.0} NM, {:.1} NM {} of course.", o.kind, thousands(o.msl), o.agl, o.d, o.xt.abs(), if o.xt < 0.0 { "left" } else { "right" }),
            Some(o.d),
        );
    }

    // ---- airspace at planned altitude ----
    let mut seen = HashSet::new();
    for c in &h.profile.airspace {
        let hit = samples.iter().find(|s| {
            if s.d < c.from || s.d > c.to {
                return false;
            }
            let alt = alt_at(path, s.d);
            let floor = if c.lo_agl { s.elev + c.lo } else { c.lo };
            alt >= floor && alt <= c.hi
        });
        let Some(hit) = hit.map(|s| s.d) else { continue };
        if !seen.insert(format!("{}:{}", c.kind, c.name)) {
            continue;
        }
        let range = cap(&format!("{} to {}", if c.lo_agl { format!("{} ft AGL", c.lo) } else { fmt_alt(c.lo) }, fmt_alt(c.hi)));
        let times = |sep: &str| if c.times.is_empty() { String::new() } else { format!("{sep}{}", c.times) };
        match c.kind.as_str() {
            "P" => push(&mut out, Level::Warning, "Airspace", format!("Route enters prohibited area {}", c.name), format!("{range}. Flight is not allowed."), Some(hit)),
            "R" => push(&mut out, Level::Warning, "Airspace", format!("Route enters restricted area {}", c.name), format!("{range}{}. Check status with ATC/FSS before entering.", times(". Times: ")), Some(hit)),
            "B" => push(&mut out, Level::Caution, "Airspace", format!("Class B: {}", c.name.replace(" Class B", "")), format!("{range}. You need an explicit \"cleared into the Class B\" from ATC."), Some(hit)),
            "C" => push(&mut out, Level::Caution, "Airspace", format!("Class C: {}", c.name.replace(" Class C", "")), format!("{range}. Establish two-way radio contact before entering."), Some(hit)),
            "D" => push(&mut out, Level::Info, "Airspace", format!("Class D: {}", c.name.replace(" Class D", "")), format!("{range}. Two-way radio contact with the tower required."), Some(hit)),
            "MOA" => push(&mut out, Level::Caution, "Airspace", format!("MOA: {}", c.name), format!("{range}{}. Military training may be active: ask Flight Service or ATC.", times(". ")), Some(hit)),
            "W" | "A" => push(&mut out, Level::Info, "Airspace", format!("{} area {}", if c.kind == "W" { "Warning" } else { "Alert" }, c.name), range, Some(hit)),
            "MODEC" => push(&mut out, Level::Info, "Airspace", "Mode C veil".into(), format!("{}: ADS-B Out / Mode C transponder required.", c.name), Some(hit)),
            _ => {}
        }
    }

    out.extend(lateral(h, total));

    // ---- fuel ----
    if log.fuel_at_landing < 0.0 {
        push(&mut out, Level::Warning, "Fuel", "Not enough fuel for this flight".into(), format!("Trip needs {:.1} gal plus {} taxi.", log.trip_fuel, log.taxi_fuel), None);
    } else if log.fuel_at_landing < log.reserve_fuel {
        push(
            &mut out,
            Level::Warning,
            "Fuel",
            format!("Below reserve at landing ({:.1} gal)", log.fuel_at_landing),
            format!("Reserve is {:.1} gal ({} min).", log.reserve_fuel, h.aircraft.map(|a| a.perf.reserve_min).unwrap_or(30.0)),
            None,
        );
    } else if log.fuel_at_landing < log.reserve_fuel * 1.5 {
        push(&mut out, Level::Caution, "Fuel", format!("Thin fuel margin: {:.1} gal at landing", log.fuel_at_landing), format!("Reserve is {:.1} gal.", log.reserve_fuel), None);
    }
    for w in &log.warnings {
        if !w.starts_with("Fuel") {
            push(&mut out, Level::Info, "Performance", w.clone(), String::new(), None);
        }
    }

    // ---- weight & balance ----
    if let Some(ac) = h.aircraft {
        for p in compute_wb(ac, log.trip_fuel).problems {
            push(&mut out, Level::Warning, "Weight & balance", p, String::new(), None);
        }
    }

    // ---- daylight ----
    if let Some(dest) = h.waypoints.last() {
        let eta = h.departure + Duration::milliseconds((log.total_time * 60_000.0) as i64);
        let fmt = |d: DateTime<Utc>| d.with_timezone(&Local).format("%H:%M").to_string();
        if let (Some((_, set)), Some((crise, cset))) = (sun_times(dest.lat, dest.lon, eta, 90.833), sun_times(dest.lat, dest.lon, eta, 96.0)) {
            if eta > cset || eta < crise {
                push(&mut out, Level::Caution, "Daylight", format!("Night arrival (ETA {})", fmt(eta)), format!("Civil twilight ends {} at {}. Night currency and lighting rules apply.", fmt(cset), dest.id), None);
            } else if eta > set {
                push(&mut out, Level::Info, "Daylight", format!("Arrival after sunset ({})", fmt(set)), format!("ETA {}, before the end of civil twilight ({}).", fmt(eta), fmt(cset)), None);
            } else if (set - eta).num_minutes() < 45 {
                push(&mut out, Level::Info, "Daylight", format!("Arriving within 45 min of sunset ({})", fmt(set)), String::new(), None);
            }
        }
    }

    out.sort_by(|a, b| a.level.cmp(&b.level).then(a.at.unwrap_or(0.0).total_cmp(&b.at.unwrap_or(0.0))));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn tfr_dates_from_titles() {
        let now = Utc.with_ymd_and_hms(2026, 10, 25, 5, 0, 0).unwrap();
        let d = |y, m, dd| chrono::NaiveDate::from_ymd_opt(y, m, dd).unwrap();
        assert_eq!(tfr_days("RALEIGH, NC, Thursday, October 15, 2026 through Sunday, October 25, 2026 Local", now), Some((d(2026, 10, 15), d(2026, 10, 25))));
        assert_eq!(tfr_days("SELFRIDGE ANGB, MI, Sunday, October 25, 2026 Local", now), Some((d(2026, 10, 25), d(2026, 10, 25))));
        assert_eq!(tfr_days("South Padre Island, TX, Tuesday, May 6, 2025 Local", now), None, "old lone date: unknown");
        assert_eq!(tfr_days("Key West, FL", now), None);
        let sunday = tfr_days("SELFRIDGE ANGB, MI, Sunday, October 25, 2026 Local", now);
        assert!(tfr_covers(sunday, Utc.with_ymd_and_hms(2026, 10, 25, 18, 0, 0).unwrap()));
        assert!(!tfr_covers(sunday, Utc.with_ymd_and_hms(2026, 10, 27, 18, 0, 0).unwrap()));
        assert!(tfr_covers(None, now));
    }
}
