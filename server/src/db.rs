//! Read-only queries against data/build/aviation.sqlite (built by pipeline/build_nasr.py).

use parking_lot::Mutex;
use rusqlite::{params, Connection, OpenFlags, OptionalExtension, Row};
use serde::Serialize;
use serde_json::{json, Value};
use std::path::Path;

pub struct Db {
    conn: Mutex<Connection>,
}

#[derive(Serialize, Clone)]
pub struct Place {
    pub kind: &'static str,
    pub id: String,
    pub name: String,
    pub detail: String,
    pub lat: f64,
    pub lon: f64,
}

#[derive(Serialize)]
pub struct Obstacle {
    pub lat: f64,
    pub lon: f64,
    pub agl: i64,
    pub msl: i64,
    pub lit: bool,
    pub kind: String,
}

fn s(r: &Row, i: usize) -> String {
    r.get::<_, Option<String>>(i).ok().flatten().unwrap_or_default()
}

/// NASR "other services" codes, spelled out.
fn service_name(code: &str) -> &str {
    match code {
        "AFRT" => "Air freight",
        "AGRI" => "Crop dusting",
        "AMB" => "Air ambulance",
        "AVNCS" => "Avionics",
        "BCHGR" => "Beaching gear",
        "CARGO" => "Cargo handling",
        "CHTR" => "Charter",
        "GLD" => "Glider service",
        "INSTR" => "Pilot instruction",
        "PAJA" => "Parachute jump activity",
        "RNTL" => "Aircraft rental",
        "SALES" => "Aircraft sales",
        "SURV" => "Annual surveying",
        "TOW" => "Glider towing",
        other => other,
    }
}

fn fuel_name(code: &str) -> String {
    match code.trim() {
        "100" => "100 octane (green)".into(),
        "100LL" => "100LL (blue)".into(),
        "A" => "Jet A".into(),
        "A+" => "Jet A+ (with additives)".into(),
        "A++" => "Jet A++".into(),
        "A1" => "Jet A-1".into(),
        "A1+" => "Jet A-1+".into(),
        "B" => "Jet B".into(),
        "B+" => "Jet B+".into(),
        "J5" => "JP-5".into(),
        "J8" => "JP-8".into(),
        "MOGAS" => "Mogas (auto fuel)".into(),
        "UL91" => "UL91 unleaded".into(),
        "UL94" => "UL94 unleaded".into(),
        "UL100" => "UL100 unleaded".into(),
        other => other.to_string(),
    }
}

impl Db {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    fn site_for(&self, conn: &Connection, ident: &str) -> rusqlite::Result<Option<String>> {
        let id = ident.trim().to_uppercase();
        conn.query_row(
            "SELECT site_no FROM airport WHERE id = ?1 OR icao = ?1 OR (length(?1) = 4 AND substr(?1,1,1) = 'K' AND id = substr(?1,2)) ORDER BY rank LIMIT 1",
            [&id],
            |r| r.get(0),
        )
        .optional()
    }

