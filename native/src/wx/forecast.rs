//! Airport conditions at a planned time, up to about five days out: the METAR within the next
//! hour, the TAF while it is valid, then the National Blend of Models text guidance (via the
//! Iowa Environmental Mesonet API: NBS is 3-hourly with ceiling and visibility to 72 h, NBE is
//! 12-hourly wind / sky / precipitation / thunder to ~10 days). Plus model weather sampled along
//! the route (Open-Meteo), which covers the en-route part and the days no TAF reaches.

use crate::backend::{Backend, Task};
use crate::model::place::{Kind, Waypoint};
use crate::nav::geo::{distance_nm, point_along};
use crate::wx::decode::str_of;
use chrono::{DateTime, TimeZone, Utc};
use serde_json::Value;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Metar,
    Taf,
    /// NBM short range (3-hourly, ceiling and visibility)
    Nbm,
    /// NBM extended (12-hourly, no ceiling / visibility)
    NbmExtended,
}

impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Source::Metar => "METAR",
            Source::Taf => "TAF",
            Source::Nbm => "NBM forecast",
            Source::NbmExtended => "NBM extended forecast",
        }
    }
}

/// Weather at one airport at one time, whatever the source.
#[derive(Clone, Debug)]
pub struct Conditions {
    /// the station the numbers come from (may be a neighbour of the airport)
    pub station: String,
    pub source: Source,
    pub valid: DateTime<Utc>,
    /// ceiling in ft AGL; None = no ceiling. Meaningless when `ceiling_known` is false.
    pub ceiling: Option<f64>,
    pub ceiling_known: bool,
    /// statute miles; None = unknown
    pub vis: Option<f64>,
    /// None = variable / calm
    pub wdir: Option<f64>,
    pub wspd: Option<f64>,
    pub gust: Option<f64>,
    pub sky_pct: Option<f64>,
    /// probability of precipitation, %
    pub pop: Option<f64>,
    /// probability of thunder, %
    pub tstm: Option<f64>,
    /// present / forecast weather, e.g. "-RA BR" or "TS"
    pub wx: String,
    pub temp_c: Option<f64>,
    pub altim_hpa: Option<f64>,
    /// worst TEMPO / PROB group covering the time: (category, short text)
    pub tempo: Option<(&'static str, String)>,
}

impl Conditions {
    fn blank(station: &str, source: Source, valid: DateTime<Utc>) -> Self {
        Conditions {
            station: station.to_string(),
            source,
            valid,
            ceiling: None,
            ceiling_known: false,
            vis: None,
            wdir: None,
            wspd: None,
            gust: None,
            sky_pct: None,
            pop: None,
            tstm: None,
            wx: String::new(),
            temp_c: None,
            altim_hpa: None,
            tempo: None,
        }
    }

    /// VFR / MVFR / IFR / LIFR, when ceiling and visibility are both known.
    pub fn category(&self) -> Option<&'static str> {
        if !self.ceiling_known && self.vis.is_none() {
            return None;
        }
        Some(category(if self.ceiling_known { self.ceiling } else { None }, self.vis))
    }

    pub fn thunder(&self) -> bool {
        self.wx.contains("TS") || self.tempo.as_ref().is_some_and(|t| t.1.contains("TS"))
    }

