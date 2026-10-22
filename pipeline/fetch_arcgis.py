"""Download FAA AIS open-data layers (ArcGIS feature services) as GeoJSON.

Usage: python3 -I pipeline/fetch_arcgis.py [layer ...]
"""
import json
import sys
import time
import urllib.parse
import urllib.request
from pathlib import Path

BASE = "https://services6.arcgis.com/ssFJjBXIUyZDrSYZ/arcgis/rest/services"
OUT = Path(__file__).resolve().parent.parent / "data" / "raw" / "arcgis"

LAYERS = {
    "Class_Airspace": 150,
    "Special_Use_Airspace": 150,
    "US_Airport": 1000,
    "NAVAIDSystem": 1000,
    "DesignatedPoint": 2000,
    "ATS_Route": 500,
    "Runways": 1000,
}


def get(url: str) -> dict:
    for attempt in range(5):
        try:
            req = urllib.request.Request(url, headers={"User-Agent": "flightplan-pipeline/0.1"})
            with urllib.request.urlopen(req, timeout=120) as r:
                return json.load(r)
        except Exception as e:  # network hiccups: back off and retry
            print(f"  retry {attempt + 1}: {e}", file=sys.stderr)
            time.sleep(2 + attempt * 3)
    raise RuntimeError(f"failed: {url}")


def fetch(layer: str, page: int) -> None:
    url = f"{BASE}/{layer}/FeatureServer/0/query"
    total = get(f"{url}?where=1%3D1&returnCountOnly=true&f=json")["count"]
    feats = []
    empty = 0
    while len(feats) < total:
        params = {
            "where": "1=1",
            "outFields": "*",
            "outSR": "4326",
            "f": "geojson",
            "resultOffset": len(feats),
            "resultRecordCount": page,
            "orderByFields": "OBJECTID",
            "geometryPrecision": 6,
        }
        batch = get(f"{url}?{urllib.parse.urlencode(params)}").get("features", [])
        if not batch:  # the service sometimes returns an empty page under load; retry it
            empty += 1
            if empty > 8:
                raise RuntimeError(f"{layer}: stuck at {len(feats)}/{total}")
            time.sleep(5 * empty)
            continue
        empty = 0
        feats.extend(batch)
        print(f"{layer}: {len(feats)}/{total}", file=sys.stderr)
    OUT.mkdir(parents=True, exist_ok=True)
    (OUT / f"{layer}.geojson").write_text(json.dumps({"type": "FeatureCollection", "features": feats}))


if __name__ == "__main__":
    for name in sys.argv[1:] or LAYERS:
        fetch(name, LAYERS.get(name, 1000))
