"""Airway segments (Victor/T low, Jet/Q high) with MEA/MOCA for chart + route planning.

Usage: python3 -I pipeline/build_airways.py
"""
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
NONE = 999999


def val(v):
    return None if v in (None, NONE) else int(v)


def build():
    fc = json.load(open(ROOT / "data" / "raw" / "arcgis" / "ATS_Route.geojson"))
    out = []
    for f in fc["features"]:
        p = f["properties"]
        if not f.get("geometry") or p["TYPE_CODE"] not in ("CONV", "RNAV", "GRNAV", "AKCAP"):
            continue
        meas = [m for m in (val(p["MEA_E_VAL"]), val(p["MEA_W_VAL"])) if m]
        level = "high" if p["LEVEL_"] == "U" else "low"
        out.append({"type": "Feature", "geometry": f["geometry"], "properties": {
            "id": p["IDENT"], "level": level, "rnav": int(p["TYPE_CODE"] != "CONV"),
            "mea": max(meas) if meas else 0, "moca": val(p["MOCA_VAL"]) or 0,
            "trk": round(p["MAGTRK"] or 0), "len": round(p["LENGTH_VAL"] or 0, 1)},
            "tippecanoe": {"minzoom": 7 if level == "low" else 6}})
    with open(ROOT / "data" / "build" / "airways.geojson", "w") as fh:
        json.dump({"type": "FeatureCollection", "features": out}, fh, separators=(",", ":"))
    print("airways", len(out))


if __name__ == "__main__":
    build()
