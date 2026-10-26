// Go / no-go: one green, yellow or red call for the planned flight at the planned time, with the
// reasons. It rolls up the hazard advisor (terrain, obstacles, airspace, TFRs, advisories, fuel,
// weight & balance, daylight), the departure, fuel-stop and destination weather when you'd be
// there against the pilot's personal minimums (ceiling, visibility, wind, gusts, crosswind,
// thunder, density altitude), the official WPC forecast chart and SPC convective outlooks the
// route crosses on the day you fly, NOTAMs, and winds aloft.
// Advisory only: the pilot in command makes the call.
import type { OfficialHit } from '../api'
import type { Hazard } from './hazards'
import { thousands, type NavLog } from './navlog'
import { categoryOf, summary, SOURCE_LABEL, type Conditions } from '../wx/forecast'
import { activeAt, activeDuring, atAirport, type Notam } from '../wx/notams'

export type Call = 'go' | 'caution' | 'nogo'
const ORDER: Record<Call, number> = { go: 0, caution: 1, nogo: 2 }
const worse = (a: Call, b: Call) => (ORDER[b] > ORDER[a] ? b : a)
export const CALL_WORD: Record<Call, string> = { go: 'GO', caution: 'CAUTION', nogo: 'NO-GO' }

export interface Factor {
  call: Call
  category: string
  title: string
  detail: string
  /** along-track NM */
  at?: number
}

export interface Verdict {
  call: Call
  headline: string
  /** problems first (no-go, then caution), then the checks that passed ('go') */
  factors: Factor[]
  /** hours from now to departure */
  leadH: number
}

/** The pilot's personal minimums (Settings → Personal minimums). */
export interface Minimums {
  /** caution below this ceiling (ft AGL) */
  ceilingFt: number
  /** caution below this visibility (SM) */
  visSm: number
  /** no-go above this steady wind (kt) */
  windKt: number
  /** no-go above this gust (kt) */
  gustKt: number
  /** caution when gusts exceed the steady wind by more than this (kt) */
  gustSpreadKt: number
  /** caution above this share of the aircraft's demonstrated crosswind (%) */
  xwindPct: number
  /** thunder probability (%) for caution / no-go */
  tstmCaution: number
  tstmNogo: number
  /** caution above this density altitude (ft) */
  densityAltFt: number
  /** night flight is acceptable (caution) rather than a no-go */
  nightOk: boolean
  /** instrument rated and current: IFR at the ends is a caution rather than a no-go */
  ifr: boolean
}

export const DEFAULT_MINIMUMS: Minimums = { ceilingFt: 3000, visSm: 5, windKt: 25, gustKt: 25, gustSpreadKt: 15, xwindPct: 80, tstmCaution: 15, tstmNogo: 40, densityAltFt: 6000, nightOk: true, ifr: false }

/** Departure, a fuel stop or the destination, with its weather at the time you'd be there. */
export interface End {
  role: 'Departure' | 'Fuel stop' | 'Destination'
  airport: string
  isAirport: boolean
  cond: Conditions | null
  runways: { id: string; hdg: number }[]
  /** field elevation, ft */
  elev: number
  /** NM to the station the weather comes from */
  offsetNm: number
  when: Date
}

export interface VerdictInput {
  hazards: Hazard[]
  ends: End[]
  /** official outlook areas the route crosses on the days of the flight */
  official: OfficialHit[]
  /** false while the official products are still loading */
  officialKnown: boolean
  /** route NOTAMs; null when NOTAMs aren't set up (or still loading) */
  notams: Notam[] | null
  /** identifiers of the navaids the route uses */
  routeNavaids: string[]
  log: NavLog | null
  /** highest terrain along the route (ft) */
  maxTerrainFt: number
  /** the aircraft's demonstrated crosswind (kt) */
  maxXwind: number
  minimums: Minimums
  departure: Date
  now: Date
  dataExpired: boolean
}

const f = (call: Call, category: string, title: string, detail = '', at?: number): Factor => ({ call, category, title, detail, at })

const localWhen = (d: Date) => d.toLocaleString([], { weekday: 'short', hour: '2-digit', minute: '2-digit', hour12: false })

