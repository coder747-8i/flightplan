//! Plain-English decoding of METAR, TAF, PIREP, AIRMET/SIGMET text and FAA airport remarks.
//! Coded groups are parsed; free text is expanded with the FAA contraction list (JO 7340).

use regex::Regex;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

macro_rules! re {
    ($s:expr) => {{
        static R: LazyLock<Regex> = LazyLock::new(|| Regex::new($s).unwrap());
        &*R
    }};
}

static CONTRACTIONS: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    [
        ("ABV", "above"), ("ACFT", "aircraft"), ("ACR", "air carrier"), ("ACT", "active"), ("ACTVT", "activate"), ("ADJ", "adjacent"), ("ADZ", "advise"),
        ("AFT", "after"), ("AGL", "AGL"), ("ALF", "aloft"), ("ALG", "along"), ("ALQDS", "all quadrants"), ("ALSTG", "altimeter setting"), ("ALT", "altitude"),
        ("ALTN", "alternate"), ("AMD", "amended"), ("AMGR", "airport manager"), ("APCH", "approach"), ("APRX", "approximately"), ("APVD", "approved"),
        ("ARPT", "airport"), ("ARR", "arrival"), ("ASGN", "assigned"), ("ATA", "airport traffic area"), ("ATCT", "control tower"), ("AVBL", "available"),
        ("AVN", "aviation"), ("AWY", "airway"), ("BCMG", "becoming"), ("BFR", "before"), ("BKN", "broken"), ("BLDG", "building"), ("BLO", "below"),
        ("BLW", "below"), ("BR", "mist"), ("BTN", "between"), ("BTW", "between"), ("BYD", "beyond"), ("CCLKWS", "counterclockwise"), ("CD", "clearance delivery"),
        ("CHG", "change"), ("CIG", "ceiling"), ("CIGS", "ceilings"), ("CLD", "cloud"), ("CLDS", "clouds"), ("CLKWS", "clockwise"), ("CLR", "clear"),
        ("CLSD", "closed"), ("CNTR", "center"), ("CNTRLN", "centerline"), ("CONDS", "conditions"), ("CONS", "continuous"), ("CONSLY", "continuously"),
        ("CONT", "continue"), ("CONTG", "continuing"), ("CONTRAILS", "contrails"), ("CTC", "contact"), ("CTN", "caution"), ("DALGT", "daylight"),
        ("DCR", "decrease"), ("DCRG", "decreasing"), ("DEP", "departure"), ("DIAM", "diameter"), ("DLY", "daily"), ("DMSHG", "diminishing"),
        ("DSIPT", "dissipate"), ("DSIPTG", "dissipating"), ("DSND", "descend"), ("DSNT", "distant"), ("DSPLCD", "displaced"), ("DTHR", "displaced threshold"),
        ("DURG", "during"), ("DVLP", "develop"), ("DVLPG", "developing"), ("E", "east"), ("EFCT", "effect"), ("EMBD", "embedded"), ("ENTR", "entire"),
        ("EQPT", "equipment"), ("EXC", "except"), ("EXTRM", "extreme"), ("FL", "FL"), ("FLD", "field"), ("FM", "from"), ("FQT", "frequent"),
        ("FREQ", "frequency"), ("FRM", "from"), ("FRQ", "frequent"), ("FT", "ft"), ("FZLVL", "freezing level"), ("FZRA", "freezing rain"), ("GEN", "general"),
        ("GRDL", "gradual"), ("GRDLY", "gradually"), ("HDG", "heading"), ("HEL", "helicopter"), ("HLCPTR", "helicopter"), ("HOL", "holidays"), ("HR", "hour"),
        ("HRS", "hours"), ("HVY", "heavy"), ("ICG", "icing"), ("ICGIC", "icing in clouds"), ("ICGICIP", "icing in clouds and precipitation"), ("IMPR", "improve"),
        ("IMPRG", "improving"), ("INCR", "increase"), ("INCRG", "increasing"), ("INDEFLY", "indefinitely"), ("INFO", "information"), ("INTSF", "intensify"),
        ("INTSFYG", "intensifying"), ("INTST", "intensity"), ("INTSTY", "intensity"), ("INVOF", "in the vicinity of"), ("ISOL", "isolated"), ("KT", "kt"),
        ("KTS", "kt"), ("LCL", "local"), ("LGT", "light"), ("LGTD", "lighted"), ("LGTS", "lights"), ("LIRL", "low-intensity runway lights"),
        ("LLWS", "low-level wind shear"), ("LN", "line"), ("LNDG", "landing"), ("LRG", "large"), ("LTD", "limited"), ("LTG", "lightning"), ("LVL", "level"),
        ("LYR", "layer"), ("LYRS", "layers"), ("MAINT", "maintenance"), ("MED", "medium"), ("MIL", "military"), ("MIRL", "medium-intensity runway lights"),
        ("MOD", "moderate"), ("MOV", "moving"), ("MOVG", "moving"), ("MSL", "MSL"), ("MT", "mountain"), ("MTN", "mountain"), ("MTNS", "mountains"),
        ("MXD", "mixed"), ("N", "north"), ("NA", "not authorized"), ("NC", "no change"), ("NE", "northeast"), ("NML", "normal"), ("NMRS", "numerous"),
        ("NR", "number"), ("NW", "northwest"), ("OBSC", "obscured"), ("OBSCD", "obscured"), ("OBSCG", "obscuring"), ("OBSTN", "obstruction"),
        ("OCNL", "occasional"), ("OCNLY", "occasionally"), ("OPNS", "operations"), ("OPR", "operate"), ("OPS", "operations"), ("OTLK", "outlook"),
        ("OTR", "other"), ("OTRW", "otherwise"), ("OVC", "overcast"), ("OVNGT", "overnight"), ("PAT", "pattern"), ("PCPN", "precipitation"),
        ("PPR", "prior permission required"), ("PRKG", "parking"), ("PROC", "procedure"), ("PRVDD", "provided"), ("PSG", "passing"), ("PSGR", "passenger"),
        ("PTNS", "portions"), ("PWR", "power"), ("QSTNRY", "quasi-stationary"), ("RA", "rain"), ("RDC", "runway design code"), ("RGN", "region"),
        ("RMN", "remain"), ("RQR", "required"), ("RQRD", "required"), ("RSTD", "restricted"), ("RUF", "rough"), ("RWY", "runway"), ("RWYS", "runways"),
        ("S", "south"), ("SCT", "scattered"), ("SE", "southeast"), ("SEV", "severe"), ("SFC", "surface"), ("SHRA", "rain showers"), ("SKED", "scheduled"),
        ("SLO", "slow"), ("SN", "snow"), ("SR", "sunrise"), ("SS", "sunset"), ("STG", "strong"), ("STNR", "stationary"), ("STWY", "stopway"),
        ("SUSTAINED", "sustained"), ("SVC", "service"), ("SVCS", "services"), ("SW", "southwest"), ("SXN", "section"), ("TFC", "traffic"),
        ("TGL", "touch-and-go landings"), ("THR", "threshold"), ("THRU", "through"), ("TKOF", "takeoff"), ("TOP", "top"), ("TOPS", "tops"),
        ("TPA", "pattern altitude"), ("TRAN", "transient"), ("TS", "thunderstorms"), ("TSNT", "transient"), ("TSTM", "thunderstorm"), ("TSTMS", "thunderstorms"),
        ("TURB", "turbulence"), ("TWR", "tower"), ("TWY", "taxiway"), ("TWYS", "taxiways"), ("UNKN", "unknown"), ("UNLGT", "unlighted"), ("UNLGTD", "unlighted"),
        ("UNMKD", "unmarked"), ("UNSKED", "unscheduled"), ("UNRELBL", "unreliable"), ("UPDFTS", "updrafts"), ("VC", "vicinity"), ("VIS", "visibility"),
        ("VRB", "variable"), ("VRBL", "variable"), ("W", "west"), ("WDLY", "widely"), ("WDSPRD", "widespread"), ("WI", "within"), ("WIND", "wind"),
        ("WINDS", "winds"), ("WK", "weak"), ("WKN", "weaken"), ("WKNG", "weakening"), ("WKDAYS", "weekdays"), ("WKENDS", "weekends"), ("WKNDS", "weekends"),
        ("WND", "wind"), ("WRMFNT", "warm front"), ("WS", "wind shear"), ("WSHFT", "wind shift"), ("WSPN", "wingspan"), ("WTR", "water"), ("WV", "wave"),
        ("WX", "weather"), ("XCP", "except"), ("CC", "credit card"), ("OFC", "office"), ("OPRS", "operators"), ("OPRG", "operating"), ("WGT", "weight"),
        ("WT", "weight"), ("CERT", "certified"), ("HAZ", "hazard"), ("PJE", "parachute jumping"), ("REIL", "runway end identifier lights"),
        ("HIRL", "high-intensity runway lights"), ("BCN", "beacon"), ("RON", "remain overnight"), ("INBD", "inbound"), ("OUTBD", "outbound"), ("DRCT", "direct"),
        ("POSS", "possible"), ("SVRL", "several"), ("OBS", "observed"), ("FCST", "forecast"), ("DMSH", "diminish"), ("SKC", "sky clear"),
    ]
    .into_iter()
    .collect()
});

