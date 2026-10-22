<script lang="ts" module>
  // Weather overlays on the chart. Toggled from the Weather panel; state persists per viewer.
  import { save } from '../stores/app.svelte'

  function load<T extends object>(key: string, fallback: T): T {
    try {
      const raw = localStorage.getItem(`fp.${key}`)
      if (raw) return { ...fallback, ...JSON.parse(raw) }
    } catch {
      /* ignore */
    }
    return fallback
  }

  class WxState {
    show = $state(load('wx', { radar: false, metars: true, tfr: true, sigmet: true, gairmet: false, pireps: false }))
    radarOpacity = $state(0.65)
    tfrs = $state<GeoJSON.FeatureCollection>({ type: 'FeatureCollection', features: [] })
    sigmets = $state<any[]>([])
    gairmets = $state<any[]>([])
    gairmetHour = $state(0)
    metarCount = $state(0)
    updated = $state<Date | null>(null)
  }
  export const wx = new WxState()
  $effect.root(() => {
    $effect(() => save('wx', wx.show))
  })
</script>

<script lang="ts">
  import type { GeoJSONSource, Map as MLMap } from 'maplibre-gl'
  import { onMount } from 'svelte'
  import { api } from '../api'
  import { mapRef } from '../chart/mapRef'
  import { FLT_CAT_HEX } from './format'

  let map: MLMap | undefined
  let ready = $state(0)
  const EMPTY: GeoJSON.FeatureCollection = { type: 'FeatureCollection', features: [] }

  const HAZARD_COLOR: Record<string, string> = {
    CONVECTIVE: '#e0452b', TS: '#e0452b', TURB: '#d48a0b', 'TURB-HI': '#d48a0b', 'TURB-LO': '#e0a23a', ICE: '#2f8fd6',
    IFR: '#7b4fc9', MT_OBSC: '#a0613a', 'MTN OBSCN': '#a0613a', MTN: '#a0613a', SFC_WND: '#5a8f3c', LLWS: '#c0397a', FZLVL: '#3a8fd6', M_FZLVL: '#3a8fd6', ASH: '#777',
  }

  function install() {
    if (!map || map.getSource('wx-metar')) return
    const before = map.getLayer('route-casing') ? 'route-casing' : map.getLayer('as-labels-e') ? 'as-labels-e' : undefined
    map.addSource('wx-radar', {
      type: 'raster', tileSize: 256, attribution: 'Radar: Iowa Environmental Mesonet',
      tiles: ['https://mesonet.agron.iastate.edu/cache/tile.py/1.0.0/nexrad-n0q-900913/{z}/{x}/{y}.png'],
    })
    map.addLayer({ id: 'wx-radar', type: 'raster', source: 'wx-radar', paint: { 'raster-opacity': wx.radarOpacity, 'raster-fade-duration': 0 } }, map.getLayer('as-E-1200') ? 'as-E-1200' : before)
    for (const id of ['wx-tfr', 'wx-sigmet', 'wx-gairmet', 'wx-pirep', 'wx-metar']) map.addSource(id, { type: 'geojson', data: EMPTY })
    map.addLayer({ id: 'wx-gairmet-fill', type: 'fill', source: 'wx-gairmet', filter: ['==', ['geometry-type'], 'Polygon'], paint: { 'fill-color': ['get', 'color'], 'fill-opacity': 0.12 } }, before)
    map.addLayer({ id: 'wx-gairmet-line', type: 'line', source: 'wx-gairmet', paint: { 'line-color': ['get', 'color'], 'line-width': 1.6, 'line-dasharray': [3, 2] } }, before)
    map.addLayer({ id: 'wx-gairmet-label', type: 'symbol', source: 'wx-gairmet', layout: { 'text-field': ['get', 'label'], 'text-font': ['Noto Sans Bold'], 'text-size': 11, 'symbol-placement': 'line', 'symbol-spacing': 300 }, paint: { 'text-color': ['get', 'color'], 'text-halo-color': '#fff', 'text-halo-width': 1.5 } })
    map.addLayer({ id: 'wx-sigmet-fill', type: 'fill', source: 'wx-sigmet', paint: { 'fill-color': ['get', 'color'], 'fill-opacity': 0.14 } }, before)
    map.addLayer({ id: 'wx-sigmet-line', type: 'line', source: 'wx-sigmet', paint: { 'line-color': ['get', 'color'], 'line-width': 2 } }, before)
    map.addLayer({ id: 'wx-sigmet-label', type: 'symbol', source: 'wx-sigmet', layout: { 'text-field': ['get', 'label'], 'text-font': ['Noto Sans Bold'], 'text-size': 11, 'symbol-placement': 'line', 'symbol-spacing': 400 }, paint: { 'text-color': ['get', 'color'], 'text-halo-color': '#fff', 'text-halo-width': 1.5 } })
    map.addLayer({ id: 'wx-tfr-fill', type: 'fill', source: 'wx-tfr', paint: { 'fill-color': '#d23a3a', 'fill-opacity': 0.16 } }, before)
    map.addLayer({ id: 'wx-tfr-line', type: 'line', source: 'wx-tfr', paint: { 'line-color': '#d23a3a', 'line-width': 2.2, 'line-dasharray': [2, 1] } }, before)
    map.addLayer({ id: 'wx-tfr-label', type: 'symbol', source: 'wx-tfr', minzoom: 6, layout: { 'text-field': ['concat', 'TFR ', ['get', 'notam']], 'text-font': ['Noto Sans Bold'], 'text-size': 11 }, paint: { 'text-color': '#d23a3a', 'text-halo-color': '#fff', 'text-halo-width': 1.6 } })
    map.addLayer({
      id: 'wx-pirep', type: 'circle', source: 'wx-pirep',
      paint: { 'circle-radius': 5, 'circle-color': ['get', 'color'], 'circle-stroke-color': '#fff', 'circle-stroke-width': 1.5 },
    })
    map.addLayer({
      id: 'wx-pirep-label', type: 'symbol', source: 'wx-pirep', minzoom: 6,
      layout: { 'text-field': ['get', 'label'], 'text-font': ['Noto Sans Bold'], 'text-size': 10, 'text-offset': [0, 1.1], 'text-anchor': 'top' },
      paint: { 'text-color': ['get', 'color'], 'text-halo-color': '#fff', 'text-halo-width': 1.5 },
    })
    map.addLayer({
      id: 'wx-metar', type: 'circle', source: 'wx-metar',
      paint: {
        'circle-radius': ['interpolate', ['linear'], ['zoom'], 4, 3.5, 9, 7],
        'circle-color': ['get', 'color'], 'circle-stroke-color': '#ffffff', 'circle-stroke-width': 1.5,
        'circle-translate': [0, 0],
      },
    })
    map.addLayer({
      id: 'wx-metar-label', type: 'symbol', source: 'wx-metar', minzoom: 8,
      layout: { 'text-field': ['get', 'label'], 'text-font': ['Noto Sans Bold'], 'text-size': 10, 'text-anchor': 'bottom-left', 'text-offset': [0.6, -0.4] },
      paint: { 'text-color': ['get', 'color'], 'text-halo-color': '#fff', 'text-halo-width': 1.5 },
    })
    ready++
  }

  function vis(ids: string[], on: boolean) {
    for (const id of ids) if (map?.getLayer(id)) map.setLayoutProperty(id, 'visibility', on ? 'visible' : 'none')
  }

  $effect(() => {
    ready
    vis(['wx-radar'], wx.show.radar)
    vis(['wx-metar', 'wx-metar-label'], wx.show.metars)
    vis(['wx-tfr-fill', 'wx-tfr-line', 'wx-tfr-label'], wx.show.tfr)
    vis(['wx-sigmet-fill', 'wx-sigmet-line', 'wx-sigmet-label'], wx.show.sigmet)
    vis(['wx-gairmet-fill', 'wx-gairmet-line', 'wx-gairmet-label'], wx.show.gairmet)
    vis(['wx-pirep', 'wx-pirep-label'], wx.show.pireps)
  })
  $effect(() => {
    ready
    if (map?.getLayer('wx-radar')) map.setPaintProperty('wx-radar', 'raster-opacity', wx.radarOpacity)
  })

  function setData(id: string, fc: GeoJSON.FeatureCollection) {
    ;(map?.getSource(id) as GeoJSONSource | undefined)?.setData(fc)
  }

  const ring = (coords: { lat: number | string; lon: number | string }[]) => coords.map((c) => [Number(c.lon), Number(c.lat)])

  async function loadAdvisories() {
    try {
      wx.tfrs = await api.tfr()
    } catch {
      /* keep old */
    }
    try {
      wx.sigmets = await api.airsigmet()
    } catch {}
    try {
      wx.gairmets = await api.gairmet()
    } catch {}
    wx.updated = new Date()
  }

  $effect(() => {
    ready
    setData('wx-tfr', wx.tfrs)
  })
  $effect(() => {
    ready
    const feats: GeoJSON.Feature[] = []
    for (const [idx, s] of wx.sigmets.entries()) {
      if (!s.coords?.length) continue
      const r = ring(s.coords)
      if (r.length < 3) continue
      const color = HAZARD_COLOR[s.hazard] ?? '#888'
      const top = s.altitudeHi1 ? `${s.altitudeHi1 >= 18000 ? 'FL' + s.altitudeHi1 / 100 : s.altitudeHi1}` : ''
      const bot = s.altitudeLow1 ? `${s.altitudeLow1}` : 'SFC'
      feats.push({
        type: 'Feature', geometry: { type: 'Polygon', coordinates: [[...r, r[0]]] },
        properties: { color, idx, label: `${s.airSigmetType} ${s.hazard}${top ? ` ${bot}–${top}` : ''}`, raw: s.rawAirSigmet, kind: 'sigmet' },
      })
    }
    setData('wx-sigmet', { type: 'FeatureCollection', features: feats })
  })
  $effect(() => {
    ready
    const feats: GeoJSON.Feature[] = []
    const hours = [...new Set(wx.gairmets.map((g) => g.forecastHour))].sort((a, b) => a - b)
    const hr = hours.includes(wx.gairmetHour) ? wx.gairmetHour : hours[0]
    for (const [idx, g] of wx.gairmets.entries()) {
      if (g.forecastHour !== hr || !g.coords?.length) continue
      const r = ring(g.coords)
      const color = HAZARD_COLOR[g.hazard] ?? '#888'
      const label = g.hazard === 'FZLVL' || g.hazard === 'M_FZLVL' ? `0°C ${g.level ? +g.level * 100 : ''}` : `${g.hazard.replace('_', ' ')}${g.top ? ` ${g.base ?? 'SFC'}–${g.top}` : ''}`
      if (g.geometryType === 'LINE' || g.geom === 'LINE') feats.push({ type: 'Feature', geometry: { type: 'LineString', coordinates: r }, properties: { color, label, idx } })
      else if (r.length >= 3) feats.push({ type: 'Feature', geometry: { type: 'Polygon', coordinates: [[...r, r[0]]] }, properties: { color, label, idx } })
    }
    setData('wx-gairmet', { type: 'FeatureCollection', features: feats })
  })

  let metarTimer: ReturnType<typeof setTimeout>
  async function loadViewData() {
    if (!map) return
    const b = map.getBounds()
    const z = map.getZoom()
    const box: [number, number, number, number] = [b.getSouth(), b.getWest(), b.getNorth(), b.getEast()]
    if (wx.show.metars && z >= 4.5) {
      try {
        const list = await api.metarBox(box)
        wx.metarCount = list.length
        setData('wx-metar', {
          type: 'FeatureCollection',
          features: list.filter((m) => m.lat && m.lon).map((m) => ({
            type: 'Feature', geometry: { type: 'Point', coordinates: [m.lon, m.lat] },
            properties: {
              id: m.icaoId, cat: m.fltCat ?? '', color: FLT_CAT_HEX[m.fltCat] ?? '#8b919c', raw: m.rawOb,
              label: `${m.icaoId}${m.wspd != null ? ` ${m.wdir === 'VRB' ? 'VRB' : String(m.wdir).padStart(3, '0')}@${m.wspd}${m.wgst ? 'G' + m.wgst : ''}` : ''}`,
            },
          })),
        })
      } catch {
        /* transient */
      }
    }
    if (wx.show.pireps && z >= 4) {
      try {
        const list = await api.pirep(box, 3)
        setData('wx-pirep', {
          type: 'FeatureCollection',
          features: list.filter((p) => p.lat && p.lon).map((p) => {
            const ice = p.icgInt1 && p.icgInt1 !== 'NEG'
            const turb = p.tbInt1 && p.tbInt1 !== 'NEG'
            const severe = /SEV|EXTRM/.test(`${p.icgInt1} ${p.tbInt1}`)
            const color = severe ? '#d23a3a' : ice ? '#2f8fd6' : turb ? '#d48a0b' : '#5c6370'
            const what = [turb && `TB ${p.tbInt1}`, ice && `IC ${p.icgInt1}`].filter(Boolean).join(' ')
            return { type: 'Feature', geometry: { type: 'Point', coordinates: [p.lon, p.lat] }, properties: { color, raw: p.rawOb, label: `${p.pirepType === 'AIREP' ? '' : 'UA '}FL${String(p.fltLvl ?? '').padStart(3, '0')} ${what}`.trim() } }
          }),
        })
      } catch {}
    }
  }

  $effect(() => {
    wx.show.metars
    wx.show.pireps
    ready
    loadViewData()
  })

  onMount(() => {
    const wait = setInterval(() => {
      if (!mapRef.map) return
      clearInterval(wait)
      map = mapRef.map
      if (map.isStyleLoaded()) install()
      map.on('styledata', () => {
        if (!map!.getSource('wx-metar')) {
          install()
          loadViewData()
        }
      })
      map.on('moveend', () => {
        clearTimeout(metarTimer)
        metarTimer = setTimeout(loadViewData, 400)
      })
      loadAdvisories()
    }, 50)
    const refresh = setInterval(() => {
      loadAdvisories()
      loadViewData()
    }, 5 * 60 * 1000)
    return () => {
      clearInterval(wait)
      clearInterval(refresh)
    }
  })
</script>
