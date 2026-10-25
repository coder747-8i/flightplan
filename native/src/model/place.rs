//! Route points: airports, navaids, fixes and lat/long user waypoints.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Airport,
    Navaid,
    Fix,
    Coord,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Place {
    pub kind: Kind,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub detail: String,
    pub lat: f64,
    pub lon: f64,
}

impl Place {
    pub fn from_server(p: &flightplan_server::db::Place) -> Self {
        Place {
            kind: match p.kind {
                "airport" => Kind::Airport,
                "navaid" => Kind::Navaid,
                "fix" => Kind::Fix,
                _ => Kind::Coord,
            },
            id: p.id.clone(),
            name: p.name.clone(),
            detail: p.detail.clone(),
            lat: p.lat,
            lon: p.lon,
        }
    }

    pub fn coord(lat: f64, lon: f64) -> Self {
        Place { kind: Kind::Coord, id: crate::nav::geo::coord_id(lat, lon), name: "User waypoint".into(), detail: format!("{lat:.4}, {lon:.4}"), lat, lon }
    }

    /// The identifier aviationweather.gov knows an airport by (K-prefix for 3-letter US ids).
    pub fn wx_id(&self) -> String {
        if self.id.len() == 3 { format!("K{}", self.id) } else { self.id.clone() }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Waypoint {
    #[serde(flatten)]
    pub place: Place,
    pub uid: u64,
    /// land here and top off before going on (airports in the middle of the route)
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub fuel_stop: bool,
}

impl std::ops::Deref for Waypoint {
    type Target = Place;
    fn deref(&self) -> &Place {
        &self.place
    }
}

pub fn new_uid() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0);
    t.wrapping_mul(31).wrapping_add(NEXT.fetch_add(1, Ordering::Relaxed))
}