static KEEP: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    "CTAF UNICOM PAPI VASI ILS VOR VORTAC NDB DME GPS RNAV ATIS ASOS AWOS ATC FAA FSS IFR VFR MVFR LIFR NOTAM NOTAMS TFR MOA UAS ADS ARFF PCL AIRMET SIGMET PIREP CB TCU CAT UTC MON TUE WED THU FRI SAT SUN JAN FEB MAR APR MAY JUN JUL AUG SEP OCT NOV DEC II III NM SM CBP DOD"
        .split(' ')
        .collect()
});

static ENGLISH: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    "FROM HAIL TORNADOES GUSTS SEVERE AREA LINE VALID A AN AND ARE AS AT BE BUT BY FOR IF IN IS IT NO NOT OF ON OR THE TO UP WITH ALL ANY USE FLY WHEN THEN ONLY MUST MAY CALL AREAS ENTER ALSO OVER UNDER INTO OUT NEAR END ENDS EAST WEST NORTH SOUTH LEFT RIGHT STRAIGHT AHEAD PRIOR UNTIL UNLESS OTHER TIMES BOTH SIDE FIELD FUEL SELF SERVE CREDIT CARD HANGAR RAMP APRON GRASS TURF GRAVEL DIRT WATER BIRDS DEER WILDLIFE TREES HILL HILLS TOWER TOWERS POLE POLES LINES ROAD HWY FENCE BLAST PAD NOISE ABATEMENT CURFEW VOLUNTARY SENSITIVE QUADS HELICOPTERS JETS JET PROP LIGHT HEAVY TURBO NON LARGE SMALL FEE FEES CUSTOMS USER PREPAID ONLINE SERVICE AVAILABLE CLOSED OPEN OPENS DAYS DAY NIGHT NIGHTS WEEK MONTH YEAR BASED PARKING RESTRICTED PRIVATE PUBLIC PERMISSION REQUIRED REQUEST CONTACT PHONE RADIO FREQUENCY MILES MILE FEET INCHES LB LBS MAX MIN MAXIMUM MINIMUM GREATER LESS THAN TIP BEYOND MIDFIELD STOP GO CALM WIND SLOPE SPEED SOFT WET ICE SNOW RAIN FOG SMOKE DUST HAZE MIST MAINTAIN CLIMB DESCEND TURN CROSSWIND DOWNWIND BASE FINAL PATTERN SIMILAR ALIGNED JOINT CIVIL MILITARY TRAINING TRAFFIC ATTENDED UNATTENDED IRREGULAR AIRPORT AIRCRAFT OWNER MANAGER OFFICE AFTER HOURS SUNRISE SUNSET STATION ONE TWO THREE FOUR FIVE ESTABLISHED REIL DEPARTURE ARRIVAL ARRIVALS DEPARTURES TAXI TAXIING RUNUP RUNUPS ENGINE TEST TESTS PROHIBITED"
        .split(' ')
        .collect()
});

