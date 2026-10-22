# Flightplan

A web flight planner for the US: a vector sectional with layers you can switch on and off
and a Garmin-style altitude filter. It also has a vertical profile (terrain, obstacles,
airspace), a navlog with winds aloft, weather (radar, METAR, TAF, PIREP, AIRMET/SIGMET,
G-AIRMET, TFRs, Windy), a hazard advisor, weight & balance, FBO and airport info, and a
printable briefing.

**For planning only.** It is not a substitute for an official briefing, current charts,
NOTAMs or the aircraft's POH.

## Stack

- `web/`: Svelte 5 + MapLibre GL, served as static files
- `server/`: Rust (axum). Airport data, search, terrain profiles, the weather/NOTAM proxy,
  and static hosting
- `pipeline/`: Python scripts that build the chart tiles (PMTiles) and `aviation.sqlite`
  from FAA data

## Data sources

- FAA NASR 28-day subscription, FAA AIS open data (airspace), FAA Digital Obstacle File
- aviationweather.gov, FAA TFR feed, Iowa Environmental Mesonet radar
- Terrain from Mapzen/AWS Terrarium; basemap from OpenFreeMap (OpenStreetMap)
- FBOs from Overture Maps places plus FAA airport remarks
- Optional: Windy (map and point forecast) and the FAA NOTAM API, configured in the app's
  Settings panel

## Build the data

Needs Python 3, Rust and Node. [tippecanoe](https://github.com/felt/tippecanoe) must be on
your PATH or in `~/.local/bin`.

```bash
python3 -m venv .venv && .venv/bin/pip install duckdb shapely
# download the NASR CSV zip into data/raw/nasr and the DOF zip into data/raw/dof (see pipeline/)
python3 -I pipeline/fetch_arcgis.py
python3 -I pipeline/build_nasr.py
.venv/bin/python -I pipeline/build_airspace.py
.venv/bin/python -I pipeline/build_airspace_index.py
python3 -I pipeline/build_airways.py
.venv/bin/python -I pipeline/fetch_overture_fbo.py
.venv/bin/python -I pipeline/build_fbo.py
./pipeline/build_tiles.sh
```

## Run

```bash
./run.sh          # builds web + server, serves http://0.0.0.0:8787
```

For development, run `npm --prefix web run dev` (port 5173, proxies `/api` to 8787).

## Keys

Add keys in the app under **Settings**. They're stored in `data/settings.json`, which is
git-ignored. Or copy `.env.example` to `.env`. Never commit either file.