/** How a hazard-advisor item counts toward the call. */
function hazardCall(h: Hazard, m: Minimums): Call | null {
  const t = h.title
  switch (h.category) {
    case 'Terrain':
      return h.level === 'warning' ? 'nogo' : h.level === 'caution' ? 'caution' : null
    case 'Obstacle':
      return 'caution'
    case 'TFR':
      return 'nogo'
    case 'Airspace':
      return t.includes('prohibited') ? 'nogo' : t.includes('restricted') ? 'caution' : null
    case 'Weather':
    case 'Fuel':
      return h.level === 'warning' ? 'nogo' : h.level === 'caution' ? 'caution' : null
    case 'Weight & balance':
      return 'nogo'
    case 'Daylight':
      return h.level === 'caution' ? (m.nightOk ? 'caution' : 'nogo') : null
    default:
      return null
  }
}

/** Crosswind and headwind components on the runway with the least crosswind (headwind > -5 kt). */
export function bestRunway(runways: { id: string; hdg: number }[], wdir: number, speed: number) {
  let best: { id: string; x: number; head: number } | null = null
  for (const r of runways) {
    const a = ((wdir - r.hdg) * Math.PI) / 180
    const x = Math.abs(speed * Math.sin(a))
    const head = speed * Math.cos(a)
    if (head > -5 && (!best || x < best.x)) best = { id: r.id, x, head }
  }
  return best
}