fn cap_first(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// Expand FAA contractions in free text and soften the all-caps.
pub fn expand(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    let out = re!(r"\bAFT HRS\b").replace_all(text, "after hours");
    let out = re!(r"\bSELF[- ]SVC\b").replace_all(&out, "self-service");
    let out = re!(r"\bH24\b").replace_all(&out, "24 hours");
    let out = re!(r"\b[A-Z][A-Z+]*\b").replace_all(&out, |c: &regex::Captures| {
        let w = &c[0];
        if let Some(x) = CONTRACTIONS.get(w) {
            x.to_string()
        } else if KEEP.contains(w) {
            w.to_string()
        } else if ENGLISH.contains(w) || w.len() > 4 {
            w.to_lowercase()
        } else {
            w.to_string() // short unknown words are usually identifiers
        }
    });
    let out = re!(r"\s+").replace_all(&out, " ");
    let out = re!(r"\s+([.,;])").replace_all(&out, "$1");
    cap_first(out.trim())
}

// ---------------------------------------------------------------- METAR / TAF groups

fn phenom(code: &str) -> Option<&'static str> {
    Some(match code {
        "DZ" => "drizzle",
        "RA" => "rain",
        "SN" => "snow",
        "SG" => "snow grains",
        "IC" => "ice crystals",
        "PL" => "ice pellets",
        "GR" => "hail",
        "GS" => "small hail",
        "UP" => "unknown precipitation",
        "BR" => "mist",
        "FG" => "fog",
        "FU" => "smoke",
        "VA" => "volcanic ash",
        "DU" => "dust",
        "SA" => "sand",
        "HZ" => "haze",
        "PY" => "spray",
        "PO" => "dust whirls",
        "SQ" => "squalls",
        "FC" => "funnel cloud",
        "SS" => "sandstorm",
        "DS" => "duststorm",
        _ => return None,
    })
}

fn cover(code: &str) -> &'static str {
    match code {
        "FEW" => "few clouds",
        "SCT" => "scattered clouds",
        "BKN" => "broken clouds",
        "OVC" => "overcast",
        _ => "sky obscured, vertical visibility",
    }
}

fn ft(hundreds: &str) -> String {
    format!("{} ft", crate::nav::navlog::thousands(hundreds.parse::<f64>().unwrap_or(0.0) * 100.0))
}

fn pint(s: &str) -> i64 {
    s.parse().unwrap_or(0)
}

fn wind(tok: &str) -> Option<String> {
    let m = re!(r"^(\d{3}|VRB)(\d{2,3})(?:G(\d{2,3}))?(KT|MPS)$").captures(tok)?;
    let unit = if &m[4] == "MPS" { "m/s" } else { "kt" };
    if &m[1] == "000" && &m[2] == "00" {
        return Some("Wind calm".into());
    }
    let dir = if &m[1] == "VRB" { "variable direction".to_string() } else { format!("from {}°", &m[1]) };
    let gust = m.get(3).map(|g| format!(", gusting {} {unit}", pint(g.as_str()))).unwrap_or_default();
    Some(format!("Wind {dir} at {} {unit}{gust}", pint(&m[2])))
}

fn visibility(tok: &str, prev: Option<&str>) -> Option<String> {
    if tok == "P6SM" {
        return Some("Visibility more than 6 miles".into());
    }
    if tok == "CAVOK" {
        return Some("Ceiling and visibility OK".into());
    }
    if let Some(m) = re!(r"^(M)?(\d+)?(?:/(\d+))?SM$").captures(tok) {
        let a = m.get(2).map(|x| pint(x.as_str())).unwrap_or(0);
        let mut v = match m.get(3) {
            Some(b) => a as f64 / pint(b.as_str()).max(1) as f64,
            None => a as f64,
        };
        if let (Some(p), Some(_)) = (prev, m.get(3)) {
            if re!(r"^\d$").is_match(p) {
                v += pint(p) as f64;
            }
        }
        let txt = if v.fract() == 0.0 {
            format!("{}", v as i64)
        } else if v < 1.0 {
            format!("{}/{}", a, m.get(3).map(|x| x.as_str()).unwrap_or("1"))
        } else {
            format!("{} {}/{}", v.floor() as i64, a, m.get(3).map(|x| x.as_str()).unwrap_or("1"))
        };
        return Some(format!("Visibility {}{txt} mile{}", if m.get(1).is_some() { "less than " } else { "" }, if v == 1.0 { "" } else { "s" }));
    }
    if let Some(m) = re!(r"^(\d{4})$").captures(tok) {
        if &m[1] == "9999" {
            return Some("Visibility 6 miles or more".into());
        }
        let sm = pint(&m[1]) as f64 / 1609.34;
        return Some(if sm < 3.0 { format!("Visibility {sm:.1} miles") } else { format!("Visibility {} miles", sm.round()) });
    }
    None
}

