"""Bundle the built data into the data pack the desktop app and setup screen install.

Output: dist/flightplan-data.zip  (aviation.sqlite, airspace_index.json, *.pmtiles, manifest.json)
Usage: python3 -I pipeline/make_datapack.py [out.zip]
"""
import datetime as dt
import json
import sys
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BUILD = ROOT / "data" / "build"
TILES = ROOT / "tiles"
FILES = [BUILD / "aviation.sqlite", BUILD / "airspace_index.json", TILES / "aero.pmtiles", TILES / "airspace.pmtiles", TILES / "obstacles.pmtiles"]


def main():
    out = Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / "dist" / "flightplan-data.zip"
    missing = [str(f.relative_to(ROOT)) for f in FILES if not f.is_file()]
    if missing:
        raise SystemExit(f"build the data first; missing {', '.join(missing)}")
    cycle_file = ROOT / "data" / "raw" / "nasr" / "CYCLE"
    cycle = dt.date.fromisoformat(cycle_file.read_text().strip()) if cycle_file.exists() else None
    manifest = {
        "format": 1,
        "cycle": cycle.isoformat() if cycle else None,
        "expires": (cycle + dt.timedelta(days=28)).isoformat() if cycle else None,
        "built": dt.datetime.now(dt.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "sources": "FAA NASR, FAA AIS open data, FAA Digital Obstacle File, Overture Maps",
    }
    out.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(out, "w", allowZip64=True) as z:
        z.writestr("manifest.json", json.dumps(manifest, indent=2))
        for f in FILES:
            # PMTiles are already compressed internally; deflate the rest
            z.write(f, f.name, compress_type=zipfile.ZIP_STORED if f.suffix == ".pmtiles" else zipfile.ZIP_DEFLATED)
    print(out, out.stat().st_size // 1_000_000, "MB", manifest)


if __name__ == "__main__":
    main()