function endChecks(e: End, m: Minimums, maxXwind: number, out: Factor[]) {
  const who = `${e.role} ${e.airport}`
  const when = localWhen(e.when)
  const c = e.cond
  if (!c) {
    if (e.isAirport) out.push(f('caution', 'Weather', `No forecast for ${who} at ${when}`, 'No METAR, TAF or model guidance covers this airport and time. Check again closer to departure.'))
    return
  }
  const src = c.station !== e.airport && e.offsetNm > 1 ? `${SOURCE_LABEL[c.source]} from ${c.station} (${Math.round(e.offsetNm)} NM away)` : SOURCE_LABEL[c.source]
  let worst: Call = 'go'
  const push = (call: Call, title: string, detail: string) => {
    worst = worse(worst, call)
    out.push(f(call, 'Weather', title, detail))
  }
  const sum = summary(c)
  // ceiling / visibility
  const cat = categoryOf(c)
  if (cat === 'IFR' || cat === 'LIFR') push(m.ifr ? 'caution' : 'nogo', `${who} ${cat} at ${when}`, `${sum} (${src}).`)
  else if (cat === 'MVFR') push('caution', `${who} marginal VFR at ${when}`, `${sum} (${src}).`)
  else if (cat) {
    const lowCeiling = c.ceilingKnown && c.ceiling != null && c.ceiling < m.ceilingFt
    const lowVis = c.vis != null && c.vis < m.visSm
    if (lowCeiling || lowVis) push('caution', `${who} below your minimums at ${when}`, `${sum}; your minimums are ${thousands(m.ceilingFt)} ft and ${m.visSm} SM (${src}).`)
  } else if ((c.skyPct ?? 0) >= 88 && (c.pop ?? 0) >= 50) {
    push('caution', `${who}: overcast with precipitation likely`, `${sum} (${src}). Ceilings aren't forecast this far out.`)
  }
  // thunder and freezing precipitation
  const tstm = c.tstm ?? 0
  if (c.wx.includes('TS')) push('nogo', `Thunderstorms at ${who} at ${when}`, `${sum} (${src}).`)
  else if (tstm >= m.tstmNogo) push('nogo', `${Math.round(tstm)}% chance of thunderstorms at ${who}`, `Around ${when} (${src}).`)
  else if (tstm >= m.tstmCaution) push('caution', `${Math.round(tstm)}% chance of thunderstorms at ${who}`, `Around ${when} (${src}).`)
  if (c.wx.includes('FZ')) push('nogo', `Freezing precipitation at ${who}`, `${sum} (${src}).`)
  else if (c.wx.includes('SN')) push('caution', `Snow at ${who}`, `${sum} (${src}).`)
  if (c.tempo) {
    const [tcat, text] = c.tempo
    if (text.includes('TS')) {
      const prob = text.startsWith('PROB')
      push(prob ? 'caution' : 'nogo', `${prob ? 'Possible' : 'Temporary'} thunderstorms in the ${who} TAF`, text)
    } else if (tcat === 'IFR' || tcat === 'LIFR') push('caution', `${who} TAF has temporary ${tcat}`, text)
  }
  // wind
  const wspd = c.wspd ?? 0
  const gust = c.gust ?? wspd
  const gtxt = c.gust != null ? ` gusting ${Math.round(c.gust)}` : ''
  if (wspd > m.windKt) push('nogo', `${who} wind ${Math.round(wspd)} kt`, `Above your ${m.windKt} kt limit (${src}).`)
  else if (gust > m.gustKt) push('nogo', `${who} gusts ${Math.round(gust)} kt`, `Above your ${m.gustKt} kt gust limit (${src}).`)
  else if (gust - wspd > m.gustSpreadKt) push('caution', `${who} gust spread ${Math.round(gust - wspd)} kt`, `Wind ${Math.round(wspd)}${gtxt} kt (${src}).`)
  const dir3 = (d: number) => String(Math.round(d)).padStart(3, '0')
  if (c.wdir != null && gust >= 3 && e.runways.length) {
    const b = bestRunway(e.runways, c.wdir, gust)
    if (b && b.x > maxXwind) push('nogo', `${who} crosswind ${Math.round(b.x)} kt (runway ${b.id})`, `Above this aircraft's ${maxXwind} kt demonstrated crosswind. Wind ${dir3(c.wdir)}° ${Math.round(wspd)}${gtxt} kt (${src}).`)
    else if (b && b.x > (maxXwind * m.xwindPct) / 100) push('caution', `${who} crosswind ${Math.round(b.x)} kt (runway ${b.id})`, `${Math.round((b.x / maxXwind) * 100)}% of the ${maxXwind} kt demonstrated crosswind. Wind ${dir3(c.wdir)}° ${Math.round(wspd)}${gtxt} kt (${src}).`)
  } else if (c.wdir == null && wspd >= 10) push('caution', `${who} wind variable at ${Math.round(wspd)} kt`, `Expect a crosswind on any runway (${src}).`)
  // density altitude at departure
  if (e.role === 'Departure' && c.tempC != null) {
    const altim = c.altimHpa ?? 1013.25
    const pa = e.elev + (1013.25 - altim) * 30
    const isa = 15 - (2 * pa) / 1000
    const da = pa + 120 * (c.tempC - isa)
    if (da > m.densityAltFt) {
      push('caution', `Density altitude ${thousands(Math.round(da / 100) * 100)} ft at ${e.airport}`, `Field elevation ${thousands(e.elev)} ft, ${Math.round((c.tempC * 9) / 5 + 32)}°F. Expect a longer takeoff roll and weaker climb; check the POH charts.`)
    }
  }
  if (worst === 'go') out.push(f('go', 'Weather', `${who} at ${when}`, `${cat ? `${cat} · ` : ''}${sum} (${src})`))
}

/** NOTAMs at an airport at the time you'd be there. */
function notamEndChecks(e: End, notams: Notam[], out: Factor[]) {
  const who = `${e.role} ${e.airport}`
  const here = notams.filter((n) => atAirport(n, e.airport) && activeAt(n, e.when))
  for (const n of here) {
    const text = `NOTAM ${n.number}: ${n.plain}`
    if (n.effect.kind === 'aerodromeClosed') out.push(f('nogo', 'NOTAM', `${who} closed`, text))
    else if (n.effect.kind === 'fuelUnavailable' && e.role === 'Fuel stop') out.push(f('nogo', 'NOTAM', `No fuel at fuel stop ${e.airport}`, text))
    else if (n.effect.kind === 'fuelUnavailable') out.push(f('caution', 'NOTAM', `Fuel unavailable at ${e.airport}`, text))
  }
  // runways: all closed is a no-go, some closed a caution
  const closed = here.flatMap((n) => (n.effect.kind === 'runwayClosed' ? n.effect.ids : []))
  if (closed.length) {
    const norm = (s: string) => s.replace(/^0+/, '')
    const all = e.runways.length > 0 && e.runways.every((r) => closed.some((c) => norm(c) === norm(r.id)))
    const list = closed.join(', ')
    if (all) out.push(f('nogo', 'NOTAM', `All runways closed at ${e.airport}`, `Runways ${list} closed when you'd be there (NOTAM).`))
    else out.push(f('caution', 'NOTAM', `Runway ${list} closed at ${e.airport}`, 'Plan for the runways that are open: check the crosswind on them.'))
  }
}