fn weather(tok: &str) -> Option<String> {
    let m = re!(r"^(-|\+|VC)?(MI|PR|BC|DR|BL|SH|TS|FZ)?((?:DZ|RA|SN|SG|IC|PL|GR|GS|UP|BR|FG|FU|VA|DU|SA|HZ|PY|PO|SQ|FC|SS|DS)+)?$").captures(tok)?;
    if m.get(2).is_none() && m.get(3).is_none() {
        return None;
    }
    let int = m.get(1).map(|x| x.as_str()).unwrap_or("");
    let desc = m.get(2).map(|x| x.as_str()).unwrap_or("");
    let codes = m.get(3).map(|x| x.as_str()).unwrap_or("");
    let ph: Vec<&str> = codes.as_bytes().chunks(2).filter_map(|c| phenom(std::str::from_utf8(c).ok()?)).collect();
    let ph = ph.join(" and ");
    let intensity = match int {
        "-" => "light ",
        "+" => "heavy ",
        "VC" => "nearby ",
        _ => "",
    };
    if desc == "TS" && ph.is_empty() {
        return Some(if int == "VC" { "Thunderstorm nearby".into() } else { "Thunderstorm".into() });
    }
    if desc == "SH" && ph.is_empty() {
        return Some(if int == "VC" { "Showers nearby".into() } else { "Showers".into() });
    }
    if desc == "TS" {
        let i = if int == "VC" { "" } else { intensity };
        return Some(format!("Thunderstorm with {i}{ph}{}", if int == "VC" { " nearby" } else { "" }));
    }
    let d = match desc {
        "MI" => "shallow ",
        "PR" => "partial ",
        "BC" => "patches of ",
        "DR" => "low drifting ",
        "BL" => "blowing ",
        "SH" => "showers of ",
        "FZ" => "freezing ",
        _ => "",
    };
    let mut s = format!("{intensity}{d}{ph}").trim().to_string();
    if int == "VC" {
        s = format!("{} nearby", s.replace("nearby ", ""));
    }
    Some(cap_first(&s))
}

fn sky(tok: &str) -> Option<String> {
    if re!(r"^(CLR|SKC|NSC|NCD)$").is_match(tok) {
        return Some("Sky clear".into());
    }
    let m = re!(r"^(FEW|SCT|BKN|OVC|VV)(\d{3}|///)(CB|TCU)?$").captures(tok)?;
    let base = if &m[2] == "///" { "unknown height".to_string() } else { ft(&m[2]) };
    let extra = match m.get(3).map(|x| x.as_str()) {
        Some("CB") => " (cumulonimbus)",
        Some("TCU") => " (towering cumulus)",
        _ => "",
    };
    Some(format!("{} {}{extra}", cap_first(cover(&m[1])), if &m[1] == "VV" { base } else { format!("at {base}") }))
}

fn temp_c(s: &str) -> f64 {
    if let Some(r) = s.strip_prefix('M') { -(pint(r) as f64) } else { pint(s) as f64 }
}
fn deg_f(c: f64, digits: usize) -> String {
    format!("{:.*}°F", digits, c * 9.0 / 5.0 + 32.0)
}
fn in_hg(hpa: f64) -> String {
    format!("{:.2} inHg", hpa * 0.02953)
}

fn common(tok: &str, prev: Option<&str>) -> Option<String> {
    wind(tok).or_else(|| visibility(tok, prev)).or_else(|| sky(tok)).or_else(|| weather(tok))
}