    pub fn airport(&self, ident: &str) -> anyhow::Result<Option<Value>> {
        let conn = self.conn.lock();
        let Some(site) = self.site_for(&conn, ident)? else { return Ok(None) };
        let mut a = conn.query_row(
            "SELECT id, icao, name, city, state, type, use, lat, lon, elev, tpa, mag_var, sectional, artcc, fss, notam_id,
                    towered, fuel, airframe_repair, powerplant_repair, bottled_oxy, bulk_oxy, hangar, tiedown, other_services,
                    lighting_schedule, beacon, landing_fee, customs, segmented_circle, wind_indicator, ownership, raw
             FROM airport WHERE site_no = ?1",
            [&site],
            |r| {
                let fuel: Vec<Value> = s(r, 17).split(',').filter(|x| !x.trim().is_empty()).map(|x| json!({"code": x.trim(), "name": fuel_name(x)})).collect();
                let other_raw = s(r, 24);
                let services: Vec<String> = other_raw.split(',').filter(|x| !x.is_empty()).map(|x| service_name(x.trim()).to_string()).collect();
                let raw: Value = serde_json::from_str(&s(r, 32)).unwrap_or(Value::Null);
                Ok(json!({
                    "id": s(r, 0), "icao": s(r, 1), "name": s(r, 2), "city": s(r, 3), "state": s(r, 4),
                    "type": s(r, 5), "use": s(r, 6), "lat": r.get::<_, f64>(7)?, "lon": r.get::<_, f64>(8)?,
                    "elev": r.get::<_, Option<f64>>(9)?, "tpa": s(r, 10), "magVar": s(r, 11), "sectional": s(r, 12),
                    "artcc": s(r, 13), "fss": s(r, 14), "notamId": s(r, 15), "towered": r.get::<_, i64>(16)? == 1,
                    "fuel": fuel,
                    "repair": {"airframe": s(r, 18), "powerplant": s(r, 19)},
                    "oxygen": {"bottled": s(r, 20), "bulk": s(r, 21)},
                    "storage": {"hangar": s(r, 22) == "Y", "tiedown": s(r, 23) == "Y"},
                    "services": services,
                    "lightingSchedule": s(r, 25), "beacon": s(r, 26), "landingFee": s(r, 27) == "Y",
                    "customs": s(r, 28) == "Y", "segmentedCircle": s(r, 29), "windIndicator": s(r, 30),
                    "ownership": s(r, 31),
                    "attendanceRaw": raw.get("ATTENDANCE").cloned(),
                    "elevMethod": raw.get("ELEV_METHOD_CODE").cloned(),
                    "patternAltitude": raw.get("TPA").cloned(),
                    "distanceFromCity": raw.get("DIST_CITY_TO_AIRPORT").cloned(),
                    "directionFromCity": raw.get("DIRECTION_CODE").cloned(),
                }))
            },
        )?;
        let id = a["id"].as_str().unwrap_or_default().to_string();

        // runways with both ends
        let mut rwys = Vec::new();
        let mut st = conn.prepare("SELECT rwy_id, len, width, surface, cond, lighting FROM runway WHERE site_no = ?1 ORDER BY len DESC")?;
        let mut ends_st = conn.prepare(
            "SELECT end_id, true_alignment, ils, right_traffic, lat, lon, elev, tdz_elev, displaced_thr_len, vgsi, approach_lights
             FROM runway_end WHERE site_no = ?1 AND rwy_id = ?2 ORDER BY end_id",
        )?;
        for row in st.query_map([&site], |r| Ok((s(r, 0), r.get::<_, i64>(1)?, r.get::<_, i64>(2)?, s(r, 3), s(r, 4), s(r, 5))))? {
            let (rid, len, width, surface, cond, lighting) = row?;
            let ends: Vec<Value> = ends_st
                .query_map(params![&site, &rid], |r| {
                    Ok(json!({
                        "id": s(r, 0), "trueHdg": r.get::<_, Option<f64>>(1)?, "ils": s(r, 2), "rightTraffic": s(r, 3) == "Y",
                        "lat": r.get::<_, Option<f64>>(4)?, "lon": r.get::<_, Option<f64>>(5)?, "elev": r.get::<_, Option<f64>>(6)?,
                        "tdz": r.get::<_, Option<f64>>(7)?, "displaced": r.get::<_, Option<f64>>(8)?, "vgsi": s(r, 9), "als": s(r, 10),
                    }))
                })?
                .filter_map(Result::ok)
                .collect();
            rwys.push(json!({"id": rid, "len": len, "width": width, "surface": surface, "cond": cond, "lighting": lighting, "ends": ends}));
        }
        a["runways"] = Value::Array(rwys);

        let mut st = conn.prepare("SELECT use, freq, call, sector, remark FROM freq WHERE facility = ?1")?;
        let freqs: Vec<Value> = st
            .query_map([&id], |r| Ok(json!({"use": s(r, 0), "freq": s(r, 1), "call": s(r, 2), "sector": s(r, 3), "remark": s(r, 4)})))?
            .filter_map(Result::ok)
            .collect();
        a["freqs"] = Value::Array(freqs);

        let mut st = conn.prepare("SELECT title, name, address, city, state, zip, phone FROM contact WHERE site_no = ?1")?;
        let contacts: Vec<Value> = st
            .query_map([&site], |r| Ok(json!({"title": s(r, 0), "name": s(r, 1), "address": s(r, 2), "city": s(r, 3), "state": s(r, 4), "zip": s(r, 5), "phone": s(r, 6)})))?
            .filter_map(Result::ok)
            .collect();
        a["contacts"] = Value::Array(contacts);

        let mut st = conn.prepare("SELECT month, day, hour FROM attendance WHERE site_no = ?1")?;
        let att: Vec<Value> = st
            .query_map([&site], |r| Ok(json!({"month": s(r, 0), "day": s(r, 1), "hour": s(r, 2)})))?
            .filter_map(Result::ok)
            .collect();
        a["attendance"] = Value::Array(att);

        let mut st = conn.prepare("SELECT element, text FROM remark WHERE site_no = ?1")?;
        let rmk: Vec<Value> = st
            .query_map([&site], |r| Ok(json!({"element": s(r, 0), "text": s(r, 1)})))?
            .filter_map(Result::ok)
            .collect();
        a["remarks"] = Value::Array(rmk);

        let mut st = conn.prepare("SELECT id, type, elev, phone, freq FROM awos WHERE site_no = ?1 OR id = ?2")?;
        let awos: Vec<Value> = st
            .query_map(params![&site, &id], |r| Ok(json!({"id": s(r, 0), "type": s(r, 1), "elev": r.get::<_, Option<f64>>(2)?, "phone": s(r, 3), "freq": s(r, 4)})))?
            .filter_map(Result::ok)
            .collect();
        a["awos"] = Value::Array(awos);
        Ok(Some(a))
    }

