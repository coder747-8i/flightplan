# Flightplan

A flight planner for the US, as a native desktop app and a web app: a vector sectional with
layers you can switch on and off and a Garmin-style altitude filter. It also has a vertical profile (terrain, obstacles,
airspace), a navlog with winds aloft and fuel stops, weather (radar and forecast radar on a
five-day timeline, METAR, TAF, PIREP, AIRMET/SIGMET, G-AIRMET, TFRs, Windy in the web app), a
go / no-go call against your personal minimums, NOTAMs, weight & balance, FBO and airport info,
FltPlan Go export and a printable briefing. Both apps have the same features.

**For planning only.** It is not a substitute for an official briefing, current charts,
NOTAMs or the aircraft's POH.

## Desktop app (Windows, macOS, Linux)

`native/` is a native desktop app written in Rust with [egui](https://github.com/emilk/egui):
no browser, no webview. It links the server crate as a library, so the airport database,
terrain and weather code is shared with the web app.

```bash
cd native
cargo run --release              # run
cargo build --release            # binary at native/target/release/flightplan
```

On first launch a setup screen walks through the app, installs the chart data, offers the
optional FAA NOTAM key, and sets up your aircraft. Help (bottom left) brings it and a
tour of the screen back later. Data lives in the per-user data folder
(`~/.local/share/flightplan`, `~/Library/Application Support/io.github.coder747-8i.Flightplan`,
`%APPDATA%\coder747-8i\Flightplan\data`); `FP_DATA_DIR` overrides it.

What it does beyond the chart and route:

- **Plan up to five days ahead.** The timeline under the chart sets the departure time. The
  weather picture follows it, always an official product shown as published: live NEXRAD now,
  then HRRR forecast radar (about 36 to 48 hours out), then the WPC National Forecast Chart
  (days 1 to 3) and the WPC precipitation forecast (days 4 and 5). An SPC convective outlook
  layer covers days 1 to 8. Winds aloft, the navlog and the airport forecasts follow it too.
  Press play to animate.
- **Go / no-go.** With a route loaded, the Route panel gives a green, yellow or red call for
  the departure time. It checks departure and arrival weather against your personal minimums
  (Settings): ceiling, visibility, wind, gusts, crosswind on the best runway, thunder and
  density altitude. The weather comes from the METAR, then the TAF, then NWS NBM guidance.
  Along the route it reads the WPC forecast chart and SPC convective outlook for the day you
  fly, and it checks G-AIRMETs, SIGMETs, strong winds aloft over mountains, terrain clearance, TFRs, prohibited and restricted areas, fuel,
  weight & balance and night. The timeline is coloured with the call for departures every 3
  hours, so the green stretches are your windows. The call is advice only: the pilot in command
  decides.
- **Fuel stops.** Hover an airport in the middle of the route and tap the pump (or use the
  airport card) to land there and top off. Each flight between stops gets its own climb,
  descent and fuel check, with a ground time you set (45 minutes by default), and the stop's
  weather is checked for when you'd land there.
- **Ruler.** The Measure button (or R) measures distance in NM, statute miles and km, with
  true / magnetic course and time at your cruise speed. Ends snap to airports and navaids.
- **Winds aloft** come from the NWS winds-aloft forecast (FB) of the nearest station, about 24
  hours ahead. Past that the navlog plans with no wind and says so.
- **Briefing** page and PDF lead with the call, then the navlog, weather and chart.

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

- **Export** (Route panel, share icon), with nothing filed with the FAA:
  - **FltPlan Go**: scan the QR code of the route with your iPad or iPhone and paste the route
    into a new FltPlan Go plan. Airports with a three-character FAA id get FltPlan's K prefix
    (C80 → KC80) and lat/long points go in as coordinates (3430N/11230W). See
    [docs/fltplan-go.md](docs/fltplan-go.md).
  - **FltPlan.com** form fields, ready to copy. Save the plan rather than filing it, and it
    syncs to FltPlan Go.
  - **Files**: Garmin `.fpl` (Garmin avionics, Garmin Pilot, ForeFlight), GPX and KML. The icon
    next to the share icon opens `.fpl` and `.gpx` files.
- **Aircraft**: create airframes step by step (template or blank, with checks), edit them the
  same way, duplicate, and export/import `.fpaircraft.json` files. The format is in
  [docs/aircraft-format.md](docs/aircraft-format.md). Aircraft are also saved on the server, so
  they survive a cleared browser and are shared by every device that opens the app.

## Stack

- `web/`: Svelte 5 + MapLibre GL, served as static files
- `server/`: Rust (axum), a library plus the `flightplan-server` binary. Airport data, search,
  terrain profiles, the weather/NOTAM proxy, data-pack setup and static hosting
- `native/`: the desktop app (Rust + egui), linking the server crate
- `tools/context-mcp/`: a local MCP server that gives coding agents a cached map of this repo
- `pipeline/`: Python scripts that build the chart tiles (PMTiles) and `aviation.sqlite`
  from FAA data

## Data sources

- FAA NASR 28-day subscription, FAA AIS open data (airspace), FAA Digital Obstacle File
- aviationweather.gov (METAR, TAF, PIREP, AIRMET/SIGMET, G-AIRMET), FAA TFR feed
- Iowa Environmental Mesonet: NEXRAD radar, HRRR forecast reflectivity, NBM text guidance
- NOAA map services (mapservices.weather.noaa.gov): WPC National Forecast Chart and
  precipitation forecasts, SPC convective outlooks
- NWS winds-aloft forecast (FB, aviationweather.gov): winds and temperatures aloft for the
  navlog and the winds layer, about 24 hours ahead
- Terrain from Mapzen/AWS Terrarium; basemap from OpenFreeMap (OpenStreetMap)
- FBOs from Overture Maps places plus FAA airport remarks
- Optional: the FAA NOTAM API (and a Windy map key for the web app), configured in Settings

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
./run.sh          # builds web + server, serves http://0.0.0.0:8787 (PORT and FP_BIND change it)
```

For development, run `npm --prefix web run dev` (port 5173, proxies `/api` and `/tiles` to 8787).
Without built data the server starts in setup mode and the app opens its setup screen.

## Sign-in

The web app is locked with a password. On first start the server prints a one-time **setup
code** (also in `data/setup-code.txt`); open the app, follow the short walkthrough, enter the
code, and choose your name, a password (10+ characters) and, optionally, your own sign-in
picture. Every browser then needs the password; a sign-in lasts 30 days of use (90 at most).
Change the password, the picture, or sign out (here or everywhere) under **Settings → Sign-in**.
Forgot it? On the server, run `./target/release/flightplan-server set-password` from `server/`
(with `FP_ROOT=..`): it sets a new password and signs every device out.

The desktop app doesn't use any of this: it has no web server.

### On the internet

The server is meant to sit behind an HTTPS reverse proxy. With [Caddy](https://caddyserver.com):

```
flightplan.example.com {
    reverse_proxy 127.0.0.1:8787
}
```

and run the server with:

```bash
FP_BIND=127.0.0.1 FP_TRUST_PROXY=1 ./run.sh
```

- `FP_BIND=127.0.0.1` listens only on this machine, so nobody can skip the proxy.
- `FP_TRUST_PROXY=1` takes the visitor's address and HTTPS from the proxy's `X-Forwarded-For`
  / `X-Forwarded-Proto` headers (rate limits per visitor, `Secure` cookies, HSTS). Only set it
  behind a proxy you control.
- `FP_SECURE_COOKIES=1` marks cookies `Secure` even when HTTPS isn't visible to the server.

What's in place: Argon2id password hashing (at most two checks at a time), a one-time setup
code for the first password, back-off after 5 wrong tries per address (30 s doubling to an hour)
plus a slow-down for everyone while many tries fail, `HttpOnly` / `SameSite=Strict` session
cookies stored only as hashes, a custom-header check on every change (no cross-site requests),
a Content-Security-Policy, `X-Frame-Options: DENY`, `nosniff` and a strict referrer policy.
Every API call and chart tile needs a session; only the page, the sign-in screen and its
picture are public. Keep the machine and Flightplan updated, and use a password you don't use
anywhere else.

## Keys

Add keys in the app under **Settings** (or during setup). They're stored in
`data/settings.json` (desktop app: `settings.json` in its data folder), which is git-ignored. Or copy `.env.example` to `.env`. Never commit either file.

## For coding agents

`tools/context-mcp/server.py` is a small MCP server (Python standard library only) that
indexes the repository. It gives agents a cached overview, per-file outlines, symbol lookup,
snippets and shared notes, so they don't have to reread whole files. `.mcp.json` registers it
for Claude Code: approve it the first time `claude` starts in this folder. Check it by hand with
`python3 tools/context-mcp/server.py --selftest`. The curated overview is in
[docs/context/OVERVIEW.md](docs/context/OVERVIEW.md), and notes agents leave are in
[docs/context/NOTES.md](docs/context/NOTES.md).

## License

Flightplan is free software under the [GNU Affero General Public License v3.0 only](LICENSE).
If you run a modified version as a network service, you must offer its users the source of
that version. Bundled third-party assets keep their own licenses (the Inter font is under the
SIL Open Font License, see `native/assets/fonts/Inter-LICENSE.txt`), and the chart data comes
from the FAA and Overture Maps under their terms.