fn remarks(tokens: &[&str]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let t = tokens[i];
        let next = |k: usize| tokens.get(i + k).copied().unwrap_or("");
        if t == "AO1" {
            out.push("Automated station without precipitation sensor".into());
        } else if t == "AO2" {
            out.push("Automated station with precipitation sensor".into());
        } else if let Some(m) = re!(r"^SLP(\d{3})$").captures(t) {
            let v = pint(&m[1]) as f64 / 10.0;
            out.push(format!("Sea-level pressure {}", in_hg(if v < 50.0 { 1000.0 + v } else { 900.0 + v })));
        } else if let Some(m) = re!(r"^T([01])(\d{3})([01])(\d{3})$").captures(t) {
            let a = if &m[1] == "1" { -1.0 } else { 1.0 } * pint(&m[2]) as f64 / 10.0;
            let b = if &m[3] == "1" { -1.0 } else { 1.0 } * pint(&m[4]) as f64 / 10.0;
            out.push(format!("Exact temperature {}, dew point {}", deg_f(a, 1), deg_f(b, 1)));
        } else if t == "PK" && next(1) == "WND" && re!(r"^(\d{3})(\d{2,3})/(\d{2,4})$").is_match(next(2)) {
            let m = re!(r"^(\d{3})(\d{2,3})/(\d{2,4})$").captures(next(2)).unwrap();
            let at = if m[3].len() > 2 { format!("{}:{}", &m[3][..2], &m[3][2..]) } else { format!(":{}", &m[3]) };
            out.push(format!("Peak wind {}° at {} kt at {at}", &m[1], pint(&m[2])));
            i += 2;
        } else if t == "WSHFT" {
            let fropa = next(2) == "FROPA";
            out.push(format!("Wind shift at {}{}", next(1), if fropa { " (frontal passage)" } else { "" }));
            i += if fropa { 2 } else { 1 };
        } else if t == "PRESRR" {
            out.push("Pressure rising rapidly".into());
        } else if t == "PRESFR" {
            out.push("Pressure falling rapidly".into());
        } else if t == "TSNO" {
            out.push("Thunderstorm sensor not working".into());
        } else if t == "PWINO" {
            out.push("Precipitation identifier not working".into());
        } else if t == "FZRANO" {
            out.push("Freezing rain sensor not working".into());
        } else if t == "PNO" {
            out.push("Rain gauge not working".into());
        } else if t == "RVRNO" {
            out.push("Runway visual range not available".into());
        } else if t == "$" {
            out.push("Station needs maintenance".into());
        } else if re!(r"^LTG(IC|CC|CG|CA)*$").is_match(t) {
            if re!(r"^(DSNT|VC|OHD|ALQDS|[NESW]+)").is_match(next(1)) {
                out.push(format!("Lightning {}", expand(next(1)).to_lowercase()));
                i += 1;
            } else {
                out.push("Lightning".into());
            }
        } else if let Some(m) = re!(r"^([A-Z]{2})B(\d{2,4})(?:E(\d{2,4}))?$").captures(t).filter(|m| phenom(&m[1]).is_some() || &m[1] == "TS") {
            let what = if &m[1] == "TS" { "Thunderstorm".to_string() } else { cap_first(phenom(&m[1]).unwrap_or("")) };
            let b = &m[2][m[2].len().saturating_sub(2)..];
            let e = m.get(3).map(|x| format!(", ended :{}", &x.as_str()[x.as_str().len().saturating_sub(2)..])).unwrap_or_default();
            out.push(format!("{what} began :{b}{e}"));
        } else if let Some(m) = re!(r"^P(\d{4})$").captures(t) {
            out.push(format!("Precipitation last hour {:.2} in", pint(&m[1]) as f64 / 100.0));
        } else if let Some(m) = re!(r"^([67])(\d{4})$").captures(t) {
            out.push(format!("Precipitation {} {:.2} in", if &m[1] == "6" { "last 3–6 hours" } else { "last 24 hours" }, pint(&m[2]) as f64 / 100.0));
        } else if let Some(m) = re!(r"^4/(\d{3})$").captures(t) {
            out.push(format!("Snow depth {} in", pint(&m[1])));
        } else if let Some(m) = re!(r"^([12])([01])(\d{3})$").captures(t) {
            let v = if &m[2] == "1" { -1.0 } else { 1.0 } * pint(&m[3]) as f64 / 10.0;
            out.push(format!("{} temperature last 6 h {}", if &m[1] == "1" { "Highest" } else { "Lowest" }, deg_f(v, 0)));
        } else if let Some(m) = re!(r"^5(\d)(\d{3})$").captures(t) {
            let trend = ["rising", "rising", "rising", "rising", "steady", "falling", "falling", "falling", "falling", "steady"].get(pint(&m[1]) as usize).copied().unwrap_or("changed");
            out.push(format!("Pressure {trend} {:.2} inHg in the last 3 h", pint(&m[2]) as f64 / 10.0 * 0.02953));
        } else if let Some(m) = re!(r"^4([01])(\d{3})([01])(\d{3})$").captures(t) {
            let hi = if &m[1] == "1" { -1.0 } else { 1.0 } * pint(&m[2]) as f64 / 10.0;
            let lo = if &m[3] == "1" { -1.0 } else { 1.0 } * pint(&m[4]) as f64 / 10.0;
            out.push(format!("24-hour high {}, low {}", deg_f(hi, 0), deg_f(lo, 0)));
        } else if t == "CIG" && !next(1).is_empty() {
            i += 1;
            match re!(r"^(\d{3})V(\d{3})$").captures(tokens[i]) {
                Some(v) => out.push(format!("Ceiling varying {} to {}", ft(&v[1]), ft(&v[2]))),
                None => out.push(format!("Ceiling {}", tokens[i])),
            }
        } else if t == "VIS" && !next(1).is_empty() {
            i += 1;
            out.push(format!("Visibility {} miles", tokens[i].replace('V', " to ")));
        } else if t == "ACSL" {
            out.push("Altocumulus standing lenticular clouds".into());
        } else if t == "VIRGA" {
            out.push("Virga (rain not reaching the ground)".into());
        } else if t == "FROPA" {
            out.push("Frontal passage".into());
        } else {
            out.push(expand(t));
        }
        i += 1;
    }
    // merge leftover single words into one line so it reads naturally
    let mut merged: Vec<String> = Vec::new();
    for s in out {
        let single = s.split(' ').count() == 1 && !s.chars().any(|c| c.is_ascii_digit());
        match merged.last_mut() {
            Some(last) if single && last.split(' ').count() < 6 => {
                last.push(' ');
                last.push_str(&s.to_lowercase());
            }
            _ => merged.push(s),
        }
    }
    merged
}

/// METAR / SPECI → plain-English lines
pub fn decode_metar(raw: &str) -> Vec<String> {
    if raw.trim().is_empty() {
        return vec![];
    }
    let (body, rmk) = match raw.split_once(" RMK ") {
        Some((b, r)) => (b, Some(r)),
        None => (raw, None),
    };
    let toks: Vec<&str> = body.split_whitespace().collect();
    let mut out = Vec::new();
    let mut i = 0;
    if matches!(toks.first(), Some(&"METAR") | Some(&"SPECI")) {
        if toks[0] == "SPECI" {
            out.push("Special (unscheduled) report".into());
        }
        i += 1;
    }
    let station = toks.get(i).copied().unwrap_or("");
    i += 1;
    if let Some(tm) = toks.get(i).and_then(|t| re!(r"^(\d{2})(\d{2})(\d{2})Z$").captures(t)) {
        out.push(format!("{station}, observed day {} at {}:{} UTC", &tm[1], &tm[2], &tm[3]));
        i += 1;
    }
    while i < toks.len() {
        let t = toks[i];
        if t == "AUTO" {
            out.push("Fully automated report".into());
        } else if t == "COR" {
            out.push("Corrected report".into());
        } else if re!(r"^\d{3}V\d{3}$").is_match(t) {
            out.push(format!("Wind direction varying {}° to {}°", &t[..3], &t[4..]));
        } else if re!(r"^R\d{2}[LRC]?/").is_match(t) {
            match re!(r"^R(\d{2}[LRC]?)/([PM])?(\d{4})(?:V([PM])?(\d{4}))?FT").captures(t) {
                Some(m) => {
                    let q = match m.get(2).map(|x| x.as_str()) {
                        Some("M") => "below ",
                        Some("P") => "above ",
                        _ => "",
                    };
                    let hi = m.get(5).map(|x| format!("–{}", crate::nav::navlog::thousands(pint(x.as_str()) as f64))).unwrap_or_default();
                    out.push(format!("Runway {} visual range {q}{}{hi} ft", &m[1], crate::nav::navlog::thousands(pint(&m[3]) as f64)));
                }
                None => out.push(t.into()),
            }
        } else if re!(r"^M?\d{2}/(M?\d{2})?$").is_match(t) {
            let (a, b) = t.split_once('/').unwrap_or((t, ""));
            if b.is_empty() {
                out.push(format!("Temperature {}", deg_f(temp_c(a), 0)));
            } else {
                out.push(format!("Temperature {}, dew point {} (spread {}°F)", deg_f(temp_c(a), 0), deg_f(temp_c(b), 0), ((temp_c(a) - temp_c(b)) * 9.0 / 5.0).round()));
            }
        } else if re!(r"^A\d{4}$").is_match(t) {
            out.push(format!("Altimeter {}.{} inHg", &t[1..3], &t[3..]));
        } else if re!(r"^Q\d{4}$").is_match(t) {
            out.push(format!("Altimeter {}", in_hg(pint(&t[1..]) as f64)));
        } else if re!(r"^\d$").is_match(t) && toks.get(i + 1).is_some_and(|n| n.ends_with("SM")) {
            // whole part of "1 1/2SM"
        } else {
            out.push(common(t, if i > 0 { Some(toks[i - 1]) } else { None }).unwrap_or_else(|| t.into()));
        }
        i += 1;
    }
    if let Some(r) = rmk {
        let toks: Vec<&str> = r.split_whitespace().collect();
        out.extend(remarks(&toks).into_iter().map(|s| format!("Remark: {s}")));
    }
    out
}

