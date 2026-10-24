//! Aircraft files (.fpaircraft.json): share an airframe, back up your fleet, or bring in data
//! typed up elsewhere. Imports are checked field by field: missing performance numbers fall
//! back to defaults (with a warning), missing weight & balance essentials are errors.
//! The format is documented in docs/aircraft-format.md.

use crate::model::aircraft::{self, Aircraft, Perf, Station, StationKind, WeightBalance};
use serde_json::{json, Map, Value};
use std::collections::HashSet;

pub const FORMAT: &str = "flightplan-aircraft";
pub const EXT: &str = "fpaircraft.json";

pub struct ImportResult {
    /// what the user would recognise this entry by
    pub label: String,
    pub aircraft: Option<Aircraft>,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

/// One or more aircraft as a file (no ids, no loading for a particular flight).
pub fn to_file(list: &[&Aircraft]) -> String {
    let defs: Vec<Value> = list
        .iter()
        .map(|a| {
            let mut v = serde_json::to_value(a).unwrap_or(Value::Null);
            if let Some(o) = v.as_object_mut() {
                o.remove("id");
                o.remove("load");
            }
            v
        })
        .collect();
    serde_json::to_string_pretty(&json!({"format": FORMAT, "version": 1, "exported": chrono::Utc::now().to_rfc3339(), "aircraft": defs})).unwrap_or_default()
}

/// A file to fill in by hand, with every field present.
pub fn template_file() -> String {
    let mut a = aircraft::presets()[0].clone();
    a.name = "My aircraft".into();
    a.tail = "N12345".into();
    a.notes = Some("Replace every number with your own airplane's.".into());
    to_file(&[&a])
}

/// Read an aircraft file (ours, a bare aircraft object, or a list of them).
pub fn parse(text: &str, file_name: &str) -> Vec<ImportResult> {
    let raw: Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(e) => return vec![ImportResult { label: file_name.into(), aircraft: None, errors: vec![format!("{file_name} isn't valid JSON ({e})")], warnings: vec![] }],
    };
    let items: Vec<Value> = match &raw {
        Value::Array(a) => a.clone(),
        Value::Object(o) if o.get("aircraft").is_some_and(|v| v.is_array()) => o["aircraft"].as_array().cloned().unwrap_or_default(),
        v => vec![v.clone()],
    };
    if items.is_empty() {
        return vec![ImportResult { label: file_name.into(), aircraft: None, errors: vec!["The file has no aircraft in it".into()], warnings: vec![] }];
    }
    let many = items.len() > 1;
    items.iter().enumerate().map(|(i, it)| normalize(it, &if many { format!("{file_name} #{}", i + 1) } else { file_name.to_string() })).collect()
}

fn num(v: Option<&Value>) -> Option<f64> {
    match v? {
        Value::Number(n) => n.as_f64().filter(|f| f.is_finite()),
        Value::String(s) => s.replace(',', "").trim().parse().ok(),
        _ => None,
    }
}

fn text(v: Option<&Value>) -> String {
    v.and_then(|v| v.as_str()).unwrap_or("").trim().to_string()
}

/// [arm, weight] pairs from [[a, w], ...] or [{arm, weight}, ...]
fn points(v: Option<&Value>) -> Option<Vec<[f64; 2]>> {
    let arr = v?.as_array()?;
    let mut out = Vec::new();
    for p in arr {
        let (a, w) = match p {
            Value::Array(x) => (num(x.first()), num(x.get(1))),
            Value::Object(o) => (num(o.get("arm").or(o.get("cg")).or(o.get("x"))), num(o.get("weight").or(o.get("w")).or(o.get("y")))),
            _ => (None, None),
        };
        out.push([a?, w?]);
    }
    Some(out)
}

