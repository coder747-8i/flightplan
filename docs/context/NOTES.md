# Notes for agents

Gotchas, decisions and conventions learned while working on this repo. Add with the
flightplan-context `add_note` tool; keep each note short and specific.

## Build
- Rust lives in ~/.cargo/bin (not on PATH in fresh shells): `export PATH=$HOME/.cargo/bin:$PATH`. Node 22 is in ~/.local/node/bin. (2026-10-24)
- Native app: `cd native && cargo build --release` → native/target/release/flightplan (~40 s incremental). The server crate is a library the app links (server/src/lib.rs). (2026-10-24)
- No sudo on the dev box. tippecanoe needs SQLite headers: point CPATH/LIBRARY_PATH at an extracted libsqlite3-dev .deb. (2026-10-24)

## Network
- IPv6 is broken on the dev machine: use `curl -4` and retries for downloads. (2026-10-24)

## egui
- egui/eframe 0.36: `eframe::App::ui(&mut self, root: &mut Ui, frame)` (not update), `ctx.global_style()/set_global_style`, `TextEdit.frame(Frame::NONE)`, `DroppedFile::path()`. (2026-10-24)
- Phosphor icons come from egui-phosphor; Inter had its private-use glyphs stripped (assets/fonts/README.md) so icons aren't shadowed. Don't put the phosphor font first: it breaks lowercase text. (2026-10-24)
- Panels take `state: &mut S` and pass it to both closures; use Cell for flags to avoid double borrows of `app`. (2026-10-24)

## Testing
- The windowed app stalls when the screen is locked (no compositor frames). Use the headless screenshot harness instead: `cargo build --features shots --bin shots`, then `FP_DATA_DIR=<dir> ./target/debug/shots <script> <outdir>`. Script actions are in native/src/ui/shot.rs and native/src/bin/shots.rs. (2026-10-24)
- Point FP_DATA_DIR at a scratch copy of a data dir so screenshots never touch the user's ~/.local/share/flightplan. (2026-10-24)

## Repo conventions
- Never add Co-Authored-By / Claude attribution lines to commits or PRs (owner's rule). (2026-10-24)
- License is AGPL-3.0-only. (2026-10-24)
- Desktop app must stay native (Rust + egui), never a webview wrapper. (2026-10-24)

## Weather data
- IEM HRRR extended-run tiles (hrrr::REFD-F1140..2880) often 503 past ~38 h even though refd_{min}.json exists; radar.rs Runs probes the last served hour by binary search on tile 4/3/5. (2026-10-24)
- G-AIRMET/SIGMET from aviationweather.gov are only valid for hours: filter by the ETA at the crossing point (hazards.rs lateral) or a departure days out gets flagged by today's advisories. (2026-10-24)
- FAA TFR shapes have no machine-readable times, but titles carry dates ("Friday, October 23, 2026 through Sunday, October 25, 2026 Local"). hazards::tfr_days parses them (12 h slack for zones); undated or old single-date titles count as always active. (2026-10-24)

## Plan
- Route.rev is serde(skip) and must deserialize to 1, not 0: Plan treats fetched_rev 0 as already fetched, so a route restored at startup never loaded its profile. The verdict waits for profile_rev == route.rev and a non-empty profile, so it never says GO without terrain. (2026-10-24)

## Chart
- Black hatched blobs on the chart at zoom 8-10 were hundreds of wind-turbine obstacle symbols overprinting; view.rs now thins obstacles tallest-first with a pixel grid below zoom 12.5. (2026-10-24)