    /// "Ceiling 2,500 ft · 4 SM · wind 240° 12G22 kt"
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        if self.ceiling_known {
            parts.push(match self.ceiling {
                Some(c) => format!("ceiling {} ft", crate::nav::navlog::thousands(c)),
                None => "no ceiling".into(),
            });
        } else if let Some(s) = self.sky_pct {
            parts.push(format!("{}% sky cover", s.round()));
        }
        if let Some(v) = self.vis {
            parts.push(if v >= 10.0 { "10+ SM".into() } else { format!("{} SM", trim(v)) });
        }
        match (self.wdir, self.wspd) {
            (_, Some(s)) if s < 3.0 && self.gust.is_none() => parts.push("calm".into()),
            (Some(d), Some(s)) => parts.push(format!("wind {:03}° {}{} kt", d.round() as i64, s.round(), self.gust.map(|g| format!("G{}", g.round())).unwrap_or_default())),
            (None, Some(s)) => parts.push(format!("wind variable {}{} kt", s.round(), self.gust.map(|g| format!("G{}", g.round())).unwrap_or_default())),
            _ => {}
        }
        if !self.wx.is_empty() {
            parts.push(self.wx.clone());
        }
        if let Some(t) = self.tstm.filter(|t| *t >= 10.0) {
            parts.push(format!("{}% thunder", t.round()));
        } else if let Some(p) = self.pop.filter(|p| *p >= 30.0) {
            parts.push(format!("{}% precip", p.round()));
        }
        let mut s = parts.join(" · ");
        if let Some(c) = s.get_mut(0..1) {
            c.make_ascii_uppercase();
        }
        s
    }
}

fn trim(v: f64) -> String {
    if (v - v.round()).abs() < 0.05 { format!("{}", v.round()) } else { format!("{v:.1}") }
}

/// FAA flight category from ceiling (ft, None = none) and visibility (SM, None = unknown).
pub fn category(ceiling: Option<f64>, vis: Option<f64>) -> &'static str {
    let c = ceiling.unwrap_or(f64::INFINITY);
    let v = vis.unwrap_or(f64::INFINITY);
    if c < 500.0 || v < 1.0 {
        "LIFR"
    } else if c < 1000.0 || v < 3.0 {
        "IFR"
    } else if c <= 3000.0 || v <= 5.0 {
        "MVFR"
    } else {
        "VFR"
    }
}

fn num(v: &Value) -> Option<f64> {
    v.as_f64().or_else(|| v.as_str().and_then(|s| s.trim().trim_end_matches('+').parse().ok()))
}

/// Ceiling (lowest BKN / OVC / OVX layer, or vertical visibility) from AWC cloud groups.
fn ceiling_of(clouds: &Value, vert_vis: &Value) -> Option<f64> {
    let mut c = clouds
        .as_array()
        .into_iter()
        .flatten()
        .filter(|l| matches!(l["cover"].as_str(), Some("BKN" | "OVC" | "OVX")))
        .filter_map(|l| num(&l["base"]))
        .fold(f64::INFINITY, f64::min);
    if let Some(vv) = num(vert_vis) {
        c = c.min(vv * if vv < 100.0 { 100.0 } else { 1.0 });
    }
    c.is_finite().then_some(c)
}

fn wdir(v: &Value) -> Option<f64> {
    if v.as_str() == Some("VRB") { None } else { num(v) }
}

pub fn from_metar(m: &Value) -> Conditions {
    let t = m["obsTime"].as_i64().and_then(|s| Utc.timestamp_opt(s, 0).single()).unwrap_or_else(Utc::now);
    let mut c = Conditions::blank(&str_of(&m["icaoId"]), Source::Metar, t);
    c.ceiling = ceiling_of(&m["clouds"], &m["vertVis"]);
    c.ceiling_known = true;
    c.vis = num(&m["visib"]);
    c.wdir = wdir(&m["wdir"]);
    c.wspd = num(&m["wspd"]);
    c.gust = num(&m["wgst"]);
    c.wx = str_of(&m["wxString"]);
    c.temp_c = num(&m["temp"]);
    c.altim_hpa = num(&m["altim"]);
    c
}

