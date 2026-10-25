//! NOTAMs from the FAA NOTAM API (via the server library; free credentials from api.faa.gov),
//! read into what a pilot needs: when each is in effect, plain English, and how it affects the
//! flight. Closed airports and runways, fuel, navaids on the route, GPS and airspace NOTAMs feed
//! the go / no-go call.

use crate::backend::{Backend, Task};
use crate::model::place::{Kind, Waypoint};
use crate::nav::geo::{distance_nm, point_along};
use crate::wx::decode::{expand, str_of};
use chrono::{DateTime, NaiveDateTime, Utc};
use serde_json::Value;
use std::time::{Duration, Instant};

/// What a NOTAM does to a flight.
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    AerodromeClosed,
    /// runway end ids it closes, e.g. ["03R", "21L"]
    RunwayClosed(Vec<String>),
    FuelUnavailable,
    NavaidOut,
    Gps,
    Airspace,
    Obstacle,
    Other,
}

#[derive(Clone, Debug)]
pub struct Notam {
    pub id: String,
    pub number: String,
    /// FAA location (PRC) and ICAO location (KPRC)
    pub location: String,
    pub icao: String,
    pub class: String,
    pub category: String,
    /// the NOTAM text as issued
    pub raw: String,
    /// contractions written out
    pub plain: String,
    pub start: Option<DateTime<Utc>>,
    pub end: Option<DateTime<Utc>>,
    pub perm: bool,
    /// the end time is an estimate
    pub est: bool,
    pub effect: Effect,
    pub geometry: Value,
}

/// "2026-10-24T13:00:00.000Z", "2026-10-30T23:59:00.000-0500EST" or "PERM".
fn time(v: &Value) -> (Option<DateTime<Utc>>, bool, bool) {
    let s = str_of(v);
    if s.is_empty() {
        return (None, false, false);
    }
    if s.starts_with("PERM") {
        return (None, true, false);
    }
    let est = s.contains("EST");
    let t = s.get(..19).and_then(|x| NaiveDateTime::parse_from_str(x, "%Y-%m-%dT%H:%M:%S").ok()).map(|n| n.and_utc());
    (t, false, est)
}

/// The effect of a NOTAM, from its text (domestic format: "RWY 03R/21L CLSD", "AD AP CLSD").
pub fn classify(raw: &str, category: &str) -> Effect {
    static RWY: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    static AD: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let t = raw.to_uppercase();
    let ad = AD.get_or_init(|| regex::Regex::new(r"\b(?:AD AP|AD|AP|AERODROME|AIRPORT) CLSD\b").unwrap());
    if ad.is_match(&t) && !t.contains("RWY") && !t.contains("TWY") {
        return Effect::AerodromeClosed;
    }
    let rwy = RWY.get_or_init(|| regex::Regex::new(r"\bRWY (\d{1,2}[LRCW]?)(?:/(\d{1,2}[LRCW]?))? CLSD\b").unwrap());
    if let Some(c) = rwy.captures(&t) {
        // "RWY 03/21 CLSD" closes the runway; "... CLSD TO ARR" etc. still counts
        let mut ids = vec![c[1].to_string()];
        if let Some(b) = c.get(2) {
            ids.push(b.as_str().to_string());
        }
        return Effect::RunwayClosed(ids);
    }
    if t.contains("FUEL") && (t.contains("UNAVBL") || t.contains("NOT AVBL") || t.contains(" OTS") || t.contains("U/S")) {
        return Effect::FuelUnavailable;
    }
    if (t.contains("NAV ") || t.contains("VOR") || t.contains("VORTAC") || t.contains("NDB")) && (t.contains("U/S") || t.contains("OTS") || t.contains("UNUSABLE")) {
        return Effect::NavaidOut;
    }
    if (t.contains("GPS") || t.contains("GNSS")) && (t.contains("UNREL") || t.contains("UNAVBL") || t.contains("MAY NOT BE AVBL")) {
        return Effect::Gps;
    }
    match category {
        "Airspace" => Effect::Airspace,
        "Obstacle" => Effect::Obstacle,
        _ => Effect::Other,
    }
}