pub struct TafPeriod {
    pub title: String,
    pub lines: Vec<String>,
}

/// TAF → periods with plain-English lines
pub fn decode_taf(raw: &str) -> Vec<TafPeriod> {
    if raw.trim().is_empty() {
        return vec![];
    }
    let pre = re!(r"^TAF\s+(AMD|COR)?\s*").replace(raw, |c: &regex::Captures| if c[0].contains("AMD") { "AMD ".to_string() } else { String::new() });
    let toks: Vec<&str> = pre.split_whitespace().collect();
    let mut periods: Vec<TafPeriod> = Vec::new();
    let mut i = 0;
    let amended = toks.first() == Some(&"AMD");
    if amended {
        i += 1;
    }
    let station = toks.get(i).copied().unwrap_or("");
    i += 1;
    if toks.get(i).is_some_and(|t| re!(r"^\d{6}Z$").is_match(t)) {
        i += 1;
    }
    let header = format!("{station}{}", if amended { " (amended)" } else { "" });
    let span = |s: &str| match re!(r"^(\d{2})(\d{2})/(\d{2})(\d{2})$").captures(s) {
        Some(m) => format!("day {} {}:00 to day {} {}:00 UTC", &m[1], &m[2], &m[3], &m[4]),
        None => s.to_string(),
    };
    if let Some(vt) = toks.get(i).and_then(|t| re!(r"^(\d{2})(\d{2})/(\d{2})(\d{2})$").captures(t)) {
        periods.push(TafPeriod { title: format!("{header}: day {} {}:00 to day {} {}:00 UTC", &vt[1], &vt[2], &vt[3], &vt[4]), lines: vec![] });
        i += 1;
    }
    let stop = re!(r"^(AUTOMATED|SENSOR|METWATCH|LAST|NO|AMDS|AFT|TIL|FROM|AND)$");
    while i < toks.len() {
        let t = toks[i];
        if let Some(m) = re!(r"^FM(\d{2})(\d{2})(\d{2})$").captures(t) {
            periods.push(TafPeriod { title: format!("From day {} at {}:{} UTC", &m[1], &m[2], &m[3]), lines: vec![] });
        } else if t == "BECMG" {
            i += 1;
            periods.push(TafPeriod { title: format!("Becoming, {}", span(toks.get(i).copied().unwrap_or(""))), lines: vec![] });
        } else if t == "TEMPO" {
            i += 1;
            periods.push(TafPeriod { title: format!("Temporarily, {}", span(toks.get(i).copied().unwrap_or(""))), lines: vec![] });
        } else if let Some(m) = re!(r"^PROB(\d{2})$").captures(t) {
            let tempo = toks.get(i + 1) == Some(&"TEMPO");
            if tempo {
                i += 1;
            }
            i += 1;
            periods.push(TafPeriod { title: format!("{}% chance{}, {}", &m[1], if tempo { " (temporary)" } else { "" }, span(toks.get(i).copied().unwrap_or(""))), lines: vec![] });
        } else if let Some(cur) = periods.last_mut() {
            if re!(r"^\d$").is_match(t) && toks.get(i + 1).is_some_and(|n| n.ends_with("SM")) {
                i += 1;
                continue;
            }
            if re!(r"^WS\d{3}/\d{5}KT$").is_match(t) {
                cur.lines.push(format!("Low-level wind shear at {}: wind {}° at {} kt", ft(&t[2..5]), &t[6..9], pint(&t[9..11])));
            } else if re!(r"^QNH\d{4}INS$").is_match(t) {
                cur.lines.push(format!("Lowest altimeter {}.{} inHg", &t[3..5], &t[5..7]));
            } else if re!(r"^5\d{5}$").is_match(t) {
                cur.lines.push("Turbulence forecast (military code)".into());
            } else if re!(r"^6\d{5}$").is_match(t) {
                cur.lines.push("Icing forecast (military code)".into());
            } else if t == "NSW" {
                cur.lines.push("No significant weather".into());
            } else if let Some(mm) = re!(r"^T([XN])(M?\d{2})/(\d{2})(\d{2})Z$").captures(t) {
                cur.lines.push(format!("{} temperature {} at {}:00 UTC on day {}", if &mm[1] == "X" { "High" } else { "Low" }, deg_f(temp_c(&mm[2]), 0), &mm[4], &mm[3]));
            } else if stop.is_match(t) {
                cur.lines.push(expand(&toks[i..].join(" ")));
                break;
            } else {
                cur.lines.push(common(t, if i > 0 { Some(toks[i - 1]) } else { None }).unwrap_or_else(|| expand(t)));
            }
        }
        i += 1;
    }
    periods
}

// ---------------------------------------------------------------- PIREP

