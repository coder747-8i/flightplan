// NOTAMs from the FAA (via the server; credentials by request from notams@faa.gov), read into
// what a pilot needs: when each is in effect, plain English, and how it affects the flight.
// Closed airports and runways, fuel, navaids on the route, GPS and airspace NOTAMs feed the
// go / no-go call.
import { expand } from './decode'

/** What a NOTAM does to a flight. */
export type Effect =
  | { kind: 'aerodromeClosed' }
  /** runway end ids it closes, e.g. ["03R", "21L"] */
  | { kind: 'runwayClosed'; ids: string[] }
  | { kind: 'fuelUnavailable' }
  | { kind: 'navaidOut' }
  | { kind: 'gps' }
  | { kind: 'airspace' }
  | { kind: 'obstacle' }
  | { kind: 'other' }

export interface Notam {
  id: string
  number: string
  /** FAA location (PRC) and ICAO location (KPRC) */
  location: string
  icao: string
  class: string
  category: string
  /** the NOTAM text as issued */
  raw: string
  /** contractions written out */
  plain: string
  start: Date | null
  end: Date | null
  perm: boolean
  /** the end time is an estimate */
  est: boolean
  effect: Effect
}

/** A ready-to-send request for NMS API access. */
export const REQUEST_MAILTO =
  'mailto:notams@faa.gov?subject=NMS%20API%20access%20request&body=Hello%2C%0A%0AI%20would%20like%20to%20request%20access%20to%20the%20NOTAM%20Management%20Service%20(NMS)%20API%20for%20a%20personal%20flight-planning%20application%20(pre-flight%20NOTAM%20review%20for%20my%20own%20flights%2C%20GeoJSON%2C%20low%20volume).%0A%0AName%3A%20%0APilot%20certificate%20(optional)%3A%20%0A%0AThank%20you.'

const str = (v: unknown) => (v == null ? '' : String(v))

/** "2026-10-24T13:00:00.000Z", "2026-10-30T23:59:00.000-0500EST" or "PERM". */
function time(v: unknown): { t: Date | null; perm: boolean; est: boolean } {
  const s = str(v)
  if (!s) return { t: null, perm: false, est: false }
  if (s.startsWith('PERM')) return { t: null, perm: true, est: false }
  const m = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})/.exec(s)
  return { t: m ? new Date(Date.UTC(+m[1], +m[2] - 1, +m[3], +m[4], +m[5], +m[6])) : null, perm: false, est: s.includes('EST') }
}

/** The effect of a NOTAM, from its text (domestic format: "RWY 03R/21L CLSD", "AD AP CLSD"). */
export function classify(raw: string, category: string): Effect {
  const t = raw.toUpperCase()
  if (/\b(?:AD AP|AD|AP|AERODROME|AIRPORT) CLSD\b/.test(t) && !t.includes('RWY') && !t.includes('TWY')) return { kind: 'aerodromeClosed' }
  const rwy = /\bRWY (\d{1,2}[LRCW]?)(?:\/(\d{1,2}[LRCW]?))? CLSD\b/.exec(t)
  // "RWY 03/21 CLSD" closes the runway; "... CLSD TO ARR" etc. still counts
  if (rwy) return { kind: 'runwayClosed', ids: rwy[2] ? [rwy[1], rwy[2]] : [rwy[1]] }
  if (t.includes('FUEL') && (t.includes('UNAVBL') || t.includes('NOT AVBL') || t.includes(' OTS') || t.includes('U/S'))) return { kind: 'fuelUnavailable' }
  if ((t.includes('NAV ') || t.includes('VOR') || t.includes('VORTAC') || t.includes('NDB')) && (t.includes('U/S') || t.includes('OTS') || t.includes('UNUSABLE'))) return { kind: 'navaidOut' }
  if ((t.includes('GPS') || t.includes('GNSS')) && (t.includes('UNREL') || t.includes('UNAVBL') || t.includes('MAY NOT BE AVBL'))) return { kind: 'gps' }
  if (category === 'Airspace') return { kind: 'airspace' }
  if (category === 'Obstacle') return { kind: 'obstacle' }
  return { kind: 'other' }
}

export function fromJson(v: any): Notam {
  const raw = str(v.icaoText)
  const text = str(v.text)
  const category = str(v.category)
  const s = time(v.start)
  const e = time(v.end)
  // the domestic text is the readable one; the ICAO text has the Q-line
  const body = raw || text
  return {
    id: str(v.id),
    number: str(v.number),
    location: str(v.location),
    icao: str(v.icao),
    class: str(v.class),
    category,
    raw: text,
    plain: expand(text.replace(/^!+/, '')),
    start: s.t,
    end: e.t,
    perm: e.perm,
    est: e.est,
    effect: classify(body, category),
  }
}

/** In effect at `t` (no times known: assume yes). */
export const activeAt = (n: Notam, t: Date) => (!n.start || t >= n.start) && (n.perm || !n.end || t <= n.end)

/** Overlaps [a, b]. */
export const activeDuring = (n: Notam, a: Date, b: Date) => (!n.start || n.start <= b) && (n.perm || !n.end || n.end >= a)

/** Whether the NOTAM is for this airport (FAA "PRC", ICAO "KPRC" or "E25"). */
export function atAirport(n: Notam, id: string) {
  const u = id.toUpperCase()
  return n.icao === u || n.location === u || (u.length === 4 && u.startsWith('K') && n.location === u.slice(1))
}

const RANK: Record<Effect['kind'], number> = { aerodromeClosed: 0, runwayClosed: 1, fuelUnavailable: 2, navaidOut: 3, gps: 4, airspace: 5, other: 6, obstacle: 7 }

export const important = (n: Notam) => RANK[n.effect.kind] <= RANK.gps

/** "Oct 24 13:00Z – Oct 30 23:59Z (est.)", "from Oct 2 13:00Z · permanent". */
export function window(n: Notam): string {
  const f = (d: Date) => `${d.toLocaleString('en-US', { month: 'short', day: 'numeric', timeZone: 'UTC' })} ${String(d.getUTCHours()).padStart(2, '0')}:${String(d.getUTCMinutes()).padStart(2, '0')}Z`
  if (n.start && n.perm) return `from ${f(n.start)} · permanent`
  if (n.start && n.end) return `${f(n.start)} – ${f(n.end)}${n.est ? ' (est.)' : ''}`
  if (n.start) return `from ${f(n.start)}`
  if (n.end) return `until ${f(n.end)}`
  return ''
}

/** Read the server's answer: most important first. */
export function parse(v: { configured?: boolean; items?: any[]; errors?: string[] } | null): { configured: boolean; items: Notam[]; errors: string[] } {
  const items = (v?.items ?? []).map(fromJson)
  items.sort((a, b) => RANK[a.effect.kind] - RANK[b.effect.kind] || a.location.localeCompare(b.location))
  return { configured: !!v?.configured, items, errors: (v?.errors ?? []).map(String) }
}
