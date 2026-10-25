//! What the app remembers between runs. Aircraft and setup preferences use the same files
//! as the web app's server (user/aircraft.json, user/prefs.json); the rest is in
//! user/desktop.json.

use crate::map::style::{AltitudeFilter, Layers};
use crate::model::aircraft::{self, Aircraft};
use crate::model::route::Route;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct WxShow {
    pub radar: bool,
    pub metars: bool,
    pub tfr: bool,
    pub sigmet: bool,
    pub gairmet: bool,
    pub pireps: bool,
    pub winds: bool,
    /// SPC convective outlook for the timeline's day
    #[serde(default)]
    pub spc: bool,
}

impl Default for WxShow {
    fn default() -> Self {
        WxShow { radar: false, metars: true, tfr: true, sigmet: true, gairmet: false, pireps: false, winds: false, spc: false }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Saved {
    pub night: bool,
    pub layers: Layers,
    pub altitude: AltitudeFilter,
    pub wx: WxShow,
    pub radar_opacity: f32,
    pub wind_level: usize,
    pub view: [f64; 3],
    pub profile_open: bool,
    pub raw_wx: bool,
    pub route: Route,
    /// personal minimums for the go / no-go call
    pub minimums: crate::nav::verdict::Minimums,
}

impl Default for Saved {
    fn default() -> Self {
        Saved {
            night: false,
            layers: Layers::default(),
            altitude: AltitudeFilter::default(),
            wx: WxShow::default(),
            radar_opacity: 0.65,
            wind_level: 2,
            view: [-112.1, 33.5, 8.0],
            profile_open: true,
            raw_wx: false,
            route: Route::default(),
            minimums: Default::default(),
        }
    }
}

/// The web app's prefs document (shared): setup finished, custom data-pack address.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Prefs {
    #[serde(default)]
    pub onboarded: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_url: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Fleet {
    pub list: Vec<Aircraft>,
    #[serde(default)]
    pub saved_at: i64,
    /// desktop addition: which aircraft is selected
    #[serde(default)]
    pub active: String,
}

impl Default for Fleet {
    fn default() -> Self {
        let a = aircraft::from_preset(aircraft::presets()[0].clone(), "N12345");
        Fleet { active: a.id.clone(), list: vec![a], saved_at: 0 }
    }
}

impl Fleet {
    pub fn active(&self) -> Option<&Aircraft> {
        self.list.iter().find(|a| a.id == self.active).or(self.list.first())
    }
    pub fn active_mut(&mut self) -> Option<&mut Aircraft> {
        let id = self.active.clone();
        let i = self.list.iter().position(|a| a.id == id).unwrap_or(0);
        self.list.get_mut(i)
    }
    pub fn upsert(&mut self, a: Aircraft) {
        self.active = a.id.clone();
        match self.list.iter_mut().find(|x| x.id == a.id) {
            Some(x) => *x = a,
            None => self.list.push(a),
        }
    }
    pub fn remove(&mut self, id: &str) {
        self.list.retain(|a| a.id != id);
        if self.active == id {
            self.active = self.list.first().map(|a| a.id.clone()).unwrap_or_default();
        }
    }
    pub fn duplicate(&mut self, id: &str) {
        if let Some(src) = self.list.iter().find(|a| a.id == id).cloned() {
            let mut c = src;
            c.id = aircraft::uid();
            c.tail.clear();
            c.name = format!("{} (copy)", c.name);
            self.active = c.id.clone();
            self.list.push(c);
        }
    }
}

pub struct Store {
    dir: PathBuf,
}

fn read<T: for<'de> Deserialize<'de>>(p: &Path) -> Option<T> {
    serde_json::from_slice(&std::fs::read(p).ok()?).ok()
}

fn write<T: Serialize>(p: &Path, v: &T) -> anyhow::Result<()> {
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d)?;
    }
    let tmp = p.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(v)?)?;
    std::fs::rename(&tmp, p)?;
    Ok(())
}

impl Store {
    pub fn new(user_dir: PathBuf) -> Self {
        Store { dir: user_dir }
    }
    pub fn load_saved(&self) -> Saved {
        read(&self.dir.join("desktop.json")).unwrap_or_default()
    }
    pub fn save_saved(&self, s: &Saved) {
        if let Err(e) = write(&self.dir.join("desktop.json"), s) {
            tracing::warn!("couldn't save state: {e:#}");
        }
    }
    pub fn load_fleet(&self) -> Fleet {
        match read::<Fleet>(&self.dir.join("aircraft.json")) {
            Some(mut f) if !f.list.is_empty() => {
                if !f.list.iter().any(|a| a.id == f.active) {
                    f.active = f.list[0].id.clone();
                }
                f
            }
            _ => Fleet::default(),
        }
    }
    pub fn save_fleet(&self, f: &mut Fleet) {
        f.saved_at = chrono::Utc::now().timestamp_millis();
        if let Err(e) = write(&self.dir.join("aircraft.json"), f) {
            tracing::warn!("couldn't save aircraft: {e:#}");
        }
    }
    pub fn load_prefs(&self) -> Prefs {
        read(&self.dir.join("prefs.json")).unwrap_or_default()
    }
    pub fn save_prefs(&self, p: &Prefs) {
        let _ = write(&self.dir.join("prefs.json"), p);
    }
}
