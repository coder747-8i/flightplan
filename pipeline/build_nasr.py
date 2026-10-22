"""Turn the FAA NASR 28-day CSV subscription into map layers and a lookup database.

Outputs (data/build/):
  airports.geojson, runways.geojson, navaids.geojson, fixes.geojson, obstacles.geojson
  aviation.sqlite   airport details, runways, frequencies, contacts, remarks, navaids, fixes

Usage: python3 -I pipeline/build_nasr.py
"""
import csv
import json
import sqlite3
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
NASR = ROOT / "data" / "raw" / "nasr"
DOF = ROOT / "data" / "raw" / "dof" / "DOF.CSV"
OUT = ROOT / "data" / "build"

HARD_SURFACES = {"CONC", "ASPH", "PEM", "BRICK", "ASPH-CONC", "CONC-ASPH"}


def rows(name: str):
    with open(NASR / f"{name}.csv", encoding="latin-1", newline="") as f:
        for r in csv.DictReader(f):
            yield {k: (v or "").strip() for k, v in r.items()}


def num(v, default=None):
    try:
        return float(v)
    except (TypeError, ValueError):
        return default


def point(lon, lat, props):
    return {"type": "Feature", "geometry": {"type": "Point", "coordinates": [round(lon, 6), round(lat, 6)]}, "properties": props}


def write_fc(name, feats):
    OUT.mkdir(parents=True, exist_ok=True)
    with open(OUT / f"{name}.geojson", "w") as f:
        json.dump({"type": "FeatureCollection", "features": feats}, f, separators=(",", ":"))
    print(f"{name}: {len(feats)}")


def surface_is_hard(code: str) -> bool:
    return any(part in HARD_SURFACES for part in code.replace("/", "-").split("-")) or code in HARD_SURFACES


