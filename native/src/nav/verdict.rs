//! Go / no-go: one green, yellow or red call for the planned flight at the planned time, with
//! the reasons. It rolls up the hazard advisor (terrain, obstacles, airspace, TFRs, advisories,
//! fuel, weight & balance, daylight), the departure and destination weather at ETD / ETA
//! against the pilot's personal minimums (ceiling, visibility, wind, gusts, crosswind, thunder,
//! density altitude), model weather along the route and winds aloft.
//! Advisory only: the pilot in command makes the call.

use super::hazards::Hazard;
use super::navlog::{thousands, NavLog};
use super::profile::Level;
use crate::wx::forecast::{wmo_text, Conditions, EnRoute, Source};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Call {
    Go,
    Caution,
    NoGo,
}

impl Call {
    pub fn word(self) -> &'static str {
        match self {
            Call::Go => "GO",
            Call::Caution => "CAUTION",
            Call::NoGo => "NO-GO",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Factor {
    pub call: Call,
    pub category: &'static str,
    pub title: String,
    pub detail: String,
    /// along-track NM
    pub at: Option<f64>,
}

#[derive(Clone, Debug)]
pub struct Verdict {
    pub call: Call,
    pub headline: String,
    /// problems first (no-go, then caution), then the checks that passed (Call::Go)
    pub factors: Vec<Factor>,
    /// hours from now to departure
    pub lead_h: f64,
}

impl Verdict {
    pub fn problems(&self) -> impl Iterator<Item = &Factor> {
        self.factors.iter().filter(|f| f.call != Call::Go)
    }
    pub fn checks(&self) -> impl Iterator<Item = &Factor> {
        self.factors.iter().filter(|f| f.call == Call::Go)
    }
}

/// The pilot's personal minimums (Settings → Personal minimums).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Minimums {
    /// caution below this ceiling (ft AGL)
    pub ceiling_ft: f64,
    /// caution below this visibility (SM)
    pub vis_sm: f64,
    /// no-go above this steady wind (kt)
    pub wind_kt: f64,
    /// no-go above this gust (kt)
    pub gust_kt: f64,
    /// caution when gusts exceed the steady wind by more than this (kt)
    pub gust_spread_kt: f64,
    /// caution above this share of the aircraft's demonstrated crosswind (%)
    pub xwind_pct: f64,
    /// thunder probability (%) for caution / no-go
    pub tstm_caution: f64,
    pub tstm_nogo: f64,
    /// caution above this density altitude (ft)
    pub density_alt_ft: f64,
    /// night flight is acceptable (caution) rather than a no-go
    pub night_ok: bool,
    /// instrument rated and current: IFR at the ends is a caution rather than a no-go
    pub ifr: bool,
}

impl Default for Minimums {
    fn default() -> Self {
        Minimums { ceiling_ft: 3000.0, vis_sm: 5.0, wind_kt: 25.0, gust_kt: 25.0, gust_spread_kt: 15.0, xwind_pct: 80.0, tstm_caution: 15.0, tstm_nogo: 40.0, density_alt_ft: 6000.0, night_ok: true, ifr: false }
    }
}

/// Departure or destination, with its weather at the time you'd be there.
pub struct End<'a> {
    pub role: &'static str,
    pub airport: &'a str,
    pub is_airport: bool,
    pub cond: Option<&'a Conditions>,
    /// (runway end id, true heading)
    pub runways: &'a [(String, f64)],
    /// field elevation, ft
    pub elev: f64,
    /// NM to the station the weather comes from
    pub offset_nm: f64,
    pub when: DateTime<Utc>,
}

pub struct Input<'a> {
    pub hazards: &'a [Hazard],
    pub ends: Vec<End<'a>>,
    pub en_route: Option<&'a EnRoute>,
    pub log: Option<&'a NavLog>,
    pub cruise_ft: f64,
    /// highest terrain along the route (ft)
    pub max_terrain_ft: f64,
    /// the aircraft's demonstrated crosswind (kt)
    pub max_xwind: f64,
    pub minimums: &'a Minimums,
    pub departure: DateTime<Utc>,
    pub now: DateTime<Utc>,
    pub data_expired: bool,
}

fn f(call: Call, category: &'static str, title: impl Into<String>, detail: impl Into<String>, at: Option<f64>) -> Factor {
    Factor { call, category, title: title.into(), detail: detail.into(), at }
}

