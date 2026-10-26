# Flightplan: architecture overview for agents

Read this first; then use the flightplan-context MCP tools (`outline`, `find`, `snippet`,
`search`, `refs`, `deps`) instead of reading whole files. Record anything you had to dig for
with `add_note`.

## What the product is

A US VFR/IFR flight planner. It comes as two front ends over one Rust core:

- `native/`: **the desktop app** (Rust + egui/eframe 0.36). It is the main product. It must
  stay native, never a webview wrapper.
- `web/`: a Svelte 5 + MapLibre web app served by the server binary. It has the same features
  as the desktop app (see "Web app map" below); keep the two in step when adding features.
- `server/`: crate `flightplan-server`, **a library first** (`server/src/lib.rs`), plus a thin
  axum binary (`main.rs`). The desktop app calls it directly (`flightplan_server::...`).
  `auth.rs` locks the web server (users: owner + optional others in multi-user mode; sessions,
  desktop device tokens via pairing codes, setup code, throttling; security headers in lib.rs).
  Handlers get the signed-in user with the `CurrentUser` extractor; `auth::user_dir` is that
  user's folder (the owner's is the old user dir). `plans.rs` has per-plan endpoints and the
  `/api/events` SSE stream (fed by `plans::announce`, also from userstore writes). Auth is skipped
  when `cfg.host` is set (embedded). The desktop app syncs plans with a remote server in
  `native/src/sync.rs` (Settings → Sync with a Flightplan server).
- `pipeline/`: Python + tippecanoe that build the chart data pack from the FAA's 28-day cycle
  (`build_datapack.sh`, also embedded in the app as "Build on this computer").
- `tools/context-mcp/`: this context server.

## Desktop app map (`native/src`)

- `app.rs`: the `App` struct (all state) and the frame loop: `plan.update`, then `wx.update`,
  then chart, shell, briefing, dialogs, onboarding / tour, toast and autosave.
- `backend.rs`: tokio runtime, the shared `AppState` (server library) and `Task<T>`, which is
  polled once per frame. Use `be.spawn(async ...)` for any I/O.
- `store.rs`: persistence. `Saved` (desktop.json: layers, wx toggles, view, route, personal
  minimums), `Fleet` (aircraft.json) and `Prefs`.
- `model/`: `route.rs` (waypoints, cruise, departure string, `rev` change counter),
  `aircraft.rs` (perf, W&B, `compute_wb`), `place.rs`, and `plan.rs` (**derived analysis**).
  `plan.rs` covers the profile fetch, winds, navlog, hazards, the forecast fetch, the verdict
  and the 5-day `windows`.
- `nav/`: pure logic, unit-tested. `geo`, `navlog` (wind triangle, TOC/TOD, fuel; splits the
  trip at fuel stops into `segments`, each its own climb/cruise/descent, topped off, with
  ground time; `alt_path` is the planned altitude), `profile`,
  `hazards` (terrain / airspace / TFR / advisories / fuel / W&B / night), `verdict`
  (**go / no-go**: green/yellow/red against `Minimums`), `magvar` (WMM) and `sun`.
- `wx/`: `state.rs` (advisories, METAR / PIREP dots, wind grid), `decode.rs` (METAR / TAF /
  PIREP / SIGMET to plain English), `forecast.rs` (conditions at a time: METAR, then TAF, then
  NBM via IEM), `official.rs` (WPC forecast chart / QPF and SPC outlooks from NOAA's map
  service: day boundaries, tile export URLs, what the route crosses) and `radar.rs` (the frame
  for a time: NEXRAD, then HRRR tiles, then WPC chart days 1-3, then WPC QPF days 4-5). The
  fetching itself (station pick + METAR / TAF / NBM, `along_route`, day-1 ends, HRRR runs) is
  in the server library, `server/src/forecast.rs`, shared with the web app.
  **Rule from the owner: never synthesize forecasts. Show and read official products as
  published.**
- `map/`: the chart renderer. It reads PMTiles v3 (`pmtiles.rs`) and MVT (`mvt.rs`), renders
  tiles with tiny-skia (`render.rs`, `style.rs`), loads and caches tiles (`tiles.rs`), and draws
  symbols and labels with collision (`view.rs`, `symbols.rs`, `icons.rs`).
- `ui/`: egui panels. `shell` (rail + panel switch), `chart` (map interaction, radar, route
  drawing), `route`, `weather`, `layers`, `aircraft`, `settings` (personal minimums, data,
  keys), `timeline` (5-day scrubber), `verdict` (go / no-go card), `profile`, `briefing` (+
  PDF via `io/pdf.rs`), `export` (FltPlan Go QR / KML, FltPlan.com fields, .fpl / GPX / KML),
  `wizard` / `import` (aircraft), `onboarding`, `tour`, `widgets` (shared styling) and `shot`
  (scripted screenshots). The ruler lives in `chart.rs` (`Ruler`, `draw_ruler`); its button is
  in `shell.rs`. Fuel stops are `Waypoint::fuel_stop`, toggled from the route list or the
  airport card.
