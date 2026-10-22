"""Airport remarks + FBO directory.

1. Classifies every NASR airport remark (fuel/FBO, PPR, noise, lighting, pattern, runway,
   customs/fees, cautions, ATC, other) and adds a plain-English version (FAA contractions spelled out).
2. Builds an FBO list per airport by merging:
     - Overture Maps places near the field whose name/category looks like an FBO
     - FBO / fuel / call-out details parsed from the NASR remarks
     - NASR services (fuel types, repair, oxygen, storage)
Writes tables `remark2` and `fbo` into data/build/aviation.sqlite.

Usage: .venv/bin/python -I pipeline/build_fbo.py
"""
import json
import math
import re
import sqlite3
from collections import defaultdict
from pathlib import Path

import duckdb

ROOT = Path(__file__).resolve().parent.parent
DB = ROOT / "data" / "build" / "aviation.sqlite"
PLACES = ROOT / "data" / "raw" / "overture" / "aviation_places.parquet"

# FAA contractions (JO 7340) seen in airport remarks
ABBR = {
    "ACFT": "aircraft", "ADZ": "advise", "AFT": "after", "AGL": "AGL", "AMGR": "airport manager", "APCH": "approach",
    "APRX": "approximately", "ARPT": "airport", "ARR": "arrival", "AVBL": "available", "BFR": "before", "BLDG": "building",
    "BTN": "between", "BTW": "between", "CLSD": "closed", "CNTR": "center", "CONSLY": "continuously", "CTAF": "CTAF",
    "CTC": "contact", "CTN": "caution", "DEP": "departure", "DLY": "daily", "DSPLCD": "displaced", "DTHR": "displaced threshold",
    "E": "east", "EFCT": "effect", "EXC": "except", "FLD": "field", "FM": "from", "FREQ": "frequency", "FT": "ft",
    "HEL": "helicopter", "HOL": "holidays", "HR": "hour", "HRS": "hours", "INDEFLY": "indefinitely", "INFO": "information",
    "INVOF": "in the vicinity of", "LCL": "local", "LGT": "light", "LGTD": "lighted", "LGTS": "lights", "LNDG": "landing",
    "LRG": "large", "LTD": "limited", "MAINT": "maintenance", "MIL": "military", "MNTN": "maintain", "MON": "Mon",
    "MSL": "MSL", "N": "north", "NA": "not authorized", "NML": "normal", "NR": "number", "NTFY": "notify", "OBSTN": "obstruction",
    "OPNS": "operations", "OPR": "operate", "OPS": "operations", "OTRW": "otherwise", "OVNGT": "overnight", "PAT": "pattern",
    "PPR": "prior permission required", "PRKG": "parking", "PROC": "procedure", "PSGR": "passenger", "PWR": "power",
    "RQR": "required", "RQRD": "required", "RSTD": "restricted", "RWY": "runway", "RWYS": "runways", "S": "south",
    "SAT": "Sat", "SKED": "scheduled", "SR": "sunrise", "SS": "sunset", "SUN": "Sun", "SVC": "service", "SVCS": "services",
    "TFC": "traffic", "TGL": "touch-and-go landings", "THR": "threshold", "TKOF": "takeoff", "TPA": "pattern altitude",
    "TRAN": "transient", "TSNT": "transient", "TWR": "tower", "TWY": "taxiway", "TWYS": "taxiways", "UNLGTD": "unlighted",
    "UNLGT": "unlighted", "UNMKD": "unmarked", "UNSKED": "unscheduled", "VIS": "visibility", "W": "west", "WI": "within",
    "WKDAYS": "weekdays", "WKENDS": "weekends", "WKNDS": "weekends", "WND": "wind", "WSPN": "wingspan", "PVT": "private",
    "PRVDD": "provided", "RDC": "runway design code", "APRC": "approach reference code", "DPRC": "departure reference code",
    "CD": "clearance delivery", "ATCT": "control tower", "RON": "remain overnight", "INBD": "inbound", "OUTBD": "outbound",
    "ADJ": "adjacent", "ALT": "altitude", "BCN": "beacon", "CONT": "continuous", "DRCT": "direct", "FUEL": "fuel",
    "MID": "middle", "NE": "northeast", "NW": "northwest", "SE": "southeast", "SW": "southwest", "ACTVT": "activate",
    "INTST": "intensity", "INTSTY": "intensity", "MED": "medium", "LIRL": "low-intensity runway lights",
    "MIRL": "medium-intensity runway lights", "HIRL": "high-intensity runway lights", "REIL": "runway end identifier lights",
    "PAPI": "PAPI", "VASI": "VASI", "SFC": "surface", "CC": "credit card", "AFT HR": "after-hours", "OFC": "office",
    "OPRS": "operators", "OPRG": "operating", "PERMANENTLY": "permanently", "WKDA": "weekdays", "WX": "weather",
    "ACCT": "account", "PRIM": "primary", "EMERG": "emergency", "MAX": "maximum", "WGT": "weight", "WT": "weight",
    "CERT": "certified", "CALM": "calm", "PTNS": "portions", "DRG": "during", "HAZ": "hazard", "HIGHWAY": "highway",
}