impl Notam {
    pub fn from_json(v: &Value) -> Notam {
        let raw = str_of(&v["icaoText"]);
        let text = str_of(&v["text"]);
        let category = str_of(&v["category"]);
        let (start, _, _) = time(&v["start"]);
        let (end, perm, est) = time(&v["end"]);
        // the domestic text is the readable one; the ICAO text has the Q-line
        let body = if raw.is_empty() { text.clone() } else { raw.clone() };
        Notam {
            id: str_of(&v["id"]),
            number: str_of(&v["number"]),
            location: str_of(&v["location"]),
            icao: str_of(&v["icao"]),
            class: str_of(&v["class"]),
            effect: classify(&body, &category),
            category,
            plain: expand(&text.trim_start_matches('!').to_string()),
            raw: text,
            start,
            end,
            perm,
            est,
            geometry: v["geometry"].clone(),
        }
    }

    /// In effect at `t` (no times known: assume yes).
    pub fn active_at(&self, t: DateTime<Utc>) -> bool {
        self.start.is_none_or(|s| t >= s) && (self.perm || self.end.is_none_or(|e| t <= e))
    }

    /// Overlaps [a, b].
    pub fn active_during(&self, a: DateTime<Utc>, b: DateTime<Utc>) -> bool {
        self.start.is_none_or(|s| s <= b) && (self.perm || self.end.is_none_or(|e| e >= a))
    }

    /// "Oct 24 13:00Z – Oct 30 23:59Z (est.)", "since Oct 2 · permanent".
    pub fn window(&self) -> String {
        let f = |d: DateTime<Utc>| d.format("%b %-d %H:%MZ").to_string();
        match (self.start, self.end, self.perm) {
            (Some(s), _, true) => format!("from {} · permanent", f(s)),
            (Some(s), Some(e), _) => format!("{} – {}{}", f(s), f(e), if self.est { " (est.)" } else { "" }),
            (Some(s), None, _) => format!("from {}", f(s)),
            (None, Some(e), _) => format!("until {}", f(e)),
            _ => String::new(),
        }
    }

    /// Whether the NOTAM is for this airport (FAA "PRC", ICAO "KPRC" or "E25").
    pub fn at_airport(&self, id: &str) -> bool {
        let id = id.to_uppercase();
        self.icao == id || self.location == id || (id.len() == 4 && id.starts_with('K') && self.location == id[1..])
    }

    /// Most important first.
    pub fn rank(&self) -> u8 {
        match self.effect {
            Effect::AerodromeClosed => 0,
            Effect::RunwayClosed(_) => 1,
            Effect::FuelUnavailable => 2,
            Effect::NavaidOut => 3,
            Effect::Gps => 4,
            Effect::Airspace => 5,
            Effect::Obstacle => 7,
            Effect::Other => 6,
        }
    }
}

pub fn parse(v: &Value) -> (bool, Vec<Notam>, Vec<String>) {
    let configured = v["configured"].as_bool().unwrap_or(false);
    let mut items: Vec<Notam> = v["items"].as_array().into_iter().flatten().map(Notam::from_json).collect();
    items.sort_by(|a, b| a.rank().cmp(&b.rank()).then(a.location.cmp(&b.location)));
    let errors = v["errors"].as_array().into_iter().flatten().map(str_of).collect();
    (configured, items, errors)
}

/// NOTAMs for the active route, refreshed when the route changes and every 15 minutes.
#[derive(Default)]
pub struct RouteNotams {
    key: String,
    at: Option<Instant>,
    task: Task<Value>,
    pub items: Vec<Notam>,
    pub configured: bool,
    /// whether a fetch has finished for the current route
    pub done: bool,
    pub loading: bool,
    pub errors: Vec<String>,
    pub generation: u64,
}

