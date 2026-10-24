"""Download the current FAA NASR 28-day CSV subscription and the Digital Obstacle File.

Output: data/raw/nasr/*.csv (+ CYCLE, the effective date), data/raw/dof/DOF.CSV
Usage: python3 -I pipeline/fetch_faa.py [YYYY-MM-DD]   (default: the current cycle)
"""
import datetime as dt
import io
import json
import sys
import time
import urllib.request
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
RAW = ROOT / "data" / "raw"
EDITION_API = "https://external-api.faa.gov/apra/nfdc/nasr/chart?edition=current"
CSV_URL = "https://nfdc.faa.gov/webContent/28DaySub/extra/{d:%d}_{d:%b}_{d:%Y}_CSV.zip"
DOF_URL = "https://aeronav.faa.gov/Obst_Data/DAILY_DOF_CSV.ZIP"


def get(url: str, accept: str = "*/*") -> bytes:
    for attempt in range(5):
        try:
            req = urllib.request.Request(url, headers={"User-Agent": "flightplan-pipeline/0.1", "Accept": accept})
            with urllib.request.urlopen(req, timeout=300) as r:
                return r.read()
        except Exception as e:  # network hiccups: back off and retry
            print(f"  retry {attempt + 1}: {e}", file=sys.stderr)
            time.sleep(3 + attempt * 5)
    raise RuntimeError(f"failed: {url}")


def current_cycle() -> dt.date:
    ed = json.loads(get(EDITION_API, "application/json"))["edition"][0]
    return dt.datetime.strptime(ed["editionDate"], "%m/%d/%Y").date()


def unzip(blob: bytes, out: Path, keep) -> int:
    out.mkdir(parents=True, exist_ok=True)
    n = 0
    with zipfile.ZipFile(io.BytesIO(blob)) as z:
        for info in z.infolist():
            name = Path(info.filename).name
            if not info.is_dir() and keep(name):
                (out / name).write_bytes(z.read(info))
                n += 1
    return n


def main():
    cycle = dt.date.fromisoformat(sys.argv[1]) if len(sys.argv) > 1 else current_cycle()
    print("NASR cycle", cycle)
    n = unzip(get(CSV_URL.format(d=cycle)), RAW / "nasr", lambda name: name.lower().endswith(".csv"))
    (RAW / "nasr" / "CYCLE").write_text(cycle.isoformat())
    print(f"nasr: {n} csv files")
    n = unzip(get(DOF_URL), RAW / "dof", lambda name: name.upper() == "DOF.CSV")
    if not n:
        raise SystemExit("DOF.CSV not found in the obstacle file zip")
    print("dof: DOF.CSV")


if __name__ == "__main__":
    main()