fn turb_word(w: &str) -> Option<&'static str> {
    Some(match w {
        "NEG" => "no",
        "SMTH" => "smooth",
        "LGT" => "light",
        "SMTH-LGT" => "smooth to light",
        "LGT-MOD" => "light to moderate",
        "MOD" => "moderate",
        "MOD-SEV" => "moderate to severe",
        "SEV" => "severe",
        "EXTRM" => "extreme",
        "OCNL" => "occasional",
        "INTMT" => "intermittent",
        "CONS" => "continuous",
        "CHOP" => "chop",
        "CAT" => "clear-air turbulence",
        "LLWS" => "low-level wind shear",
        "TRACE" | "TR" => "trace",
        "CLR" => "clear",
        "RIME" => "rime",
        "MX" | "MXD" => "mixed",
        _ => return None,
    })
}

/// PIREP (UA/UUA) → plain-English lines
pub fn decode_pirep(raw: &str) -> Vec<String> {
    if raw.trim().is_empty() {
        return vec![];
    }
    let mut parts: Vec<&str> = raw.split('/').map(str::trim).filter(|s| !s.is_empty()).collect();
    let mut out = Vec::new();
    let head = if parts.is_empty() { "" } else { parts.remove(0) };
    out.push(if re!(r"\bUUA\b").is_match(head) { "URGENT pilot report".to_string() } else { "Pilot report".to_string() });
    let words = |s: &str| s.split_whitespace().map(|w| turb_word(w).map(String::from).or_else(|| CONTRACTIONS.get(w).map(|x| x.to_string())).unwrap_or_else(|| w.to_lowercase())).collect::<Vec<_>>().join(" ");
    for part in parts {
        let p = re!(r"\b[A-Z]{5}-\d+\b").replace_all(part, "").trim().to_string();
        let (k, v) = if p.len() >= 2 && p.is_char_boundary(2) { (&p[..2], p[2..].trim()) } else { (p.as_str(), "") };
        match k {
            "OV" => match re!(r"^([A-Z0-9]{3,4})(\d{3})(\d{3})").captures(v) {
                Some(m) => out.push(format!("Location: {} NM from {} on the {}° radial", pint(&m[3]), &m[1], &m[2])),
                None => out.push(format!("Location: {v}")),
            },
            "TM" if v.len() >= 4 => out.push(format!("Time: {}:{} UTC", &v[..2], &v[2..4])),
            "FL" => out.push(if re!(r"^\d{3}$").is_match(v) {
                format!("Altitude {} ft MSL", crate::nav::navlog::thousands(pint(v) as f64 * 100.0))
            } else if v == "UNKN" {
                "Altitude unknown".into()
            } else {
                format!("Altitude {v}")
            }),
            "TP" => out.push(format!("Aircraft type {v}")),
            "SK" => {
                let layers: Vec<String> = v
                    .split_whitespace()
                    .map(|t| {
                        let top = re!(r"TOPS?(\d{3})").captures(t).map(|c| format!("tops {}", ft(&c[1])));
                        let layer = sky(&re!(r"-?TOPS?\d{3}").replace(t, ""));
                        let parts: Vec<String> = [layer, top].into_iter().flatten().collect();
                        if parts.is_empty() { t.to_string() } else { parts.join(", ") }
                    })
                    .collect();
                out.push(format!("Sky: {}", layers.join("; ")));
            }
            "WX" => out.push(format!("Weather: {}", v.split_whitespace().map(|t| visibility(t, None).or_else(|| weather(t)).unwrap_or_else(|| t.into())).collect::<Vec<_>>().join(", "))),
            "TA" => out.push(format!("Outside air temperature {}", deg_f(temp_c(v), 0))),
            "WV" => match re!(r"^(\d{3})(\d{2,3})").captures(v) {
                Some(m) => out.push(format!("Wind {}° at {} kt", &m[1], pint(&m[2]))),
                None => out.push(format!("Wind {v}")),
            },
            "TB" | "IC" => {
                let alts = re!(r"(\d{3})-(\d{3})").captures(v).map(|c| format!(" from {} to {}", ft(&c[1]), ft(&c[2]))).unwrap_or_default();
                let rest = re!(r"\d{3}-\d{3}").replace(v, "");
                out.push(format!("{}: {}{alts}", if k == "TB" { "Turbulence" } else { "Icing" }, words(&rest).trim()));
            }
            "RM" => out.push(format!("Remarks: {}", expand(v))),
            _ => out.push(expand(&p)),
        }
    }
    out
}

// ---------------------------------------------------------------- AIRMET / SIGMET

fn hazard_name(h: &str) -> Option<&'static str> {
    Some(match h {
        "CONVECTIVE" | "TS" => "Thunderstorms",
        "TURB" => "Turbulence",
        "ICE" => "Icing",
        "IFR" => "IFR conditions (low ceilings/visibility)",
        "MTN OBSCN" | "MTN_OBSC" | "MT_OBSC" => "Mountains obscured",
        "LLWS" => "Low-level wind shear",
        "SFC_WND" => "Strong surface winds",
        "ASH" => "Volcanic ash",
        "TC" => "Tropical cyclone",
        "DS" => "Dust storm",
        "SS" => "Sandstorm",
        _ => return None,
    })
}

fn alt_txt(v: Option<f64>) -> Option<String> {
    let v = v?;
    Some(if v == 0.0 {
        "the surface".into()
    } else if v >= 18000.0 {
        format!("FL{}", (v / 100.0).round())
    } else {
        format!("{} ft", crate::nav::navlog::thousands(v))
    })
}

fn local_time(unix: i64) -> String {
    use chrono::TimeZone;
    chrono::Local.timestamp_opt(unix, 0).single().map(|d| d.format("%a %H:%M").to_string()).unwrap_or_default()
}

pub fn str_of(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        x => x.to_string(),
    }
}

