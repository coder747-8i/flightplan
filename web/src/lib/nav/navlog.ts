// Navlog: wind triangle per leg, climb to TOC and descent from TOD, fuel and times.
import geomagnetism from 'geomagnetism'
import type { WindPoint } from '../api'
import type { Aircraft } from '../stores/aircraft.svelte'
import type { Waypoint } from '../stores/route.svelte'
import { bearing, distanceNm, interpolate } from './geo'

export interface Wind {
  dir: number
  spd: number
  temp: number | null
}

export interface NavRow {
  from: string
  to: string
  phase: 'climb' | 'cruise' | 'descent'
  alt: number
  tc: number
  mc: number
  variation: number
  wind: Wind | null
  wca: number
  th: number
  mh: number
  tas: number
  gs: number
  dist: number
  ete: number // minutes
  fuel: number // gal
  cumDist: number
  cumTime: number
  eta: Date
  fuelRem: number
  lat: number // end point of this row
  lon: number
}

export interface NavLog {
  rows: NavRow[]
  totalDist: number
  totalTime: number
  tripFuel: number
  taxiFuel: number
  reserveFuel: number
  fuelAtLanding: number
  tocDist: number | null
  todDist: number | null
  warnings: string[]
}

const norm = (d: number) => ((d % 360) + 360) % 360

let magModel: ReturnType<typeof geomagnetism.model> | null = null
export function variation(lat: number, lon: number, when = new Date()) {
  try {
    magModel ??= geomagnetism.model(when, { allowOutOfBoundsModel: true })
    return magModel.point([lat, lon]).decl // east positive
  } catch {
    return 0
  }
}

/** wind at altitude (ft) from a point's level data, at the hour nearest `when` */
export function windAt(wp: WindPoint | undefined, altFt: number, when: Date): Wind | null {
  if (!wp || wp.error || !wp.levels?.length) return null
  const t = when.getTime() / 1000
  const perLevel = wp.levels
    .map((l) => {
      if (!l.times.length) return null
      let best = l.times[0]
      for (const x of l.times) if (Math.abs(x.t - t) < Math.abs(best.t - t)) best = x
      return { ...best, alt: best.alt ?? l.alt }
    })
    .filter(Boolean) as { alt: number; dir: number; spd: number; temp: number | null }[]
  perLevel.sort((a, b) => a.alt - b.alt)
  if (!perLevel.length) return null
  if (altFt <= perLevel[0].alt) return perLevel[0]
  for (let i = 1; i < perLevel.length; i++) {
    const a = perLevel[i - 1]
    const b = perLevel[i]
    if (altFt <= b.alt) {
      const f = (altFt - a.alt) / (b.alt - a.alt)
      // interpolate as vectors so 350° and 010° average to 000°
      const ax = a.spd * Math.sin((a.dir * Math.PI) / 180)
      const ay = a.spd * Math.cos((a.dir * Math.PI) / 180)
      const bx = b.spd * Math.sin((b.dir * Math.PI) / 180)
      const by = b.spd * Math.cos((b.dir * Math.PI) / 180)
      const x = ax + (bx - ax) * f
      const y = ay + (by - ay) * f
      const temp = a.temp != null && b.temp != null ? a.temp + (b.temp - a.temp) * f : null
      return { dir: Math.round(norm((Math.atan2(x, y) * 180) / Math.PI)), spd: Math.round(Math.hypot(x, y)), temp }
    }
  }
  return perLevel[perLevel.length - 1]
}

/** Wind triangle. Wind direction is where it blows FROM (true). */
export function triangle(tc: number, tas: number, w: Wind | null) {
  if (!w || !w.spd) return { wca: 0, th: tc, gs: tas }
  const rel = ((w.dir - tc) * Math.PI) / 180
  const s = (w.spd * Math.sin(rel)) / tas
  if (Math.abs(s) >= 1) return { wca: 0, th: tc, gs: Math.max(1, tas - w.spd) }
  const wca = (Math.asin(s) * 180) / Math.PI
  const gs = tas * Math.cos((wca * Math.PI) / 180) - w.spd * Math.cos(rel)
  return { wca, th: norm(tc + wca), gs: Math.max(1, gs) }
}

export interface NavInput {
  waypoints: Waypoint[]
  aircraft: Aircraft
  cruise: number
  departure: Date
  depElev: number
  destElev: number
  /** winds per waypoint index (same order as waypoints); optional */
  winds?: (WindPoint | undefined)[]
}

/**
 * Builds the log. Climb starts at the departure field, descent ends 1,000 ft above the
 * destination; both are spread over legs as needed (TOC/TOD may fall mid-leg).
 */
