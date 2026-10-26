<script lang="ts">
  // The ruler's lines, points and readouts: each leg's distance and magnetic course, and per line
  // the total in NM / statute miles / km with the time at the aircraft's cruise speed. Its button
  // (RulerButton) sits top right by the altitude filter; R toggles it (chart/ruler.svelte.ts).
  import type { GeoJSONSource, Map as MLMap, MapMouseEvent } from 'maplibre-gl'
  import { onMount } from 'svelte'
  import { mapRef } from './mapRef'
  import { ruler, type RulerPoint } from './ruler.svelte'
  import { app } from '../stores/app.svelte'
  import { aircraft } from '../stores/aircraft.svelte'
  import { bearing, distanceNm } from '../nav/geo'
  import { deg, fmtTime, variation } from '../nav/navlog'

  let map: MLMap | undefined
  let ready = $state(0)
  /** bumps on every map move, so the HTML readouts follow */
  let tick = $state(0)
  let hover = $state<{ lat: number; lon: number } | null>(null)

  const inks = $derived(app.theme === 'night' ? ['#f0f0f0', '#6eaaff', '#ff78c8'] : ['#191c22', '#1e5ac8', '#be1482'])
  const empty: GeoJSON.FeatureCollection = { type: 'FeatureCollection', features: [] }

  function install() {
    if (!map || map.getSource('ruler')) return
    map.addSource('ruler', { type: 'geojson', data: empty })
    map.addLayer({ id: 'ruler-casing', type: 'line', source: 'ruler', filter: ['==', ['geometry-type'], 'LineString'], paint: { 'line-color': '#ffffff', 'line-width': 5, 'line-opacity': 0.9 } })
    map.addLayer({ id: 'ruler-line', type: 'line', source: 'ruler', filter: ['==', ['geometry-type'], 'LineString'], paint: { 'line-color': ['get', 'ink'], 'line-width': 2.5, 'line-dasharray': [4, 2.4] } })
    map.addLayer({ id: 'ruler-pt', type: 'circle', source: 'ruler', filter: ['==', ['geometry-type'], 'Point'], paint: { 'circle-radius': 4.5, 'circle-color': '#ffffff', 'circle-stroke-color': ['get', 'ink'], 'circle-stroke-width': 2.5 } })
    ready++
  }

  /** the lines as drawn: the one being drawn follows the pointer */
  const drawn = $derived.by(() => {
    if (!ruler.on) return []
    return ruler.lines.map((l, i) => (i === ruler.lines.length - 1 && l.length && hover ? [...l, { ...hover, label: '' }] : l))
  })

  $effect(() => {
    ready
    const feats: GeoJSON.Feature[] = []
    drawn.forEach((pts, li) => {
      const ink = inks[li % inks.length]
      if (pts.length >= 2) feats.push({ type: 'Feature', properties: { ink }, geometry: { type: 'LineString', coordinates: pts.map((p) => [p.lon, p.lat]) } })
      for (const p of pts) feats.push({ type: 'Feature', properties: { ink }, geometry: { type: 'Point', coordinates: [p.lon, p.lat] } })
    })
    ;(map?.getSource('ruler') as GeoJSONSource | undefined)?.setData({ type: 'FeatureCollection', features: feats })
    if (map) map.getCanvas().style.cursor = ruler.on ? 'crosshair' : ''
  })

  const mc = (a: RulerPoint, b: RulerPoint) => deg(bearing(a.lat, a.lon, b.lat, b.lon) - variation(a.lat, a.lon))

  /** readouts in screen pixels */
  const labels = $derived.by(() => {
    tick
    if (!map || !ruler.on) return { legs: [], totals: [] }
    const tas = aircraft.active?.perf.tas
    const legs: { x: number; y: number; text: string; ink: string }[] = []
    const totals: { x: number; y: number; lines: string[]; ink: string }[] = []
    drawn.forEach((pts, li) => {
      const ink = inks[li % inks.length]
      let total = 0
      for (let k = 1; k < pts.length; k++) {
        const a = pts[k - 1]
        const b = pts[k]
        const nm = distanceNm(a.lat, a.lon, b.lat, b.lon)
        total += nm
        if (pts.length > 2) {
          const pa = map!.project([a.lon, a.lat])
          const pb = map!.project([b.lon, b.lat])
          legs.push({ x: (pa.x + pb.x) / 2, y: (pa.y + pb.y) / 2, text: `${nm.toFixed(1)} NM  ${mc(a, b)}°M`, ink })
        }
      }
      if (pts.length < 2) return
      const a = pts[0]
      const b = pts[pts.length - 1]
      const lines: string[] = []
      const names = pts.map((p) => p.label).filter(Boolean)
      if (names.length >= 2) lines.push(names.join(' → '))
      lines.push(`${total.toFixed(1)} NM · ${(total * 1.150779).toFixed(1)} mi · ${(total * 1.852).toFixed(1)} km`)
      let second = pts.length === 2 ? `${deg(bearing(a.lat, a.lon, b.lat, b.lon))}°T · ${mc(a, b)}°M` : `${pts.length - 1} legs`
      if (tas && tas > 0) second += ` · ${fmtTime((total / tas) * 60)} at ${Math.round(tas)} kt`
      lines.push(second)
      const end = map!.project([b.lon, b.lat])
      totals.push({ x: end.x, y: end.y, lines, ink })
    })
    return { legs, totals }
  })

  function keys(e: KeyboardEvent) {
    const t = document.activeElement?.tagName
    if (t === 'INPUT' || t === 'TEXTAREA' || t === 'SELECT' || e.ctrlKey || e.metaKey || e.altKey) return
    if (e.key === 'r' || e.key === 'R') {
      ruler.toggle()
      e.preventDefault()
      return
    }
    if (!ruler.on) return
    if (e.key === ' ') ruler.newLine()
    else if (e.key === 'Backspace') ruler.undo()
    else if (e.key === 'Escape') ruler.escape()
    else return
    e.preventDefault()
  }

  onMount(() => {
    const wait = setInterval(() => {
      if (!mapRef.map) return
      clearInterval(wait)
      map = mapRef.map
      if (map.isStyleLoaded()) install()
      map.on('styledata', () => install())
      map.on('move', () => tick++)
      map.on('mousemove', (e: MapMouseEvent) => {
        if (ruler.on) hover = { lat: e.lngLat.lat, lon: e.lngLat.lng }
      })
      map.getCanvas().addEventListener('mouseleave', () => (hover = null))
    }, 50)
    return () => clearInterval(wait)
  })
</script>

<svelte:window onkeydown={keys} />


{#each labels.legs as l}
  <div class="leg" style:left={`${l.x}px`} style:top={`${l.y}px`} style:color={l.ink}>{l.text}</div>
{/each}
{#each labels.totals as t}
  <div class="total" style:left={`${t.x + 14}px`} style:top={`${t.y + 8}px`} style:color={t.ink} style:border-color={t.ink}>
    {#each t.lines as line}<div>{line}</div>{/each}
  </div>
{/each}

<style>
  .leg,
  .total {
    position: absolute;
    z-index: 6;
    pointer-events: none;
    font-weight: 600;
    white-space: nowrap;
  }
  .leg {
    font-size: 11px;
    transform: translate(6px, calc(-100% - 4px));
    text-shadow: 0 0 2px var(--surface-solid), 0 0 2px var(--surface-solid), 0 0 3px var(--surface-solid);
  }
  .total {
    font-size: 12px;
    line-height: 1.4;
    padding: 5px 8px;
    border-radius: 8px;
    border: 1.5px solid;
    background: var(--surface-solid);
    box-shadow: var(--shadow);
  }
</style>
