//! Derived flight analysis for the active route: terrain/airspace profile, winds aloft and
//! the navlog, hazards, departure / destination forecasts and the go / no-go verdict.
//! Refetches (debounced) when the route changes; recomputes the cheap parts whenever any input
//! (route, aircraft, departure time, weather, minimums) changes.

use crate::backend::{Backend, Task};
use crate::model::aircraft::Aircraft;
use crate::model::place::Kind;
use crate::model::route::Route;
use crate::nav::hazards::{self, Hazard};
use crate::nav::navlog::{self, NavLog, WindPoint};
use crate::nav::profile::{self, Incursion, Profile, TerrainConflict};
use crate::nav::verdict::{self, Minimums, Verdict};
use crate::wx::forecast::RouteForecast;
use crate::wx::state::Wx;
use serde_json::Value;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};

#[derive(Default)]
pub struct EndInfo {
    pub dep_elev: Option<f64>,
    pub dest_elev: Option<f64>,
    /// (runway end id, true heading) by airport id, for every airport on the route
    pub runways: HashMap<String, Vec<(String, f64)>>,
    /// field elevation by airport id
    pub elevs: HashMap<String, f64>,
}

#[derive(Default)]
pub struct Plan {
    pub profile: Profile,
    pub winds: Vec<Option<WindPoint>>,
    pub wind_source: String,
    pub dep_elev: f64,
    pub dest_elev: f64,
    pub ends: EndInfo,
    pub loading: bool,
    pub error: String,
    /// along-track NM under the pointer (shared by map and profile); true = from the profile
    pub hover: Option<(f64, bool)>,
    /// airspace picked in the profile (index into profile.airspace)
    pub picked: Option<usize>,

    pub log: Option<NavLog>,
    pub alt_path: Vec<[f64; 2]>,
    pub conflicts: Vec<TerrainConflict>,
    pub incursions: Vec<Incursion>,
    pub hazards: Vec<Hazard>,
    /// METAR / TAF / NBM at the ends and model weather along the route
    pub forecast: RouteForecast,
    /// route NOTAMs (FAA NOTAM API)
    pub notams: crate::wx::notams::RouteNotams,
    pub verdict: Option<Verdict>,
    /// the call for departures every 3 h over the next five days: (unix time, call)
    pub windows: Vec<(i64, verdict::Call)>,

    fetched_rev: u64,
    changed_at: Option<Instant>,
    profile_task: Task<anyhow::Result<(Profile, EndInfo)>>,
    winds_task: Task<Value>,
    generation: u64,
    derived_key: u64,
    windows_key: u64,
    /// route revision the profile in hand belongs to
    profile_rev: u64,
    pending_rev: u64,
}

async fn fetch_profile(be: std::sync::Arc<flightplan_server::AppState>, path: Vec<[f64; 2]>, ends: Vec<(usize, String)>) -> anyhow::Result<(Profile, EndInfo)> {
    let req = flightplan_server::terrain::ProfileReq { path, step: None, corridor: Some(1.0) };
    let v = flightplan_server::route_profile(&be, &req).await?;
    let profile: Profile = serde_json::from_value(v)?;
    let mut info = EndInfo::default();
    for (i, id) in ends {
        let Ok(data) = be.try_data() else { break };
        let Ok(Some(a)) = data.db.airport(&id) else { continue };
        if let Some(e) = a["elev"].as_f64() {
            match i {
                0 => info.dep_elev = Some(e),
                1 => info.dest_elev = Some(e),
                _ => {}
            }
            info.elevs.insert(id.clone(), e);
        }
        let mut rw = Vec::new();
        for r in a["runways"].as_array().into_iter().flatten() {
            for e in r["ends"].as_array().into_iter().flatten() {
                if let (Some(eid), Some(h)) = (e["id"].as_str(), e["trueHdg"].as_f64()) {
                    rw.push((eid.to_string(), h));
                }
            }
        }
        info.runways.insert(id.clone(), rw);
    }
    Ok((profile, info))
}

impl Plan {
    pub fn total(&self) -> f64 {
        self.profile.samples.last().map(|s| s.d).unwrap_or(0.0)
    }