CATS = [
    ("fuel", re.compile(r"\b(FUEL|100LL|JET-?A|MOGAS|UL94|SELF[- ]?S(ER)?VE?|AVGAS|FBO|CALL ?OUT|AFT(ER)? HRS?|CREDIT CARD|AVCARD|FUEL TRUCK)\b")),
    ("ppr", re.compile(r"\b(PPR|PRIOR PERMISSION|PERMISSION RQR|PRIOR APVL)\b")),
    ("customs", re.compile(r"\b(CUSTOMS|USER FEE|LNDG FEE|LANDING FEE|FEE|CBP|AOE)\b")),
    ("noise", re.compile(r"\b(NOISE|CURFEW|ABATEMENT)\b")),
    ("caution", re.compile(r"\b(CTN|CAUTION|BIRDS?|WILDLIFE|DEER|COYOTE|ELK|GEESE|OBSTN|PWR LINES?|TREES|HILL|MTS|MOUNTAIN|PJE|PARACHUT|GLIDER|ULTRALIGHT|UAS|DRONE|MIL JET)\b")),
    ("lighting", re.compile(r"\b(LGT|LGTS|MIRL|HIRL|LIRL|REIL|PAPI|VASI|BCN|ACTVT|PCL|LIGHTING)\b")),
    ("pattern", re.compile(r"\b(TPA|PAT|PATTERN|RGT TFC|RIGHT TFC|TFC PAT|OVERHEAD)\b")),
    ("atc", re.compile(r"\b(ATCT|TWR|CLNC|CD WHEN|APCH|CTC [A-Z]+ (APCH|CNTR|TWR)|UNICOM|CTAF)\b")),
    ("runway", re.compile(r"\b(RWY|TWY|DTHR|THR|BLAST PAD|RUNUP|TGL|PAVEMENT|GRAVEL|TURF|SOFT)\b")),
]
SKIP_ELEMENTS = {"ASP_ANLYS_DTRM_CODE", "ARPT_PSN_SOURCE", "ARPT_ELEV_SOURCE", "MANAGER", "OWNER", "INSPECTOR_CODE"}
PHONE = re.compile(r"(?:C?1?[- (]*)?(\d{3})[) .-]*(\d{3})[ .-]*(\d{4})")


def plain(text: str) -> str:
    def rep(m):
        w = m.group(0)
        return ABBR.get(w, w.lower() if len(w) > 3 and w.isalpha() and w not in ("CTAF", "PAPI", "VASI", "UNICOM") else w)
    out = re.sub(r"\b[A-Z][A-Z]+\b", rep, text)
    out = re.sub(r"\s+", " ", out).strip()
    return out[:1].upper() + out[1:]


def classify(element: str, text: str) -> str:
    if element in ("100LL", "A", "A+", "A1", "A1+", "B", "MOGAS", "UL94", "100", "J8", "J5"):
        return "fuel"
    for name, rx in CATS:
        if rx.search(text):
            return name
    if element in ("LGT_SKED", "BCN_LGT_SKED", "H1"):
        return "lighting"
    if element in ("TPA",):
        return "pattern"
    if element in ("LNDG_FEE_FLAG", "USER_FEE_FLAG", "CUST_FLAG"):
        return "customs"
    if re.fullmatch(r"\d{2}[LRC]?(/\d{2}[LRC]?)?", element or ""):
        return "runway"
    return "other"


def phones(text: str):
    return [f"{a}-{b}-{c}" for a, b, c in PHONE.findall(text) if a[0] in "23456789"]


# Strong FBO signals: generic FBO words, or a chain's full name.
FBO_WORDS = re.compile(r"(?i)(\bfbo\b|jet ?cent(er|re)s?\b|air ?cent(er|re)\b|aero ?cent(er|re)|jet ?port\b|"
                       r"executive terminal|fixed base)")