/// How a hazard-advisor item counts toward the call.
fn hazard_call(h: &Hazard, m: &Minimums) -> Option<Call> {
    let t = h.title.as_str();
    Some(match (h.category, h.level) {
        ("Terrain", Level::Warning) => Call::NoGo,
        ("Terrain", Level::Caution) => Call::Caution,
        ("Obstacle", _) => Call::Caution,
        ("TFR", _) => Call::NoGo,
        ("Airspace", _) if t.contains("prohibited") => Call::NoGo,
        ("Airspace", _) if t.contains("restricted") => Call::Caution,
        ("Airspace", _) => return None,
        ("Weather", Level::Warning) => Call::NoGo,
        ("Weather", Level::Caution) => Call::Caution,
        ("Fuel", Level::Warning) => Call::NoGo,
        ("Fuel", Level::Caution) => Call::Caution,
        ("Weight & balance", _) => Call::NoGo,
        ("Daylight", Level::Caution) => {
            if m.night_ok {
                Call::Caution
            } else {
                Call::NoGo
            }
        }
        _ => return None,
    })
}

/// Crosswind and headwind components on the runway with the least crosswind (headwind > -5 kt).
pub fn best_runway(runways: &[(String, f64)], wdir: f64, speed: f64) -> Option<(String, f64, f64)> {
    runways
        .iter()
        .map(|(id, hdg)| {
            let a = (wdir - hdg).to_radians();
            (id.clone(), (speed * a.sin()).abs(), speed * a.cos())
        })
        .filter(|r| r.2 > -5.0)
        .min_by(|a, b| a.1.total_cmp(&b.1))
}

