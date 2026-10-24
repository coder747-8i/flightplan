# Flightplan

A web flight planner for the US: a vector sectional with layers you can switch on and off
and a Garmin-style altitude filter. It also has a vertical profile (terrain, obstacles,
airspace), a navlog with winds aloft, weather (radar, METAR, TAF, PIREP, AIRMET/SIGMET,
G-AIRMET, TFRs, Windy), a hazard advisor, weight & balance, FBO and airport info, and a
printable briefing.

**For planning only.** It is not a substitute for an official briefing, current charts,
NOTAMs or the aircraft's POH.

## Desktop app (Windows, macOS, Linux)

`desktop/` wraps the app in a native window with [Tauri](https://tauri.app). The server runs
inside the app on `127.0.0.1`; nothing is exposed to the network.

On first launch a setup screen walks through the app, installs the chart data, offers the
optional Windy / FAA NOTAM keys, and sets up your aircraft. Help (bottom left) brings it and a
tour of the screen back later. Data lives in the per-user app-data folder (for example
`~/.local/share/io.github.coder747-8i.flightplan`, `%APPDATA%\io.github.coder747-8i.flightplan`,
`~/Library/Application Support/io.github.coder747-8i.flightplan`).

**Installers** are built by GitHub Actions (`.github/workflows/desktop.yml`): run the
*Desktop app* workflow by hand to get them as workflow artifacts, or push a tag such as `v0.1.0`
to attach them to a draft release. Output: `.msi` and `-setup.exe` (Windows), a universal `.dmg`
(macOS), and `.deb`, `.rpm`, `.AppImage` (Linux). The builds are not code-signed, so Windows
SmartScreen and macOS Gatekeeper warn on first open (macOS: right-click the app → Open).

To build locally you need Rust, Node 22 and the
[Tauri prerequisites](https://tauri.app/start/prerequisites/) for your OS (on Linux:
`libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev`):

```bash
cd desktop && npm ci
npx tauri dev      # run
npx tauri build    # installers in desktop/src-tauri/target/release/bundle
```

## Chart data pack

The setup screen and **Settings → Chart data** install a data pack (`flightplan-data.zip`,
about 180 MB) in one of three ways: download it, import the zip, or copy it from a folder (an
unzipped pack, or a checkout with `data/build` and `tiles`). The new data is checked before it
replaces the old, and takes effect without a restart.

There are two ways to make a pack:

- **On your computer** (works with a private repository): `pipeline/build_datapack.sh` downloads
  the current FAA cycle, runs the whole pipeline and writes `dist/flightplan-data.zip` (about
  10–20 minutes). Add `--install` to put it straight into the desktop app (close the app first),
  or import the zip from the app's setup screen / Settings. Run it again each cycle (every 28
  days; the app says when the installed data has expired). It needs Python 3, git, make, a C++
  compiler and the SQLite/zlib headers to build tippecanoe the first time; the top of the script
  lists the packages per OS (Windows: use WSL). `--help` shows the options.
- **On GitHub**: `.github/workflows/data.yml` runs the same script every week, does nothing unless
  the cycle has changed, and publishes the pack on the `data` release (the app's default download
  address). Release downloads need the repository to be public; otherwise set the `FP_DATA_URL`
  repository variable to a public address before building the installers.

## Routes and aircraft

- **Export** (Route panel, share icon): FltPlan.com (every flight plan field ready to copy, plus
  the route string), Garmin `.fpl` (Garmin avionics, Garmin Pilot, ForeFlight) and GPX. The icon
  next to it opens `.fpl` and `.gpx` files.
- **Aircraft**: create airframes step by step (template or blank, with checks), edit them the
  same way, duplicate, and export/import `.fpaircraft.json` files. The format is in
  [docs/aircraft-format.md](docs/aircraft-format.md). Aircraft are also saved on the server, so
  they survive a cleared browser and are shared by every device that opens the app.

## Stack

- `web/`: Svelte 5 + MapLibre GL, served as static files
- `server/`: Rust (axum), a library plus the `flightplan-server` binary. Airport data, search,
  terrain profiles, the weather/NOTAM proxy, data-pack setup and static hosting
- `desktop/`: Tauri shell that embeds the server
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
python3 -m venv .venv && .venv/bin/pip install "duckdb<2" shapely
python3 -I pipeline/fetch_faa.py          # current NASR cycle + Digital Obstacle File
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

For development, run `npm --prefix web run dev` (port 5173, proxies `/api` and `/tiles` to 8787).
Without built data the server starts in setup mode and the app opens its setup screen.

## Keys

Add keys in the app under **Settings** (or during setup). They're stored in
`data/settings.json` (desktop app: `settings.json` in its data folder), which is git-ignored. Or copy `.env.example` to `.env`. Never commit either file.

## License

Flightplan is free software under the [GNU Affero General Public License v3.0 only](LICENSE).
If you run a modified version as a network service, you must offer its users the source of
that version. Bundled third-party assets keep their own licenses (the Inter font is under the
SIL Open Font License, see `native/assets/fonts/Inter-LICENSE.txt`), and the chart data comes
from the FAA and Overture Maps under their terms.