impl RouteNotams {
    pub fn update(&mut self, be: &Backend, waypoints: &[Waypoint], configured: bool) {
        let key = format!("{configured}{}", waypoints.iter().map(|w| w.id.as_str()).collect::<Vec<_>>().join(" "));
        let stale = self.at.is_none_or(|t| t.elapsed() > Duration::from_secs(900));
        if (key != self.key || stale) && !self.task.pending() {
            self.key = key;
            self.at = Some(Instant::now());
            self.done = false;
            if waypoints.len() < 2 || !configured {
                self.items.clear();
                self.configured = configured;
                self.done = true;
                self.generation += 1;
                return;
            }
            // airports and navaids by id, the route every ~25 NM, and the ARTCCs crossed
            let locations: Vec<String> = waypoints.iter().filter(|w| matches!(w.kind, Kind::Airport | Kind::Navaid)).map(|w| if w.kind == Kind::Airport { w.wx_id() } else { w.id.clone() }).collect();
            let path: Vec<[f64; 2]> = waypoints.iter().map(|w| [w.lon, w.lat]).collect();
            let total: f64 = waypoints.windows(2).map(|w| distance_nm(w[0].lat, w[0].lon, w[1].lat, w[1].lon)).sum();
            let n = ((total / 25.0).ceil() as usize).clamp(1, 30);
            let points: Vec<[f64; 2]> = (0..=n).map(|i| point_along(&path, total * i as f64 / n as f64)).collect();
            let artccs: Vec<String> = be
                .server
                .try_data()
                .ok()
                .map(|d| waypoints.iter().filter(|w| w.kind == Kind::Airport).filter_map(|w| d.db.airport(&w.id).ok().flatten()).filter_map(|a| a["artcc"].as_str().map(String::from)).filter(|s| !s.is_empty()).collect())
                .unwrap_or_default();
            let mut artccs = artccs;
            artccs.sort();
            artccs.dedup();
            self.loading = true;
            let st = be.server.clone();
            self.task = be.spawn(async move { flightplan_server::notam::fetch(st, flightplan_server::notam::NotamReq { locations, points, radius: Some(10.0), artccs }).await });
        }
        if let Some(v) = self.task.poll() {
            let (configured, items, errors) = parse(&v);
            self.configured = configured;
            self.items = items;
            self.errors = errors;
            self.loading = false;
            self.done = true;
            self.generation += 1;
        }
    }
}

/// NOTAMs for one airport (the airport card).
pub fn airport(be: &Backend, id: &str) -> Task<Value> {
    let st = be.server.clone();
    let id = id.to_string();
    be.spawn(async move { flightplan_server::notam::fetch(st, flightplan_server::notam::NotamReq { locations: vec![id], points: vec![], radius: None, artccs: vec![] }).await })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn effects_from_text() {
        assert_eq!(classify("!PRC 10/123 PRC AD AP CLSD 2610241300-2610302359", "Aerodrome"), Effect::AerodromeClosed);
        assert_eq!(classify("!PRC 10/124 PRC RWY 03R/21L CLSD 2610241300-2610302359", "Runway"), Effect::RunwayClosed(vec!["03R".into(), "21L".into()]));
        assert_eq!(classify("!SDL 10/5 SDL RWY 21 CLSD TO ARR", "Runway"), Effect::RunwayClosed(vec!["21".into()]));
        assert_eq!(classify("!BUF 10/9 BUF SVC FUEL 100LL UNAVBL", "Services"), Effect::FuelUnavailable);
        assert_eq!(classify("!PXR 10/1 PXR NAV VORTAC U/S", "Navaid"), Effect::NavaidOut);
        assert_eq!(classify("!GPS 10/2 GPS UNREL WI 150NM", "GPS"), Effect::Gps);
        assert_eq!(classify("!PRC 10/3 PRC TWY A CLSD", "Taxiway"), Effect::Other);
    }

    #[test]
    fn times_and_location() {
        let v = json!({"id": "N1", "number": "10/124", "location": "PRC", "icao": "KPRC", "class": "DOM", "category": "Runway",
                       "start": "2026-10-24T13:00:00.000Z", "end": "2026-10-30T23:59:00.000-0500EST", "text": "!PRC 10/124 PRC RWY 03R/21L CLSD", "icaoText": ""});
        let n = Notam::from_json(&v);
        assert!(n.est);
        assert!(n.at_airport("KPRC") && n.at_airport("PRC") && !n.at_airport("KSDL"));
        let t = |s: &str| DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc);
        assert!(n.active_at(t("2026-10-25T00:00:00Z")));
        assert!(!n.active_at(t("2026-10-31T12:00:00Z")));
        assert!(n.plain.to_lowercase().contains("runway") && n.plain.to_lowercase().contains("closed"), "{}", n.plain);
        let perm = Notam::from_json(&json!({"start": "2026-01-01T00:00:00.000Z", "end": "PERM", "text": "X"}));
        assert!(perm.perm && perm.active_at(t("2030-01-01T00:00:00Z")));
    }
}
