//! The active flight plan: ordered waypoints plus planning settings.

use super::place::{new_uid, Place, Waypoint};
use crate::nav::geo::distance_nm;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Route {
    pub waypoints: Vec<Waypoint>,
    pub cruise: f64,
    /// local departure time "YYYY-MM-DDTHH:MM"; empty = now
    #[serde(default)]
    pub departure: String,
    /// minutes on the ground at each fuel stop
    #[serde(default = "default_stop_minutes")]
    pub stop_minutes: f64,
    /// clicking the map adds waypoints
    #[serde(skip)]
    pub editing: bool,
    /// bumped on every change to the waypoints (drives profile / wind refetches); starts at 1
    /// so a route restored from disk differs from "nothing fetched yet" (0)
    #[serde(skip, default = "first_rev")]
    pub rev: u64,
}

fn first_rev() -> u64 {
    1
}

fn default_stop_minutes() -> f64 {
    45.0
}

impl Default for Route {
    fn default() -> Self {
        Route { waypoints: vec![], cruise: 6500.0, departure: String::new(), stop_minutes: default_stop_minutes(), editing: false, rev: 1 }
    }
}

impl Route {
    fn changed(&mut self) {
        self.rev += 1;
    }

    pub fn add(&mut self, p: Place, at: Option<usize>) {
        let wp = Waypoint { place: p, uid: new_uid(), fuel_stop: false };
        match at {
            Some(i) if i < self.waypoints.len() => self.waypoints.insert(i, wp),
            _ => self.waypoints.push(wp),
        }
        self.changed();
    }

    pub fn set(&mut self, i: usize, p: Place) {
        if let Some(w) = self.waypoints.get_mut(i) {
            w.place = p;
            self.changed();
        }
    }

    pub fn remove(&mut self, uid: u64) {
        self.waypoints.retain(|w| w.uid != uid);
        self.changed();
    }

    pub fn move_to(&mut self, from: usize, to: usize) {
        if from < self.waypoints.len() && to < self.waypoints.len() && from != to {
            let w = self.waypoints.remove(from);
            self.waypoints.insert(to, w);
            self.changed();
        }
    }

    pub fn replace(&mut self, list: Vec<Place>) {
        self.waypoints = list.into_iter().map(|p| Waypoint { place: p, uid: new_uid(), fuel_stop: false }).collect();
        self.changed();
    }

    /// Make an intermediate airport a fuel stop, or not. Doesn't change the geometry, so `rev`
    /// stays (the plan hashes the stops itself).
    pub fn toggle_fuel_stop(&mut self, i: usize) {
        let n = self.waypoints.len();
        if let Some(w) = self.waypoints.get_mut(i) {
            if i > 0 && i + 1 < n && w.kind == super::place::Kind::Airport {
                w.fuel_stop = !w.fuel_stop;
            }
        }
    }

    /// Waypoint indices of the fuel stops.
    pub fn fuel_stops(&self) -> Vec<usize> {
        let n = self.waypoints.len();
        self.waypoints.iter().enumerate().filter(|(i, w)| w.fuel_stop && *i > 0 && *i + 1 < n).map(|(i, _)| i).collect()
    }

    pub fn reverse(&mut self) {
        self.waypoints.reverse();
        self.changed();
    }

    pub fn clear(&mut self) {
        self.waypoints.clear();
        self.changed();
    }

    /// Insert a point into whichever leg it is closest to (rubber-banding).
    pub fn leg_index_for(&self, lat: f64, lon: f64) -> usize {
        let w = &self.waypoints;
        let mut best = 1;
        let mut cost = f64::INFINITY;
        for i in 1..w.len() {
            let c = distance_nm(w[i - 1].lat, w[i - 1].lon, lat, lon) + distance_nm(lat, lon, w[i].lat, w[i].lon) - distance_nm(w[i - 1].lat, w[i - 1].lon, w[i].lat, w[i].lon);
            if c < cost {
                cost = c;
                best = i;
            }
        }
        best
    }

    /// [lon, lat] path
    pub fn path(&self) -> Vec<[f64; 2]> {
        self.waypoints.iter().map(|w| [w.lon, w.lat]).collect()
    }

    /// The route as typed: identifiers, with airway runs written as the airway.
    pub fn text(&self) -> String {
        super::place::compact_ids(&self.places(), |p| p.id.clone()).join(" ")
    }

    #[cfg(test)]
    pub fn restored(json: &str) -> Route {
        serde_json::from_str(json).unwrap()
    }

    pub fn departure_time(&self) -> chrono::DateTime<chrono::Utc> {
        use chrono::TimeZone;
        chrono::NaiveDateTime::parse_from_str(&self.departure, "%Y-%m-%dT%H:%M")
            .ok()
            .and_then(|n| chrono::Local.from_local_datetime(&n).single())
            .map(|d| d.with_timezone(&chrono::Utc))
            .unwrap_or_else(chrono::Utc::now)
    }

    pub fn places(&self) -> Vec<&Place> {
        self.waypoints.iter().map(|w| &w.place).collect()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn restored_route_needs_a_fetch() {
        let r = super::Route::restored(r#"{"waypoints": [], "cruise": 6500.0, "departure": ""}"#);
        assert_eq!(r.rev, 1, "0 means 'already fetched' to the plan");
    }
}
