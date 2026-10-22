"""Compact, simplified airspace polygons for server-side route intersection.

Output: data/build/airspace_index.json  [{k, n, lo, hi, agl, b:[minx,miny,maxx,maxy], r:[[ring...]...]}...]
Usage: .venv/bin/python -I pipeline/build_airspace_index.py
"""
import json
from pathlib import Path

from shapely.geometry import shape

ROOT = Path(__file__).resolve().parent.parent
B = ROOT / "data" / "build"


def rings(geom):
    polys = list(geom.geoms) if geom.geom_type == "MultiPolygon" else [geom]
    for p in polys:
        # exterior first, holes after; a None separator starts a new polygon
        yield [[[round(x, 5), round(y, 5)] for x, y in p.exterior.coords]] + [
            [[round(x, 5), round(y, 5)] for x, y in h.coords] for h in p.interiors
        ]


def build():
    fc = json.load(open(B / "airspace.geojson"))
    out = []
    for f in fc["features"]:
        p = f["properties"]
        g = shape(f["geometry"]).simplify(0.0003, preserve_topology=True)
        if g.is_empty:
            continue
        out.append({
            "k": p["kind"], "n": p["name"], "lo": p["lo"], "hi": p["hi"], "agl": p["lo_agl"],
            "t": p.get("times", ""), "b": [round(v, 4) for v in g.bounds], "p": list(rings(g)),
        })
    with open(B / "airspace_index.json", "w") as fh:
        json.dump(out, fh, separators=(",", ":"))
    print("airspace_index", len(out), (B / "airspace_index.json").stat().st_size // 1_000_000, "MB")


if __name__ == "__main__":
    build()