/** NOTAMs along the route during the flight: navaids you use, GPS, airspace. */
function notamRouteChecks(notams: Notam[], navaids: string[], from: Date, to: Date, out: Factor[]) {
  const during = notams.filter((n) => activeDuring(n, from, to))
  for (const n of during) {
    if (n.effect.kind === 'navaidOut' && navaids.includes(n.location)) out.push(f('caution', 'NOTAM', `${n.location} out of service`, `Your route uses it. NOTAM ${n.number}: ${n.plain}`))
  }
  const gps = during.find((n) => n.effect.kind === 'gps')
  if (gps) out.push(f('caution', 'NOTAM', 'GPS may be unreliable along the route', `NOTAM ${gps.number}: ${gps.plain}`))
  const air = during.filter((n) => n.effect.kind === 'airspace')
  if (air.length) out.push(f('caution', 'NOTAM', `${air.length} airspace NOTAM${air.length === 1 ? '' : 's'} along the route`, air.slice(0, 3).map((n) => n.plain.slice(0, 90)).join(' · ')))
}

function officialChecks(hits: OfficialHit[], out: Factor[]) {
  let any = false
  for (const h of hits) {
    const w = h.label
    let call: Call
    let title: string
    if (h.product === 'SPC convective outlook' && w === 'TSTM') [call, title] = ['caution', `Thunderstorms possible along the route (SPC day ${h.day})`]
    else if (h.product === 'SPC convective outlook' && h.day >= 4) [call, title] = [w.startsWith('15') ? 'caution' : 'nogo', `SPC day ${h.day} outlook: ${w} along the route`]
    else if (h.product === 'SPC convective outlook') [call, title] = ['nogo', `SPC ${h.detail || w} along the route (day ${h.day})`]
    else if (w.includes('Severe') || w.includes('Freezing Rain') || w.includes('Heavy Snow')) [call, title] = ['nogo', `WPC: ${w.toLowerCase()} along the route (day ${h.day})`]
    else if (w.includes('Thunderstorm') || w.includes('Snow') || w.includes('Mixed') || w.includes('Heavy Rain')) [call, title] = ['caution', `WPC: ${w.toLowerCase()} area along the route (day ${h.day})`]
    else if (w === 'Rain') [call, title] = ['caution', `WPC: rain area along the route (day ${h.day})`]
    else continue
    any = true
    const detail = `${h.product} for the day you fly. Check the product and the TAFs closer to departure.`
    out.push(f(call, 'En route', title, h.product.startsWith('SPC') && h.detail && h.day < 4 ? `${h.detail}: ${detail}` : detail))
  }
  if (!any) out.push(f('go', 'En route', 'No forecast weather areas along the route', 'WPC forecast chart and SPC convective outlook for the day you fly.'))
}

