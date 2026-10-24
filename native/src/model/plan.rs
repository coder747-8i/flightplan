//! Derived flight analysis for the active route: terrain/airspace profile, winds aloft and
//! the navlog, hazards. Refetches (debounced) when the route changes; recomputes the cheap
//! parts whenever any input changes.

use crate::backend::{Backend, Task};
use crate::model::aircraft::Aircraft;
use crate::model::place::Kind;
use crate::model::route::Route;
use crate::nav::hazards::{self, Hazard};
use crate::nav::navlog::{self, NavLog, WindPoint};
use crate::nav::profile::{self, Incursion, Profile, TerrainConflict};
use crate::wx::state::Wx;
use serde_json::Value;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};

#[derive(Default)]
pub struct EndInfo {
    pub dep_elev: Option<f64>,
    pub dest_elev: Option<f64>,
    pub metars: HashMap<String, Value>,
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

    fetched_rev: u64,
    changed_at: Option<Instant>,
    profile_task: Task<anyhow::Result<(Profile, EndInfo)>>,
    winds_task: Task<Value>,
    generation: u64,
    derived_key: u64,
}

async fn fetch_profile(be: std::sync::Arc<flightplan_server::AppState>, path: Vec<[f64; 2]>, ends: Vec<(usize, String)>) -> anyhow::Result<(Profile, EndInfo)> {
    let req = flightplan_server::terrain::ProfileReq { path, step: None, corridor: Some(1.0) };
    let v = flightplan_server::route_profile(&be, &req).await?;
    let profile: Profile = serde_json::from_value(v)?;
    let mut info = EndInfo::default();
    let mut wx_ids = Vec::new();
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
        let icao = a["icao"].as_str().filter(|s| !s.is_empty()).map(String::from).unwrap_or_else(|| if id.len() == 3 { format!("K{id}") } else { id.clone() });
        wx_ids.push(icao);
    }
    if !wx_ids.is_empty() {
        if let Ok(Value::Array(list)) = flightplan_server::wx::metars(&be, Some(&wx_ids.join(",")), None).await {
            for m in list {
                if let Some(id) = m["icaoId"].as_str() {
                    info.metars.insert(id.to_string(), m.clone());
                }
            }
        }
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

    pub fn update(&mut self, be: &Backend, route: &Route, ac: Option<&Aircraft>, wx: &Wx) {
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
        (route.rev, route.cruise.to_bits(), &route.departure, self.generation, wx.generation).hash(&mut h);
        if let Some(a) = ac {
            format!("{a:?}").hash(&mut h);
        }
        let key = h.finish();
        if key == self.derived_key {
            return;
        }
        self.derived_key = key;
        let Some(ac) = ac.filter(|_| route.waypoints.len() >= 2) else {
            self.log = None;
            self.alt_path.clear();
            self.conflicts.clear();
            self.incursions.clear();
            self.hazards.clear();
            return;
        };
        let departure = route.departure_time();
        let log = navlog::build(&navlog::NavInput {
            waypoints: &route.waypoints,
            aircraft: ac,
            cruise: route.cruise,
            departure,
            dep_elev: self.dep_elev,
            dest_elev: self.dest_elev,
            winds: &self.winds,
        });
        self.alt_path = if log.rows.is_empty() {
            vec![]
        } else {
            let cruise = route.cruise.max(self.dep_elev);
            vec![[0.0, self.dep_elev], [log.toc_dist.unwrap_or(0.0), cruise], [log.tod_dist.unwrap_or(log.total_dist), cruise], [log.total_dist, self.dest_elev + 1000.0]]
        };
        self.conflicts = profile::terrain_conflicts(&self.profile, &self.alt_path);
        self.incursions = profile::incursions(&self.profile, &self.alt_path);
        self.hazards = hazards::analyze(&hazards::HazardInput {
            waypoints: &route.waypoints,
            profile: &self.profile,
            altitude_path: &self.alt_path,
            log: Some(&log),
            aircraft: Some(ac),
            departure,
            tfrs: &wx.tfrs,
            sigmets: &wx.sigmets,
            gairmets: &wx.gairmets,
            metars: &self.ends.metars,
            runways: &self.ends.runways,
        });
        self.log = Some(log);
    }
}
