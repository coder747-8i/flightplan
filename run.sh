#!/usr/bin/env bash
# Build the web app and run the server on port 8787 (all interfaces).
set -euo pipefail
cd "$(dirname "$0")"
(cd web && npm run build)
(cd server && cargo build --release)
cd server && FP_ROOT=.. exec ./target/release/flightplan-server