    /// Resolve one route identifier: airport (FAA or ICAO), navaid, then fix.
    pub fn resolve(&self, ident: &str, near: Option<(f64, f64)>) -> anyhow::Result<Option<Place>> {
        let id = ident.trim().to_uppercase();
        if let Some(p) = parse_latlon(&id) {
            return Ok(Some(Place { kind: "coord", id: id.clone(), name: id, detail: String::new(), lat: p.0, lon: p.1 }));
        }
        let conn = self.conn.lock();
        if let Some(site) = self.site_for(&conn, &id)? {
            return Ok(conn
                .query_row("SELECT id, icao, name, city, state, lat, lon, elev FROM airport WHERE site_no = ?1", [&site], |r| {
                    let icao = s(r, 1);
                    Ok(Place {
                        kind: "airport",
                        id: if icao.is_empty() { s(r, 0) } else { icao },
                        name: s(r, 2),
                        detail: format!("{}, {} · {} ft", s(r, 3), s(r, 4), r.get::<_, Option<f64>>(7)?.unwrap_or(0.0) as i64),
                        lat: r.get(5)?,
                        lon: r.get(6)?,
                    })
                })
                .optional()?);
        }
        let pick = |cands: Vec<Place>| -> Option<Place> {
            match near {
                Some((la, lo)) => cands.into_iter().min_by(|a, b| {
                    let da = (a.lat - la).powi(2) + ((a.lon - lo) * la.to_radians().cos()).powi(2);
                    let db = (b.lat - la).powi(2) + ((b.lon - lo) * la.to_radians().cos()).powi(2);
                    da.total_cmp(&db)
                }),
                None => cands.into_iter().next(),
            }
        };
        let mut st = conn.prepare("SELECT id, name, type, freq, lat, lon FROM navaid WHERE id = ?1")?;
        let navs: Vec<Place> = st
            .query_map([&id], |r| Ok(Place { kind: "navaid", id: s(r, 0), name: s(r, 1), detail: format!("{} {}", s(r, 2), s(r, 3)), lat: r.get(4)?, lon: r.get(5)? }))?
            .filter_map(Result::ok)
            .collect();
        if let Some(p) = pick(navs) {
            return Ok(Some(p));
        }
        let mut st = conn.prepare("SELECT id, vfr, state, lat, lon FROM fix WHERE id = ?1")?;
        let fixes: Vec<Place> = st
            .query_map([&id], |r| {
                let vfr: i64 = r.get(1)?;
                Ok(Place { kind: "fix", id: s(r, 0), name: s(r, 0), detail: format!("{} · {}", if vfr == 1 { "VFR waypoint" } else { "Fix" }, s(r, 2)), lat: r.get(3)?, lon: r.get(4)? })
            })?
            .filter_map(Result::ok)
            .collect();
        Ok(pick(fixes))
    }

    pub fn search(&self, q: &str) -> anyhow::Result<Vec<Place>> {
        let q = q.trim().to_uppercase();
        if q.is_empty() {
            return Ok(vec![]);
        }
        let conn = self.conn.lock();
        let mut out: Vec<Place> = Vec::new();
        let like = format!("{q}%");
        let contains = format!("%{q}%");
        let mut st = conn.prepare(
            "SELECT id, icao, name, city, state, lat, lon, elev,
                    CASE WHEN id = ?1 OR icao = ?1 THEN 0 WHEN id LIKE ?2 OR icao LIKE ?2 THEN 1 ELSE 2 END AS score
             FROM airport
             WHERE id = ?1 OR icao = ?1 OR id LIKE ?2 OR icao LIKE ?2 OR upper(name) LIKE ?3 OR upper(city) LIKE ?3
             ORDER BY score, rank, id LIMIT 10",
        )?;
        for p in st.query_map(params![&q, &like, &contains], |r| {
            let icao = s(r, 1);
            Ok(Place {
                kind: "airport",
                id: if icao.is_empty() { s(r, 0) } else { icao },
                name: s(r, 2),
                detail: format!("{}, {}", s(r, 3), s(r, 4)),
                lat: r.get(5)?,
                lon: r.get(6)?,
            })
        })? {
            out.push(p?);
        }
        let mut st = conn.prepare("SELECT id, name, type, freq, lat, lon FROM navaid WHERE id = ?1 OR upper(name) LIKE ?2 LIMIT 5")?;
        for p in st.query_map(params![&q, &like], |r| Ok(Place { kind: "navaid", id: s(r, 0), name: s(r, 1), detail: format!("{} {}", s(r, 2), s(r, 3)), lat: r.get(4)?, lon: r.get(5)? }))? {
            out.push(p?);
        }
        if q.len() >= 3 {
            let mut st = conn.prepare("SELECT id, vfr, state, lat, lon FROM fix WHERE id LIKE ?1 ORDER BY vfr DESC, id LIMIT 5")?;
            for p in st.query_map([&like], |r| {
                let vfr: i64 = r.get(1)?;
                Ok(Place { kind: "fix", id: s(r, 0), name: s(r, 0), detail: format!("{} · {}", if vfr == 1 { "VFR waypoint" } else { "Fix" }, s(r, 2)), lat: r.get(3)?, lon: r.get(4)? })
            })? {
                out.push(p?);
            }
        }
        Ok(out)
    }