/// TAF conditions at `when` (None outside the TAF's validity).
pub fn from_taf(taf: &Value, when: DateTime<Utc>) -> Option<Conditions> {
    let t = when.timestamp();
    let (from, to) = (taf["validTimeFrom"].as_i64()?, taf["validTimeTo"].as_i64()?);
    if t < from || t >= to {
        return None;
    }
    let fc = taf["fcsts"].as_array()?;
    let mut c = Conditions::blank(&str_of(&taf["icaoId"]), Source::Taf, when);
    c.ceiling_known = true;
    let mut base: Option<&Value> = None;
    // the prevailing group: the last initial / FM / BECMG group that has started
    for f in fc {
        let change = f["fcstChange"].as_str().unwrap_or("");
        if matches!(change, "" | "FM" | "BECMG") && f["timeFrom"].as_i64().is_some_and(|s| s <= t) {
            match (change, base) {
                // BECMG only changes the elements it mentions
                ("BECMG", Some(_)) => apply(&mut c, f),
                _ => {
                    c = Conditions::blank(&c.station, Source::Taf, when);
                    c.ceiling_known = true;
                    apply(&mut c, f);
                }
            }
            base = Some(f);
        }
    }
    base?;
    // TEMPO / PROB groups covering the time: keep the worst as a caveat
    let mut worst: Option<(&'static str, String)> = None;
    for f in fc {
        let change = f["fcstChange"].as_str().unwrap_or("");
        if !(change == "TEMPO" || change == "PROB" || f["probability"].as_i64().is_some()) {
            continue;
        }
        let (Some(a), Some(b)) = (f["timeFrom"].as_i64(), f["timeTo"].as_i64()) else { continue };
        if t < a || t >= b {
            continue;
        }
        let mut g = c.clone();
        apply(&mut g, f);
        let cat = category(g.ceiling, g.vis);
        let wx = str_of(&f["wxString"]);
        let rank = |k: &str| ["VFR", "MVFR", "IFR", "LIFR"].iter().position(|x| *x == k).unwrap_or(0) + if k.contains("TS") { 10 } else { 0 };
        let label = if change == "TEMPO" { "TEMPO".to_string() } else { format!("PROB{}", f["probability"].as_i64().unwrap_or(30)) };
        let text = format!("{label}: {}", g.summary());
        let score = rank(cat) + if wx.contains("TS") { 10 } else { 0 };
        if worst.as_ref().is_none_or(|w| score > rank(w.0) + if w.1.contains("TS") { 10 } else { 0 }) {
            worst = Some((cat, if wx.is_empty() { text } else { format!("{text} {wx}") }));
        }
    }
    c.tempo = worst;
    Some(c)
}

fn apply(c: &mut Conditions, f: &Value) {
    if let Some(cl) = f["clouds"].as_array().filter(|a| !a.is_empty()) {
        c.ceiling = ceiling_of(&Value::Array(cl.clone()), &f["vertVis"]);
    } else if !f["vertVis"].is_null() {
        c.ceiling = ceiling_of(&Value::Null, &f["vertVis"]);
    }
    if !f["visib"].is_null() {
        c.vis = num(&f["visib"]);
    }
    if !f["wspd"].is_null() {
        c.wdir = wdir(&f["wdir"]);
        c.wspd = num(&f["wspd"]);
        c.gust = num(&f["wgst"]);
    }
    if !f["wxString"].is_null() {
        c.wx = str_of(&f["wxString"]);
    }
}

/// One row of NBM text guidance (IEM `mos.json`).
#[derive(Clone, Debug)]
pub struct NbmRow {
    pub t: DateTime<Utc>,
    pub wdir: Option<f64>,
    pub wspd: Option<f64>,
    pub gust: Option<f64>,
    /// ft; None = unlimited (NBS -88), only meaningful for NBS
    pub cig: Option<f64>,
    /// SM
    pub vis: Option<f64>,
    pub sky: Option<f64>,
    pub pop: Option<f64>,
    pub tstm: Option<f64>,
    pub temp_f: Option<f64>,
}

pub fn parse_nbm(v: &Value) -> Vec<NbmRow> {
    let mut out = Vec::new();
    for r in v["data"].as_array().into_iter().flatten() {
        let Some(t) = r["ftime"].as_str().and_then(|s| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").ok()).map(|n| Utc.from_utc_datetime(&n)) else { continue };
        let n = |k: &str| num(&r[k]).filter(|x| *x > -80.0);
        let cig = num(&r["cig"]).map(|c| if c <= -80.0 { f64::INFINITY } else { c * 100.0 });
        out.push(NbmRow {
            t,
            wdir: n("wdr"),
            wspd: n("wsp"),
            gust: n("gst"),
            cig,
            vis: n("vis").map(|v| v / 10.0),
            sky: n("sky"),
            pop: n("p06").or(n("p12")),
            tstm: n("t03").or(n("t06_1")).or(n("t12_1")),
            temp_f: n("tmp"),
        });
    }
    out
}

fn nbm_at(rows: &[NbmRow], when: DateTime<Utc>, max_gap_h: i64, station: &str, source: Source) -> Option<Conditions> {
    let best = rows.iter().min_by_key(|r| (r.t - when).num_minutes().abs())?;
    if (best.t - when).num_hours().abs() > max_gap_h {
        return None;
    }
    let mut c = Conditions::blank(station, source, when);
    if source == Source::Nbm {
        c.ceiling_known = best.cig.is_some();
        c.ceiling = best.cig.filter(|v| v.is_finite());
        c.vis = best.vis;
    }
    c.wdir = best.wdir;
    c.wspd = best.wspd;
    c.gust = best.gust.filter(|g| best.wspd.is_none_or(|w| *g > w + 5.0));
    c.sky_pct = best.sky;
    // probabilities: the worst of the rows around the time (they cover 3-12 h windows)
    let near: Vec<&NbmRow> = rows.iter().filter(|r| (r.t - when).num_hours().abs() <= max_gap_h).collect();
    c.pop = near.iter().filter_map(|r| r.pop).fold(None, |a: Option<f64>, b| Some(a.map_or(b, |a| a.max(b))));
    c.tstm = near.iter().filter_map(|r| r.tstm).fold(None, |a: Option<f64>, b| Some(a.map_or(b, |a| a.max(b))));
    c.temp_c = best.temp_f.map(|f| (f - 32.0) * 5.0 / 9.0);
    Some(c)
}

/// Everything we know about one airport's weather.
#[derive(Clone, Debug, Default)]
pub struct StationWx {
    /// the airport the pilot planned
    pub airport: String,
    /// the station whose METAR / TAF / NBM we use
    pub station: String,
    /// NM from the airport to `station`
    pub offset_nm: f64,
    pub metar: Option<Value>,
    pub taf: Option<Value>,
    pub nbs: Vec<NbmRow>,
    pub nbe: Vec<NbmRow>,
}

impl StationWx {
    /// Conditions at `when`, from the best source that covers it.
    pub fn at(&self, when: DateTime<Utc>) -> Option<Conditions> {
        let now = Utc::now();
        let lead = (when - now).num_minutes();
        if lead <= 75 {
            if let Some(m) = &self.metar {
                let mut c = from_metar(m);
                // a TAF still adds its TEMPO caveats
                if let Some(t) = self.taf.as_ref().and_then(|t| from_taf(t, when.max(now))) {
                    c.tempo = t.tempo;
                }
                c.valid = when;
                return Some(c);
            }
        }
        if let Some(c) = self.taf.as_ref().and_then(|t| from_taf(t, when)) {
            return Some(c);
        }
        nbm_at(&self.nbs, when, 2, &self.station, Source::Nbm).or_else(|| nbm_at(&self.nbe, when, 7, &self.station, Source::NbmExtended))
    }
}

/// Fetch METAR, TAF and NBM for an airport, falling back to the nearest reporting station
/// within 30 NM when the airport has none.
async fn fetch_station(st: std::sync::Arc<flightplan_server::AppState>, airport: String, icao: String, lat: f64, lon: f64) -> StationWx {
    let mut candidates: Vec<(String, f64)> = vec![(icao, 0.0)];
    if let Ok(data) = st.try_data() {
        if let Ok(near) = data.db.airports_near(lat, lon, 30.0, false) {
            for (p, d) in near.into_iter().take(12) {
                let id = if p.id.len() == 3 && p.id.chars().all(|c| c.is_ascii_alphabetic()) { format!("K{}", p.id) } else { p.id.clone() };
                if !candidates.iter().any(|c| c.0 == id) && id.len() == 4 && id.chars().all(|c| c.is_ascii_alphanumeric()) {
                    candidates.push((id, d));
                }
            }
        }
    }
    let mut out = StationWx { airport: airport.clone(), station: candidates[0].0.clone(), ..Default::default() };
    // METAR / TAF for the first few candidates in one request each
    let ids: Vec<String> = candidates.iter().take(8).map(|c| c.0.clone()).collect();
    let metars = flightplan_server::wx::metars(&st, Some(&ids.join(",")), None).await.unwrap_or(Value::Null);
    let tafs = flightplan_server::wx::tafs(&st, &ids.join(",")).await.unwrap_or(Value::Null);
    let find = |v: &Value, id: &str| v.as_array().and_then(|a| a.iter().find(|x| x["icaoId"].as_str() == Some(id)).cloned());
    // the closest candidate that reports, preferring one with a TAF
    let pick = candidates.iter().take(8).find(|c| find(&tafs, &c.0).is_some()).or_else(|| candidates.iter().take(8).find(|c| find(&metars, &c.0).is_some()));
    if let Some((id, d)) = pick {
        out.station = id.clone();
        out.offset_nm = *d;
    }
    out.metar = find(&metars, &out.station);
    out.taf = find(&tafs, &out.station);
    let ttl = Duration::from_secs(3600);
    for (model, slot) in [("NBS", 0), ("NBE", 1)] {
        // NBM covers far more sites than report METARs; try the chosen station, then the rest
        for (id, d) in std::iter::once((out.station.clone(), out.offset_nm)).chain(candidates.iter().take(5).cloned()) {
            let url = format!("https://mesonet.agron.iastate.edu/api/1/mos.json?station={id}&model={model}");
            let rows = match st.cache.get_json(&url, ttl).await {
                Ok(v) => parse_nbm(&v),
                Err(_) => vec![],
            };
            if !rows.is_empty() {
                if slot == 0 {
                    out.nbs = rows;
                } else {
                    out.nbe = rows;
                }
                if out.metar.is_none() && out.taf.is_none() {
                    out.station = id;
                    out.offset_nm = d;
                }
                break;
            }
        }
    }
    out
}

/// Model weather at one route sample (Open-Meteo, hourly).
#[derive(Clone, Debug, Default)]
pub struct ModelPoint {
    pub d: f64,
    pub lat: f64,
    pub lon: f64,
    pub t: Vec<i64>,
    pub precip: Vec<f64>,
    pub code: Vec<i64>,
    pub cloud_low: Vec<f64>,
    pub freezing_ft: Vec<f64>,
    pub vis_sm: Vec<f64>,
    pub gust: Vec<f64>,
    pub cape: Vec<f64>,
}

impl ModelPoint {
    fn idx(&self, when: i64) -> Option<usize> {
        let i = self.t.iter().enumerate().min_by_key(|(_, t)| (**t - when).abs())?.0;
        ((self.t[i] - when).abs() <= 5400).then_some(i)
    }
}

/// What the model expects along the route at the times you'd pass each point.
#[derive(Clone, Debug, Default)]
pub struct EnRoute {
    /// worst WMO weather code: (code, along-track NM)
    pub worst_code: Option<(i64, f64)>,
    /// heaviest precipitation mm/h and where
    pub max_precip: Option<(f64, f64)>,
    pub max_cape: Option<(f64, f64)>,
    /// lowest freezing level (ft MSL) where there's precipitation or low cloud, and where
    pub icing: Option<(f64, f64)>,
    /// worst visibility (SM) and where
    pub min_vis: Option<(f64, f64)>,
    /// mostly low overcast points (fraction of samples with >= 85% low cloud)
    pub low_cloud_frac: f64,
}

pub fn wmo_text(code: i64) -> &'static str {
    match code {
        0..=3 => "no significant weather",
        45 | 48 => "fog",
        51..=57 => "drizzle",
        61 | 63 => "rain",
        65 => "heavy rain",
        66 | 67 => "freezing rain",
        71..=77 => "snow",
        80..=82 => "rain showers",
        85 | 86 => "snow showers",
        95 => "thunderstorms",
        96 | 99 => "thunderstorms with hail",
        _ => "weather",
    }
}

/// Severity rank of a WMO code for "worst along the route".
fn code_rank(c: i64) -> i64 {
    match c {
        95..=99 => 100 + c,
        66 | 67 | 56 | 57 => 90,
        65 | 82 | 75 | 86 => 80,
        71..=77 | 85 => 70,
        45 | 48 => 60,
        61..=64 | 80 | 81 => 50,
        51..=55 => 40,
        _ => c.min(3),
    }
}

/// Summarise model weather along the route. `eta(d)` gives the unix time you pass along-track NM `d`.
pub fn en_route(points: &[ModelPoint], cruise_ft: f64, eta: impl Fn(f64) -> i64) -> EnRoute {
    let mut e = EnRoute::default();
    let mut low = 0usize;
    let mut n = 0usize;
    for p in points {
        let Some(i) = p.idx(eta(p.d)) else { continue };
        n += 1;
        if let Some(&c) = p.code.get(i) {
            if e.worst_code.is_none_or(|(w, _)| code_rank(c) > code_rank(w)) {
                e.worst_code = Some((c, p.d));
            }
        }
        if let Some(&r) = p.precip.get(i) {
            if r > 0.0 && e.max_precip.is_none_or(|(m, _)| r > m) {
                e.max_precip = Some((r, p.d));
            }
        }
        if let Some(&c) = p.cape.get(i) {
            if e.max_cape.is_none_or(|(m, _)| c > m) {
                e.max_cape = Some((c, p.d));
            }
        }
        let wet = p.precip.get(i).is_some_and(|r| *r >= 0.1) || p.cloud_low.get(i).is_some_and(|c| *c >= 70.0);
        if let Some(&fz) = p.freezing_ft.get(i) {
            if wet && fz < cruise_ft + 1000.0 && e.icing.is_none_or(|(m, _)| fz < m) {
                e.icing = Some((fz, p.d));
            }
        }
        if let Some(&v) = p.vis_sm.get(i) {
            if e.min_vis.is_none_or(|(m, _)| v < m) {
                e.min_vis = Some((v, p.d));
            }
        }
        if p.cloud_low.get(i).is_some_and(|c| *c >= 85.0) {
            low += 1;
        }
    }
    e.low_cloud_frac = if n > 0 { low as f64 / n as f64 } else { 0.0 };
    e
}

async fn fetch_model(st: std::sync::Arc<flightplan_server::AppState>, samples: Vec<(f64, f64, f64)>) -> Vec<ModelPoint> {
    if samples.is_empty() {
        return vec![];
    }
    let lats: Vec<String> = samples.iter().map(|s| format!("{:.2}", s.1)).collect();
    let lons: Vec<String> = samples.iter().map(|s| format!("{:.2}", s.2)).collect();
    let url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={}&longitude={}&hourly=precipitation,weather_code,cloud_cover_low,freezing_level_height,visibility,wind_gusts_10m,cape&wind_speed_unit=kn&forecast_days=7&timeformat=unixtime",
        lats.join(","),
        lons.join(",")
    );
    let Ok(v) = st.cache.get_json(&url, Duration::from_secs(3600)).await else { return vec![] };
    let list: Vec<Value> = match v {
        Value::Array(a) => a,
        o => vec![o],
    };
    let f = |h: &Value, k: &str| -> Vec<f64> { h[k].as_array().map(|a| a.iter().map(|x| x.as_f64().unwrap_or(0.0)).collect()).unwrap_or_default() };
    samples
        .iter()
        .zip(list.iter())
        .map(|(s, o)| {
            let h = &o["hourly"];
            ModelPoint {
                d: s.0,
                lat: s.1,
                lon: s.2,
                t: h["time"].as_array().map(|a| a.iter().filter_map(|x| x.as_i64()).collect()).unwrap_or_default(),
                precip: f(h, "precipitation"),
                code: h["weather_code"].as_array().map(|a| a.iter().map(|x| x.as_i64().unwrap_or(0)).collect()).unwrap_or_default(),
                cloud_low: f(h, "cloud_cover_low"),
                freezing_ft: f(h, "freezing_level_height").into_iter().map(|m| m * 3.28084).collect(),
                vis_sm: f(h, "visibility").into_iter().map(|m| m / 1609.34).collect(),
                gust: f(h, "wind_gusts_10m"),
                cape: f(h, "cape"),
            }
        })
        .collect()
}