const PERF_FIELDS: [(&str, &str); 11] = [
    ("tas", "cruise TAS"),
    ("gph", "cruise fuel flow"),
    ("climbFpm", "climb rate"),
    ("climbTas", "climb speed"),
    ("climbGph", "climb fuel flow"),
    ("descentFpm", "descent rate"),
    ("descentTas", "descent speed"),
    ("descentGph", "descent fuel flow"),
    ("taxiGal", "taxi fuel"),
    ("reserveMin", "reserve"),
    ("maxXwind", "max crosswind"),
];

pub fn normalize(input: &Value, file_label: &str) -> ImportResult {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let empty = Map::new();
    let Some(o) = input.as_object() else {
        return ImportResult { label: file_label.into(), aircraft: None, errors: vec!["Expected an aircraft object".into()], warnings };
    };
    let tail = text(o.get("tail")).to_uppercase();
    let mut type_code = text(o.get("type")).to_uppercase();
    if type_code.is_empty() {
        type_code = "ZZZZ".into();
    }
    let mut name = text(o.get("name"));
    if name.is_empty() {
        name = if type_code != "ZZZZ" { type_code.clone() } else { "Imported aircraft".into() };
    }
    let label = [tail.as_str(), name.as_str()].iter().filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>().join(" · ");
    let label = if label.is_empty() { file_label.to_string() } else { label };

    let ft_in = text(o.get("fuelType"));
    let fuel_type = match aircraft::FUEL_TYPES.iter().find(|f| f.eq_ignore_ascii_case(&ft_in)) {
        Some(f) => f.to_string(),
        None => {
            if !ft_in.is_empty() {
                warnings.push(format!("Unknown fuel type \"{ft_in}\", using 100LL"));
            }
            "100LL".into()
        }
    };
    let lb_per_gal = num(o.get("lbPerGal")).unwrap_or(if fuel_type == "Jet A" { 6.7 } else { 6.0 });

    let perf_in = o.get("perf").and_then(|v| v.as_object()).unwrap_or(&empty);
    let d = serde_json::to_value(Perf::default()).unwrap_or(Value::Null);
    let mut perf_v = Map::new();
    let mut defaulted = Vec::new();
    for (k, label) in PERF_FIELDS {
        match num(perf_in.get(k)).filter(|v| *v >= 0.0) {
            Some(v) => {
                perf_v.insert(k.into(), json!(v));
            }
            None => {
                defaulted.push(label);
                perf_v.insert(k.into(), d[k].clone());
            }
        }
    }
    if !defaulted.is_empty() {
        warnings.push(format!("Using default values for {}. Check them under Performance.", defaulted.join(", ")));
    }
    let perf: Perf = serde_json::from_value(Value::Object(perf_v)).unwrap_or_default();
    if perf.tas <= 0.0 {
        errors.push("Cruise TAS must be more than 0".into());
    }

    let wb_in = o.get("wb").and_then(|v| v.as_object()).unwrap_or(&empty);
    let empty_weight = num(wb_in.get("emptyWeight"));
    let empty_arm = num(wb_in.get("emptyArm"));
    if !empty_weight.is_some_and(|w| w > 0.0) {
        errors.push("Missing basic empty weight (wb.emptyWeight, lb)".into());
    }
    if empty_arm.is_none() {
        errors.push("Missing empty-weight arm (wb.emptyArm, inches)".into());
    }
    let max_takeoff = num(wb_in.get("maxTakeoff")).filter(|v| *v > 0.0);
    if max_takeoff.is_none() {
        errors.push("Missing maximum takeoff weight (wb.maxTakeoff, lb)".into());
    }
    let max_ramp = num(wb_in.get("maxRamp")).or(max_takeoff).unwrap_or(0.0);
    let max_landing = num(wb_in.get("maxLanding")).or(max_takeoff).unwrap_or(0.0);
    if let (Some(e), Some(m)) = (empty_weight, max_takeoff) {
        if e >= m {
            errors.push(format!("Empty weight {} lb isn't below max takeoff {} lb", aircraft::fmt_num(e), aircraft::fmt_num(m)));
        }
    }

    let mut stations: Vec<Station> = Vec::new();
    let mut seen = HashSet::new();
    match wb_in.get("stations").and_then(|v| v.as_array()) {
        Some(list) if !list.is_empty() => {
            for (i, s) in list.iter().enumerate() {
                let Some(so) = s.as_object() else {
                    errors.push(format!("Station {} isn't an object", i + 1));
                    continue;
                };
                let mut sname = text(so.get("name"));
                if sname.is_empty() {
                    sname = format!("Station {}", i + 1);
                }
                let Some(arm) = num(so.get("arm")) else {
                    errors.push(format!("Station \"{sname}\" has no arm"));
                    continue;
                };
                let kind = guess_kind(&text(so.get("kind")), &sname);
                let mut id = text(so.get("id"));
                if id.is_empty() {
                    id = sname.to_lowercase().chars().filter(|c| c.is_ascii_alphanumeric()).take(12).collect();
                }
                if id.is_empty() {
                    id = aircraft::uid();
                }
                while !seen.insert(id.clone()) {
                    id = format!("{id}_{i}");
                }
                stations.push(Station { id, name: sname, arm, kind, max: num(so.get("max")).filter(|v| *v > 0.0) });
            }
        }
        _ => errors.push("No loading stations (wb.stations): add at least seats and fuel".into()),
    }
    let fuel_stations: Vec<&Station> = stations.iter().filter(|s| s.kind == StationKind::Fuel).collect();
    if !stations.is_empty() && fuel_stations.is_empty() {
        errors.push("No fuel station: add one with kind \"fuel\" and its arm".into());
    }
    if fuel_stations.len() > 1 {
        warnings.push("More than one fuel station: weight & balance uses the arm of the last one for fuel burn".into());
    }

    let mut usable = num(o.get("usableFuel")).filter(|v| *v > 0.0);
    if usable.is_none() {
        let sum: f64 = fuel_stations.iter().filter_map(|s| s.max).sum();
        if sum > 0.0 {
            warnings.push(format!("Usable fuel taken from the fuel station capacity ({} gal)", aircraft::fmt_num(sum)));
            usable = Some(sum);
        } else {
            errors.push("Missing usable fuel (usableFuel, gallons)".into());
        }
    }

    let envelope = points(wb_in.get("envelope"));
    if envelope.as_ref().is_none_or(|e| e.len() < 3) {
        errors.push("CG envelope (wb.envelope) needs at least 3 [arm, weight] points".into());
    }
    let utility = match wb_in.get("utility") {
        None | Some(Value::Null) => None,
        Some(v) => match points(Some(v)) {
            Some(p) if p.len() >= 3 => Some(p),
            _ => {
                warnings.push("Ignored the utility envelope: it needs at least 3 [arm, weight] points".into());
                None
            }
        },
    };

    // plausibility: the envelope should reach max takeoff weight; station arms should be near it
    if let (Some(env), Some(mt)) = (&envelope, max_takeoff) {
        if env.len() >= 3 {
            let top = env.iter().map(|p| p[1]).fold(f64::MIN, f64::max);
            if (top - mt).abs() > mt * 0.05 {
                warnings.push(format!("The envelope tops out at {} lb but max takeoff is {} lb. Check both.", aircraft::fmt_num(top), aircraft::fmt_num(mt)));
            }
            let lo = env.iter().map(|p| p[0]).fold(f64::MAX, f64::min);
            let hi = env.iter().map(|p| p[0]).fold(f64::MIN, f64::max);
            let far: Vec<&str> = stations.iter().filter(|s| s.arm < lo - 120.0 || s.arm > hi + 120.0).map(|s| s.name.as_str()).collect();
            if !far.is_empty() {
                warnings.push(format!("Station arms look out of range for this envelope ({}). Are all arms in inches from the same datum?", far.join(", ")));
            }
        }
    }

    if !errors.is_empty() {
        return ImportResult { label, aircraft: None, errors, warnings };
    }
    let notes = text(o.get("notes"));
    let mut a = Aircraft {
        id: aircraft::uid(),
        name,
        tail,
        type_code,
        fuel_type,
        lb_per_gal,
        perf,
        usable_fuel: usable.unwrap_or(0.0),
        wb: WeightBalance {
            empty_weight: empty_weight.unwrap_or(0.0),
            empty_arm: empty_arm.unwrap_or(0.0),
            max_ramp,
            max_takeoff: max_takeoff.unwrap_or(0.0),
            max_landing,
            stations,
            envelope: envelope.unwrap_or_default(),
            utility,
        },
        load: Default::default(),
        notes: if notes.is_empty() { None } else { Some(notes) },
    };
    a.load = aircraft::default_load(&a);
    ImportResult { label, aircraft: Some(a), errors, warnings }
}

