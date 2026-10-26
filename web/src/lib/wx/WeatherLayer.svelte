<script lang="ts" module>
  // Weather overlays on the chart. Toggled from the Weather panel; state persists per viewer.
  // The radar and the SPC outlook follow the timeline's departure time: live NEXRAD now, HRRR
  // forecast radar to about 48 h, then the WPC forecast charts (wx/radar.svelte.ts). Winds aloft
  // are the NWS FB forecast at its stations for that time.
  import { save } from '../stores/app.svelte'
  import type { WindPoint } from '../api'
  import type { Frame } from './radar.svelte'

  /** Levels of the NWS winds-aloft forecast offered on the chart: [ft, label]. */
  export const WIND_LEVELS: [number, string][] = [[3000, '3,000 ft'], [6000, '6,000 ft'], [9000, '9,000 ft'], [12000, '12,000 ft'], [18000, 'FL180'], [24000, 'FL240'], [30000, 'FL300'], [34000, 'FL340'], [39000, 'FL390']]

  /** Wind speed colour, light blue (calm) through green and amber to red (80 kt+). */
  export function speedColor(kt: number) {
    const stops: [number, [number, number, number]][] = [[0, [90, 160, 210]], [15, [60, 180, 120]], [30, [230, 180, 40]], [50, [230, 110, 40]], [80, [200, 40, 80]]]
    for (let i = 1; i < stops.length; i++) {
      const [a, ca] = stops[i - 1]
      const [b, cb] = stops[i]
      if (kt <= b) {
        const f = Math.min(1, Math.max(0, (kt - a) / (b - a)))
        const c = ca.map((x, k) => Math.round(x + (cb[k] - x) * f))
        return `rgb(${c[0]},${c[1]},${c[2]})`
      }
    }
    return 'rgb(200,40,80)'
  }

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
    show = $state(load('wx', { radar: false, metars: true, tfr: true, sigmet: true, gairmet: false, pireps: false, spc: false, winds: false }))
    radarOpacity = $state(0.65)
    /** index into WIND_LEVELS */
    windLevel = $state<number>(load('wxLevel', { i: 2 }).i)
    /** what the radar layer shows for the departure time (null: nothing covers it) */
    frame = $state.raw<Frame | null>(null)
    radarLoading = $state(false)
    /** every FB station */
    fb = $state.raw<(WindPoint & { station: string })[]>([])
    tfrs = $state.raw<GeoJSON.FeatureCollection>({ type: 'FeatureCollection', features: [] })
    sigmets = $state.raw<any[]>([])
    gairmets = $state.raw<any[]>([])
    gairmetHour = $state(0)
    metarCount = $state(0)
    updated = $state<Date | null>(null)
  }
  export const wx = new WxState()
  $effect.root(() => {
    $effect(() => save('wx', wx.show))
    $effect(() => save('wxLevel', { i: wx.windLevel }))
  })
</script>

