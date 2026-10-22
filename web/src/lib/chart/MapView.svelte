<script lang="ts">
  import * as maplibregl from 'maplibre-gl'
  import type { MapGeoJSONFeature } from 'maplibre-gl'
  import 'maplibre-gl/dist/maplibre-gl.css'
  // ship the worker as a build asset; MapLibre's own sibling-file lookup breaks once bundled
  import workerUrl from 'maplibre-gl/dist/maplibre-gl-worker.mjs?url'
  import { Protocol } from 'pmtiles'
  import { onMount } from 'svelte'
  import { app, type PointItem } from '../stores/app.svelte'
  import { registerIcons } from './icons'
  import { DAY, NIGHT } from './palette'
  import { AIRSPACE_FILTERS, LAYER_GROUPS, altitudeFilter, buildStyle, type LayerGroup } from './style'
  import { mapRef } from './mapRef'
  import { wx } from '../wx/WeatherLayer.svelte'

  let container: HTMLDivElement
  let map: maplibregl.Map | undefined = $state()
  let styleReady = $state(0)

  const palette = () => (app.theme === 'night' ? NIGHT : DAY)

  function applyLayers() {
    if (!map) return
    for (const [group, ids] of Object.entries(LAYER_GROUPS) as [LayerGroup, readonly string[]][]) {
      const vis = app.layers[group] ? 'visible' : 'none'
      for (const id of ids) if (map.getLayer(id)) map.setLayoutProperty(id, 'visibility', vis)
    }
  }

  function applyAltitude() {
    if (!map) return
    const { enabled, lo, hi } = app.altitude
    for (const [id, base] of Object.entries(AIRSPACE_FILTERS)) {
      if (!map.getLayer(id)) continue
      map.setFilter(id, enabled ? altitudeFilter(base, lo, hi) : base)
    }
  }

  const KIND_NAME: Record<string, string> = {
    B: 'Class B', C: 'Class C', D: 'Class D', E2: 'Class E surface', E3: 'Class E extension', E4: 'Class E extension',
    E5: 'Class E', E6: 'Class E (en route)', MODEC: 'Mode C veil', R: 'Restricted', P: 'Prohibited', W: 'Warning', A: 'Alert', MOA: 'MOA',
  }

  function describe(features: MapGeoJSONFeature[]): PointItem[] {
    const p = palette()
    const seen = new Set<string>()
    const items: PointItem[] = []
    for (const f of features) {
      const pr = f.properties as Record<string, any>
      let item: PointItem | null = null
      if (f.sourceLayer === 'airspace') {
        const kind = pr.kind as string
        const color = ['B', 'D', 'R', 'P', 'W'].includes(kind) ? p.classB : p.classC
        item = { type: 'airspace', title: pr.name || KIND_NAME[kind], subtitle: `${KIND_NAME[kind]} · ${pr.range}${pr.times ? ' · ' + pr.times : ''}`, color }
      } else if (f.sourceLayer === 'airports') {
        item = { type: 'airport', id: pr.id, title: `${pr.icao || pr.id} · ${pr.name}`, subtitle: `Elev ${pr.elev} ft${pr.ctaf ? ' · CTAF ' + pr.ctaf : ''}`, color: pr.twr ? p.towered : p.nontowered }
      } else if (f.sourceLayer === 'navaids') {
        item = { type: 'navaid', id: pr.id, title: `${pr.id} · ${pr.name}`, subtitle: `${pr.type} ${pr.freq}`, color: p.navaid }
      } else if (f.sourceLayer === 'fixes') {
        item = { type: 'fix', id: pr.id, title: pr.id, subtitle: pr.vfr ? 'VFR waypoint' : 'Fix', color: p.nontowered }
      } else if (f.source === 'wx-tfr') {
        item = { type: 'airspace', title: `TFR ${pr.notam} · ${pr.type ?? pr.legal ?? ''}`, subtitle: pr.title || pr.description || '', color: '#d23a3a' }
      } else if (f.source === 'wx-sigmet') {
        item = { type: 'airspace', title: pr.label, subtitle: '', color: pr.color, decode: { kind: 'sigmet', data: wx.sigmets[pr.idx], raw: pr.raw } }
      } else if (f.source === 'wx-gairmet') {
        item = { type: 'airspace', title: `G-AIRMET ${pr.label}`, subtitle: '', color: pr.color, decode: wx.gairmets[pr.idx] ? { kind: 'gairmet', data: wx.gairmets[pr.idx] } : undefined }
      } else if (f.source === 'wx-metar') {
        item = { type: 'airport', id: pr.id, title: `${pr.id} ${pr.cat}`, subtitle: '', color: pr.color, decode: { kind: 'metar', raw: pr.raw } }
      } else if (f.source === 'wx-pirep') {
        item = { type: 'obstacle', title: `PIREP ${pr.label}`, subtitle: '', color: pr.color, decode: { kind: 'pirep', raw: pr.raw } }
      } else if (f.sourceLayer === 'obstacles') {
        item = { type: 'obstacle', title: `${pr.type} ${pr.msl} ft MSL`, subtitle: `${pr.agl} ft AGL${pr.lit ? ' · lighted' : ''}`, color: p.obstacle }
      }
      if (!item) continue
      const key = item.title + item.subtitle
      if (seen.has(key)) continue
      seen.add(key)
      items.push(item)
    }
    // airspace from the top down, like a vertical slice
    return items.sort((a, b) => (a.type === 'airspace' ? 1 : 0) - (b.type === 'airspace' ? 1 : 0))
  }

  onMount(() => {
    maplibregl.setWorkerUrl(workerUrl)
    const protocol = new Protocol()
    maplibregl.addProtocol('pmtiles', protocol.tile)
    const saved = (() => {
      try { return JSON.parse(localStorage.getItem('fp.view') || 'null') } catch { return null }
    })()
    map = new maplibregl.Map({
      container,
      style: buildStyle(palette()),
      center: saved?.center ?? [-114.6, 32.66],
      zoom: saved?.zoom ?? 8,
      attributionControl: { compact: true },
      dragRotate: false,
      pitchWithRotate: false,
      maxPitch: 0,
    })
    mapRef.map = map
    if (import.meta.env.DEV) (window as any).__map = map
    map.touchZoomRotate.disableRotation()
    map.addControl(new maplibregl.ScaleControl({ unit: 'nautical' }), 'bottom-right')

    map.on('style.load', () => {
      registerIcons(map!, palette())
      applyLayers()
      applyAltitude()
      styleReady++
    })
    map.on('moveend', () => {
      const c = map!.getCenter()
      try { localStorage.setItem('fp.view', JSON.stringify({ center: [c.lng, c.lat], zoom: map!.getZoom() })) } catch {}
    })
    map.on('mousemove', (e) => {
      app.cursor = { lat: e.lngLat.lat, lon: e.lngLat.lng, zoom: map!.getZoom() }
    })
    map.on('click', (e) => {
      if (mapRef.onMapClick?.(e)) return
      const pad = 6
      const box: [maplibregl.PointLike, maplibregl.PointLike] = [[e.point.x - pad, e.point.y - pad], [e.point.x + pad, e.point.y + pad]]
      const pointFeatures = map!.queryRenderedFeatures(box).filter((f) => ['airports', 'navaids', 'fixes', 'obstacles'].includes(f.sourceLayer ?? '') || ['wx-metar', 'wx-pirep'].includes(f.source))
      const wxAreas = map!.queryRenderedFeatures(e.point, { layers: ['wx-tfr-fill', 'wx-sigmet-fill', 'wx-gairmet-fill'].filter((l) => map!.getLayer(l) && map!.getLayoutProperty(l, 'visibility') !== 'none') })
      const airport = pointFeatures.find((f) => f.sourceLayer === 'airports')
      if (airport) {
        app.selection = { kind: 'airport', id: (airport.properties as any).id }
        return
      }
      // airspace polygons are drawn as outlines, so query the source data at the exact point
      const airspace = map!.querySourceFeatures('airspace', { sourceLayer: 'airspace' })
        .filter((f) => pointInFeature(e.lngLat.lng, e.lngLat.lat, f))
      const { enabled, lo, hi } = app.altitude
      const shown = airspace.filter((f) => {
        const pr = f.properties as any
        if (!kindVisible(pr.kind, pr.lo_agl, pr.lo)) return false
        return !enabled || (pr.lo <= hi && pr.hi >= lo)
      })
      const items = describe([...pointFeatures, ...wxAreas, ...shown] as MapGeoJSONFeature[])
      app.selection = items.length ? { kind: 'point', lngLat: [e.lngLat.lng, e.lngLat.lat], items } : null
    })
    map.getCanvas().style.cursor = 'default'
    map.on('mousemove', ['apt-public', 'apt-private', 'nav-symbol'], () => (map!.getCanvas().style.cursor = 'pointer'))
    map.on('mouseleave', ['apt-public', 'apt-private', 'nav-symbol'], () => (map!.getCanvas().style.cursor = 'default'))

    return () => {
      maplibregl.removeProtocol('pmtiles')
      map?.remove()
    }
  })

  function kindVisible(kind: string, loAgl: number, loFt: number) {
    const L = app.layers
    if (kind === 'B') return L.classB
    if (kind === 'C') return L.classC
    if (kind === 'D') return L.classD
    if (kind === 'MODEC') return L.modeC
    if (kind === 'E6' || (kind === 'E5' && loAgl && loFt > 700)) return L.classEhigh
    if (kind.startsWith('E')) return L.classE
    return L.sua
  }

  function pointInRing(x: number, y: number, ring: number[][]) {
    let inside = false
    for (let i = 0, j = ring.length - 1; i < ring.length; j = i++) {
      const [xi, yi] = ring[i]
      const [xj, yj] = ring[j]
      if (yi > y !== yj > y && x < ((xj - xi) * (y - yi)) / (yj - yi) + xi) inside = !inside
    }
    return inside
  }

  function pointInFeature(x: number, y: number, f: GeoJSON.Feature) {
    const g = f.geometry
    const polys = g.type === 'Polygon' ? [g.coordinates] : g.type === 'MultiPolygon' ? g.coordinates : []
    return polys.some((rings) => pointInRing(x, y, rings[0]) && !rings.slice(1).some((h) => pointInRing(x, y, h)))
  }

  // theme switch rebuilds the style
  let lastTheme = app.theme
  $effect(() => {
    const t = app.theme
    if (map && t !== lastTheme) {
      lastTheme = t
      map.setStyle(buildStyle(palette()))
    }
  })
  $effect(() => {
    JSON.stringify(app.layers)
    styleReady
    applyLayers()
  })
  $effect(() => {
    app.altitude.enabled; app.altitude.lo; app.altitude.hi
    styleReady
    applyAltitude()
  })
</script>

<div class="map" bind:this={container}></div>

<style>
  .map {
    position: absolute;
    inset: 0;
  }
</style>
