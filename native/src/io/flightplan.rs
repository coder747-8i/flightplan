//! Route exchange formats: the FltPlan.com flight plan fields, Garmin .fpl (Garmin avionics SD
//! cards, Garmin Pilot, ForeFlight, FltPlan Go via Garmin), and GPX; plus reading .fpl/.gpx back.

use crate::model::aircraft::Aircraft;
use crate::model::place::{Kind, Place};
use crate::nav::geo::coord_id;
use std::collections::HashMap;

/// Identifier to file: coordinates get a lat-long id; everything else keeps its id.
fn route_id(p: &Place) -> String {
    if p.kind == Kind::Coord { coord_id(p.lat, p.lon) } else { p.id.to_uppercase() }
}

pub struct FltPlanFields {
    pub departure: String,
    pub destination: String,
    pub route: String,
    pub altitude: String,
    pub aircraft_id: String,
    pub aircraft_type: String,
    pub tas: String,
    pub ete: String,
    pub fuel: String,
    /// the whole route on one line, for FltPlan's route box
    pub full: String,
}

impl FltPlanFields {
    pub fn rows(&self) -> Vec<(&'static str, &str)> {
        vec![
            ("Departure", &self.departure),
            ("Destination", &self.destination),
            ("Route", &self.route),
            ("Altitude", &self.altitude),
            ("Aircraft ID", &self.aircraft_id),
            ("Type", &self.aircraft_type),
            ("True airspeed", &self.tas),
            ("Time en route", &self.ete),
            ("Fuel on board", &self.fuel),
        ]
    }
}

fn hhmm(min: f64) -> String {
    format!("{}:{:02}", (min / 60.0).floor() as i64, (min % 60.0).round() as i64)
}

/// The values FltPlan.com's flight plan form asks for, ready to copy.
pub fn fltplan_fields(wps: &[&Place], cruise: f64, ac: Option<&Aircraft>, ete_min: Option<f64>) -> FltPlanFields {
    let ids: Vec<String> = wps.iter().map(|p| route_id(p)).collect();
    let mid: &[String] = if ids.len() > 2 { &ids[1..ids.len() - 1] } else { &[] };
    // FltPlan, like the FAA form, wants altitude in hundreds of feet ("065"), FL above 17,999
    let altitude = if cruise >= 18000.0 { format!("FL{}", (cruise / 100.0).round()) } else { format!("{:03}", (cruise / 100.0).round() as i64) };
    FltPlanFields {
        departure: ids.first().cloned().unwrap_or_default(),
        destination: if ids.len() > 1 { ids.last().cloned().unwrap_or_default() } else { String::new() },
        route: if mid.is_empty() { "DCT".into() } else { mid.join(" ") },
        altitude,
        aircraft_id: ac.map(|a| a.tail.to_uppercase()).unwrap_or_default(),
        aircraft_type: ac.map(|a| a.type_code.to_uppercase()).unwrap_or_default(),
        tas: ac.map(|a| format!("{}", a.perf.tas.round())).unwrap_or_default(),
        ete: ete_min.filter(|m| *m > 0.0).map(hhmm).unwrap_or_default(),
        fuel: ac.filter(|a| a.perf.gph > 0.0).map(|a| hhmm(a.usable_fuel / a.perf.gph * 60.0)).unwrap_or_default(),
        full: ids.join(" "),
    }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&apos;")
}

fn garmin_type(p: &Place) -> &'static str {
    match p.kind {
        Kind::Airport => "AIRPORT",
        Kind::Navaid => {
            let d = p.detail.to_uppercase();
            if d.contains("NDB") && !d.contains("VOR") { "NDB" } else { "VOR" }
        }
        Kind::Fix => "INT",
        Kind::Coord => "USER WAYPOINT",
    }
}