# names that are the airport itself, airlines, brokers etc., never an FBO
NOT_FBO_NAME = re.compile(r"(?i)(airport|aeropuerto|a[eé]roport|aeroporto|flughafen|terminal|\bgate\b|airlines?\b|airways|air lines|"
                          r"cargo|sales|broker|international\b|municipal|\bmuni\b|ambulance|attendants|association|operations center|"
                          r"air station|air force|air base|\bbase\b|national guard|city of|county|department|authority|"
                          r"air canada|air france|air china|aer lingus|westjet|volaris|aeromexico|air india|air new zealand|"
                          r"charter|ministry|museum)")
CHAINS = ["Signature Flight Support", "Signature Aviation", "Atlantic Aviation", "Million Air", "Sheltair", "Wilson Air Center",
          "Cutter Aviation", "Ross Aviation", "TAC Air", "Modern Aviation", "Jet Aviation", "Clay Lacy", "Avflight",
          "Banyan Air", "Lynx FBO", "XJet", "Pentastar", "Ascend Aviation", "Skyservice", "Mercury Air", "Jackson Jet Center",
          "Lux Air", "Galaxy FBO", "Meridian", "Henriksen Jet", "Sun Air Jets", "Desert Jet", "Swift Aviation", "Ponderosa Aviation",
          "Fontainebleau Aviation", "Cirrus Aviation Services", "Saker Aviation", "Hawthorne Global", "Business Jet Center",
          "Landmark Aviation", "Odyssey Aviation", "World Fuel", "Epic Fuels", "Phillips 66 Aviation", "Shell Aviation",
          "Avfuel", "Sheltair Aviation", "Western Aircraft", "Elliott Aviation"]
CHAIN_RX = re.compile("(?i)(" + "|".join(re.escape(c) for c in CHAINS) + ")")
AVIATION = re.compile(r"(?i)(aviation|\baero|aircraft|avionics|\bjets?\b|flight|\bair\b|airport|hangar|heli|airways|aviator|aerospace|airmotive|\bmro\b)")
NOT_FBO = re.compile(r"(?i)(museum|school|academy|college|university|institute|church|restaurant|cafe|grill|hotel|\binn\b|rental car|"
                     r"hertz|avis|enterprise|budget|shuttle|taxi|parking|\btsa\b|terminal [a-z0-9]$|baggage|lounge|club\b|"
                     r"insurance|credit union|bank|realty|real estate|homes|dental|physician|\bmd\b|medicine|salon|spa\b|"
                     r"photography|vacation|wealth|cpa|accountant|tax|title|valet|theatre|theater|garage|auto|collision|"
                     r"motorcycle|car wash|apartment|graphics|audio|properties|travel|federal aviation administration|tower contract)")
SERVICE_CATS = ("aircraft_service", "private_jet_charter", "aircraft_parts_and_supplies", "flight_school", "aircraft_dealer")


def kind_of(name: str, cat: str):
    """'fbo', 'service' (maintenance, charter, avionics, school...) or None."""
    if NOT_FBO.search(name) and not CHAIN_RX.search(name):
        return None
    if CHAIN_RX.search(name):
        return "fbo"
    if FBO_WORDS.search(name) and not NOT_FBO_NAME.search(name):
        return "fbo"
    if cat == "airport" and re.search(r"(?i)\b(aviation|air|jet|aero)\b", name) and not NOT_FBO_NAME.search(name):
        return "fbo"  # Overture often files FBOs as "airport" (e.g. "Million Air Yuma")
    if NOT_FBO_NAME.search(name):
        return None
    if AVIATION.search(name) and (cat in SERVICE_CATS or cat in ("airport", "airline", "airport_terminal")):
        return "service"
    return None


def km(lat1, lon1, lat2, lon2):
    k = math.cos(math.radians((lat1 + lat2) / 2))
    return math.hypot(lat1 - lat2, (lon1 - lon2) * k) * 111.2