fn end_checks(e: &End, m: &Minimums, max_xwind: f64, out: &mut Vec<Factor>) {
    let who = format!("{} {}", e.role, e.airport);
    let when = e.when.with_timezone(&chrono::Local).format("%a %H:%M");
    let Some(c) = e.cond else {
        if e.is_airport {
            out.push(f(Call::Caution, "Weather", format!("No forecast for {who} at {when}"), "No METAR, TAF or model guidance covers this airport and time. Check again closer to departure.", None));
        }
        return;
    };
    let src = if c.station != e.airport && e.offset_nm > 1.0 { format!("{} from {} ({:.0} NM away)", c.source.label(), c.station, e.offset_nm) } else { c.source.label().to_string() };
    let mut worst = Call::Go;
    let mut push = |out: &mut Vec<Factor>, call: Call, title: String, detail: String| {
        worst = worst.max(call);
        out.push(f(call, "Weather", title, detail, None));
    };
    // ceiling / visibility
    match c.category() {
        Some(cat @ ("IFR" | "LIFR")) => push(out, if m.ifr { Call::Caution } else { Call::NoGo }, format!("{who} {cat} at {when}"), format!("{} ({src}).", c.summary())),
        Some("MVFR") => push(out, Call::Caution, format!("{who} marginal VFR at {when}"), format!("{} ({src}).", c.summary())),
        Some(_) => {
            let low_ceiling = c.ceiling_known && c.ceiling.is_some_and(|x| x < m.ceiling_ft);
            let low_vis = c.vis.is_some_and(|v| v < m.vis_sm);
            if low_ceiling || low_vis {
                push(out, Call::Caution, format!("{who} below your minimums at {when}"), format!("{}; your minimums are {} ft and {} SM ({src}).", c.summary(), thousands(m.ceiling_ft), m.vis_sm));
            }
        }
        None => {
            if c.sky_pct.is_some_and(|s| s >= 88.0) && c.pop.is_some_and(|p| p >= 50.0) {
                push(out, Call::Caution, format!("{who}: overcast with precipitation likely"), format!("{} ({src}). Ceilings aren't forecast this far out.", c.summary()));
            }
        }
    }
    // thunder and freezing precipitation
    let tstm = c.tstm.unwrap_or(0.0);
    if c.wx.contains("TS") {
        push(out, Call::NoGo, format!("Thunderstorms at {who} at {when}"), format!("{} ({src}).", c.summary()));
    } else if tstm >= m.tstm_nogo {
        push(out, Call::NoGo, format!("{}% chance of thunderstorms at {who}", tstm.round()), format!("Around {when} ({src})."));
    } else if tstm >= m.tstm_caution {
        push(out, Call::Caution, format!("{}% chance of thunderstorms at {who}", tstm.round()), format!("Around {when} ({src})."));
    }
    if c.wx.contains("FZ") {
        push(out, Call::NoGo, format!("Freezing precipitation at {who}"), format!("{} ({src}).", c.summary()));
    } else if c.wx.contains("SN") {
        push(out, Call::Caution, format!("Snow at {who}"), format!("{} ({src}).", c.summary()));
    }
    if let Some((cat, text)) = &c.tempo {
        if text.contains("TS") {
            let prob = text.starts_with("PROB");
            push(out, if prob { Call::Caution } else { Call::NoGo }, format!("{} thunderstorms in the {who} TAF", if prob { "Possible" } else { "Temporary" }), text.clone());
        } else if matches!(*cat, "IFR" | "LIFR") {
            push(out, Call::Caution, format!("{who} TAF has temporary {cat}"), text.clone());
        }
    }
    // wind
    let wspd = c.wspd.unwrap_or(0.0);
    let gust = c.gust.unwrap_or(wspd);
    let gtxt = c.gust.map(|g| format!(" gusting {}", g.round())).unwrap_or_default();
    if wspd > m.wind_kt {
        push(out, Call::NoGo, format!("{who} wind {} kt", wspd.round()), format!("Above your {} kt limit ({src}).", m.wind_kt));
    } else if gust > m.gust_kt {
        push(out, Call::NoGo, format!("{who} gusts {} kt", gust.round()), format!("Above your {} kt gust limit ({src}).", m.gust_kt));
    } else if gust - wspd > m.gust_spread_kt {
        push(out, Call::Caution, format!("{who} gust spread {} kt", (gust - wspd).round()), format!("Wind {}{gtxt} kt ({src})."  , wspd.round()));
    }
    match c.wdir {
        Some(dir) if gust >= 3.0 && !e.runways.is_empty() => {
            if let Some((rwy, x, _)) = best_runway(e.runways, dir, gust) {
                if x > max_xwind {
                    push(out, Call::NoGo, format!("{who} crosswind {} kt (runway {rwy})", x.round()), format!("Above this aircraft's {} kt demonstrated crosswind. Wind {:03}° {}{gtxt} kt ({src}).", max_xwind, dir.round() as i64, wspd.round()));
                } else if x > max_xwind * m.xwind_pct / 100.0 {
                    push(out, Call::Caution, format!("{who} crosswind {} kt (runway {rwy})", x.round()), format!("{}% of the {} kt demonstrated crosswind. Wind {:03}° {}{gtxt} kt ({src}).", (x / max_xwind * 100.0).round(), max_xwind, dir.round() as i64, wspd.round()));
                }
            }
        }
        None if wspd >= 10.0 => push(out, Call::Caution, format!("{who} wind variable at {} kt", wspd.round()), format!("Expect a crosswind on any runway ({src}).")),
        _ => {}
    }
    // density altitude at departure
    if e.role == "Departure" {
        if let Some(temp) = c.temp_c {
            let altim = c.altim_hpa.unwrap_or(1013.25);
            let pa = e.elev + (1013.25 - altim) * 30.0;
            let isa = 15.0 - 2.0 * pa / 1000.0;
            let da = pa + 120.0 * (temp - isa);
            if da > m.density_alt_ft {
                push(
                    out,
                    Call::Caution,
                    format!("Density altitude {} ft at {}", thousands((da / 100.0).round() * 100.0), e.airport),
                    format!("Field elevation {} ft, {}°F. Expect a longer takeoff roll and weaker climb; check the POH charts.", thousands(e.elev.round()), (temp * 9.0 / 5.0 + 32.0).round()),
                );
            }
        }
    }
    if worst == Call::Go {
        let cat = c.category().map(|c| format!("{c} · ")).unwrap_or_default();
        out.push(f(Call::Go, "Weather", format!("{who} at {when}"), format!("{cat}{} ({src})", c.summary()), None));
    }
}