/// Departure / destination forecasts and en-route model weather for the active route,
/// refreshed when the route's airports change and every 30 minutes.
#[derive(Default)]
pub struct RouteForecast {
    key: String,
    at: Option<Instant>,
    stations_task: Task<Vec<StationWx>>,
    model_task: Task<Vec<ModelPoint>>,
    pub dep: Option<StationWx>,
    pub dest: Option<StationWx>,
    pub model: Vec<ModelPoint>,
    pub loading: bool,
    /// bumped when new data arrives
    pub generation: u64,
}

impl RouteForecast {
    pub fn update(&mut self, be: &Backend, waypoints: &[Waypoint]) {
        let ends: Vec<&Waypoint> = [waypoints.first(), waypoints.last()].into_iter().flatten().collect();
        let key = waypoints.iter().map(|w| format!("{}{:.3}{:.3}", w.id, w.lat, w.lon)).collect::<Vec<_>>().join(" ");
        let stale = self.at.is_none_or(|t| t.elapsed() > Duration::from_secs(1800));
        if (key != self.key || stale) && !self.stations_task.pending() && !self.model_task.pending() {
            self.key = key;
            self.at = Some(Instant::now());
            if waypoints.len() < 2 {
                self.dep = None;
                self.dest = None;
                self.model.clear();
                self.generation += 1;
                return;
            }
            self.loading = true;
            let st = be.server.clone();
            let ends: Vec<(String, String, f64, f64, bool)> = ends.iter().map(|w| (w.id.clone(), w.wx_id(), w.lat, w.lon, w.kind == Kind::Airport)).collect();
            self.stations_task = be.spawn(async move {
                let mut out = Vec::new();
                for (id, icao, lat, lon, is_apt) in ends {
                    out.push(if is_apt { fetch_station(st.clone(), id, icao, lat, lon).await } else { StationWx { airport: id, ..Default::default() } });
                }
                out
            });
            // model samples: ends plus points every ~25 NM, at most 16
            let path: Vec<[f64; 2]> = waypoints.iter().map(|w| [w.lon, w.lat]).collect();
            let total: f64 = waypoints.windows(2).map(|w| distance_nm(w[0].lat, w[0].lon, w[1].lat, w[1].lon)).sum();
            let n = ((total / 25.0).ceil() as usize).clamp(1, 15);
            let samples: Vec<(f64, f64, f64)> = (0..=n)
                .map(|i| {
                    let d = total * i as f64 / n as f64;
                    let p = point_along(&path, d);
                    (d, p[1], p[0])
                })
                .collect();
            let st = be.server.clone();
            self.model_task = be.spawn(fetch_model(st, samples));
        }
        if let Some(mut list) = self.stations_task.poll() {
            self.dest = if list.len() > 1 { list.pop() } else { None };
            self.dep = list.pop();
            self.generation += 1;
            self.loading = self.model_task.pending();
        }
        if let Some(m) = self.model_task.poll() {
            self.model = m;
            self.generation += 1;
            self.loading = self.stations_task.pending();
        }
    }
}