    pub fn alt_at(&self, d: f64) -> f64 {
        profile::alt_at(&self.alt_path, d)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update(&mut self, be: &Backend, route: &Route, ac: Option<&Aircraft>, wx: &Wx, outlooks: &crate::wx::official::Outlooks, minimums: &Minimums, data_expired: bool, notams_configured: bool) {
        self.forecast.update(be, &route.waypoints);
        self.notams.update(be, &route.waypoints, notams_configured);
        // ---- refetch geometry-dependent data, debounced ----
        if route.rev != self.fetched_rev {
            let first = self.changed_at.is_none();
            let at = *self.changed_at.get_or_insert_with(Instant::now);
            if route.waypoints.len() < 2 {
                self.profile = Profile::default();
                self.winds.clear();
                self.picked = None;
                self.fetched_rev = route.rev;
                self.changed_at = None;
                self.generation += 1;
            } else if at.elapsed() >= Duration::from_millis(250) || (first && self.fetched_rev == 0) {
                self.fetched_rev = route.rev;
                self.changed_at = None;
                let path = route.path();
                // every airport: departure / destination elevations, plus runways and field
                // elevations for crosswind and fuel stops (0 = first, 1 = last, 2 = in between)
                let n = route.waypoints.len();
                let ends: Vec<(usize, String)> = route
                    .waypoints
                    .iter()
                    .enumerate()
                    .filter(|(_, w)| w.kind == Kind::Airport)
                    .map(|(i, w)| (if i == 0 { 0 } else if i + 1 == n { 1 } else { 2 }, w.id.clone()))
                    .collect();
                self.loading = true;
                self.pending_rev = route.rev;
                self.picked = None;
                self.profile_task = be.spawn(fetch_profile(be.server.clone(), path.clone(), ends));
                let st = be.server.clone();
                self.winds_task = be.spawn(async move { flightplan_server::wx::winds_at(&st, path).await });
            } else {
                be.ctx.request_repaint_after(Duration::from_millis(260));
            }
        }
        if let Some(r) = self.profile_task.poll() {
            self.loading = false;
            match r {
                Ok((p, info)) => {
                    self.profile_rev = self.pending_rev;
                    self.dep_elev = info.dep_elev.unwrap_or_else(|| p.samples.first().map(|s| s.elev).unwrap_or(0.0));
                    self.dest_elev = info.dest_elev.unwrap_or_else(|| p.samples.last().map(|s| s.elev).unwrap_or(0.0));
                    self.profile = p;
                    self.ends = info;
                    self.error.clear();
                }
                Err(e) => self.error = format!("{e:#}"),
            }
            self.generation += 1;
        }
        if let Some(v) = self.winds_task.poll() {
            self.wind_source = v["source"].as_str().unwrap_or("").to_string();
            self.winds = v["points"].as_array().map(|a| a.iter().map(|p| serde_json::from_value(p.clone()).ok()).collect()).unwrap_or_default();
            self.generation += 1;
        }

        // ---- derived, recomputed when any input changes ----
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (route.rev, route.cruise.to_bits(), self.generation, wx.generation, self.forecast.generation, self.notams.generation, data_expired).hash(&mut h);
        format!("{minimums:?}{:?}{:?}{:?}{}", outlooks.wpc_day1_end, outlooks.spc_day1_end, route.fuel_stops(), route.stop_minutes).hash(&mut h);
        if let Some(a) = ac {
            format!("{a:?}").hash(&mut h);
        }
        let base = h.finish();
        // the verdict's "now" moves on: re-evaluate every few minutes
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (base, &route.departure, chrono::Utc::now().timestamp() / 300).hash(&mut h);
        let key = h.finish();
        // the 5-day go / no-go strip doesn't depend on the chosen departure time
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (base, chrono::Utc::now().timestamp() / 1800).hash(&mut h);
        let windows_key = h.finish();
        if key == self.derived_key && windows_key == self.windows_key {
            return;
        }
        let Some(ac) = ac.filter(|_| route.waypoints.len() >= 2) else {
            self.derived_key = key;
            self.windows_key = windows_key;
            self.log = None;
            self.alt_path.clear();
            self.conflicts.clear();
            self.incursions.clear();
            self.hazards.clear();
            self.verdict = None;
            self.windows.clear();
            return;
        };
        // judge only with the profile for this route and the first forecasts in hand: a call
        // made without terrain or weather would read as a false "go"
        let ready = self.profile_rev == route.rev && !self.loading && !self.profile.samples.is_empty() && !(self.forecast.loading && self.forecast.dep.is_none());
        if key != self.derived_key {
            self.derived_key = key;
            let departure = route.departure_time();
            let (log, alt_path, hazards) = self.analyze_at(route, ac, wx, departure);
            self.alt_path = alt_path;
            self.conflicts = profile::terrain_conflicts(&self.profile, &self.alt_path);
            self.incursions = profile::incursions(&self.profile, &self.alt_path);
            self.hazards = hazards;
            self.verdict = ready.then(|| self.judge(route, ac, &log, &self.hazards, departure, outlooks, minimums, data_expired));
            self.log = Some(log);
            if !ready {
                // look again next frame
                self.derived_key = 0;
                be.ctx.request_repaint_after(Duration::from_millis(250));
            }
        }
        if windows_key != self.windows_key && ready && !self.forecast.loading {
            self.windows_key = windows_key;
            // every 3 hours for five days
            let start = (chrono::Utc::now().timestamp() / 3600 + 1) * 3600;
            self.windows = (0..=40)
                .filter_map(|i| {
                    let t = chrono::DateTime::from_timestamp(start + i * 3 * 3600, 0)?;
                    let (log, _, hazards) = self.analyze_at(route, ac, wx, t);
                    let v = self.judge(route, ac, &log, &hazards, t, outlooks, minimums, data_expired);
                    Some((t.timestamp(), v.call))
                })
                .collect();
        }
    }

    /// Navlog, altitude path and hazards for a departure time.
    fn analyze_at(&self, route: &Route, ac: &Aircraft, wx: &Wx, departure: chrono::DateTime<chrono::Utc>) -> (NavLog, Vec<[f64; 2]>, Vec<Hazard>) {
        // fuel stops with their field elevation (the database's, else the terrain there)
        let mut along = 0.0;
        let mut stops = Vec::new();
        for (i, w) in route.waypoints.iter().enumerate() {
            if i > 0 {
                let a = &route.waypoints[i - 1];
                along += crate::nav::geo::distance_nm(a.lat, a.lon, w.lat, w.lon);
            }
            if w.fuel_stop && i > 0 && i + 1 < route.waypoints.len() {
                let elev = self.ends.elevs.get(&w.id).copied().or_else(|| self.profile.samples.iter().min_by(|a, b| (a.d - along).abs().total_cmp(&(b.d - along).abs())).map(|s| s.elev)).unwrap_or(0.0);
                stops.push((i, elev));
            }
        }
        let log = navlog::build(&navlog::NavInput {
            waypoints: &route.waypoints,
            aircraft: ac,
            cruise: route.cruise,
            departure,
            dep_elev: self.dep_elev,
            dest_elev: self.dest_elev,
            winds: &self.winds,
            stops: &stops,
            ground_min: route.stop_minutes,
        });
        let alt_path = if log.rows.is_empty() { vec![] } else { log.alt_path.clone() };
        let stop_dists: Vec<f64> = log.segments.iter().rev().skip(1).map(|s| s.end_d).collect();
        let hazards = hazards::analyze(&hazards::HazardInput {
            waypoints: &route.waypoints,
            profile: &self.profile,
            altitude_path: &alt_path,
            log: Some(&log),
            aircraft: Some(ac),
            departure,
            tfrs: &wx.tfrs,
            sigmets: &wx.sigmets,
            gairmets: &wx.gairmets,
            stop_dists: &stop_dists,
        });
        (log, alt_path, hazards)
    }

    /// The go / no-go call for the plan at a departure time.
    #[allow(clippy::too_many_arguments)]
    fn judge(&self, route: &Route, ac: &Aircraft, log: &NavLog, hazards: &[Hazard], departure: chrono::DateTime<chrono::Utc>, outlooks: &crate::wx::official::Outlooks, minimums: &Minimums, data_expired: bool) -> Verdict {
        let eta = departure + chrono::Duration::milliseconds((log.total_time * 60_000.0) as i64);
        let fc = &self.forecast;
        let empty = Vec::new();
        let mut ends = Vec::new();
        let n = route.waypoints.len();
        // departure, each fuel stop at its arrival time, destination
        let mut points: Vec<(&'static str, usize, Option<&crate::wx::forecast::StationWx>, f64, chrono::DateTime<chrono::Utc>)> = vec![("Departure", 0, fc.dep.as_ref(), self.dep_elev, departure)];
        for seg in log.segments.iter().filter(|s| s.to_index + 1 < n) {
            let id = &route.waypoints[seg.to_index].id;
            let elev = self.ends.elevs.get(id).copied().unwrap_or(0.0);
            points.push(("Fuel stop", seg.to_index, fc.stops.iter().find(|s| &s.airport == id), elev, seg.arrive));
        }
        points.push(("Destination", n - 1, fc.dest.as_ref(), self.dest_elev, eta));
        let conds: Vec<Option<crate::wx::forecast::Conditions>> = points.iter().map(|p| p.2.and_then(|s| s.at(p.4))).collect();
        for (k, (role, i, stn, elev, when)) in points.iter().enumerate() {
            let w = &route.waypoints[*i];
            ends.push(verdict::End {
                role,
                airport: &w.id,
                is_airport: w.kind == Kind::Airport && (stn.is_some() || !fc.loading),
                cond: conds[k].as_ref(),
                runways: self.ends.runways.get(&w.id).unwrap_or(&empty),
                elev: *elev,
                offset_nm: stn.map(|s| s.offset_nm).unwrap_or(0.0),
                when: *when,
            });
        }
        // the official outlook days the flight spans
        let (spc_a, spc_b) = (outlooks.spc_day(departure), outlooks.spc_day(eta));
        let (wpc_a, wpc_b) = (outlooks.wpc_day(departure), outlooks.wpc_day(eta));
        let official: Vec<&crate::wx::official::Hit> = fc
            .official
            .iter()
            .filter(|h| if h.product.starts_with("SPC") { (spc_a..=spc_b).contains(&h.day) } else { (wpc_a..=wpc_b).contains(&h.day) })
            .collect();
        let max_terrain = self.profile.samples.iter().map(|s| s.max).fold(0.0, f64::max);
        verdict::evaluate(&verdict::Input {
            hazards,
            ends,
            official,
            official_known: !fc.loading,
            notams: (self.notams.configured && self.notams.done).then_some(self.notams.items.as_slice()),
            route_navaids: route.waypoints.iter().filter(|w| w.kind == Kind::Navaid).map(|w| w.id.clone()).collect(),
            log: Some(log),
            cruise_ft: route.cruise,
            max_terrain_ft: max_terrain,
            max_xwind: ac.perf.max_xwind,
            minimums,
            departure,
            now: chrono::Utc::now(),
            data_expired,
        })
    }
}