- `io/`: `flightplan.rs` (route formats), `aircraft_file.rs` (`.fpaircraft.json`) and
  `pdf.rs`.
- `localbuild.rs`: runs the embedded pipeline to build a data pack locally.

## Web app map (`web/src/lib`)

`main.ts` asks `/api/auth/status` first: signed out, it mounts `auth/Login.svelte` (sign-in, or the
first-run `auth/SetupWalk.svelte`) and loads `App.svelte` only after sign-in, because the stores
start calling the API as soon as they load. Code under `auth/` must not import the stores or
`api.ts` (a 401 there reloads the page). Settings → Sign-in is `auth/SignInSettings.svelte`.

The same plan logic as the desktop app, ported to TypeScript (keep them in step):
`nav/navlog.ts` (fuel stops / segments), `nav/hazards.ts`, `nav/verdict.ts` (go / no-go),
`wx/forecast.ts` (conditions at a time from the server's `/api/wx/station`), `wx/notams.ts`,
`wx/radar.svelte.ts` (frame for a time, outlook days, HRRR runs). `stores/plan.svelte.ts` fetches
and derives everything (the 5-day `windows` are computed in the background once inputs
settle); `stores/route.svelte.ts` (fuel stops, `locked` = view only), `minimums`, `plans`
(saved plans) and `history` (undo / redo) persist on the server via `/api/user/{key}`. UI:
`ui/VerdictCard`, `ui/Timeline`, `ui/NotamList`, `ui/MinimumsForm`, `chart/RulerLayer` +
`chart/ruler.svelte.ts`. Server endpoints for it: `/api/route` (airways expanded,
`resolve_route` in lib.rs), `/api/wx/station`, `/api/wx/official`, `/api/wx/outlooks`,
`/api/wx/hrrr`, `/api/wx/fb`, `/api/notams`.

## Data flow for a plan

The route changes, so `route.rev` bumps. `Plan::update` waits 250 ms, then fetches the profile
(terrain, obstacles, airspace) and winds, and `RouteForecast` fetches the end stations (METAR /
TAF / NBS / NBE) and the WPC / SPC areas the route crosses (`official::along_route`). When the inputs hash changes, it recomputes the navlog,
hazards and `verdict::evaluate`. It also scores departures every 3 h for 5 days into
`plan.windows`, which the timeline draws. The departure time is `route.departure` (local
"YYYY-MM-DDTHH:MM", empty = now). The timeline writes it with `timeline::set_departure`.

## External services

- aviationweather.gov: METAR, TAF, PIREP, AIRMET/SIGMET, G-AIRMET.
- FAA: TFR feed, NOTAM API (key).
- IEM (mesonet.agron.iastate.edu): NEXRAD tiles `nexrad-n0q-900913` and HRRR tiles
  `hrrr::REFD-F{min:04}-0`. Run info is at `/data/gis/images/4326/hrrr/refd_{min}.json`, and
  extended-run tiles often stop before 48 h, so the app probes the last hour served. NBM text
  guidance comes from `/api/1/mos.json?station=K...&model=NBS|NBE`. NBS cig is in hundreds of
  ft (-88 = unlimited), vis in tenths of SM.
- NOAA map services (`mapservices.weather.noaa.gov/vector/rest/services`):
  `outlooks/natl_fcst_wx_chart` (WPC chart, layers per day), `precip/wpc_qpf` and
  `outlooks/SPC_wx_outlks`. Use `MapServer/export` for tiles (bbox in EPSG:3857) and
  `MapServer/identify` with a polyline for what the route crosses.
- aviationweather.gov `windtemp` (NWS FB winds aloft, text, fixed columns; 6/12/24 h bulletins
  with "FOR USE" windows): `server/src/wx.rs` `parse_fb` / `fb_stations` / `winds_at`
  (nearest station within 150 NM). Station ids are placed with the FAA database.
- OpenFreeMap basemap, Terrarium DEM (S3). No Open-Meteo / Windy model data any more.

## Testing

- `cd native && cargo test` covers nav, wx, io and map. `cd server && cargo test` covers the
  server.
- Visual checks run headless: `cargo build --features shots --bin shots`, then
  `FP_DATA_DIR=<scratch copy of a data dir> ./target/debug/shots <script> <outdir>`. The
  script is `action [arg] > name` per line: panel, route, view, depart <h>, cruise, wx radar,
  briefing, export, sleep, click and more (see `ui/shot.rs` and `bin/shots.rs`).

## Conventions

- Comments explain why, briefly. Module docs (`//!`) say what the file is for, and the context
  index uses them as file summaries, so keep them accurate.
- UI copy is plain English and short. Keep the UI uncluttered: one-line toggles with hover
  hints, folds for secondary lists, details on demand.
- Never add Co-Authored-By / Claude attribution to commits. License: AGPL-3.0-only.
