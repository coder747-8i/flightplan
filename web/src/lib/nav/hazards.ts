// Hazard advisor: scans the planned flight for terrain, obstacles, airspace, TFRs, weather
// advisories, fuel, weight & balance, night arrival, density altitude and crosswind.
import type { ProfileObstacle, ProfileSample } from '../api'
import type { Aircraft } from '../stores/aircraft.svelte'
import { computeWB } from '../stores/aircraft.svelte'
import type { Crossing } from '../stores/plan.svelte'
import type { Waypoint } from '../stores/route.svelte'
import type { NavLog } from './navlog'
import { sunTimes } from './sun'

export type Level = 'warning' | 'caution' | 'info'
export interface Hazard {
  level: Level
  category: 'Terrain' | 'Obstacle' | 'Airspace' | 'TFR' | 'Weather' | 'Fuel' | 'Weight & balance' | 'Daylight' | 'Performance'
  title: string
  detail?: string
  at?: number // along-track NM
}

export interface HazardInput {
  waypoints: Waypoint[]
  samples: ProfileSample[]
  obstacles: ProfileObstacle[]
  airspace: Crossing[]
  altitudePath: [number, number][]
  log: NavLog | null
  aircraft?: Aircraft
  departure: Date
  tfrs: GeoJSON.FeatureCollection
  sigmets: any[]
  gairmets: any[]
  metars: Record<string, any>
  runways: Record<string, { id: string; hdg: number }[]>
}

function plannedAlt(path: [number, number][], d: number) {
  for (let i = 1; i < path.length; i++) {
    if (d <= path[i][0]) {
      const f = (d - path[i - 1][0]) / Math.max(path[i][0] - path[i - 1][0], 1e-6)
      return path[i - 1][1] + (path[i][1] - path[i - 1][1]) * f
    }
  }
  return path[path.length - 1]?.[1] ?? 0
}

function inRing(x: number, y: number, ring: number[][]) {
  let inside = false
  for (let i = 0, j = ring.length - 1; i < ring.length; j = i++) {
    const [xi, yi] = ring[i]
    const [xj, yj] = ring[j]
    if (yi > y !== yj > y && x < ((xj - xi) * (y - yi)) / (yj - yi) + xi) inside = !inside
  }
  return inside
}
function inGeom(lon: number, lat: number, g: GeoJSON.Geometry) {
  const polys = g.type === 'Polygon' ? [g.coordinates] : g.type === 'MultiPolygon' ? g.coordinates : []
  return polys.some((p) => inRing(lon, lat, p[0] as number[][]))
}

const fmtAlt = (v: number) => (v >= 99999 ? 'unlimited' : v === 17999 ? 'FL180' : v >= 18000 ? `FL${Math.round(v / 100)}` : v === 0 ? 'surface' : `${v.toLocaleString()} ft`)
const cap = (t: string) => t.charAt(0).toUpperCase() + t.slice(1)

