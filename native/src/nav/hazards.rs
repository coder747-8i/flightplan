//! Hazard advisor: scans the planned flight for terrain, obstacles, airspace, TFRs, weather
//! advisories, fuel, weight & balance, night arrival, density altitude and crosswind.

use super::geo::{in_ring, BBox};
use super::navlog::{thousands, NavLog};
use super::profile::{alt_at, Level, Profile};
use super::sun::sun_times;
use crate::model::aircraft::{compute_wb, Aircraft};
use crate::model::place::{Kind, Waypoint};
use crate::wx::decode::{decode_metar, describe_sigmet, str_of};
use chrono::{DateTime, Duration, Local, Utc};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};

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
    /// METAR records by ICAO id (departure / destination)
    pub metars: &'a HashMap<String, Value>,
    /// (runway end id, true heading) by airport id
    pub runways: &'a HashMap<String, Vec<(String, f64)>>,
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
    for (id, p, d) in tfr_hits {
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
    let total_min = h.log.map(|l| l.total_time).unwrap_or(0.0);
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
        let t = eta_at(s.d);
        if let Some(to) = sg["validTimeTo"].as_f64() {
            if to < t.min(now) {
                continue;
            }
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
        if let Some(s) = samples.iter().find(|x| b.contains(x.lon, x.lat) && in_ring(x.lon, x.lat, &ring)) {
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

    // ---- departure / destination weather ----
    let ends = [h.waypoints.first(), h.waypoints.last()];
    for (i, w) in ends.into_iter().enumerate() {
        let Some(w) = w.filter(|w| w.kind == Kind::Airport) else { continue };
        let id = w.wx_id();
        let Some(m) = h.metars.get(&id) else { continue };
        let place = if i == 0 { "Departure" } else { "Destination" };
        let plain: Vec<String> = decode_metar(&str_of(&m["rawOb"]))
            .into_iter()
            .filter(|l| ["Visibility", "Broken", "Overcast", "Sky", "Wind"].iter().any(|p| l.starts_with(p)) || ["rain", "snow", "fog", "mist", "thunder", "drizzle", "haze", "smoke"].iter().any(|w| l.to_lowercase().contains(w)))
            .collect();
        let plain = plain.join(". ");
        let cat = str_of(&m["fltCat"]);
        if cat == "IFR" || cat == "LIFR" {
            push(&mut out, Level::Warning, "Weather", format!("{place} {id} is {cat}"), plain.clone(), None);
        } else if cat == "MVFR" {
            push(&mut out, Level::Caution, "Weather", format!("{place} {id} is marginal VFR"), plain.clone(), None);
        }
        // crosswind
        let rwys = h.runways.get(&w.id).cloned().unwrap_or_default();
        if let (Some(wspd), Some(wdir), Some(ac)) = (m["wspd"].as_f64().filter(|v| *v > 0.0), m["wdir"].as_f64(), h.aircraft) {
            if !rwys.is_empty() {
                let gust = m["wgst"].as_f64().unwrap_or(wspd);
                let best = rwys
                    .iter()
                    .map(|(id, hdg)| {
                        let a = (wdir - hdg).to_radians();
                        (id.clone(), (gust * a.sin()).abs(), gust * a.cos())
                    })
                    .filter(|r| r.2 > -5.0)
                    .min_by(|a, b| a.1.total_cmp(&b.1));
                let limit = ac.perf.max_xwind;
                let gtxt = m["wgst"].as_f64().map(|g| format!(" gusting {g}")).unwrap_or_default();
                if let Some((rid, x, _)) = best {
                    if x > limit {
                        push(&mut out, Level::Warning, "Weather", format!("{place} crosswind {} kt on the best runway ({rid})", x.round()), format!("Above the {limit} kt demonstrated crosswind for this aircraft. Wind {wdir}° at {wspd}{gtxt} kt."), None);
                    } else if x > limit * 0.75 {
                        push(&mut out, Level::Caution, "Weather", format!("{place} crosswind {} kt (runway {rid})", x.round()), format!("Wind {wdir}° at {wspd}{gtxt} kt."), None);
                    }
                }
            }
        }
        // density altitude
        if i == 0 {
            if let (Some(temp), Some(altim), Some(elev)) = (m["temp"].as_f64(), m["altim"].as_f64(), m["elev"].as_f64()) {
                let elev_ft = elev * 3.28084;
                let pa = elev_ft + (1013.25 - altim) * 30.0;
                let isa = 15.0 - 2.0 * pa / 1000.0;
                let da = pa + 120.0 * (temp - isa);
                if da > 5000.0 {
                    push(
                        &mut out,
                        if da > 8000.0 { Level::Warning } else { Level::Caution },
                        "Performance",
                        format!("Density altitude {} ft at {id}", thousands((da / 100.0).round() * 100.0)),
                        format!("Field elevation {} ft, {}°F. Expect longer takeoff roll and reduced climb. Check the POH charts.", elev_ft.round(), (temp * 9.0 / 5.0 + 32.0).round()),
                        None,
                    );
                }
            }
        }
    }

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