/// A lead-time phrase: "now", "in 3 h", "in 2 days".
pub fn lead_text(when: DateTime<Utc>) -> String {
    let m = (when - Utc::now()).num_minutes();
    if m < 20 {
        "now".into()
    } else if m < 90 {
        format!("in {m} min")
    } else if m < 48 * 60 {
        format!("in {} h", (m as f64 / 60.0).round())
    } else {
        format!("in {:.1} days", m as f64 / 1440.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn categories() {
        assert_eq!(category(None, Some(10.0)), "VFR");
        assert_eq!(category(Some(2500.0), Some(10.0)), "MVFR");
        assert_eq!(category(Some(800.0), Some(10.0)), "IFR");
        assert_eq!(category(Some(3500.0), Some(0.5)), "LIFR");
    }

    #[test]
    fn taf_groups() {
        let taf = json!({"icaoId": "KXYZ", "validTimeFrom": 1000, "validTimeTo": 100000, "fcsts": [
            {"timeFrom": 1000, "timeTo": 20000, "fcstChange": null, "wdir": 240, "wspd": 10, "wgst": null, "visib": "6+", "clouds": [{"cover": "SCT", "base": 5000}], "wxString": null},
            {"timeFrom": 20000, "timeTo": 100000, "fcstChange": "FM", "wdir": "VRB", "wspd": 4, "wgst": null, "visib": 3, "clouds": [{"cover": "OVC", "base": 900}], "wxString": "-RA BR"},
            {"timeFrom": 5000, "timeTo": 9000, "fcstChange": "TEMPO", "probability": null, "wdir": null, "wspd": null, "visib": 2, "clouds": [{"cover": "BKN", "base": 1500}], "wxString": "TSRA"}
        ]});
        let at = |s: i64| from_taf(&taf, Utc.timestamp_opt(s, 0).unwrap()).unwrap();
        let a = at(2000);
        assert_eq!(a.category(), Some("VFR"));
        assert!(a.tempo.is_none());
        let b = at(6000);
        assert_eq!(b.category(), Some("VFR"));
        assert!(b.tempo.as_ref().unwrap().1.contains("TSRA"), "{:?}", b.tempo);
        let c = at(30000);
        assert_eq!(c.category(), Some("IFR"));
        assert_eq!(c.wdir, None);
        assert!(from_taf(&taf, Utc.timestamp_opt(200000, 0).unwrap()).is_none());
    }

    #[test]
    fn nbm_rows() {
        let v = json!({"data": [
            {"ftime": "2026-10-25 06:00", "wdr": 20, "wsp": 4, "gst": 15, "cig": -88, "vis": 100, "sky": 6, "p06": 1.0, "t03": 0, "tmp": 65},
            {"ftime": "2026-10-25 09:00", "wdr": 30, "wsp": 8, "gst": 9, "cig": 8, "vis": 25, "sky": 100, "t03": 30, "tmp": 62}
        ]});
        let rows = parse_nbm(&v);
        assert_eq!(rows.len(), 2);
        let t = Utc.with_ymd_and_hms(2026, 10, 25, 9, 30, 0).unwrap();
        let c = nbm_at(&rows, t, 2, "KXYZ", Source::Nbm).unwrap();
        assert_eq!(c.ceiling, Some(800.0));
        assert_eq!(c.vis, Some(2.5));
        assert_eq!(c.category(), Some("IFR"));
        assert_eq!(c.tstm, Some(30.0));
        assert_eq!(c.gust, None, "a gust within 5 kt of the wind isn't a gust");
        let first = nbm_at(&rows, Utc.with_ymd_and_hms(2026, 10, 25, 6, 0, 0).unwrap(), 1, "KXYZ", Source::Nbm).unwrap();
        assert_eq!(first.ceiling, None);
        assert!(first.ceiling_known);
        assert_eq!(first.category(), Some("VFR"));
    }

    #[test]
    fn en_route_summary() {
        let p = ModelPoint { d: 10.0, t: vec![0, 3600], precip: vec![0.0, 2.0], code: vec![1, 95], cloud_low: vec![0.0, 90.0], freezing_ft: vec![12000.0, 6000.0], vis_sm: vec![10.0, 3.0], gust: vec![5.0, 30.0], cape: vec![0.0, 900.0], ..Default::default() };
        let e = en_route(&[p], 7500.0, |_| 3600);
        assert_eq!(e.worst_code, Some((95, 10.0)));
        assert_eq!(e.icing.map(|i| i.0), Some(6000.0));
        assert_eq!(e.low_cloud_frac, 1.0);
    }
}