export function analyze(h: HazardInput): Hazard[] {
  const out: Hazard[] = []
  const total = h.samples.length ? h.samples[h.samples.length - 1].d : 0
  if (!h.log || h.waypoints.length < 2) return out

  // ---- terrain ----
  let worst: { clr: number; d: number; elev: number } | null = null
  for (const s of h.samples) {
    if (s.d < 5 || s.d > total - 5) continue
    const clr = plannedAlt(h.altitudePath, s.d) - s.max
    if (!worst || clr < worst.clr) worst = { clr, d: s.d, elev: s.max }
  }
  if (worst && worst.clr < 0) out.push({ level: 'warning', category: 'Terrain', title: `Planned altitude is below terrain at ${worst.d.toFixed(0)} NM`, detail: `Terrain within 1 NM rises to ${Math.round(worst.elev).toLocaleString()} ft.`, at: worst.d })
  else if (worst && worst.clr < 500) out.push({ level: 'warning', category: 'Terrain', title: `Only ${Math.round(worst.clr)} ft above terrain at ${worst.d.toFixed(0)} NM`, detail: `Highest terrain within 1 NM: ${Math.round(worst.elev).toLocaleString()} ft.`, at: worst.d })
  else if (worst && worst.clr < 1000) out.push({ level: 'caution', category: 'Terrain', title: `Less than 1,000 ft terrain clearance at ${worst.d.toFixed(0)} NM`, detail: `${Math.round(worst.clr)} ft above the highest terrain within 1 NM.`, at: worst.d })

  // ---- obstacles ----
  const tall = h.obstacles.filter((o) => o.d > 2 && o.d < total - 2 && plannedAlt(h.altitudePath, o.d) - o.msl < 500)
  if (tall.length) {
    const o = tall.reduce((a, b) => (b.msl > a.msl ? b : a))
    out.push({ level: 'warning', category: 'Obstacle', title: `${tall.length} obstacle${tall.length > 1 ? 's' : ''} within 500 ft of your altitude`, detail: `Tallest: ${o.type} ${o.msl.toLocaleString()} ft MSL (${o.agl} AGL) at ${o.d.toFixed(0)} NM, ${Math.abs(o.xt).toFixed(1)} NM ${o.xt < 0 ? 'left' : 'right'} of course.`, at: o.d })
  }

  // ---- airspace at planned altitude ----
  const seen = new Set<string>()
  for (const c of h.airspace) {
    // check the planned altitude across the part of the route inside the volume
    let hit: number | null = null
    for (const s of h.samples) {
      if (s.d < c.from || s.d > c.to) continue
      const alt = plannedAlt(h.altitudePath, s.d)
      const floor = c.lo_agl ? s.elev + c.lo : c.lo
      if (alt >= floor && alt <= c.hi) {
        hit = s.d
        break
      }
    }
    if (hit == null) continue
    const key = `${c.kind}:${c.name}`
    if (seen.has(key)) continue
    seen.add(key)
    const range = cap(`${c.lo_agl ? `${c.lo} ft AGL` : fmtAlt(c.lo)} to ${fmtAlt(c.hi)}`)
    switch (c.kind) {
      case 'P':
        out.push({ level: 'warning', category: 'Airspace', title: `Route enters prohibited area ${c.name}`, detail: `${range}. Flight is not allowed.`, at: hit })
        break
      case 'R':
        out.push({ level: 'warning', category: 'Airspace', title: `Route enters restricted area ${c.name}`, detail: `${range}${c.times ? `. Times: ${c.times}` : ''}. Check status with ATC/FSS before entering.`, at: hit })
        break
      case 'B':
        out.push({ level: 'caution', category: 'Airspace', title: `Class B: ${c.name.replace(' Class B', '')}`, detail: `${range}. You need an explicit "cleared into the Class B" from ATC.`, at: hit })
        break
      case 'C':
        out.push({ level: 'caution', category: 'Airspace', title: `Class C: ${c.name.replace(' Class C', '')}`, detail: `${range}. Establish two-way radio contact before entering.`, at: hit })
        break
      case 'D':
        out.push({ level: 'info', category: 'Airspace', title: `Class D: ${c.name.replace(' Class D', '')}`, detail: `${range}. Two-way radio contact with the tower required.`, at: hit })
        break
      case 'MOA':
        out.push({ level: 'caution', category: 'Airspace', title: `MOA: ${c.name}`, detail: `${range}${c.times ? `. ${c.times}` : ''}. Military training may be active: ask Flight Service or ATC.`, at: hit })
        break
      case 'W':
      case 'A':
        out.push({ level: 'info', category: 'Airspace', title: `${c.kind === 'W' ? 'Warning' : 'Alert'} area ${c.name}`, detail: range, at: hit })
        break
      case 'MODEC':
        out.push({ level: 'info', category: 'Airspace', title: 'Mode C veil', detail: `${c.name}: ADS-B Out / Mode C transponder required.`, at: hit })
        break
    }
  }

  // ---- TFRs (lateral check; altitudes are in the NOTAM text) ----
  const tfrHits = new Map<string, { f: GeoJSON.Feature; d: number }>()
  for (const f of h.tfrs.features) {
    for (const s of h.samples) {
      if (inGeom(s.lon, s.lat, f.geometry)) {
        const id = (f.properties as any)?.notam
        if (!tfrHits.has(id)) tfrHits.set(id, { f, d: s.d })
        break
      }
    }
  }
  for (const [id, { f, d }] of tfrHits) {
    const p = f.properties as any
    out.push({ level: 'warning', category: 'TFR', title: `Route crosses TFR ${id}${p.type ? ` (${String(p.type).toLowerCase()})` : ''}`, detail: `${p.title ?? ''} Read the NOTAM for altitudes and times.`, at: d })
  }

  // ---- AIRMET / SIGMET / G-AIRMET ----
  const now = Date.now() / 1000
  const etaAt = (d: number) => h.departure.getTime() / 1000 + ((h.log!.totalTime * 60) * d) / Math.max(total, 1)
  for (const sg of h.sigmets) {
    if (!sg.coords?.length) continue
    const ring = sg.coords.map((c: any) => [Number(c.lon), Number(c.lat)])
    const s = h.samples.find((x) => inRing(x.lon, x.lat, ring))
    if (!s) continue
    const t = etaAt(s.d)
    if (sg.validTimeTo && sg.validTimeTo < Math.min(t, now)) continue
    const sev = sg.airSigmetType === 'SIGMET' || sg.hazard === 'CONVECTIVE'
    const alts = sg.altitudeHi1 ? ` up to ${fmtAlt(sg.altitudeHi1)}` : ''
    out.push({ level: sev ? 'warning' : 'caution', category: 'Weather', title: sg.hazard === 'CONVECTIVE' ? 'Convective SIGMET on your route' : `${sg.airSigmetType} for ${String(sg.hazard).toLowerCase()} on your route`, detail: `${alts ? cap(alts.trim()) + '. ' : ''}${(sg.rawAirSigmet ?? '').split('\n').slice(0, 3).join(' ').slice(0, 220)}`, at: s.d })
  }
  const gHits = new Map<string, number>()
  for (const g of h.gairmets) {
    if (!g.coords?.length || g.geometryType === 'LINE' || g.hazard === 'FZLVL') continue
    const ring = g.coords.map((c: any) => [Number(c.lon), Number(c.lat)])
    const s = h.samples.find((x) => inRing(x.lon, x.lat, ring))
    if (s && !gHits.has(g.hazard)) gHits.set(g.hazard, s.d)
  }
  const G_NAME: Record<string, string> = { IFR: 'IFR conditions', MT_OBSC: 'mountain obscuration', 'TURB-HI': 'high-altitude turbulence', 'TURB-LO': 'turbulence below FL180', ICE: 'icing', SFC_WND: 'strong surface wind', LLWS: 'low-level wind shear', M_FZLVL: 'multiple freezing levels' }
  for (const [hz, d] of gHits) {
    if (hz === 'TURB-HI' && Math.max(...h.altitudePath.map((p) => p[1])) < 18000) continue
    out.push({ level: hz === 'IFR' || hz === 'MT_OBSC' || hz === 'ICE' ? 'caution' : 'info', category: 'Weather', title: `G-AIRMET: ${G_NAME[hz] ?? hz} along route`, at: d })
  }

  // ---- destination / departure weather ----
  const ends = [h.waypoints[0], h.waypoints[h.waypoints.length - 1]]
  ends.forEach((w, i) => {
    if (w?.kind !== 'airport') return
    const id = w.id.length === 3 ? `K${w.id}` : w.id
    const m = h.metars[id]
    if (!m) return
    const where = i === 0 ? 'Departure' : 'Destination'
    if (m.fltCat === 'IFR' || m.fltCat === 'LIFR') out.push({ level: 'warning', category: 'Weather', title: `${where} ${id} is ${m.fltCat}`, detail: m.rawOb })
    else if (m.fltCat === 'MVFR') out.push({ level: 'caution', category: 'Weather', title: `${where} ${id} is marginal VFR`, detail: m.rawOb })
    // crosswind
    const rwys = h.runways[w.id] ?? []
    if (m.wspd && typeof m.wdir === 'number' && rwys.length && h.aircraft) {
      const gust = m.wgst ?? m.wspd
      const best = rwys
        .map((r) => {
          const a = ((m.wdir - r.hdg) * Math.PI) / 180
          return { id: r.id, x: Math.abs(gust * Math.sin(a)), head: gust * Math.cos(a) }
        })
        .filter((r) => r.head > -5)
        .sort((a, b) => a.x - b.x)[0]
      const limit = h.aircraft.perf.maxXwind ?? 15
      if (best && best.x > limit) out.push({ level: 'warning', category: 'Weather', title: `${where} crosswind ${Math.round(best.x)} kt on the best runway (${best.id})`, detail: `Above the ${limit} kt demonstrated crosswind for this aircraft. Wind ${m.wdir}° at ${m.wspd}${m.wgst ? ` gusting ${m.wgst}` : ''} kt.` })
      else if (best && best.x > limit * 0.75) out.push({ level: 'caution', category: 'Weather', title: `${where} crosswind ${Math.round(best.x)} kt (runway ${best.id})`, detail: `Wind ${m.wdir}° at ${m.wspd}${m.wgst ? ` gusting ${m.wgst}` : ''} kt.` })
    }
    // density altitude
    if (i === 0 && m.temp != null && m.altim && m.elev != null) {
      const elevFt = m.elev * 3.28084
      const pa = elevFt + (1013.25 - m.altim) * 30
      const isa = 15 - (2 * pa) / 1000
      const da = pa + 120 * (m.temp - isa)
      if (da > 5000) out.push({ level: da > 8000 ? 'warning' : 'caution', category: 'Performance', title: `Density altitude ${Math.round(da / 100) * 100} ft at ${id}`, detail: `Field elevation ${Math.round(elevFt)} ft, ${m.temp}°C. Expect longer takeoff roll and reduced climb. Check the POH charts.` })
    }
  })

  // ---- fuel ----
  const log = h.log
  if (log.fuelAtLanding < 0) out.push({ level: 'warning', category: 'Fuel', title: 'Not enough fuel for this flight', detail: `Trip needs ${log.tripFuel.toFixed(1)} gal plus ${log.taxiFuel} taxi.` })
  else if (log.fuelAtLanding < log.reserveFuel) out.push({ level: 'warning', category: 'Fuel', title: `Below reserve at landing (${log.fuelAtLanding.toFixed(1)} gal)`, detail: `Reserve is ${log.reserveFuel.toFixed(1)} gal (${h.aircraft?.perf.reserveMin} min).` })
  else if (log.fuelAtLanding < log.reserveFuel * 1.5) out.push({ level: 'caution', category: 'Fuel', title: `Thin fuel margin: ${log.fuelAtLanding.toFixed(1)} gal at landing`, detail: `Reserve is ${log.reserveFuel.toFixed(1)} gal.` })

  for (const w of log.warnings) if (!w.startsWith('Fuel')) out.push({ level: 'info', category: 'Performance', title: w })

  // ---- weight & balance ----
  if (h.aircraft) {
    const wb = computeWB(h.aircraft, log.tripFuel)
    for (const p of wb.problems) out.push({ level: 'warning', category: 'Weight & balance', title: p })
  }

  // ---- daylight ----
  const dest = h.waypoints[h.waypoints.length - 1]
  const eta = new Date(h.departure.getTime() + log.totalTime * 60000)
  const st = sunTimes(dest.lat, dest.lon, eta)
  const civil = sunTimes(dest.lat, dest.lon, eta, 96)
  if (st.set && civil.set) {
    const fmt = (d: Date) => d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
    if (eta > civil.set || (civil.rise && eta < civil.rise)) out.push({ level: 'caution', category: 'Daylight', title: `Night arrival (ETA ${fmt(eta)})`, detail: `Civil twilight ends ${fmt(civil.set)} at ${dest.id}. Night currency and lighting rules apply.` })
    else if (eta > st.set) out.push({ level: 'info', category: 'Daylight', title: `Arrival after sunset (${fmt(st.set)})`, detail: `ETA ${fmt(eta)}, before the end of civil twilight (${fmt(civil.set)}).` })
    else if (st.set.getTime() - eta.getTime() < 45 * 60000) out.push({ level: 'info', category: 'Daylight', title: `Arriving within 45 min of sunset (${fmt(st.set)})` })
  }

  const rank = { warning: 0, caution: 1, info: 2 }
  return out.sort((a, b) => rank[a.level] - rank[b.level] || (a.at ?? 0) - (b.at ?? 0))
}
