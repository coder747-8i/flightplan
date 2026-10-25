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
use crate::wx::forecast::{self, RouteForecast};
use crate::wx::state::Wx;
use serde_json::Value;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};

#[derive(Default)]
pub struct EndInfo {
    pub dep_elev: Option<f64>,
    pub dest_elev: Option<f64>,
    /// (runway end id, true heading) by airport id
    pub runways: HashMap<String, Vec<(String, f64)>>,
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
            if i == 0 {
                info.dep_elev = Some(e);
            } else {
                info.dest_elev = Some(e);
            }
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

    pub fn update(&mut self, be: &Backend, route: &Route, ac: Option<&Aircraft>, wx: &Wx, minimums: &Minimums, data_expired: bool) {
        self.forecast.update(be, &route.waypoints);
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
                let mut ends = Vec::new();
                if let Some(w) = route.waypoints.first().filter(|w| w.kind == Kind::Airport) {
                    ends.push((0, w.id.clone()));
                }
                if let Some(w) = route.waypoints.last().filter(|w| w.kind == Kind::Airport) {
                    ends.push((1, w.id.clone()));
                }
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
        (route.rev, route.cruise.to_bits(), self.generation, wx.generation, self.forecast.generation, data_expired).hash(&mut h);
        format!("{minimums:?}").hash(&mut h);
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
            self.verdict = ready.then(|| self.judge(route, ac, &log, &self.hazards, departure, minimums, data_expired));
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
                    let v = self.judge(route, ac, &log, &hazards, t, minimums, data_expired);
                    Some((t.timestamp(), v.call))
                })
                .collect();
        }
    }

    /// Navlog, altitude path and hazards for a departure time.
    fn analyze_at(&self, route: &Route, ac: &Aircraft, wx: &Wx, departure: chrono::DateTime<chrono::Utc>) -> (NavLog, Vec<[f64; 2]>, Vec<Hazard>) {
        let log = navlog::build(&navlog::NavInput {
            waypoints: &route.waypoints,
            aircraft: ac,
            cruise: route.cruise,
            departure,
            dep_elev: self.dep_elev,
            dest_elev: self.dest_elev,
            winds: &self.winds,
        });
        let alt_path = if log.rows.is_empty() {
            vec![]
        } else {
            let cruise = route.cruise.max(self.dep_elev);
            vec![[0.0, self.dep_elev], [log.toc_dist.unwrap_or(0.0), cruise], [log.tod_dist.unwrap_or(log.total_dist), cruise], [log.total_dist, self.dest_elev + 1000.0]]
        };
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
        });
        (log, alt_path, hazards)
    }

    /// The go / no-go call for the plan at a departure time.
    #[allow(clippy::too_many_arguments)]
    fn judge(&self, route: &Route, ac: &Aircraft, log: &NavLog, hazards: &[Hazard], departure: chrono::DateTime<chrono::Utc>, minimums: &Minimums, data_expired: bool) -> Verdict {
        let eta = departure + chrono::Duration::milliseconds((log.total_time * 60_000.0) as i64);
        let total = log.total_dist.max(1.0);
        let fc = &self.forecast;
        let dep_c = fc.dep.as_ref().and_then(|s| s.at(departure));
        let dest_c = fc.dest.as_ref().and_then(|s| s.at(eta));
        let empty = Vec::new();
        let mut ends = Vec::new();
        for (role, w, stn, cond, elev, when) in [
            ("Departure", route.waypoints.first(), fc.dep.as_ref(), dep_c.as_ref(), self.dep_elev, departure),
            ("Destination", route.waypoints.last(), fc.dest.as_ref(), dest_c.as_ref(), self.dest_elev, eta),
        ] {
            let Some(w) = w else { continue };
            ends.push(verdict::End {
                role,
                airport: &w.id,
                is_airport: w.kind == Kind::Airport && (stn.is_some() || !fc.loading),
                cond,
                runways: self.ends.runways.get(&w.id).unwrap_or(&empty),
                elev,
                offset_nm: stn.map(|s| s.offset_nm).unwrap_or(0.0),
                when,
            });
        }
        let dep_ts = departure.timestamp();
        let en_route = (!fc.model.is_empty()).then(|| forecast::en_route(&fc.model, route.cruise, |d| dep_ts + (log.total_time * 60.0 * d / total) as i64));
        let max_terrain = self.profile.samples.iter().map(|s| s.max).fold(0.0, f64::max);
        verdict::evaluate(&verdict::Input {
            hazards,
            ends,
            en_route: en_route.as_ref(),
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