def main():
    db = sqlite3.connect(DB)
    db.executescript("""
        DROP TABLE IF EXISTS remark2; DROP TABLE IF EXISTS fbo;
        CREATE TABLE remark2 (site_no TEXT, element TEXT, category TEXT, text TEXT, plain TEXT, phones TEXT);
        CREATE INDEX remark2_site ON remark2(site_no);
        CREATE TABLE fbo (site_no TEXT, airport TEXT, name TEXT, brand TEXT, phone TEXT, website TEXT, address TEXT,
                          lat REAL, lon REAL, confidence REAL, sources TEXT, notes TEXT, kind TEXT);
        CREATE INDEX fbo_site ON fbo(site_no); CREATE INDEX fbo_airport ON fbo(airport);
    """)

    # 1. remarks
    fuel_notes = defaultdict(list)
    n = 0
    for site, element, text in db.execute("SELECT site_no, element, text FROM remark").fetchall():
        if not text or element in SKIP_ELEMENTS:
            continue
        cat = classify(element or "", text)
        ph = phones(text)
        db.execute("INSERT INTO remark2 VALUES (?,?,?,?,?,?)", (site, element, cat, text, plain(text), json.dumps(ph)))
        if cat == "fuel":
            fuel_notes[site].append({"text": plain(text), "raw": text, "phones": ph, "element": element})
        n += 1
    print("remarks classified:", n)

    # 2. FBO candidates from Overture
    airports = db.execute("SELECT site_no, id, icao, name, lat, lon, rank, type, use FROM airport WHERE type IN ('A','S') ").fetchall()
    grid = defaultdict(list)
    for a in airports:
        grid[(round(a[4] * 10), round(a[5] * 10))].append(a)
    con = duckdb.connect()
    places = con.execute(f"SELECT name, cat, confidence, phones, websites, brand, street, city, region, zip, lat, lon, datasets FROM '{PLACES}'").fetchall()
    print("overture candidates:", len(places))
    matched = defaultdict(list)
    for name, cat, conf, ph, web, brand, street, city, region, zp, lat, lon, ds in places:
        if not name:
            continue
        kind = kind_of(name, cat or "")
        if not kind:
            continue
        best = None
        for dy in (-1, 0, 1):
            for dx in (-1, 0, 1):
                for a in grid.get((round(lat * 10) + dy, round(lon * 10) + dx), []):
                    d = km(lat, lon, a[4], a[5])
                    lim = 3.5 if a[6] == 0 else 2.2
                    if d <= lim and (best is None or d < best[1]):
                        best = (a, d)
        if not best:
            continue
        a, d = best
        if name.strip().lower() == (a[3] or "").strip().lower():
            continue  # the airport itself
        m = CHAIN_RX.search(name)
        chain = m.group(0) if m else (brand or "")
        score = (conf or 0.5) + (0.5 if kind == "fbo" else 0) - d * 0.05
        addr = ", ".join(x for x in (street, city, region) if x)
        matched[a[0]].append({
            "airport": a[2] or a[1], "name": name.strip(), "brand": chain, "phone": (ph or [None])[0],
            "website": (web or [None])[0], "address": addr, "lat": lat, "lon": lon, "score": score, "kind": kind,
            "sources": sorted(set(["Overture Maps"] + [x for x in (ds or []) if x and x != "meta"]))})

    # 3. merge with remark-derived info; dedupe by normalised name
    total = 0
    sites = set(matched) | set(fuel_notes)
    apt_by_site = {a[0]: a for a in airports}
    for site in sites:
        a = apt_by_site.get(site)
        if not a:
            continue
        seen = {}
        for f in sorted(matched.get(site, []), key=lambda f: -f["score"]):
            key = re.sub(r"[^a-z]", "", f["name"].lower().replace("fbo", ""))[:14]
            if key in seen:
                continue
            seen[key] = f
        notes = fuel_notes.get(site, [])
        # remark text that names an FBO but no Overture match: create an entry from the remark
        for nte in notes:
            m = re.search(r"\bFBO\b[^.;]*", nte["raw"])
            if m and not seen and nte["phones"]:
                seen["remark"] = {"airport": a[2] or a[1], "name": "FBO (from FAA remarks)", "brand": "", "phone": nte["phones"][0],
                                  "website": None, "address": "", "lat": a[4], "lon": a[5], "score": 0.4, "kind": "fbo", "sources": ["FAA airport remarks"]}
        for f in seen.values():
            db.execute("INSERT INTO fbo VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)", (
                site, f["airport"], f["name"], f["brand"], f["phone"], f["website"], f["address"], f["lat"], f["lon"],
                round(f["score"], 3), json.dumps(f["sources"]), json.dumps([x["text"] for x in notes][:8]), f["kind"]))
            total += 1
    db.commit()
    print("fbo rows:", total, "airports with FBOs:", len(sites))
    print("fbo-kind rows:", db.execute("SELECT count(*), count(DISTINCT site_no) FROM fbo WHERE kind='fbo'").fetchone())
    for row in db.execute("SELECT airport, kind, name, phone FROM fbo WHERE airport IN ('KNYL','KSDL','KPHX','KGYR','KDVT','KFFZ') ORDER BY airport, confidence DESC"):
        print(row)


if __name__ == "__main__":
    main()