def build():
    # --- runways --------------------------------------------------------------
    rwys = defaultdict(list)
    for r in rows("APT_RWY"):
        rwys[r["SITE_NO"]].append(r)
    ends = defaultdict(list)
    for r in rows("APT_RWY_END"):
        ends[(r["SITE_NO"], r["RWY_ID"])].append(r)

    runway_feats = []
    longest = {}
    for site, lst in rwys.items():
        for r in lst:
            length = num(r["RWY_LEN"], 0)
            hard = surface_is_hard(r["SURFACE_TYPE_CODE"])
            e = ends.get((site, r["RWY_ID"]), [])
            align = next((num(x["TRUE_ALIGNMENT"]) for x in e if num(x["TRUE_ALIGNMENT"]) is not None), None)
            pts = [(num(x["LONG_DECIMAL"]), num(x["LAT_DECIMAL"])) for x in e]
            pts = [p for p in pts if p[0] is not None and p[1] is not None]
            if len(pts) == 2 and r["RWY_ID"][:1] != "H":
                runway_feats.append({
                    "type": "Feature",
                    "geometry": {"type": "LineString", "coordinates": [[round(a, 6), round(b, 6)] for a, b in pts]},
                    "properties": {"apt": r["ARPT_ID"], "id": r["RWY_ID"], "len": int(length), "wid": int(num(r["RWY_WIDTH"], 0)), "hard": int(hard)},
                })
            best = longest.get(site)
            if r["RWY_ID"][:1] != "H" and (best is None or length > best["len"]):
                longest[site] = {"len": length, "hard": hard, "align": align, "lit": r["RWY_LGT_CODE"] not in ("", "NONE", "NSTD")}
    write_fc("runways", runway_feats)

    # --- frequencies ----------------------------------------------------------
    ctaf = {}
    freqs = defaultdict(list)
    for r in rows("FRQ"):
        fac = r["SERVICED_FACILITY"] or r["FACILITY"]
        freqs[fac].append((r["FREQ_USE"], r["FREQ"], r["TOWER_OR_COMM_CALL"] or r["FAC_NAME"], r["SECTORIZATION"], r["REMARK"]))
        if r["FREQ_USE"] == "CTAF" and fac not in ctaf:
            ctaf[fac] = r["FREQ"]

    # --- airports -------------------------------------------------------------
    apt_feats = []
    apt_rows = []
    for r in rows("APT_BASE"):
        lat, lon = num(r["LAT_DECIMAL"]), num(r["LONG_DECIMAL"])
        if lat is None or lon is None or r["ARPT_STATUS"] not in ("O", ""):
            continue
        t, use, twr = r["SITE_TYPE_CODE"], r["FACILITY_USE_CODE"], r["TWR_TYPE_CODE"].startswith("ATCT")
        mil = r["OWNERSHIP_TYPE_CODE"] in ("MA", "MN", "MR", "CG")
        lg = longest.get(r["SITE_NO"], {"len": 0, "hard": False, "align": None, "lit": False})
        # Zoom at which the airport first appears: big towered fields early, private strips late.
        if t == "A" and twr and lg["len"] >= 5000:
            minz, rank = 4, 0
        elif t == "A" and use == "PU" and lg["hard"] and lg["len"] >= 3000:
            minz, rank = 6, 1
        elif t == "A" and use == "PU":
            minz, rank = 7, 2
        elif t == "A" or mil:
            minz, rank = 9, 3
        elif t in ("C", "G", "U", "B"):
            minz, rank = 9, 4
        else:  # heliports
            minz, rank = 10, 5
        props = {
            "id": r["ARPT_ID"], "icao": r["ICAO_ID"], "name": r["ARPT_NAME"].title(), "type": t,
            "pvt": int(use == "PR"), "twr": int(twr), "mil": int(mil), "elev": int(num(r["ELEV"], 0)),
            "rwy": int(lg["len"]), "hard": int(lg["hard"]), "lit": int(lg["lit"]),
            "hdg": int(lg["align"]) if lg["align"] is not None else -1,
            "fuel": int(bool(r["FUEL_TYPES"])), "ctaf": ctaf.get(r["ARPT_ID"], ""), "rank": rank,
        }
        f = point(lon, lat, props)
        f["tippecanoe"] = {"minzoom": minz}
        apt_feats.append(f)
        apt_rows.append(r | {"_lat": lat, "_lon": lon, "_rank": rank, "_twr": int(twr)})
    write_fc("airports", apt_feats)

    # --- navaids --------------------------------------------------------------
    nav_feats = []
    nav_rows = []
    for r in rows("NAV_BASE"):
        lat, lon = num(r["LAT_DECIMAL"]), num(r["LONG_DECIMAL"])
        if lat is None or r["NAV_STATUS"] not in ("OPERATIONAL IFR", "OPERATIONAL RESTRICTED", "OPERATIONAL VFR ONLY", ""):
            continue
        if r["NAV_TYPE"] in ("VOT", "FAN MARKER", "MARINE NDB"):
            continue
        var = num(r["MAG_VARN"], 0) * (-1 if r["MAG_VARN_HEMIS"] == "E" else 1)  # west variation is positive (rose rotates)
        f = point(lon, lat, {"id": r["NAV_ID"], "name": r["NAME"].title(), "type": r["NAV_TYPE"], "freq": r["FREQ"], "chan": r["CHAN"], "var": var})
        f["tippecanoe"] = {"minzoom": 6 if r["NAV_TYPE"] != "NDB" else 8}
        nav_feats.append(f)
        nav_rows.append((r["NAV_ID"], r["NAV_TYPE"], r["NAME"].title(), r["FREQ"], r["CHAN"], lat, lon, var, r["ELEV"]))
    write_fc("navaids", nav_feats)

    # --- fixes ----------------------------------------------------------------
    charts = defaultdict(set)
    for r in rows("FIX_CHRT"):
        charts[r["FIX_ID"]].add(r["CHARTING_TYPE_DESC"])
    fix_feats = []
    fix_rows = []
    for r in rows("FIX_BASE"):
        lat, lon = num(r["LAT_DECIMAL"]), num(r["LONG_DECIMAL"])
        if lat is None:
            continue
        c = charts.get(r["FIX_ID"], set())
        vfr = r["FIX_USE_CODE"] == "VFR" or "VFR TERMINAL AREA" in c or "SECTIONAL" in c
        low = "ENROUTE LOW" in c
        fix_rows.append((r["FIX_ID"], lat, lon, int(vfr), r["STATE_CODE"]))
        if vfr or low:
            f = point(lon, lat, {"id": r["FIX_ID"], "vfr": int(vfr)})
            f["tippecanoe"] = {"minzoom": 9 if vfr else 8}
            fix_feats.append(f)
    write_fc("fixes", fix_feats)

    # --- obstacles ------------------------------------------------------------
    obs_feats = []
    obs_rows = []
    with open(DOF, encoding="latin-1", newline="") as fh:
        for r in csv.DictReader(fh):
            agl = num(r["AGL"], 0)
            if agl < 200 or r["COUNTRY"].strip() != "US":
                continue
            f = point(num(r["LONDEC"]), num(r["LATDEC"]), {"agl": int(agl), "msl": int(num(r["AMSL"], 0)), "lit": int(r["LIGHTING"].strip() not in ("N", "U", "")), "type": r["TYPE"].strip().title()})
            f["tippecanoe"] = {"minzoom": 8 if agl >= 1000 else 9 if agl >= 500 else 10}
            obs_feats.append(f)
            obs_rows.append((num(r["LATDEC"]), num(r["LONDEC"]), int(agl), int(num(r["AMSL"], 0)), f["properties"]["lit"], f["properties"]["type"]))
    write_fc("obstacles", obs_feats)

    # --- database -------------------------------------------------------------
    dbp = OUT / "aviation.sqlite"
    dbp.unlink(missing_ok=True)
    db = sqlite3.connect(dbp)
    db.executescript("""
        CREATE TABLE airport (site_no TEXT, id TEXT, icao TEXT, name TEXT, city TEXT, state TEXT,
            type TEXT, use TEXT, lat REAL, lon REAL, elev REAL, tpa TEXT, mag_var TEXT, sectional TEXT,
            artcc TEXT, fss TEXT, notam_id TEXT, towered INTEGER, rank INTEGER, fuel TEXT, airframe_repair TEXT,
            powerplant_repair TEXT, bottled_oxy TEXT, bulk_oxy TEXT, hangar TEXT, tiedown TEXT, other_services TEXT,
            lighting_schedule TEXT, beacon TEXT, landing_fee TEXT, customs TEXT, segmented_circle TEXT, wind_indicator TEXT,
            ownership TEXT, raw JSON);
        CREATE INDEX airport_site ON airport(site_no); CREATE INDEX airport_id ON airport(id); CREATE INDEX airport_icao ON airport(icao);
        CREATE TABLE runway (site_no TEXT, rwy_id TEXT, len INTEGER, width INTEGER, surface TEXT, cond TEXT, lighting TEXT, raw JSON);
        CREATE INDEX runway_site ON runway(site_no);
        CREATE TABLE runway_end (site_no TEXT, rwy_id TEXT, end_id TEXT, true_alignment REAL, ils TEXT, right_traffic TEXT,
            lat REAL, lon REAL, elev REAL, tdz_elev REAL, displaced_thr_len REAL, vgsi TEXT, approach_lights TEXT, raw JSON);
        CREATE TABLE freq (facility TEXT, use TEXT, freq TEXT, call TEXT, sector TEXT, remark TEXT);
        CREATE INDEX freq_fac ON freq(facility);
        CREATE TABLE contact (site_no TEXT, title TEXT, name TEXT, address TEXT, city TEXT, state TEXT, zip TEXT, phone TEXT);
        CREATE TABLE remark (site_no TEXT, element TEXT, text TEXT);
        CREATE TABLE attendance (site_no TEXT, month TEXT, day TEXT, hour TEXT);
        CREATE TABLE awos (id TEXT, type TEXT, lat REAL, lon REAL, elev REAL, phone TEXT, freq TEXT, site_no TEXT);
        CREATE TABLE navaid (id TEXT, type TEXT, name TEXT, freq TEXT, chan TEXT, lat REAL, lon REAL, var REAL, elev TEXT);
        CREATE INDEX navaid_id ON navaid(id);
        CREATE TABLE fix (id TEXT, lat REAL, lon REAL, vfr INTEGER, state TEXT);
        CREATE INDEX fix_id ON fix(id);
        CREATE TABLE obstacle (lat REAL, lon REAL, agl INTEGER, msl INTEGER, lit INTEGER, type TEXT);
        CREATE INDEX obstacle_ll ON obstacle(lat, lon);
    """)
    for r in apt_rows:
        db.execute("INSERT INTO airport VALUES (" + ",".join("?" * 35) + ")", (
            r["SITE_NO"], r["ARPT_ID"], r["ICAO_ID"], r["ARPT_NAME"].title(), r["CITY"].title(), r["STATE_CODE"],
            r["SITE_TYPE_CODE"], r["FACILITY_USE_CODE"], r["_lat"], r["_lon"], num(r["ELEV"]), r["TPA"],
            f'{r["MAG_VARN"]}{r["MAG_HEMIS"]}', r["CHART_NAME"].title(), r["RESP_ARTCC_ID"], r["FSS_NAME"].title(),
            r["NOTAM_ID"], r["_twr"], r["_rank"], r["FUEL_TYPES"], r["AIRFRAME_REPAIR_SER_CODE"],
            r["PWR_PLANT_REPAIR_SER"], r["BOTTLED_OXY_TYPE"], r["BULK_OXY_TYPE"], r["TRNS_STRG_HGR_FLAG"],
            r["TRNS_STRG_TIE_FLAG"], r["OTHER_SERVICES"], r["LGT_SKED"], r["BCN_LENS_COLOR"], r["LNDG_FEE_FLAG"],
            r["CUST_FLAG"], r["SEG_CIRCLE_MKR_FLAG"], r["WIND_INDCR_FLAG"], r["OWNERSHIP_TYPE_CODE"],
            json.dumps({k: v for k, v in r.items() if not k.startswith("_") and v}),
        ))
    for site, lst in rwys.items():
        for r in lst:
            db.execute("INSERT INTO runway VALUES (?,?,?,?,?,?,?,?)", (site, r["RWY_ID"], int(num(r["RWY_LEN"], 0)), int(num(r["RWY_WIDTH"], 0)),
                       r["SURFACE_TYPE_CODE"], r["COND"], r["RWY_LGT_CODE"], json.dumps({k: v for k, v in r.items() if v})))
    for (site, rid), lst in ends.items():
        for r in lst:
            db.execute("INSERT INTO runway_end VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?)", (
                site, rid, r["RWY_END_ID"], num(r["TRUE_ALIGNMENT"]), r["ILS_TYPE"], r["RIGHT_HAND_TRAFFIC_PAT_FLAG"],
                num(r["LAT_DECIMAL"]), num(r["LONG_DECIMAL"]), num(r["RWY_END_ELEV"]), num(r["TDZ_ELEV"]),
                num(r["DISPLACED_THR_LEN"]), r["VGSI_CODE"], r["APCH_LGT_SYSTEM_CODE"], json.dumps({k: v for k, v in r.items() if v})))
    for fac, lst in freqs.items():
        db.executemany("INSERT INTO freq VALUES (?,?,?,?,?,?)", [(fac, *x) for x in lst])
    for r in rows("APT_CON"):
        db.execute("INSERT INTO contact VALUES (?,?,?,?,?,?,?,?)", (r["SITE_NO"], r["TITLE"], r["NAME"], " ".join(filter(None, [r["ADDRESS1"], r["ADDRESS2"]])),
                   r["TITLE_CITY"], r["STATE"], r["ZIP_CODE"], r["PHONE_NO"]))
    for r in rows("APT_RMK"):
        db.execute("INSERT INTO remark VALUES (?,?,?)", (r["SITE_NO"], r.get("ELEMENT", "") or r.get("REF_COL_NAME", ""), r.get("REMARK", "")))
    for r in rows("APT_ATT"):
        db.execute("INSERT INTO attendance VALUES (?,?,?,?)", (r["SITE_NO"], r["MONTH"], r["DAY"], r["HOUR"]))
    for r in rows("AWOS"):
        db.execute("INSERT INTO awos VALUES (?,?,?,?,?,?,?,?)", (r["ASOS_AWOS_ID"], r["ASOS_AWOS_TYPE"], num(r["LAT_DECIMAL"]), num(r["LONG_DECIMAL"]),
                   num(r["ELEV"]), r["PHONE_NO"], r.get("FREQ", ""), r["SITE_NO"]))
    db.executemany("INSERT INTO navaid VALUES (?,?,?,?,?,?,?,?,?)", nav_rows)
    db.executemany("INSERT INTO fix VALUES (?,?,?,?,?)", fix_rows)
    db.executemany("INSERT INTO obstacle VALUES (?,?,?,?,?,?)", obs_rows)
    db.commit()
    db.close()
    print("aviation.sqlite written")


if __name__ == "__main__":
    build()