<script lang="ts">
  import type { GeoJSONSource, Map as MLMap } from 'maplibre-gl'
  import { onMount } from 'svelte'
  import { api } from '../api'
  import { mapRef } from '../chart/mapRef'
  import { FLT_CAT_HEX } from './format'
  import { frameFor, products, spcFrame } from './radar.svelte'
  import { plan } from '../stores/plan.svelte'
  import { app } from '../stores/app.svelte'

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
    const below = map.getLayer('as-E-1200') ? 'as-E-1200' : before
    map.addSource('wx-spc', { type: 'raster', tileSize: 256, attribution: 'NOAA SPC', tiles: [spcFrame(plan.departureTime)?.tiles ?? 'about:blank'] })
    map.addLayer({ id: 'wx-spc', type: 'raster', source: 'wx-spc', paint: { 'raster-opacity': 0.55, 'raster-fade-duration': 0 } }, below)
    radarKey = ''
    addRadar(below)
    for (const id of ['wx-tfr', 'wx-sigmet', 'wx-gairmet', 'wx-pirep', 'wx-metar', 'wx-winds']) map.addSource(id, { type: 'geojson', data: EMPTY })
    addBarbs()
    map.addLayer({
      id: 'wx-winds', type: 'symbol', source: 'wx-winds',
      layout: {
        'icon-image': ['get', 'icon'], 'icon-rotate': ['get', 'dir'], 'icon-rotation-alignment': 'map', 'icon-allow-overlap': true, 'icon-anchor': 'bottom',
        'text-field': ['get', 'label'], 'text-font': ['Noto Sans Bold'], 'text-size': 10, 'text-anchor': 'top-left', 'text-offset': [0.5, 0.3], 'text-optional': true,
      },
      paint: { 'text-color': app.theme === 'night' ? '#dce2ea' : '#1e242e', 'text-halo-color': app.theme === 'night' ? '#161b22' : '#fff', 'text-halo-width': 1.2 },
    })
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

  let radarKey = ''
  let radarTimer: ReturnType<typeof setTimeout>

  /** (Re)create the radar source for the current frame. */
  function addRadar(beforeId?: string) {
    if (!map) return
    const f = wx.frame
    const key = f?.key ?? ''
    if (key === radarKey && map.getSource('wx-radar')) return
    radarKey = key
    const before = beforeId ?? (map.getLayer('wx-spc') ? 'wx-spc' : undefined)
    if (map.getLayer('wx-radar')) map.removeLayer('wx-radar')
    if (map.getSource('wx-radar')) map.removeSource('wx-radar')
    if (!f) return
    const attribution = f.kind === 'observed' || f.kind === 'hrrr' ? 'Radar: Iowa Environmental Mesonet' : 'NOAA WPC'
    map.addSource('wx-radar', { type: 'raster', tileSize: 256, attribution, tiles: [f.tiles] })
    map.addLayer({ id: 'wx-radar', type: 'raster', source: 'wx-radar', layout: { visibility: wx.show.radar ? 'visible' : 'none' }, paint: { 'raster-opacity': wx.radarOpacity, 'raster-fade-duration': 0 } }, before)
    if (wx.show.radar) {
      wx.radarLoading = true
      clearTimeout(radarTimer)
      radarTimer = setTimeout(() => (wx.radarLoading = false), 10_000)
      map.once('idle', () => {
        clearTimeout(radarTimer)
        wx.radarLoading = false
      })
    }
  }

  /** Wind barb images, one per 5 kt, coloured by speed: the shaft points where the wind comes
   *  from (up = north before rotation), barbs on the left. */
  function addBarbs() {
    if (!map || map.hasImage('barb-0')) return
    const r = window.devicePixelRatio || 1
    const W = 30
    const H = 34
    for (let kt = 0; kt <= 160; kt += 5) {
      const c = document.createElement('canvas')
      c.width = W * r
      c.height = H * r
      const g = c.getContext('2d')!
      g.scale(r, r)
      const color = speedColor(kt)
      g.strokeStyle = color
      g.fillStyle = color
      g.lineWidth = 2
      g.lineCap = 'round'
      const x = W / 2
      const base = H - 3
      if (kt < 5) {
        g.beginPath()
        g.arc(x, base - 5, 5, 0, Math.PI * 2)
        g.stroke()
      } else {
        const tip = base - 28
        g.beginPath()
        g.moveTo(x, base)
        g.lineTo(x, tip)
        g.stroke()
        let left = kt
        let at = 0
        while (left >= 50) {
          g.beginPath()
          g.moveTo(x, tip + at)
          g.lineTo(x, tip + at + 6)
          g.lineTo(x - 11, tip + at)
          g.fill()
          at += 8
          left -= 50
        }
        while (left >= 10) {
          g.beginPath()
          g.moveTo(x, tip + at)
          g.lineTo(x - 11, tip + at - 3)
          g.stroke()
          at += 4.5
          left -= 10
        }
        if (left >= 5) {
          const y = tip + Math.max(at, 4)
          g.beginPath()
          g.moveTo(x, y)
          g.lineTo(x - 6, y - 1.5)
          g.stroke()
        }
      }
      g.beginPath()
      g.arc(x, base, 2.5, 0, Math.PI * 2)
      g.fill()
      map.addImage(`barb-${kt}`, g.getImageData(0, 0, W * r, H * r), { pixelRatio: r })
    }
  }

  // the frame for the departure time
  $effect(() => {
    products.hrrrShort
    products.hrrrExtended
    products.wpcDay1End
    wx.frame = frameFor(plan.departureTime)
  })
  $effect(() => {
    ready
    wx.frame
    wx.show.radar
    addRadar()
  })
  $effect(() => {
    ready
    products.spcDay1End
    const f = spcFrame(plan.departureTime)
    const src = map?.getSource('wx-spc') as { setTiles?: (t: string[]) => void } | undefined
    if (f && src?.setTiles) src.setTiles([f.tiles])
    vis(['wx-spc'], wx.show.spc && !!f)
  })

  // winds aloft at the chosen level, from the bulletin whose "for use" window covers the time
  let fbAt = 0
  $effect(() => {
    if (!wx.show.winds || Date.now() - fbAt < 1800_000) return
    fbAt = Date.now()
    api.fbPoints().then((v) => (wx.fb = v.points)).catch(() => (fbAt = 0))
  })
  $effect(() => {
    ready
    const level = WIND_LEVELS[Math.min(wx.windLevel, WIND_LEVELS.length - 1)][0]
    const t = plan.departureTime.getTime() / 1000
    const feats: GeoJSON.Feature[] = []
    for (const p of wx.fb) {
      const l = p.levels.find((x) => x.alt === level)
      const best = l?.times.find((x) => (x.from == null || t >= x.from) && (x.to == null || t < x.to))
      if (!best) continue
      const kt = Math.min(160, Math.round(best.spd / 5) * 5)
      const label = `${String(Math.round(best.dir)).padStart(3, '0')}/${Math.round(best.spd)}${best.temp != null ? ` ${best.temp >= 0 ? '+' : ''}${Math.round(best.temp)}°` : ''}`
      feats.push({ type: 'Feature', geometry: { type: 'Point', coordinates: [p.lon, p.lat] }, properties: { icon: `barb-${kt}`, dir: best.dir, label, station: p.station } })
    }
    setData('wx-winds', { type: 'FeatureCollection', features: feats })
  })

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
    vis(['wx-winds'], wx.show.winds)
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
