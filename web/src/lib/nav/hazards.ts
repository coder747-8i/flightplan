// Hazard advisor: scans the planned flight for terrain, obstacles, airspace, TFRs, weather
// advisories, fuel, weight & balance and night arrival. Departure / destination weather is
// judged at ETD / ETA by the go / no-go verdict (nav/verdict.ts).
import type { ProfileObstacle, ProfileSample } from '../api'
import type { Aircraft } from '../stores/aircraft.svelte'
import { computeWB } from '../stores/aircraft.svelte'
import type { Crossing } from '../stores/plan.svelte'
import type { Waypoint } from '../stores/route.svelte'
import type { NavLog } from './navlog'
import { sunTimes } from './sun'
import { describeSigmet } from '../wx/decode'

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
  /** along-track NM of fuel stops (landings in the middle of the trip) */
  stopDists?: number[]
}

const MONTHS = ['January', 'February', 'March', 'April', 'May', 'June', 'July', 'August', 'September', 'October', 'November', 'December']

/** The days a TFR covers (UTC midnight of each, ms), from FAA titles like "City, ST, Friday,
 *  October 23, 2026 through Sunday, October 25, 2026 Local". null when the title has no dates
 *  (permanent TFRs, or ones whose times are only in the NOTAM text). A lone date already in the
 *  past is treated as unknown: some long-standing TFRs carry their issue date. */
export function tfrDays(title: string, now = new Date()): [number, number] | null {
  const re = /(?:Monday|Tuesday|Wednesday|Thursday|Friday|Saturday|Sunday), ([A-Z][a-z]+) (\d{1,2}), (\d{4})/g
  const dates: number[] = []
  for (const m of title.matchAll(re)) {
    const mo = MONTHS.indexOf(m[1])
    if (mo >= 0) dates.push(Date.UTC(Number(m[3]), mo, Number(m[2])))
  }
  if (!dates.length) return null
  if (dates.length === 1) {
    const today = Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), now.getUTCDate())
    return dates[0] < today - 2 * 86400000 ? null : [dates[0], dates[0]]
  }
  return [dates[0], dates[dates.length - 1]]
}

/** Whether a TFR with these days could be in effect at `t` (12 h slack covers US time zones and
 *  "Local" vs "UTC" titles). */