pub fn guess_kind(kind: &str, name: &str) -> StationKind {
    match kind.to_lowercase().as_str() {
        "seat" => StationKind::Seat,
        "baggage" => StationKind::Baggage,
        "fuel" => StationKind::Fuel,
        "other" => StationKind::Other,
        _ => {
            let n = name.to_lowercase();
            if n.contains("fuel") || n.contains("tank") {
                StationKind::Fuel
            } else if n.contains("bag") || n.contains("cargo") {
                StationKind::Baggage
            } else if ["seat", "pilot", "pass", "row", "crew"].iter().any(|w| n.contains(w)) {
                StationKind::Seat
            } else {
                StationKind::Other
            }
        }
    }
}

/// Rows pasted from a spreadsheet or POH table: tab, semicolon, comma or space separated.
pub fn parse_table(text: &str) -> Vec<Vec<String>> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|l| {
            let cells: Vec<&str> = if l.contains('\t') {
                l.split('\t').collect()
            } else if l.contains(';') {
                l.split(';').collect()
            } else if l.contains(',') {
                l.split(',').collect()
            } else {
                l.split_whitespace().collect()
            };
            cells.into_iter().map(|c| c.trim().to_string()).collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_errors() {
        let a = aircraft::from_preset(aircraft::presets()[0].clone(), "N1");
        let file = to_file(&[&a]);
        let r = parse(&file, "x");
        assert_eq!(r.len(), 1);
        let got = r[0].aircraft.as_ref().unwrap();
        assert_eq!(got.wb, a.wb);
        assert!(r[0].warnings.is_empty(), "{:?}", r[0].warnings);

        let bad = parse(r#"{"name":"Broken","perf":{"tas":100},"wb":{"stations":[{"name":"Seat","arm":37}]}}"#, "bad");
        assert!(bad[0].aircraft.is_none());
        assert!(bad[0].errors.iter().any(|e| e.contains("fuel station")));
    }

    #[test]
    fn web_export_reads() {
        // the web app's export of the C152 template
        let web = r#"{"format":"flightplan-aircraft","version":1,"aircraft":[{"name":"Cessna 152","tail":"N4821B","type":"C152","fuelType":"100LL","lbPerGal":6,
          "perf":{"tas":95,"gph":6.1,"climbFpm":715,"climbTas":67,"climbGph":7.5,"descentFpm":500,"descentTas":95,"descentGph":5,"taxiGal":0.8,"reserveMin":30,"maxXwind":12},
          "usableFuel":24.5,"wb":{"emptyWeight":1131,"emptyArm":30.4,"maxRamp":1675,"maxTakeoff":1670,"maxLanding":1670,
          "stations":[{"id":"front","name":"Pilot & passenger","arm":39,"kind":"seat"},{"id":"fuel","name":"Fuel (usable)","arm":42,"kind":"fuel","max":24.5}],
          "envelope":[[31,1100],[31,1350],[32.65,1670],[36.5,1670],[36.5,1100]]}}]}"#;
        let r = parse(web, "web");
        assert!(r[0].aircraft.is_some(), "{:?}", r[0].errors);
    }
}
