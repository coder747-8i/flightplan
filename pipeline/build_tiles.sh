#!/usr/bin/env bash
# Build the chart's vector tiles (PMTiles) from data/build/*.geojson.
set -euo pipefail
cd "$(dirname "$0")/.."
TC="${TIPPECANOE:-$HOME/.local/bin/tippecanoe}"
B=data/build
mkdir -p tiles

"$TC" -q -f -o tiles/aero.pmtiles -n "FAA aero points" -Z3 -z13 -r1 \
  --no-feature-limit --no-tile-size-limit --extend-zooms-if-still-dropping \
  -L airports:$B/airports.geojson -L navaids:$B/navaids.geojson -L fixes:$B/fixes.geojson \
  -L runways:$B/runways.geojson -L airways:$B/airways.geojson

"$TC" -q -f -o tiles/airspace.pmtiles -n "FAA airspace" -Z4 -z11 \
  --no-feature-limit --no-tile-size-limit --no-tiny-polygon-reduction --detect-shared-borders \
  --simplification=3 -L airspace:$B/airspace.geojson -L airspace_labels:$B/airspace_labels.geojson

"$TC" -q -f -o tiles/obstacles.pmtiles -n "FAA obstacles" -Z8 -z12 -r1 \
  --no-feature-limit --no-tile-size-limit -L obstacles:$B/obstacles.geojson

ls -la tiles
