#!/usr/bin/env bash
# Build the Flightplan server package: one tarball with everything a Debian 12 x86_64 machine
# (the Proxmox LXC) needs, compiled here so the server never needs a compiler:
#   flightplan/bin/flightplan-server, flightplan/bin/tippecanoe, flightplan/web/dist,
#   flightplan/pipeline, flightplan/deploy (install.sh, flightplan-lxc.sh, updaters), VERSION
# The Rust server and tippecanoe are built in Debian 12 (bookworm) containers so they link
# against Debian 12's libraries whatever this machine runs. Needs Docker; the web app is built
# with this machine's npm when there is one.
#
# Usage: deploy/build-package.sh [out.tar.gz]   (default: dist/flightplan-server-linux-x86_64.tar.gz)
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT="$PWD"
OUT="${1:-$ROOT/dist/flightplan-server-linux-x86_64.tar.gz}"
CACHE="$ROOT/dist/.build-cache"
mkdir -p "$CACHE/cargo" "$CACHE/target" "$CACHE/tippecanoe" "$(dirname "$OUT")"
command -v docker >/dev/null || { echo "needs Docker" >&2; exit 1; }
me="$(id -u):$(id -g)"
step() { printf '\n\033[1m== %s\033[0m\n' "$*"; }

step "Web app"
if command -v npm >/dev/null; then
  (cd web && npm ci --no-audit --no-fund --loglevel=error && npm run build >/dev/null)
else
  docker run --rm --user "$me" -e HOME=/tmp -v "$ROOT/web:/src" -w /src node:22-bookworm \
    sh -c 'npm ci --no-audit --no-fund --loglevel=error && npm run build >/dev/null'
fi

step "Server (Debian 12)"
docker run --rm --user "$me" -e CARGO_HOME=/cache/cargo -e CARGO_TARGET_DIR=/cache/target \
  -v "$ROOT/server:/src:ro" -v "$CACHE:/cache" -w /src rust:1-bookworm \
  cargo build --release --locked

step "tippecanoe (Debian 12)"
if [ ! -x "$CACHE/tippecanoe/tippecanoe" ]; then
  docker run --rm --user "$me" -v "$CACHE/tippecanoe:/out" rust:1-bookworm sh -c '
    set -e
    cd /tmp
    curl -fsSL https://codeload.github.com/felt/tippecanoe/tar.gz/refs/heads/main | tar -xz
    make -C tippecanoe-main -j"$(nproc)" tippecanoe >/dev/null 2>&1
    strip tippecanoe-main/tippecanoe
    cp tippecanoe-main/tippecanoe /out/'
fi

step "Package"
pkg="$(mktemp -d)"
trap 'rm -rf "$pkg"' EXIT
dst="$pkg/flightplan"
mkdir -p "$dst/bin" "$dst/web" "$dst/deploy"
cp "$CACHE/target/release/flightplan-server" "$dst/bin/"
strip "$dst/bin/flightplan-server" 2>/dev/null || true
cp "$CACHE/tippecanoe/tippecanoe" "$dst/bin/"
cp -r web/dist "$dst/web/dist"
mkdir -p "$dst/pipeline"
(cd pipeline && find . -type f ! -path './.tools/*' ! -path '*/__pycache__/*' -print0 | xargs -0 -I{} cp --parents {} "$dst/pipeline/")
cp deploy/lxc/install.sh deploy/lxc/flightplan-data-update deploy/lxc/flightplan-update deploy/proxmox/flightplan-lxc.sh "$dst/deploy/"
chmod 0755 "$dst"/deploy/* "$dst"/bin/*
printf '%s (%s)\n' "$(git rev-parse --short HEAD 2>/dev/null || echo local)$(git diff --quiet 2>/dev/null || echo +changes)" "$(date -u +%Y-%m-%dT%H:%MZ)" >"$dst/VERSION"
tar -C "$pkg" -czf "$OUT" flightplan
echo "$OUT ($(du -h "$OUT" | cut -f1)), version $(cat "$dst/VERSION")"