/// Garmin FlightPlan v1 XML. User waypoints get short unique ids (Garmin allows 6 characters).
pub fn to_fpl(wps: &[&Place], name: &str) -> String {
    let mut table: Vec<(String, &'static str, &Place)> = Vec::new();
    let mut points = Vec::new();
    let mut user = 0;
    for p in wps {
        let ty = garmin_type(p);
        let id = if ty == "USER WAYPOINT" {
            user += 1;
            format!("USR{user:03}")
        } else {
            p.id.to_uppercase().chars().take(12).collect()
        };
        if !table.iter().any(|(i, t, _)| *i == id && *t == ty) {
            table.push((id.clone(), ty, p));
        }
        points.push((id, ty));
    }
    let wp_xml: Vec<String> = table
        .iter()
        .map(|(id, ty, p)| {
            let comment = if *ty == "USER WAYPOINT" { coord_id(p.lat, p.lon) } else { p.name.chars().take(25).collect::<String>().to_uppercase() };
            format!(
                "    <waypoint>\n      <identifier>{}</identifier>\n      <type>{ty}</type>\n      <lat>{:.6}</lat>\n      <lon>{:.6}</lon>\n      <comment>{}</comment>\n    </waypoint>",
                esc(id),
                p.lat,
                p.lon,
                esc(&comment)
            )
        })
        .collect();
    let rt_xml: Vec<String> = points
        .iter()
        .map(|(id, ty)| format!("    <route-point>\n      <waypoint-identifier>{}</waypoint-identifier>\n      <waypoint-type>{ty}</waypoint-type>\n    </route-point>", esc(id)))
        .collect();
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<flight-plan xmlns=\"http://www8.garmin.com/xmlschemas/FlightPlan/v1\">\n  <created>{}</created>\n  <waypoint-table>\n{}\n  </waypoint-table>\n  <route>\n    <route-name>{}</route-name>\n    <flight-plan-index>1</flight-plan-index>\n{}\n  </route>\n</flight-plan>\n",
        chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ"),
        wp_xml.join("\n"),
        esc(&name.to_uppercase().chars().take(25).collect::<String>()),
        rt_xml.join("\n")
    )
}

pub fn to_gpx(wps: &[&Place], name: &str) -> String {
    let pts: Vec<String> = wps
        .iter()
        .map(|p| {
            let desc = if p.name.is_empty() { String::new() } else { format!("<desc>{}</desc>", esc(&p.name)) };
            format!("    <rtept lat=\"{:.6}\" lon=\"{:.6}\"><name>{}</name>{desc}</rtept>", p.lat, p.lon, esc(&route_id(p)))
        })
        .collect();
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<gpx version=\"1.1\" creator=\"Flightplan\" xmlns=\"http://www.topografix.com/GPX/1/1\">\n  <rte>\n    <name>{}</name>\n{}\n  </rte>\n</gpx>\n",
        esc(name),
        pts.join("\n")
    )
}

