# Flightplan

US flight planner: native desktop app (`native/`, Rust + egui), Rust server library
(`server/`), Svelte web app (`web/`), FAA data pipeline (`pipeline/`).

## Start here

- Use the **flightplan-context** MCP server (`.mcp.json`) before reading files: `overview`
  first, then `outline` / `find` / `snippet` / `search` / `refs` / `deps`. It keeps a cached
  index, so you don't have to reread large files. When you learn something non-obvious (a
  gotcha, an API quirk, a decision), save it with `add_note`.
- Architecture: `docs/context/OVERVIEW.md`. Notes from earlier agents: `docs/context/NOTES.md`.

## Build and test

```bash
export PATH=$HOME/.cargo/bin:$PATH
cd native && cargo build --release && cargo test       # desktop app
cd server && cargo test                                 # server library
python3 tools/context-mcp/server.py --selftest          # context server
```

UI checks are headless screenshots (the windowed app stalls on a locked screen):
`cargo build --features shots --bin shots`, then
`FP_DATA_DIR=<scratch data dir> ./target/debug/shots <script> <outdir>`. Never point it at the
user's real data folder.

## Rules

- The desktop app stays native (egui). No webview wrappers.
- No Co-Authored-By or other Claude attribution lines in commits or PRs.
- License is AGPL-3.0-only.
- Ask before pushing.
