<script lang="ts">
  // Draws the active route (magenta great-circle line, nodes, leg labels, TOC/TOD) and handles
  // editing: drag a waypoint to move it, drag the line to rubber-band in a new waypoint, or
  // click the map while "add points" is on. Dropped points snap to nearby airports/navaids/fixes.
  // View only (route.locked) and the ruler turn editing off.
  import type { GeoJSONSource, Map as MLMap, MapMouseEvent } from 'maplibre-gl'
  import { onMount } from 'svelte'
  import { route } from '../stores/route.svelte'
  import { plan } from '../stores/plan.svelte'
  import { app } from '../stores/app.svelte'
  import { bearing, distanceNm, greatCircle, interpolate, pointAlong } from '../nav/geo'
  import { variation, deg } from '../nav/navlog'
  import { mapRef } from './mapRef'
  import { DAY, NIGHT } from './palette'
  import { snapPlace } from './snap'
  import { ruler } from './ruler.svelte'

  let map: MLMap | undefined
  let ready = $state(0)
  let drag: { mode: 'move' | 'insert'; index: number; lngLat: [number, number] } | null = null

  const empty: GeoJSON.FeatureCollection = { type: 'FeatureCollection', features: [] }

  function install() {
    if (!map || map.getSource('route')) return
    const p = app.theme === 'night' ? NIGHT : DAY
    const before = map.getLayer('apt-heli') ? 'apt-heli' : undefined
    map.addSource('route', { type: 'geojson', data: empty })
    map.addSource('route-drag', { type: 'geojson', data: empty })
    map.addSource('route-hover', { type: 'geojson', data: empty })
    map.addSource('as-pick', { type: 'geojson', data: empty })
    map.addLayer({ id: 'as-pick-fill', type: 'fill', source: 'as-pick', paint: { 'fill-color': p.route, 'fill-opacity': 0.12 } }, before)
    map.addLayer({ id: 'as-pick-line', type: 'line', source: 'as-pick', paint: { 'line-color': p.route, 'line-width': 3, 'line-dasharray': [2, 1] } }, before)
    map.addLayer({ id: 'route-casing', type: 'line', source: 'route', filter: ['==', ['get', 't'], 'line'], layout: { 'line-cap': 'round', 'line-join': 'round' }, paint: { 'line-color': '#ffffff', 'line-width': 8, 'line-opacity': 0.85 } }, before)
    map.addLayer({ id: 'route-line', type: 'line', source: 'route', filter: ['==', ['get', 't'], 'line'], layout: { 'line-cap': 'round', 'line-join': 'round' }, paint: { 'line-color': p.route, 'line-width': 4 } }, before)
    map.addLayer({ id: 'route-drag-line', type: 'line', source: 'route-drag', paint: { 'line-color': p.route, 'line-width': 3, 'line-dasharray': [2, 1.5], 'line-opacity': 0.8 } })
    map.addLayer({
      id: 'route-leg-label', type: 'symbol', source: 'route', filter: ['==', ['get', 't'], 'leg'],
      layout: {
        'symbol-placement': 'point', 'text-field': ['get', 'label'], 'text-font': ['Noto Sans Bold'], 'text-size': 11.5,
        'text-rotate': ['get', 'rot'], 'text-rotation-alignment': 'map', 'text-offset': [0, -1.1], 'text-allow-overlap': false,
      },
      paint: { 'text-color': p.route, 'text-halo-color': '#ffffff', 'text-halo-width': 2 },
    })
    map.addLayer({
      id: 'route-mark', type: 'circle', source: 'route', filter: ['==', ['get', 't'], 'mark'],
      paint: { 'circle-radius': 5, 'circle-color': '#ffffff', 'circle-stroke-color': p.route, 'circle-stroke-width': 2.5 },
    })
    map.addLayer({
      id: 'route-mark-label', type: 'symbol', source: 'route', filter: ['==', ['get', 't'], 'mark'],
      layout: { 'text-field': ['get', 'label'], 'text-font': ['Noto Sans Bold'], 'text-size': 10, 'text-offset': [0, 1.2], 'text-anchor': 'top' },
      paint: { 'text-color': p.route, 'text-halo-color': '#ffffff', 'text-halo-width': 2 },
    })
    map.addLayer({
      id: 'route-node', type: 'circle', source: 'route', filter: ['==', ['get', 't'], 'node'],
      paint: { 'circle-radius': 7, 'circle-color': p.route, 'circle-stroke-color': '#ffffff', 'circle-stroke-width': 2.5 },
    })
    map.addLayer({
      id: 'route-node-label', type: 'symbol', source: 'route', filter: ['==', ['get', 't'], 'node'],
      layout: { 'text-field': ['get', 'label'], 'text-font': ['Noto Sans Bold'], 'text-size': 12, 'text-anchor': 'left', 'text-offset': [1, 0], 'text-allow-overlap': true },
      paint: { 'text-color': '#ffffff', 'text-halo-color': p.route, 'text-halo-width': 3 },
    })
    map.addLayer({ id: 'route-hover', type: 'circle', source: 'route-hover', paint: { 'circle-radius': 8, 'circle-color': '#ffffff', 'circle-stroke-color': p.route, 'circle-stroke-width': 3.5 } })
    ready++
  }

  // along-track distance of the profile sample nearest a map position
  function alongTrack(lng: number, lat: number) {
    let best = null as null | { d: number; e: number }
    const k = Math.cos((lat * Math.PI) / 180)
    for (const s of plan.samples) {
      const e = (s.lat - lat) ** 2 + ((s.lon - lng) * k) ** 2
      if (!best || e < best.e) best = { d: s.d, e }
    }
    return best?.d ?? null
  }

  // hover marker, shared with the profile
  $effect(() => {
    ready
    const h = plan.hover
    let fc: GeoJSON.FeatureCollection | GeoJSON.Feature = empty
    if (h && plan.samples.length) {
      let s = plan.samples[0]
      for (const x of plan.samples) if (Math.abs(x.d - h.d) < Math.abs(s.d - h.d)) s = x
      fc = { type: 'Feature', properties: {}, geometry: { type: 'Point', coordinates: [s.lon, s.lat] } }
    }
    ;(map?.getSource('route-hover') as GeoJSONSource | undefined)?.setData(fc)
  })

  // airspace picked in the profile: outline it and bring it into view
  $effect(() => {
    ready
    const c = plan.picked
    const src = map?.getSource('as-pick') as GeoJSONSource | undefined
    if (!src) return
    if (!c?.geom?.length) {
      src.setData(empty)
      return
    }
    src.setData({ type: 'Feature', properties: {}, geometry: { type: 'MultiPolygon', coordinates: c.geom } })
    const pts = c.geom.flatMap((poly) => poly[0] ?? [])
    if (pts.length) {
      const lons = pts.map((p) => p[0])
      const lats = pts.map((p) => p[1])
      map!.fitBounds([[Math.min(...lons), Math.min(...lats)], [Math.max(...lons), Math.max(...lats)]], { padding: { top: 80, left: 120, right: 80, bottom: app.profileOpen ? 320 : 80 }, maxZoom: 11, duration: 600 })
    }
  })

  function features(): GeoJSON.FeatureCollection {
    const w = route.waypoints
    const feats: GeoJSON.Feature[] = []
    if (w.length >= 2) feats.push({ type: 'Feature', properties: { t: 'line' }, geometry: { type: 'LineString', coordinates: greatCircle(route.path, 5) } })
    w.forEach((p, i) => feats.push({ type: 'Feature', properties: { t: 'node', i, label: p.fuelStop ? `${p.id} · fuel stop` : p.id }, geometry: { type: 'Point', coordinates: [p.lon, p.lat] } }))
    for (let i = 1; i < w.length; i++) {
      const a = w[i - 1]
      const b = w[i]
      const d = distanceNm(a.lat, a.lon, b.lat, b.lon)
      if (d < 3) continue
      const [mla, mlo] = interpolate(a.lat, a.lon, b.lat, b.lon, 0.5)
      const tc = bearing(mla, mlo, b.lat, b.lon)
      const mc = tc - variation(mla, mlo)
      // keep the label upright: rotate along the line but never upside down
      let rot = tc - 90
      if (rot > 90) rot -= 180
      if (rot < -90) rot += 180
      feats.push({ type: 'Feature', properties: { t: 'leg', label: `${deg(mc)}°  ${Math.round(d)} NM`, rot }, geometry: { type: 'Point', coordinates: [mlo, mla] } })
    }
    const log = plan.log
    if (log && w.length >= 2) {
      for (const [d, label] of [[log.tocDist, 'TOC'], [log.todDist, 'TOD']] as const) {
        if (d == null || d <= 0.5 || d >= log.totalDist - 0.5) continue
        feats.push({ type: 'Feature', properties: { t: 'mark', label }, geometry: { type: 'Point', coordinates: pointAlong(route.path, d) } })
      }
    }
    return { type: 'FeatureCollection', features: feats }
  }

  $effect(() => {
    ready
    const fc = features()
    ;(map?.getSource('route') as GeoJSONSource | undefined)?.setData(fc)
  })

  const snap = (e: Parameters<typeof snapPlace>[1]) => snapPlace(map!, e)

  function setDragLine(lngLat: [number, number] | null) {
    const src = map?.getSource('route-drag') as GeoJSONSource | undefined
    if (!src) return
    if (!drag || !lngLat) return src.setData(empty)
    const w = route.waypoints
    const coords: [number, number][] = []
    if (drag.mode === 'move') {
      if (w[drag.index - 1]) coords.push([w[drag.index - 1].lon, w[drag.index - 1].lat])
      coords.push(lngLat)
      if (w[drag.index + 1]) coords.push([w[drag.index + 1].lon, w[drag.index + 1].lat])
    } else {
      coords.push([w[drag.index - 1].lon, w[drag.index - 1].lat], lngLat, [w[drag.index].lon, w[drag.index].lat])
    }
    src.setData({ type: 'Feature', properties: {}, geometry: { type: 'LineString', coordinates: coords } })
  }

  function legIndexAt(lng: number, lat: number) {
    const w = route.waypoints
    let best = 1
    let cost = Infinity
    for (let i = 1; i < w.length; i++) {
      const c = distanceNm(w[i - 1].lat, w[i - 1].lon, lat, lng) + distanceNm(lat, lng, w[i].lat, w[i].lon) - distanceNm(w[i - 1].lat, w[i - 1].lon, w[i].lat, w[i].lon)
      if (c < cost) {
        cost = c
        best = i
      }
    }
    return best
  }

  onMount(() => {
    const wait = setInterval(() => {
      if (!mapRef.map) return
      clearInterval(wait)
      map = mapRef.map
      if (map.isStyleLoaded()) install()
      map.on('style.load', () => install())
      map.on('styledata', () => install())

      map.on('mousedown', 'route-node', (e) => {
        if (route.locked || ruler.on) return
        e.preventDefault()
        const i = (e.features?.[0]?.properties as any)?.i
        if (i == null) return
        drag = { mode: 'move', index: i, lngLat: [e.lngLat.lng, e.lngLat.lat] }
        map!.dragPan.disable()
      })
      map.on('mousedown', 'route-line', (e) => {
        if (route.locked || ruler.on || drag || map!.queryRenderedFeatures(e.point, { layers: ['route-node'] }).length) return
        e.preventDefault()
        drag = { mode: 'insert', index: legIndexAt(e.lngLat.lng, e.lngLat.lat), lngLat: [e.lngLat.lng, e.lngLat.lat] }
        map!.dragPan.disable()
      })
      map.on('mousemove', (e) => {
        if (!drag) return
        drag.lngLat = [e.lngLat.lng, e.lngLat.lat]
        setDragLine(drag.lngLat)
        map!.getCanvas().style.cursor = 'grabbing'
      })
      map.on('mouseup', (e) => {
        if (!drag) return
        const d = drag
        drag = null
        map!.dragPan.enable()
        setDragLine(null)
        map!.getCanvas().style.cursor = ''
        const moved = map!.project(d.lngLat as any)
        const start = map!.project(d.mode === 'move' ? [route.waypoints[d.index].lon, route.waypoints[d.index].lat] : (d.lngLat as any))
        const place = snap(e)
        if (d.mode === 'move') {
          if (Math.hypot(moved.x - start.x, moved.y - start.y) < 3 && distanceNm(place.lat, place.lon, route.waypoints[d.index].lat, route.waypoints[d.index].lon) < 0.2) return
          const uid = route.waypoints[d.index].uid
          route.waypoints[d.index] = { ...place, uid }
        } else {
          route.add(place, d.index)
        }
        mapRef.suppressClick = true
      })
      map.on('mouseenter', 'route-node', () => !route.locked && !ruler.on && (map!.getCanvas().style.cursor = 'grab'))
      map.on('mouseleave', 'route-node', () => (map!.getCanvas().style.cursor = ''))
      map.on('mouseenter', 'route-line', () => !route.locked && !ruler.on && (map!.getCanvas().style.cursor = 'copy'))
      map.on('mouseleave', 'route-line', () => {
        map!.getCanvas().style.cursor = ''
        if (plan.hover?.from === 'map') plan.hover = null
      })
      map.on('mousemove', 'route-line', (e) => {
        if (drag) return
        const d = alongTrack(e.lngLat.lng, e.lngLat.lat)
        if (d != null) plan.hover = { d, from: 'map' }
      })

      mapRef.onMapClick = (e: MapMouseEvent) => {
        if (mapRef.suppressClick) {
          mapRef.suppressClick = false
          return true
        }
        if (ruler.on) {
          ruler.add(snap(e))
          return true
        }
        if (route.editing && !route.locked) {
          route.add(snap(e))
          return true
        }
        // a tap on the route line (touch screens have no hover) marks that spot on the profile
        const r = 10
        const onLine = map!.queryRenderedFeatures([[e.point.x - r, e.point.y - r], [e.point.x + r, e.point.y + r]], { layers: ['route-line'] }).length
        if (onLine) {
          const d = alongTrack(e.lngLat.lng, e.lngLat.lat)
          if (d != null) {
            plan.hover = { d, from: 'map' }
            app.profileOpen = true
            return true
          }
        }
        return false
      }
    }, 50)
    return () => clearInterval(wait)
  })
</script>
