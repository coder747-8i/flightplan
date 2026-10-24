// Route exchange formats: the FltPlan.com flight plan fields, Garmin .fpl (Garmin avionics SD
// cards, Garmin Pilot, ForeFlight, FltPlan Go via Garmin), and GPX; plus reading .fpl/.gpx back.
import type { Place } from '../api'
import type { Aircraft } from '../stores/aircraft.svelte'

type Wp = Pick<Place, 'kind' | 'id' | 'name' | 'detail' | 'lat' | 'lon'>

/** ICAO / FAA lat-long waypoint, e.g. 3330N11203W (whole minutes, carried into degrees). */
export function coordId(lat: number, lon: number) {
  const part = (v: number, width: number) => {
    let d = Math.floor(Math.abs(v))
    let m = Math.round((Math.abs(v) - d) * 60)
    if (m === 60) {
      d += 1
      m = 0
    }
    return `${String(d).padStart(width, '0')}${String(m).padStart(2, '0')}`
  }
  return `${part(lat, 2)}${lat >= 0 ? 'N' : 'S'}${part(lon, 3)}${lon >= 0 ? 'E' : 'W'}`
}

/** identifier to file: coordinates get a lat-long id; everything else keeps its id */
const routeId = (w: Wp) => (w.kind === 'coord' ? coordId(w.lat, w.lon) : w.id.toUpperCase())

// --- FltPlan.com ------------------------------------------------------------------------

export interface FltPlanFields {
  departure: string
  destination: string
  route: string
  altitude: string
  aircraftId: string
  aircraftType: string
  tas: string
  ete: string
  fuel: string
  /** the whole route on one line, for FltPlan's "paste route" box */
  full: string
}

const hhmm = (min: number) => `${Math.floor(min / 60)}:${String(Math.round(min % 60)).padStart(2, '0')}`

/** The values FltPlan.com's flight plan form asks for, ready to copy. */
export function fltplanFields(wps: Wp[], cruise: number, ac: Aircraft | undefined, eteMin: number | undefined): FltPlanFields {
  const ids = wps.map(routeId)
  const dep = ids[0] ?? ''
  const dest = ids.length > 1 ? ids[ids.length - 1] : ''
  const mid = ids.slice(1, -1)
  // FltPlan, like the FAA form, wants altitude in hundreds of feet ("065"), FL above 17,999
  const alt = cruise >= 18000 ? `FL${Math.round(cruise / 100)}` : String(Math.round(cruise / 100)).padStart(3, '0')
  const endurance = ac && ac.perf.gph > 0 ? hhmm((ac.usableFuel / ac.perf.gph) * 60) : ''
  return {
    departure: dep,
    destination: dest,
    route: mid.length ? mid.join(' ') : 'DCT',
    altitude: alt,
    aircraftId: ac?.tail.toUpperCase() ?? '',
    aircraftType: ac?.type.toUpperCase() ?? '',
    tas: ac ? String(Math.round(ac.perf.tas)) : '',
    ete: eteMin ? hhmm(eteMin) : '',
    fuel: endurance,
    full: ids.join(' '),
  }
}

// --- Garmin .fpl ------------------------------------------------------------------------