fn en_route_checks(e: &EnRoute, cruise: f64, out: &mut Vec<Factor>) {
    let mut any = false;
    let mut push = |out: &mut Vec<Factor>, call: Call, title: String, detail: String, at: f64| {
        any = true;
        out.push(f(call, "En route", title, detail, Some(at)));
    };
    if let Some((code, d)) = e.worst_code {
        match code {
            95..=99 => push(out, Call::NoGo, format!("Model forecasts {} along the route", wmo_text(code)), format!("Near {d:.0} NM at the time you'd pass. Plan around them or change the time."), d),
            66 | 67 | 56 | 57 => push(out, Call::NoGo, format!("Model forecasts {} along the route", wmo_text(code)), format!("Near {d:.0} NM."), d),
            65 | 82 | 75 | 86 => push(out, Call::Caution, format!("Model forecasts {} along the route", wmo_text(code)), format!("Near {d:.0} NM."), d),
            _ => {}
        }
    }
    if let Some((cape, d)) = e.max_cape {
        if cape >= 1500.0 && !e.worst_code.is_some_and(|c| c.0 >= 95) {
            push(out, Call::Caution, "Unstable air along the route".into(), format!("CAPE {} J/kg near {d:.0} NM: afternoon build-ups and convection are possible.", cape.round()), d);
        }
    }
    if let Some((fz, d)) = e.icing {
        let call = if fz < cruise - 2000.0 { Call::NoGo } else { Call::Caution };
        push(out, call, format!("Freezing level {} ft with moisture", thousands((fz / 100.0).round() * 100.0)), format!("At your {} ft cruise near {d:.0} NM there's cloud or precipitation at or above the freezing level: icing is possible.", thousands(cruise)), d);
    }
    if let Some((v, d)) = e.min_vis {
        if v < 1.0 {
            push(out, Call::NoGo, format!("Visibility under 1 SM along the route"), format!("Model visibility {v:.1} SM near {d:.0} NM."), d);
        } else if v < 3.0 {
            push(out, Call::Caution, format!("Low visibility along the route"), format!("Model visibility {v:.1} SM near {d:.0} NM."), d);
        }
    }
    if e.low_cloud_frac >= 0.5 {
        push(out, Call::Caution, "Low cloud along much of the route".into(), format!("The model has 85%+ low cloud cover over {}% of the route.", (e.low_cloud_frac * 100.0).round()), 0.0);
    }
    if !any {
        out.push(f(Call::Go, "En route", "No significant weather along the route", "Model precipitation, thunder, icing and visibility checked at the times you'd pass.", None));
    }
}

pub fn evaluate(i: &Input) -> Verdict {
    let m = i.minimums;
    let mut out: Vec<Factor> = Vec::new();
    for h in i.hazards {
        if let Some(call) = hazard_call(h, m) {
            out.push(f(call, h.category, h.title.clone(), h.detail.clone(), h.at));
        }
    }
    for e in &i.ends {
        end_checks(e, m, i.max_xwind, &mut out);
    }
    if let Some(e) = i.en_route {
        en_route_checks(e, i.cruise_ft, &mut out);
    }
    // winds aloft: strong flow over high terrain means turbulence / mountain wave
    if let Some(log) = i.log {
        if let Some((spd, row)) = log.rows.iter().filter(|r| r.phase == super::navlog::Phase::Cruise).filter_map(|r| r.wind.map(|w| (w.spd, r))).max_by(|a, b| a.0.total_cmp(&b.0)) {
            if spd >= 40.0 && i.max_terrain_ft >= 5000.0 {
                out.push(f(Call::Caution, "Winds aloft", format!("{} kt winds at cruise over high terrain", spd.round()), format!("Near {} at {} ft. Expect turbulence and mountain wave downwind of ridges.", row.to, thousands(row.alt)), Some(row.cum_dist)));
            } else if spd >= 50.0 {
                out.push(f(Call::Caution, "Winds aloft", format!("{} kt winds at cruise", spd.round()), format!("Near {}. Check groundspeed and fuel.", row.to), Some(row.cum_dist)));
            }
        }
        if !out.iter().any(|x| x.category == "Fuel") {
            out.push(f(Call::Go, "Fuel", format!("{:.1} gal at landing", log.fuel_at_landing), format!("Reserve {:.1} gal.", log.reserve_fuel), None));
        }
    }
    if i.data_expired {
        out.push(f(Call::Caution, "Chart data", "Chart data is out of date", "Airspace, frequencies and obstacles may have changed. Update the data pack in Settings.", None));
    }
    let lead_h = (i.departure - i.now).num_minutes() as f64 / 60.0;
    if lead_h > 72.0 {
        out.push(f(Call::Go, "Confidence", format!("Departure is {:.1} days out", lead_h / 24.0), "Forecasts this far ahead are rough guidance: no TAF, no ceiling forecast. Check again closer to the day.", None));
    } else if lead_h < -60.0 {
        out.push(f(Call::Caution, "Time", "Departure time is in the past", "Set the departure time on the timeline to check the weather you'd actually fly in.", None));
    }
    out.sort_by(|a, b| b.call.cmp(&a.call));
    let call = out.iter().map(|x| x.call).max().unwrap_or(Call::Go);
    let problems = out.iter().filter(|x| x.call == call && call != Call::Go).count();
    let headline = match call {
        Call::Go => "Within your minimums".to_string(),
        _ if problems == 1 => out[0].title.clone(),
        Call::Caution => format!("{} · and {} more to review", out[0].title, problems - 1),
        Call::NoGo => format!("{} · and {} more", out[0].title, problems - 1),
    };
    Verdict { call, headline, factors: out, lead_h }
}