export function tfrCovers(days: [number, number] | null, t: Date) {
  if (!days) return true
  const h12 = 12 * 3600000
  return t.getTime() >= days[0] - h12 && t.getTime() <= days[1] + 86399000 + h12
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
type BBox = [number, number, number, number]
function bboxOf(rings: number[][]): BBox {
  let a = Infinity, b = Infinity, c = -Infinity, d = -Infinity
  for (const [x, y] of rings) {
    if (x < a) a = x
    if (y < b) b = y
    if (x > c) c = x
    if (y > d) d = y
  }
  return [a, b, c, d]
}
const overlaps = (p: BBox, q: BBox) => p[0] <= q[2] && p[2] >= q[0] && p[1] <= q[3] && p[3] >= q[1]
const inBox = (x: number, y: number, b: BBox) => x >= b[0] && x <= b[2] && y >= b[1] && y <= b[3]

function inGeom(lon: number, lat: number, g: GeoJSON.Geometry) {
  const polys = g.type === 'Polygon' ? [g.coordinates] : g.type === 'MultiPolygon' ? g.coordinates : []
  return polys.some((p) => inRing(lon, lat, p[0] as number[][]))
}

const fmtAlt = (v: number) => (v >= 99999 ? 'unlimited' : v === 17999 ? 'FL180' : v >= 18000 ? `FL${Math.round(v / 100)}` : v === 0 ? 'surface' : `${v.toLocaleString()} ft`)
const cap = (t: string) => t.charAt(0).toUpperCase() + t.slice(1)

// Lateral (route-vs-polygon) weather checks don't depend on altitude, so they are cached per
// route/weather data and not redone while the cruise altitude is dragged.
let lateralCache: { key: unknown[]; out: Hazard[] } | null = null
function lateral(h: HazardInput, total: number): Hazard[] {
  const key = [h.samples, h.tfrs, h.sigmets, h.gairmets, h.departure.getTime(), h.log?.totalTime, Math.max(...h.altitudePath.map((p) => p[1])) >= 18000, Math.floor(Date.now() / 300000)]
  if (lateralCache && lateralCache.key.length === key.length && lateralCache.key.every((k, i) => k === key[i])) return lateralCache.out
  const out: Hazard[] = []
  // ---- TFRs (lateral check; altitudes are in the NOTAM text) ----
  const routeBox = bboxOf(h.samples.map((s) => [s.lon, s.lat]))
  const tfrHits = new Map<string, { f: GeoJSON.Feature; d: number }>()
  for (const f of h.tfrs.features) {
    const g = f.geometry as GeoJSON.Polygon | GeoJSON.MultiPolygon
    const outer = g.type === 'Polygon' ? g.coordinates[0] : g.type === 'MultiPolygon' ? g.coordinates.flatMap((p) => p[0]) : []
    if (!outer.length) continue
    const box = bboxOf(outer as number[][])
    if (!overlaps(box, routeBox)) continue
    for (const s of h.samples) {
      if (inBox(s.lon, s.lat, box) && inGeom(s.lon, s.lat, f.geometry)) {
        const id = (f.properties as any)?.notam
        if (!tfrHits.has(id)) tfrHits.set(id, { f, d: s.d })
        break
      }
    }
  }
  const totalMin = h.log?.totalTime ?? 0
  for (const [id, { f, d }] of tfrHits) {
    const p = f.properties as any
    // dated TFRs only count if they cover the time you'd be there
    const when = new Date(h.departure.getTime() + (totalMin * 60000 * d) / Math.max(total, 1))
    if (!tfrCovers(tfrDays(String(p.title ?? '')) ?? tfrDays(String(p.description ?? '')), when)) continue
    out.push({ level: 'warning', category: 'TFR', title: `Route crosses TFR ${id}${p.type ? ` (${String(p.type).toLowerCase()})` : ''}`, detail: `${p.title ?? ''} Read the NOTAM for altitudes and times.`, at: d })
  }

  // ---- AIRMET / SIGMET / G-AIRMET ----
  const now = Date.now() / 1000
  const etaAt = (d: number) => h.departure.getTime() / 1000 + (totalMin * 60 * d) / Math.max(total, 1)
  for (const sg of h.sigmets) {
    if (!sg.coords?.length) continue
    const ring = sg.coords.map((c: any) => [Number(c.lon), Number(c.lat)])
    const box = bboxOf(ring)
    if (!overlaps(box, routeBox)) continue
    const s = h.samples.find((x) => inBox(x.lon, x.lat, box) && inRing(x.lon, x.lat, ring))
    if (!s) continue
    // it has to be in force when you get there (and not already over)
    const t = Math.max(etaAt(s.d), now)
    if ((sg.validTimeTo && sg.validTimeTo < t) || (sg.validTimeFrom && sg.validTimeFrom > t + 3600)) continue
    const sev = sg.airSigmetType === 'SIGMET' || sg.hazard === 'CONVECTIVE'
    out.push({ level: sev ? 'warning' : 'caution', category: 'Weather', title: sg.hazard === 'CONVECTIVE' ? 'Convective SIGMET on your route' : `${sg.airSigmetType} for ${String(sg.hazard).toLowerCase()} on your route`, detail: describeSigmet(sg).summary.slice(1).join('. ') + '.', at: s.d })
  }
  const gHits = new Map<string, number>()
  for (const g of h.gairmets) {
    if (!g.coords?.length || g.geometryType === 'LINE' || g.hazard === 'FZLVL') continue
    if (gHits.has(g.hazard)) continue
    const ring = g.coords.map((c: any) => [Number(c.lon), Number(c.lat)])
    const box = bboxOf(ring)
    if (!overlaps(box, routeBox)) continue
    const s = h.samples.find((x) => inBox(x.lon, x.lat, box) && inRing(x.lon, x.lat, ring))
    if (!s) continue
    // each G-AIRMET is a snapshot valid around `validTime` until the product expires
    const t = Math.max(etaAt(s.d), now)
    const valid = g.validTime ? Date.parse(g.validTime) / 1000 : null
    if ((g.expireTime && t > g.expireTime) || (valid != null && t < valid - 3 * 3600)) continue
    gHits.set(g.hazard, s.d)
  }
  const G_NAME: Record<string, string> = { IFR: 'IFR conditions', MT_OBSC: 'mountain obscuration', 'TURB-HI': 'high-altitude turbulence', 'TURB-LO': 'turbulence below FL180', ICE: 'icing', SFC_WND: 'strong surface wind', LLWS: 'low-level wind shear', M_FZLVL: 'multiple freezing levels' }
  for (const [hz, d] of gHits) {
    if (hz === 'TURB-HI' && Math.max(...h.altitudePath.map((p) => p[1])) < 18000) continue
    out.push({ level: hz === 'IFR' || hz === 'MT_OBSC' || hz === 'ICE' ? 'caution' : 'info', category: 'Weather', title: `G-AIRMET: ${G_NAME[hz] ?? hz} along route`, at: d })
  }

  lateralCache = { key, out }
  return out
}

export function analyze(h: HazardInput): Hazard[] {
  const out: Hazard[] = []
  const total = h.samples.length ? h.samples[h.samples.length - 1].d : 0
  if (!h.log || h.waypoints.length < 2) return out

  // ---- terrain ----
  const nearStop = (d: number, nm: number) => (h.stopDists ?? []).some((s) => Math.abs(d - s) < nm)
  let worst: { clr: number; d: number; elev: number } | null = null
  for (const s of h.samples) {
    if (s.d < 5 || s.d > total - 5 || nearStop(s.d, 5)) continue
    const clr = plannedAlt(h.altitudePath, s.d) - s.max
    if (!worst || clr < worst.clr) worst = { clr, d: s.d, elev: s.max }
  }
  if (worst && worst.clr < 0) out.push({ level: 'warning', category: 'Terrain', title: `Planned altitude is below terrain at ${worst.d.toFixed(0)} NM`, detail: `Terrain within 1 NM rises to ${Math.round(worst.elev).toLocaleString()} ft.`, at: worst.d })
  else if (worst && worst.clr < 500) out.push({ level: 'warning', category: 'Terrain', title: `Only ${Math.round(worst.clr)} ft above terrain at ${worst.d.toFixed(0)} NM`, detail: `Highest terrain within 1 NM: ${Math.round(worst.elev).toLocaleString()} ft.`, at: worst.d })
  else if (worst && worst.clr < 1000) out.push({ level: 'caution', category: 'Terrain', title: `Less than 1,000 ft terrain clearance at ${worst.d.toFixed(0)} NM`, detail: `${Math.round(worst.clr)} ft above the highest terrain within 1 NM.`, at: worst.d })

  // ---- obstacles ----
  const tall = h.obstacles.filter((o) => o.d > 2 && o.d < total - 2 && !nearStop(o.d, 2) && plannedAlt(h.altitudePath, o.d) - o.msl < 500)
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

  out.push(...lateral(h, total))

  // ---- fuel, for each flight between fuel stops ----
  const log = h.log
  const reserveMin = h.aircraft?.perf.reserveMin ?? 30
  const segs = log.segments.length ? log.segments.map((s) => ({ to: s.to, landing: s.fuelAtLanding, trip: s.tripFuel })) : [{ to: '', landing: log.fuelAtLanding, trip: log.tripFuel }]
  const many = segs.length > 1
  for (const { to, landing, trip } of segs) {
    const at = many ? ` at ${to}` : ''
    if (landing < 0) {
      out.push({
        level: 'warning',
        category: 'Fuel',
        title: many ? `Not enough fuel to reach ${to}` : 'Not enough fuel for this flight',
        detail: `That leg needs ${trip.toFixed(1)} gal plus ${(log.taxiFuel / segs.length).toFixed(1)} taxi.${many ? '' : ' Add a fuel stop: tap the pump next to an airport in the route.'}`,
      })
    } else if (landing < log.reserveFuel) out.push({ level: 'warning', category: 'Fuel', title: `Below reserve${at} (${landing.toFixed(1)} gal)`, detail: `Reserve is ${log.reserveFuel.toFixed(1)} gal (${reserveMin} min).` })
    else if (landing < log.reserveFuel * 1.5) out.push({ level: 'caution', category: 'Fuel', title: `Thin fuel margin${at}: ${landing.toFixed(1)} gal`, detail: `Reserve is ${log.reserveFuel.toFixed(1)} gal.` })
  }

  for (const w of log.warnings) if (!w.startsWith('Fuel')) out.push({ level: 'info', category: 'Performance', title: w })

  // ---- weight & balance ----
  if (h.aircraft) {
    // the longest flight burns the most: the worst landing weight / CG shift
    const burn = log.segments.length ? Math.max(...log.segments.map((s) => s.tripFuel)) : log.tripFuel
    const wb = computeWB(h.aircraft, burn)
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