const esc = (s: string) => s.replace(/[<>&'"]/g, (c) => ({ '<': '&lt;', '>': '&gt;', '&': '&amp;', "'": '&apos;', '"': '&quot;' })[c]!)

function garminType(w: Wp): string {
  if (w.kind === 'airport') return 'AIRPORT'
  if (w.kind === 'navaid') return /NDB/i.test(w.detail) && !/VOR/i.test(w.detail) ? 'NDB' : 'VOR'
  if (w.kind === 'fix') return 'INT'
  return 'USER WAYPOINT'
}

/** Garmin FlightPlan v1 XML. User waypoints get short unique ids (Garmin allows 6 characters). */
export function toFpl(wps: Wp[], name: string): string {
  const table = new Map<string, { id: string; type: string; w: Wp }>()
  let user = 0
  const points = wps.map((w) => {
    const type = garminType(w)
    let id = type === 'USER WAYPOINT' ? `USR${String(++user).padStart(3, '0')}` : w.id.toUpperCase().slice(0, 12)
    const key = `${type}|${id}`
    if (!table.has(key)) table.set(key, { id, type, w })
    return { id, type }
  })
  const wpXml = [...table.values()]
    .map(
      ({ id, type, w }) => `    <waypoint>
      <identifier>${esc(id)}</identifier>
      <type>${type}</type>
      <lat>${w.lat.toFixed(6)}</lat>
      <lon>${w.lon.toFixed(6)}</lon>
      <comment>${esc(type === 'USER WAYPOINT' ? coordId(w.lat, w.lon) : (w.name || '').slice(0, 25).toUpperCase())}</comment>
    </waypoint>`,
    )
    .join('\n')
  const rtXml = points
    .map(
      (p) => `    <route-point>
      <waypoint-identifier>${esc(p.id)}</waypoint-identifier>
      <waypoint-type>${p.type}</waypoint-type>
    </route-point>`,
    )
    .join('\n')
  return `<?xml version="1.0" encoding="utf-8"?>
<flight-plan xmlns="http://www8.garmin.com/xmlschemas/FlightPlan/v1">
  <created>${new Date().toISOString().replace(/\.\d+Z$/, 'Z')}</created>
  <waypoint-table>
${wpXml}
  </waypoint-table>
  <route>
    <route-name>${esc(name.toUpperCase().slice(0, 25))}</route-name>
    <flight-plan-index>1</flight-plan-index>
${rtXml}
  </route>
</flight-plan>
`
}

// --- GPX --------------------------------------------------------------------------------

export function toGpx(wps: Wp[], name: string): string {
  const pts = wps
    .map((w) => `    <rtept lat="${w.lat.toFixed(6)}" lon="${w.lon.toFixed(6)}"><name>${esc(routeId(w))}</name>${w.name ? `<desc>${esc(w.name)}</desc>` : ''}</rtept>`)
    .join('\n')
  return `<?xml version="1.0" encoding="UTF-8"?>
<gpx version="1.1" creator="Flightplan" xmlns="http://www.topografix.com/GPX/1/1">
  <rte>
    <name>${esc(name)}</name>
${pts}
  </rte>
</gpx>
`
}

// --- reading ----------------------------------------------------------------------------

/** Waypoints from a Garmin .fpl or a GPX route/track/waypoint file. */
export function parseRouteFile(text: string): Place[] {
  const doc = new DOMParser().parseFromString(text, 'application/xml')
  if (doc.querySelector('parsererror')) throw new Error("This file isn't valid XML")
  const root = doc.documentElement.localName
  const txt = (el: Element | null | undefined, tag: string) => el?.getElementsByTagNameNS('*', tag)[0]?.textContent?.trim() ?? ''

  if (root === 'flight-plan') {
    const table = new Map<string, Element>()
    for (const w of Array.from(doc.getElementsByTagNameNS('*', 'waypoint'))) table.set(`${txt(w, 'type')}|${txt(w, 'identifier')}`, w)
    const out: Place[] = []
    for (const rp of Array.from(doc.getElementsByTagNameNS('*', 'route-point'))) {
      const id = txt(rp, 'waypoint-identifier')
      const type = txt(rp, 'waypoint-type')
      const w = table.get(`${type}|${id}`) ?? [...table.entries()].find(([k]) => k.endsWith(`|${id}`))?.[1]
      if (!w) continue
      const lat = Number(txt(w, 'lat'))
      const lon = Number(txt(w, 'lon'))
      if (!Number.isFinite(lat) || !Number.isFinite(lon)) continue
      out.push(fromFile(id, type, txt(w, 'comment'), lat, lon))
    }
    if (!out.length) throw new Error('No route points found in this flight plan')
    return out
  }

  if (root === 'gpx') {
    const pick = (tag: string) => Array.from(doc.getElementsByTagNameNS('*', tag))
    const pts = pick('rtept').length ? pick('rtept') : pick('wpt').length ? pick('wpt') : pick('trkpt')
    const out = pts
      .map((p, i) => {
        const lat = Number(p.getAttribute('lat'))
        const lon = Number(p.getAttribute('lon'))
        const id = txt(p, 'name') || `WP${i + 1}`
        return fromFile(id, '', txt(p, 'desc'), lat, lon)
      })
      .filter((p) => Number.isFinite(p.lat) && Number.isFinite(p.lon))
    if (!out.length) throw new Error('No waypoints found in this GPX file')
    return out
  }

  throw new Error('Not a Garmin .fpl or GPX file')
}

function fromFile(id: string, type: string, comment: string, lat: number, lon: number): Place {
  const t = type.toUpperCase()
  if (t === 'AIRPORT') return { kind: 'airport', id, name: comment || id, detail: '', lat, lon }
  if (t === 'VOR' || t === 'NDB') return { kind: 'navaid', id, name: comment || id, detail: t, lat, lon }
  if (t.startsWith('INT')) return { kind: 'fix', id, name: id, detail: 'Fix', lat, lon }
  // user waypoints and GPX points: keep a readable id but plan by coordinates
  const cid = /^[A-Z0-9]{2,5}$/.test(id) && t !== 'USER WAYPOINT' ? id : coordId(lat, lon)
  return { kind: 'coord', id: cid, name: comment || 'User waypoint', detail: `${lat.toFixed(4)}, ${lon.toFixed(4)}`, lat, lon }
}
