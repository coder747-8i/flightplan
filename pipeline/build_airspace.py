"""Normalise FAA class + special-use airspace into one layer with numeric floors/ceilings.

Every feature gets:
  kind   B | C | D | E2 | E3 | E4 | E5 | E6 | MODEC | R | P | W | A | MOA
  lo/hi  floor/ceiling in feet (hi 99999 = unlimited)
  lo_agl 1 when the floor is above ground level instead of MSL
  label  short sectional-style altitude label ("100/SFC", "[32]", "8500")
Label points (pole of inaccessibility) go to a second file so each shelf is labelled once.

Usage: .venv/bin/python -I pipeline/build_airspace.py
"""
import json
from pathlib import Path

from shapely.geometry import mapping, shape
from shapely.ops import polylabel

ROOT = Path(__file__).resolve().parent.parent
RAW = ROOT / "data" / "raw" / "arcgis"
OUT = ROOT / "data" / "build"

CLASS_KIND = {
    "CLASS_B": "B", "CLASS_C": "C", "CLASS_D": "D", "CLASS_E2": "E2", "CLASS_E3": "E3",
    "CLASS_E4": "E4", "CLASS_E5": "E5", "CLASS_E6": "E6", "MODE C": "MODEC",
}
# zoom where each kind starts to draw (keeps the national view readable)
MINZOOM = {"B": 4, "C": 5, "MODEC": 6, "D": 6, "E2": 7, "E3": 7, "E4": 7, "E5": 7, "E6": 8,
           "R": 4, "P": 4, "W": 4, "MOA": 5, "A": 6}
DRAW_ORDER = {"E6": 0, "E5": 1, "E4": 2, "E3": 2, "E2": 3, "MODEC": 4, "MOA": 5, "A": 5, "W": 5, "R": 6, "P": 6, "D": 7, "C": 8, "B": 9}


def feet(val, uom, code):
    if code == "UNLTD":
        return 99999
    if val is None:
        return None
    v = float(val)
    return int(v * 100) if uom == "FL" else int(v)


def alt_text(ft, agl=False):
    if ft is None:
        return "?"
    if ft >= 99999:
        return "UNL"
    if ft >= 18000:
        return f"FL{ft // 100}"
    return f"{ft:,}{' AGL' if agl else ''}"


def norm(p, sua: bool):
    if sua:
        kind = p["TYPE_CODE"]
        if kind not in ("R", "P", "W", "A", "MOA"):
            return None
    else:
        kind = CLASS_KIND.get(p["LOCAL_TYPE"] or "")
        if not kind:
            return None
    lo_code, hi_code = p.get("LOWER_CODE"), p.get("UPPER_CODE")
    lo = feet(p.get("LOWER_VAL"), p.get("LOWER_UOM"), lo_code) or 0
    lo_agl = int(lo_code == "SFC" and lo > 0)
    if lo_code == "SFC" and not lo_agl:
        lo = 0
    if p.get("UPPER_DESC") == "AA" and kind.startswith("E"):
        hi = 17999  # Class E extends up to, not including, 18,000 MSL
    else:
        hi = feet(p.get("UPPER_VAL"), p.get("UPPER_UOM"), hi_code)
        if hi is None or hi_code in ("BYNOTAM",):
            hi = 99999 if hi_code in ("UNLTD", None) else (hi or 99999)
        if p.get("UPPER_DESC") == "TNI":
            hi -= 1
    # sectional-style label
    if kind in ("B", "C"):
        top = "T" if hi >= 99999 else str((hi + 1) // 100 if hi % 100 == 99 else hi // 100)
        label = f"{top}/{'SFC' if lo == 0 else lo // 100}"
    elif kind == "D":
        label = f"[{(hi + 1) // 100 if hi % 100 == 99 else hi // 100}]"
    elif kind == "E5" and not lo_agl:
        label = f"{lo:,}"
    elif sua:
        label = p["NAME"].title().replace("Moa", "MOA")
    else:
        label = ""
    name = (p.get("NAME") or "").title().replace("Class ", "Class ").replace("Moa", "MOA")
    props = {
        "kind": kind, "lo": lo, "hi": hi, "lo_agl": lo_agl, "label": label, "name": name,
        "range": f"{'SFC' if lo == 0 else alt_text(lo, bool(lo_agl))} – {alt_text(hi + (1 if hi % 100 == 99 else 0))}",
        "z": DRAW_ORDER[kind],
    }
    if sua:
        props["times"] = (p.get("TIMESOFUSE") or "").strip()
        props["agency"] = (p.get("CONT_AGENT") or "").strip().title()
        props["remarks"] = (p.get("REMARKS") or "").strip()
    if not sua:
        props["hours"] = (p.get("WKHR_CODE") or "").strip()
        props["ident"] = (p.get("ICAO_ID") or p.get("IDENT") or "").strip()
    return props


def build():
    shapes, labels = [], []
    for fname, sua in (("Class_Airspace.geojson", False), ("Special_Use_Airspace.geojson", True)):
        path = RAW / fname
        if not path.exists() and (RAW / fname.replace(".geojson", ".partial")).exists():
            path = RAW / fname.replace(".geojson", ".partial")  # mid-download dev builds
        fc = json.load(open(path))
        for f in fc["features"]:
            if not f.get("geometry"):
                continue
            props = norm(f["properties"], sua)
            if not props:
                continue
            geom = shape(f["geometry"])
            if not geom.is_valid:
                geom = geom.buffer(0)
            if geom.is_empty:
                continue
            shapes.append({"type": "Feature", "geometry": mapping(geom), "properties": props,
                           "tippecanoe": {"minzoom": MINZOOM[props["kind"]]}})
            if props["label"] and props["kind"] not in ("E2", "E3", "E4", "E6", "MODEC"):
                polys = list(geom.geoms) if geom.geom_type == "MultiPolygon" else [geom]
                biggest = max(polys, key=lambda g: g.area)
                try:
                    pt = polylabel(biggest, tolerance=0.002)
                except Exception:
                    pt = biggest.representative_point()
                lp = {k: props[k] for k in ("kind", "lo", "hi", "lo_agl", "label", "name", "range")}
                labels.append({"type": "Feature", "geometry": mapping(pt), "properties": lp,
                               "tippecanoe": {"minzoom": max(MINZOOM[props["kind"]] + 2, 7)}})
    OUT.mkdir(parents=True, exist_ok=True)
    for name, feats in (("airspace", shapes), ("airspace_labels", labels)):
        with open(OUT / f"{name}.geojson", "w") as fh:
            json.dump({"type": "FeatureCollection", "features": feats}, fh, separators=(",", ":"))
        print(name, len(feats))


if __name__ == "__main__":
    build()
