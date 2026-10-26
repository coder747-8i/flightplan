#!/usr/bin/env bash
# Build the chart data pack (dist/flightplan-data.zip) from the FAA's current 28-day cycle on
# this computer. The same steps .github/workflows/data.yml runs.
#
# Usage: pipeline/build_datapack.sh [--cycle YYYY-MM-DD] [--out FILE] [--install] [--skip-fetch]
#   --cycle       build a specific NASR cycle (default: the current one)
#   --out         where to write the pack (default: dist/flightplan-data.zip)
#   --install     also install it into the desktop app's data folder (close the app first;
#                 FP_DATA_DIR overrides the folder)
#   --skip-fetch  reuse the downloads already in data/raw (rebuild only)
#
# Needs python3 (3.10+), git, make and a C++ compiler, plus the SQLite and zlib headers to
# build tippecanoe the first time:
#   Debian/Ubuntu:  sudo apt install build-essential git python3-venv libsqlite3-dev zlib1g-dev
#   Fedora:         sudo dnf install gcc-c++ make git python3 sqlite-devel zlib-devel
#   macOS:          xcode-select --install && brew install sqlite
#   Windows:        run it in WSL (Ubuntu)
# Set TIPPECANOE=/path/to/tippecanoe to use one you already have.
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT="$PWD"

cycle=""
out="$ROOT/dist/flightplan-data.zip"
install=0
fetch=1
while [ $# -gt 0 ]; do
  case "$1" in
    --cycle) cycle="$2"; shift 2 ;;
    --out) out="$2"; shift 2 ;;
    --install) install=1; shift ;;
    --skip-fetch) fetch=0; shift ;;
    -h|--help) sed -n '2,21p' "$0"; exit 0 ;;
    *) echo "unknown option $1 (see --help)" >&2; exit 2 ;;
  esac
done

step() { printf '\n\033[1m== %s\033[0m\n' "$*"; }
need() { command -v "$1" >/dev/null || { echo "missing $1: $2" >&2; exit 1; }; }
need python3 "install Python 3.10 or newer"

step "Python environment"
if [ ! -x .venv/bin/python ]; then
  python3 -m venv .venv || { echo "couldn't create .venv (Debian/Ubuntu: sudo apt install python3-venv)" >&2; exit 1; }
fi
# duckdb 2.x has no httpfs extension published yet; the Overture step needs it
deps=("duckdb<2" shapely)
if .venv/bin/python -m pip --version >/dev/null 2>&1 || .venv/bin/python -m ensurepip -q >/dev/null 2>&1; then
  .venv/bin/python -m pip install -q --upgrade "${deps[@]}"
elif command -v uv >/dev/null; then
  # a venv made by uv has no pip
  uv pip install -q -p .venv/bin/python "${deps[@]}"
else
  echo ".venv has no pip; delete .venv and run again" >&2
  exit 1
fi

step "tippecanoe"
TC="${TIPPECANOE:-}"
if [ -z "$TC" ] && command -v tippecanoe >/dev/null; then TC="$(command -v tippecanoe)"; fi
if [ -z "$TC" ]; then
  TC="$ROOT/pipeline/.tools/tippecanoe/tippecanoe"
  if [ ! -x "$TC" ]; then
    # only building tippecanoe needs these
    need git "install git"
    need make "install make and a C++ compiler (build-essential / Xcode command line tools)"
    rm -rf pipeline/.tools/tippecanoe
    mkdir -p pipeline/.tools
    git clone -q --depth 1 https://github.com/felt/tippecanoe.git pipeline/.tools/tippecanoe
    jobs="$( (nproc || sysctl -n hw.ncpu) 2>/dev/null || echo 4)"
    if ! make -C pipeline/.tools/tippecanoe -j"$jobs" >pipeline/.tools/tippecanoe-build.log 2>&1; then
      tail -n 5 pipeline/.tools/tippecanoe-build.log >&2
      echo "building tippecanoe failed; install the SQLite and zlib headers (see the top of this script)" >&2
      exit 1
    fi
  fi
fi
echo "using $TC"

start=$(date +%s)
if [ "$fetch" = 1 ]; then
  step "FAA NASR + obstacle file"
  python3 -I pipeline/fetch_faa.py $cycle
  step "FAA airspace, airports, navaids, airways (ArcGIS)"
  python3 -I pipeline/fetch_arcgis.py 2>&1 | tail -n 7
  step "FBO candidates (Overture Maps)"
  # the FBO directory hardly changes: reuse a download younger than 90 days (FP_REFRESH_FBO=1 forces)
  places=data/raw/overture/aviation_places.parquet
  if [ -f "$places" ] && [ -z "${FP_REFRESH_FBO:-}" ] && [ -n "$(find "$places" -mtime -90 2>/dev/null)" ]; then
    echo "reusing $places"
  else
    .venv/bin/python -I pipeline/fetch_overture_fbo.py
  fi
fi
step "Building"
python3 -I pipeline/build_nasr.py
.venv/bin/python -I pipeline/build_airspace.py
.venv/bin/python -I pipeline/build_airspace_index.py
python3 -I pipeline/build_airways.py
.venv/bin/python -I pipeline/build_fbo.py
step "Tiles"
TIPPECANOE="$TC" ./pipeline/build_tiles.sh
step "Data pack"
python3 -I pipeline/make_datapack.py "$out"

if [ "$install" = 1 ]; then
  step "Installing into the desktop app"
  if [ -n "${FP_DATA_DIR:-}" ]; then
    dest="$FP_DATA_DIR"
  else
    case "$(uname -s)" in
      Darwin) dest="$HOME/Library/Application Support/io.github.coder747-8i.Flightplan" ;;
      MINGW*|MSYS*|CYGWIN*) dest="$APPDATA/coder747-8i/Flightplan/data" ;;
      *) dest="${XDG_DATA_HOME:-$HOME/.local/share}/flightplan" ;;
    esac
  fi
  python3 -I - "$out" "$dest" <<'EOF'
import json, os, shutil, sys, zipfile, datetime as dt
pack, dest = sys.argv[1], sys.argv[2]
where = {"aviation.sqlite": "build", "airspace_index.json": "build", "manifest.json": "build",
         "aero.pmtiles": "tiles", "airspace.pmtiles": "tiles", "obstacles.pmtiles": "tiles"}
with zipfile.ZipFile(pack) as z:
    for name, sub in where.items():
        os.makedirs(os.path.join(dest, sub), exist_ok=True)
        tmp = os.path.join(dest, sub, name + ".tmp")
        with z.open(name) as src, open(tmp, "wb") as dst:
            shutil.copyfileobj(src, dst)
        os.replace(tmp, os.path.join(dest, sub, name))
m = os.path.join(dest, "build", "manifest.json")
info = json.load(open(m))
info["installedFrom"] = pack
info["installedAt"] = dt.datetime.now(dt.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
json.dump(info, open(m, "w"), indent=2)
print("installed into", dest)
EOF
fi

printf '\nDone in %d min: %s\n' "$(( ($(date +%s) - start) / 60 ))" "$out"
[ "$install" = 1 ] || echo "Install it from the app: Settings → Chart data → Import a file (or run again with --install)."