/// Structured summary of an aviationweather.gov airsigmet record + the expanded bulletin.
pub fn describe_sigmet(s: &Value) -> (Vec<String>, String) {
    let mut summary = Vec::new();
    let hazard = str_of(&s["hazard"]);
    let ty = str_of(&s["airSigmetType"]);
    let kind = if hazard == "CONVECTIVE" {
        "Convective SIGMET".to_string()
    } else if !ty.is_empty() {
        ty.clone()
    } else {
        "Advisory".to_string()
    };
    let sev = s["severity"].as_f64().filter(|_| hazard != "CONVECTIVE").map(|v| format!(" (severity {v})")).unwrap_or_default();
    summary.push(format!("{kind}: {}{sev}", hazard_name(&hazard).map(String::from).unwrap_or_else(|| hazard.to_lowercase())));
    let lo = alt_txt(s["altitudeLow1"].as_f64());
    let hi = alt_txt(s["altitudeHi1"].as_f64());
    if lo.is_some() || hi.is_some() {
        summary.push(format!("From {} to {}", lo.unwrap_or_else(|| "the surface".into()), hi.unwrap_or_else(|| "unknown".into())));
    }
    if let (Some(a), Some(b)) = (s["validTimeFrom"].as_i64(), s["validTimeTo"].as_i64()) {
        summary.push(format!("Valid {} to {}", local_time(a), local_time(b)));
    }
    if let (Some(d), Some(spd)) = (s["movementDir"].as_f64(), s["movementSpd"].as_f64().filter(|v| *v > 0.0)) {
        summary.push(format!("Moving from {d}° at {spd} kt"));
    }
    let raw = str_of(&s["rawAirSigmet"]);
    let body: Vec<&str> = raw
        .lines()
        .filter(|l| !re!(r"^(WS|WA|WH|WC|WV)\w*\s+K\w+\s+\d{6}").is_match(l) && !re!(r"^[A-Z]{4}\s+(WST|WS|WA)\s+\d{6}").is_match(l))
        .collect();
    let body = body.join(" ");
    let body = re!(r"\bFRM\b").replace_all(&body, "FROM");
    let body = re!(r"(\d{3})(\d{2})KT").replace_all(&body, |c: &regex::Captures| format!("{}° at {} kt", &c[1], pint(&c[2])));
    let body = re!(r"\bTOPS (TO|ABV) FL(\d{3})").replace_all(&body, |c: &regex::Captures| format!("TOPS {} FL{}", if &c[1] == "ABV" { "ABOVE" } else { "TO" }, &c[2]));
    let body = re!(r"\b(\d{2})(\d{2})Z\b").replace_all(&body, "$1:$2 UTC");
    (summary, expand(&body))
}

/// G-AIRMET record → one line
pub fn describe_gairmet(g: &Value) -> String {
    let hz = str_of(&g["hazard"]);
    let name = hazard_name(&hz).map(String::from).unwrap_or_else(|| match hz.as_str() {
        "FZLVL" => "Freezing level".into(),
        "M_FZLVL" => "Multiple freezing levels".into(),
        "TURB-HI" => "Turbulence above FL180".into(),
        "TURB-LO" => "Turbulence below FL180".into(),
        _ => hz.clone(),
    });
    let level = |v: &Value| -> Option<f64> {
        let s = str_of(v);
        let n: f64 = s.parse().ok()?;
        Some(if s.len() <= 3 { n * 100.0 } else { n })
    };
    if hz == "FZLVL" {
        return match level(&g["level"]) {
            Some(l) => format!("Freezing level {} ft", crate::nav::navlog::thousands(l)),
            None => "Freezing level".into(),
        };
    }
    let alt = match level(&g["top"]) {
        Some(top) => format!(" from {} to {}", alt_txt(level(&g["base"])).unwrap_or_else(|| "the surface".into()), alt_txt(Some(top)).unwrap_or_default()),
        None => String::new(),
    };
    let due = str_of(&g["due_to"]);
    let due = if due.is_empty() { String::new() } else { format!(" due to {}", expand(&due).to_lowercase()) };
    let when = chrono::DateTime::parse_from_rfc3339(&str_of(&g["validTime"]))
        .map(|d| d.with_timezone(&chrono::Local).format("%H:%M").to_string())
        .unwrap_or_default();
    format!("{name}{alt}{due}, forecast for {when}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metar() {
        let l = decode_metar("METAR KPHX 241851Z 26008G15KT 10SM FEW080 BKN250 32/M04 A2992 RMK AO2 SLP119 T03221044");
        assert!(l[0].starts_with("KPHX, observed day 24 at 18:51 UTC"));
        assert!(l.contains(&"Wind from 260° at 8 kt, gusting 15 kt".to_string()));
        assert!(l.contains(&"Few clouds at 8,000 ft".to_string()));
        assert!(l.iter().any(|x| x.starts_with("Temperature 90°F, dew point 25°F")));
        assert!(l.contains(&"Altimeter 29.92 inHg".to_string()));
        assert!(l.iter().any(|x| x == "Remark: Automated station with precipitation sensor"));
        let v = decode_metar("KXYZ 241851Z 00000KT 1 1/2SM -RA BR OVC004 10/09 A3001");
        assert!(v.contains(&"Visibility 1 1/2 miles".to_string()), "{v:?}");
        assert!(v.contains(&"Light rain".to_string()), "{v:?}");
    }

    #[test]
    fn taf() {
        let p = decode_taf("TAF KPHX 241720Z 2418/2524 27010KT P6SM FEW100 FM250200 VRB05KT P6SM SKC TEMPO 2510/2514 BKN030");
        assert_eq!(p.len(), 3);
        assert!(p[1].title.starts_with("From day 25 at 02:00"));
        assert!(p[2].title.starts_with("Temporarily"));
    }

    #[test]
    fn pirep_and_expand() {
        let l = decode_pirep("PRC UA /OV DRK090020/TM 1830/FL085/TP C172/TB LGT/RM SMOOTH ABV 080");
        assert!(l.iter().any(|x| x.contains("20 NM from DRK on the 090° radial")), "{l:?}");
        assert!(l.iter().any(|x| x == "Turbulence: light"), "{l:?}");
        assert_eq!(expand("RWY 03 CLSD TO ACFT OVER 12500 LBS"), "Runway 03 closed to aircraft over 12500 lbs");
    }
}