export function buildNavLog(inp: NavInput): NavLog {
  const { waypoints: w, aircraft: ac, cruise, departure } = inp
  const p = ac.perf
  const warnings: string[] = []
  const empty: NavLog = { rows: [], totalDist: 0, totalTime: 0, tripFuel: 0, taxiFuel: p.taxiGal, reserveFuel: (p.gph * p.reserveMin) / 60, fuelAtLanding: 0, tocDist: null, todDist: null, warnings }
  if (w.length < 2) return empty

  const legs = w.slice(1).map((b, i) => {
    const a = w[i]
    return { a, b, dist: distanceNm(a.lat, a.lon, b.lat, b.lon), tc: bearing(a.lat, a.lon, b.lat, b.lon) }
  })
  const total = legs.reduce((t, l) => t + l.dist, 0)

  // climb & descent distances (still-air estimate using phase TAS; refined by GS below)
  const climbFt = Math.max(0, cruise - inp.depElev)
  const descFt = Math.max(0, cruise - (inp.destElev + 1000))
  const climbMin = climbFt / p.climbFpm
  const descMin = descFt / p.descentFpm
  let tocD = (climbMin / 60) * p.climbTas
  let todFromEnd = (descMin / 60) * p.descentTas
  if (tocD + todFromEnd > total) {
    warnings.push(`Route is too short to reach ${cruise.toLocaleString()} ft; climb and descent overlap`)
    const k = total / (tocD + todFromEnd)
    tocD *= k
    todFromEnd *= k
  }
  const todD = total - todFromEnd

  const fuelStart = (ac.load.fuel ?? ac.usableFuel) - p.taxiGal
  const rows: NavRow[] = []
  let cumD = 0
  let cumT = 0
  let fuel = fuelStart
  const windFor = (i: number, alt: number, when: Date) => {
    // average the winds at both ends of the leg
    const a = windAt(inp.winds?.[i], alt, when)
    const b = windAt(inp.winds?.[i + 1], alt, when)
    if (!a) return b
    if (!b) return a
    const ax = a.spd * Math.sin((a.dir * Math.PI) / 180) + b.spd * Math.sin((b.dir * Math.PI) / 180)
    const ay = a.spd * Math.cos((a.dir * Math.PI) / 180) + b.spd * Math.cos((b.dir * Math.PI) / 180)
    return { dir: Math.round(norm((Math.atan2(ax, ay) * 180) / Math.PI)), spd: Math.round(Math.hypot(ax, ay) / 2), temp: a.temp != null && b.temp != null ? (a.temp + b.temp) / 2 : a.temp }
  }

  legs.forEach((leg, i) => {
    const start = cumD
    const end = cumD + leg.dist
    // split points inside this leg
    const cuts = [start, ...[tocD, todD].filter((d) => d > start + 0.05 && d < end - 0.05), end].sort((x, y) => x - y)
    for (let k = 1; k < cuts.length; k++) {
      const d0 = cuts[k - 1]
      const d1 = cuts[k]
      const mid = (d0 + d1) / 2
      const phase: NavRow['phase'] = mid < tocD ? 'climb' : mid > todD ? 'descent' : 'cruise'
      const tas = phase === 'climb' ? p.climbTas : phase === 'descent' ? p.descentTas : p.tas
      const gph = phase === 'climb' ? p.climbGph : phase === 'descent' ? p.descentGph : p.gph
      const altMid = phase === 'climb' ? inp.depElev + climbFt * (mid / Math.max(tocD, 0.01)) : phase === 'descent' ? cruise - descFt * ((mid - todD) / Math.max(todFromEnd, 0.01)) : cruise
      const f = (mid - start) / Math.max(leg.dist, 0.001)
      const [mlat, mlon] = interpolate(leg.a.lat, leg.a.lon, leg.b.lat, leg.b.lon, f)
      const when = new Date(departure.getTime() + cumT * 60000)
      const wind = windFor(i, altMid, when)
      const tc = bearing(mlat, mlon, leg.b.lat, leg.b.lon)
      const tri = triangle(tc, tas, wind)
      const variationDeg = variation(mlat, mlon, departure)
      const dist = d1 - d0
      const ete = (dist / tri.gs) * 60
      const burn = (ete / 60) * gph
      cumT += ete
      fuel -= burn
      const fe = (d1 - start) / Math.max(leg.dist, 0.001)
      const [elat, elon] = interpolate(leg.a.lat, leg.a.lon, leg.b.lat, leg.b.lon, fe)
      const toName = Math.abs(d1 - tocD) < 0.06 && d1 < end - 0.05 ? 'TOC' : Math.abs(d1 - todD) < 0.06 && d1 < end - 0.05 ? 'TOD' : leg.b.id
      const fromName = rows.length && rows[rows.length - 1].to !== leg.a.id && k > 1 ? rows[rows.length - 1].to : leg.a.id
      rows.push({
        from: fromName, to: toName, phase, alt: Math.round(phase === 'cruise' ? cruise : altMid), tc, mc: norm(tc - variationDeg), variation: variationDeg,
        wind, wca: tri.wca, th: tri.th, mh: norm(tri.th - variationDeg), tas, gs: tri.gs, dist, ete, fuel: burn,
        cumDist: d1, cumTime: cumT, eta: new Date(departure.getTime() + cumT * 60000), fuelRem: fuel, lat: elat, lon: elon,
      })
    }
    cumD = end
  })

  const tripFuel = rows.reduce((t, r) => t + r.fuel, 0)
  const reserveFuel = (p.gph * p.reserveMin) / 60
  const fuelAtLanding = fuelStart - tripFuel
  if (fuelAtLanding < reserveFuel) {
    warnings.push(`Fuel at landing ${fuelAtLanding.toFixed(1)} gal is below the ${p.reserveMin}-minute reserve (${reserveFuel.toFixed(1)} gal)`)
  }
  const strongHead = rows.find((r) => r.tas - r.gs > 30)
  if (strongHead) warnings.push(`Headwind over 30 kt on ${strongHead.from}–${strongHead.to}`)
  return { rows, totalDist: total, totalTime: cumT, tripFuel, taxiFuel: p.taxiGal, reserveFuel, fuelAtLanding, tocDist: climbFt ? tocD : null, todDist: descFt ? todD : null, warnings }
}

export function fmtTime(min: number) {
  const h = Math.floor(min / 60)
  const m = Math.round(min % 60)
  return h ? `${h}:${String(m).padStart(2, '0')}` : `0:${String(m).padStart(2, '0')}`
}

export const deg = (d: number) => String(Math.round(norm(d)) || 360).padStart(3, '0')