/// Text summary for clipboards and the briefing.
pub fn source_note(s: Source) -> &'static str {
    match s {
        Source::Metar => "current observation",
        Source::Taf => "terminal forecast",
        Source::Nbm => "model guidance (3-hourly)",
        Source::NbmExtended => "model guidance (12-hourly, low detail)",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wx::forecast::Conditions;
    use chrono::TimeZone;

    fn cond(cig: Option<f64>, vis: f64, wdir: Option<f64>, wspd: f64, gust: Option<f64>) -> Conditions {
        Conditions {
            station: "KAAA".into(),
            source: Source::Taf,
            valid: Utc::now(),
            ceiling: cig,
            ceiling_known: true,
            vis: Some(vis),
            wdir,
            wspd: Some(wspd),
            gust,
            sky_pct: None,
            pop: None,
            tstm: None,
            wx: String::new(),
            temp_c: None,
            altim_hpa: None,
            tempo: None,
        }
    }

    fn run(c: &Conditions, m: &Minimums) -> Verdict {
        let rw = vec![("09".to_string(), 90.0), ("27".to_string(), 270.0)];
        let now = Utc.with_ymd_and_hms(2026, 10, 25, 12, 0, 0).unwrap();
        evaluate(&Input {
            hazards: &[],
            ends: vec![End { role: "Departure", airport: "KAAA", is_airport: true, cond: Some(c), runways: &rw, elev: 1000.0, offset_nm: 0.0, when: now }],
            en_route: None,
            log: None,
            cruise_ft: 6500.0,
            max_terrain_ft: 2000.0,
            max_xwind: 15.0,
            minimums: m,
            departure: now,
            now,
            data_expired: false,
        })
    }

    #[test]
    fn green_yellow_red() {
        let m = Minimums::default();
        assert_eq!(run(&cond(None, 10.0, Some(270.0), 8.0, None), &m).call, Call::Go);
        assert_eq!(run(&cond(Some(2500.0), 10.0, Some(270.0), 8.0, None), &m).call, Call::Caution, "MVFR");
        assert_eq!(run(&cond(Some(800.0), 10.0, Some(270.0), 8.0, None), &m).call, Call::NoGo, "IFR for a VFR pilot");
        let ifr = Minimums { ifr: true, ..Minimums::default() };
        assert_eq!(run(&cond(Some(800.0), 10.0, Some(270.0), 8.0, None), &ifr).call, Call::Caution, "IFR-rated pilot");
        // 90° crosswind at 14 kt: 93% of 15 kt → caution; 20 kt → no-go
        assert_eq!(run(&cond(None, 10.0, Some(0.0), 14.0, None), &m).call, Call::Caution);
        assert_eq!(run(&cond(None, 10.0, Some(0.0), 20.0, None), &m).call, Call::NoGo);
        // gusts
        assert_eq!(run(&cond(None, 10.0, Some(270.0), 10.0, Some(30.0)), &m).call, Call::NoGo);
        let mut ts = cond(None, 10.0, Some(270.0), 5.0, None);
        ts.wx = "TSRA".into();
        assert_eq!(run(&ts, &m).call, Call::NoGo);
        ts.wx.clear();
        ts.tstm = Some(20.0);
        assert_eq!(run(&ts, &m).call, Call::Caution);
    }

    #[test]
    fn crosswind_components() {
        let rw = vec![("09".to_string(), 90.0), ("27".to_string(), 270.0)];
        let (id, x, h) = best_runway(&rw, 300.0, 20.0).unwrap();
        assert_eq!(id, "27");
        assert!((x - 10.0).abs() < 0.01);
        assert!(h > 17.0);
    }
}