    pub fn obstacles_in(&self, lat0: f64, lon0: f64, lat1: f64, lon1: f64, min_agl: i64) -> anyhow::Result<Vec<Obstacle>> {
        let conn = self.conn.lock();
        let mut st = conn.prepare("SELECT lat, lon, agl, msl, lit, type FROM obstacle WHERE lat BETWEEN ?1 AND ?2 AND lon BETWEEN ?3 AND ?4 AND agl >= ?5")?;
        let rows = st
            .query_map(params![lat0.min(lat1), lat0.max(lat1), lon0.min(lon1), lon0.max(lon1), min_agl], |r| {
                Ok(Obstacle { lat: r.get(0)?, lon: r.get(1)?, agl: r.get(2)?, msl: r.get(3)?, lit: r.get::<_, i64>(4)? == 1, kind: s(r, 5) })
            })?
            .filter_map(Result::ok)
            .collect();
        Ok(rows)
    }

    /// Airports near a point (for alternates, diversion, nearest).
    pub fn airports_near(&self, lat: f64, lon: f64, radius_nm: f64, public_only: bool) -> anyhow::Result<Vec<(Place, f64)>> {
        let dlat = radius_nm / 60.0;
        let dlon = dlat / lat.to_radians().cos().max(0.1);
        let conn = self.conn.lock();
        let mut st = conn.prepare(
            "SELECT id, icao, name, city, state, lat, lon, elev FROM airport
             WHERE lat BETWEEN ?1 AND ?2 AND lon BETWEEN ?3 AND ?4 AND type = 'A' AND (?5 = 0 OR use = 'PU')",
        )?;
        let mut out: Vec<(Place, f64)> = st
            .query_map(params![lat - dlat, lat + dlat, lon - dlon, lon + dlon, public_only as i64], |r| {
                let icao = s(r, 1);
                Ok(Place {
                    kind: "airport",
                    id: if icao.is_empty() { s(r, 0) } else { icao },
                    name: s(r, 2),
                    detail: format!("{}, {}", s(r, 3), s(r, 4)),
                    lat: r.get(5)?,
                    lon: r.get(6)?,
                })
            })?
            .filter_map(Result::ok)
            .map(|p| {
                let d = crate::geo::distance_nm(lat, lon, p.lat, p.lon);
                (p, d)
            })
            .filter(|(_, d)| *d <= radius_nm)
            .collect();
        out.sort_by(|a, b| a.1.total_cmp(&b.1));
        Ok(out)
    }
}

/// Accepts "3240N11436W", "32.66/-114.6" and "32.66,-114.6".
fn parse_latlon(s: &str) -> Option<(f64, f64)> {
    if let Some((a, b)) = s.split_once(['/', ',']) {
        let (la, lo) = (a.trim().parse::<f64>().ok()?, b.trim().parse::<f64>().ok()?);
        if la.abs() <= 90.0 && lo.abs() <= 180.0 {
            return Some((la, lo));
        }
    }
    let b = s.as_bytes();
    if b.len() == 11 && (b[4] == b'N' || b[4] == b'S') && (b[10] == b'E' || b[10] == b'W') {
        let la = s[0..2].parse::<f64>().ok()? + s[2..4].parse::<f64>().ok()? / 60.0;
        let lo = s[5..8].parse::<f64>().ok()? + s[8..10].parse::<f64>().ok()? / 60.0;
        return Some((if b[4] == b'S' { -la } else { la }, if b[10] == b'W' { -lo } else { lo }));
    }
    None
}