/// Waypoints from a Garmin .fpl or a GPX route / track / waypoint file.
pub fn parse_route_file(text: &str) -> Result<Vec<Place>, String> {
    let doc = roxmltree::Document::parse(text).map_err(|_| "This file isn't valid XML".to_string())?;
    let root = doc.root_element();
    let child_text = |n: roxmltree::Node, tag: &str| n.descendants().find(|c| c.tag_name().name() == tag).and_then(|c| c.text()).unwrap_or("").trim().to_string();
    match root.tag_name().name() {
        "flight-plan" => {
            let mut table: HashMap<(String, String), roxmltree::Node> = HashMap::new();
            for w in root.descendants().filter(|n| n.tag_name().name() == "waypoint") {
                table.insert((child_text(w, "type"), child_text(w, "identifier")), w);
            }
            let mut out = Vec::new();
            for rp in root.descendants().filter(|n| n.tag_name().name() == "route-point") {
                let id = child_text(rp, "waypoint-identifier");
                let ty = child_text(rp, "waypoint-type");
                let w = table.get(&(ty.clone(), id.clone())).or_else(|| table.iter().find(|((_, i), _)| *i == id).map(|(_, n)| n));
                let Some(w) = w else { continue };
                let (Ok(lat), Ok(lon)) = (child_text(*w, "lat").parse::<f64>(), child_text(*w, "lon").parse::<f64>()) else { continue };
                out.push(from_file(&id, &ty, &child_text(*w, "comment"), lat, lon));
            }
            if out.is_empty() { Err("No route points found in this flight plan".into()) } else { Ok(out) }
        }
        "gpx" => {
            let pick = |tag: &str| root.descendants().filter(|n| n.tag_name().name() == tag).collect::<Vec<_>>();
            let mut pts = pick("rtept");
            if pts.is_empty() {
                pts = pick("wpt");
            }
            if pts.is_empty() {
                pts = pick("trkpt");
            }
            let out: Vec<Place> = pts
                .iter()
                .enumerate()
                .filter_map(|(i, p)| {
                    let lat: f64 = p.attribute("lat")?.parse().ok()?;
                    let lon: f64 = p.attribute("lon")?.parse().ok()?;
                    let mut id = child_text(*p, "name");
                    if id.is_empty() {
                        id = format!("WP{}", i + 1);
                    }
                    Some(from_file(&id, "", &child_text(*p, "desc"), lat, lon))
                })
                .collect();
            if out.is_empty() { Err("No waypoints found in this GPX file".into()) } else { Ok(out) }
        }
        _ => Err("Not a Garmin .fpl or GPX file".into()),
    }
}

fn from_file(id: &str, ty: &str, comment: &str, lat: f64, lon: f64) -> Place {
    let t = ty.to_uppercase();
    let name = if comment.is_empty() { id.to_string() } else { comment.to_string() };
    match t.as_str() {
        "AIRPORT" => Place { kind: Kind::Airport, id: id.into(), name, detail: String::new(), lat, lon },
        "VOR" | "NDB" => Place { kind: Kind::Navaid, id: id.into(), name, detail: t.clone(), lat, lon },
        x if x.starts_with("INT") => Place { kind: Kind::Fix, id: id.into(), name: id.into(), detail: "Fix".into(), lat, lon },
        _ => {
            // user waypoints and GPX points: keep a readable id but plan by coordinates
            let plain = id.len() >= 2 && id.len() <= 5 && id.chars().all(|c| c.is_ascii_alphanumeric());
            let cid = if plain && t != "USER WAYPOINT" { id.to_uppercase() } else { coord_id(lat, lon) };
            Place { kind: Kind::Coord, id: cid, name: if comment.is_empty() { "User waypoint".into() } else { comment.into() }, detail: format!("{lat:.4}, {lon:.4}"), lat, lon }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(kind: Kind, id: &str, lat: f64, lon: f64) -> Place {
        Place { kind, id: id.into(), name: id.into(), detail: if kind == Kind::Navaid { "VORTAC 114.1".into() } else { String::new() }, lat, lon }
    }

    #[test]
    fn fpl_roundtrip() {
        let route = vec![p(Kind::Airport, "KSDL", 33.62, -111.91), p(Kind::Navaid, "DRK", 34.70, -112.48), Place::coord(34.5, -112.5), p(Kind::Airport, "KPRC", 34.65, -112.42)];
        let refs: Vec<&Place> = route.iter().collect();
        let back = parse_route_file(&to_fpl(&refs, "KSDL to KPRC")).unwrap();
        let ids: Vec<String> = back.iter().map(route_id).collect();
        assert_eq!(ids, ["KSDL", "DRK", "3430N11230W", "KPRC"]);
        let gpx = parse_route_file(&to_gpx(&refs, "x")).unwrap();
        assert_eq!(gpx.len(), 4);
        let f = fltplan_fields(&refs, 6500.0, None, Some(61.0));
        assert_eq!(f.route, "DRK 3430N11230W");
        assert_eq!(f.altitude, "065");
        assert_eq!(f.ete, "1:01");
    }
}