export function evaluate(i: VerdictInput): Verdict {
  const m = i.minimums
  const out: Factor[] = []
  for (const h of i.hazards) {
    const call = hazardCall(h, m)
    if (call) out.push(f(call, h.category, h.title, h.detail ?? '', h.at))
  }
  for (const e of i.ends) endChecks(e, m, i.maxXwind, out)
  if (i.notams) {
    const before = out.length
    for (const e of i.ends) notamEndChecks(e, i.notams, out)
    notamRouteChecks(i.notams, i.routeNavaids, i.departure, i.ends[i.ends.length - 1]?.when ?? i.departure, out)
    if (out.length === before) out.push(f('go', 'NOTAM', `${i.notams.length} NOTAMs checked`, 'Nothing closes an airport or runway you use, no fuel or navaid outage on your route.'))
  } else {
    out.push(f('go', 'NOTAM', 'NOTAMs not checked', 'Add FAA NOTAM credentials in Settings (free, by email request to notams@faa.gov) to include them, or check NOTAMs at 1800wxbrief.'))
  }
  if (i.officialKnown) officialChecks(i.official, out)
  // winds aloft: strong flow over high terrain means turbulence / mountain wave
  const log = i.log
  if (log) {
    const cruise = log.rows.filter((r) => r.phase === 'cruise')
    let top: (typeof cruise)[number] | null = null
    for (const r of cruise) if (r.wind && (!top || r.wind.spd > top.wind!.spd)) top = r
    if (top?.wind) {
      const spd = top.wind.spd
      if (spd >= 40 && i.maxTerrainFt >= 5000) out.push(f('caution', 'Winds aloft', `${Math.round(spd)} kt winds at cruise over high terrain`, `Near ${top.to} at ${thousands(top.alt)} ft. Expect turbulence and mountain wave downwind of ridges.`, top.cumDist))
      else if (spd >= 50) out.push(f('caution', 'Winds aloft', `${Math.round(spd)} kt winds at cruise`, `Near ${top.to}. Check groundspeed and fuel.`, top.cumDist))
    }
    if (!out.some((x) => x.category === 'Fuel')) {
      const stops = Math.max(0, log.segments.length - 1)
      const detail = stops > 0 ? `Reserve ${log.reserveFuel.toFixed(1)} gal; topped off at ${stops} fuel stop${stops === 1 ? '' : 's'}.` : `Reserve ${log.reserveFuel.toFixed(1)} gal.`
      out.push(f('go', 'Fuel', `${log.fuelAtLanding.toFixed(1)} gal at landing`, detail))
    }
    // official winds aloft only reach about 24 hours out
    if (cruise.length && cruise.every((r) => !r.wind)) {
      out.push(f('go', 'Winds aloft', 'No official winds-aloft forecast for this time', 'The NWS winds-aloft forecast covers about the next 24 hours: times and fuel assume no wind. Check winds again closer to departure.'))
    }
  }
  if (i.dataExpired) out.push(f('caution', 'Chart data', 'Chart data is out of date', 'Airspace, frequencies and obstacles may have changed. Update the data pack in Settings.'))
  const leadH = (i.departure.getTime() - i.now.getTime()) / 3600000
  if (leadH > 72) out.push(f('go', 'Confidence', `Departure is ${(leadH / 24).toFixed(1)} days out`, 'Forecasts this far ahead are rough guidance: no TAF, no ceiling forecast. Check again closer to the day.'))
  else if (leadH < -60) out.push(f('caution', 'Time', 'Departure time is in the past', 'Set the departure time on the timeline to check the weather you\'d actually fly in.'))
  // stable sort: worst first, otherwise in the order found
  const sorted = out.map((x, k) => [x, k] as const).sort((a, b) => ORDER[b[0].call] - ORDER[a[0].call] || a[1] - b[1]).map((x) => x[0])
  const call = sorted.reduce<Call>((a, x) => worse(a, x.call), 'go')
  const problems = sorted.filter((x) => x.call === call && call !== 'go').length
  const headline = call === 'go' ? 'Within your minimums' : problems === 1 ? sorted[0].title : call === 'caution' ? `${sorted[0].title} · and ${problems - 1} more to review` : `${sorted[0].title} · and ${problems - 1} more`
  return { call, headline, factors: sorted, leadH }
}

/** The next run of "go" departures after `from` in the 3-hourly windows: [start, end] ms. */
export function nextGreen(windows: [number, Call][], from: number): [number, number] | null {
  const start = windows.findIndex((w) => w[0] > from && w[1] === 'go')
  if (start < 0) return null
  let end = start
  while (end + 1 < windows.length && windows[end + 1][1] === 'go') end++
  return [windows[start][0], windows[end][0]]
}
