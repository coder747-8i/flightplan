// The active flight plan: ordered waypoints plus planning settings.
import type { Place } from '../api'
import { save } from './app.svelte'

export interface Waypoint extends Place {
  uid: string
  /** optional altitude override for the leg starting here */
  alt?: number
  /** land here and top off before going on (airports in the middle of the route) */
  fuelStop?: boolean
}

function load<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(`fp.${key}`)
    if (raw) return JSON.parse(raw)
  } catch {
    /* ignore */
  }
  return fallback
}

const uid = () => Math.random().toString(36).slice(2, 10)

/** The airway a point was expanded from ("V327" from a detail like "V327 · MEA 8000"). */
export function airwayOf(p: Pick<Place, 'kind' | 'detail'>): string | null {
  if (p.kind === 'airport' || p.kind === 'coord') return null
  const tok = (p.detail ?? '').split(/\s+/)[0] ?? ''
  return /^[VTJQABGR]\d{1,4}$/.test(tok) ? tok : null
}

/** Route identifiers with airway runs written as the airway ("PXR V327 FLG"), the way flight
 *  plans and route strings write them. `id` gives each point's identifier. */
export function compactIds<P extends Pick<Place, 'kind' | 'detail'>>(places: P[], id: (p: P) => string): string[] {
  const out: string[] = []
  let last: string | null = null
  for (const p of places) {
    const a = airwayOf(p)
    if (a && a === last) continue
    out.push(a ?? id(p))
    last = a
  }
  return out
}

/** The identifier aviationweather.gov knows an airport by (K-prefix for 3-letter US ids). */
export const wxId = (id: string) => (id.length === 3 ? `K${id}` : id)

class RouteState {
  waypoints = $state<Waypoint[]>(load('route.wps', []))
  cruise = $state<number>(load('route.cruise', 6500))
  departure = $state<string>(load('route.departure', '')) // ISO local datetime; '' = now
  /** minutes on the ground at each fuel stop */
  stopMinutes = $state<number>(load('route.stopMinutes', 45))
  aircraftId = $state<string>(load('route.aircraft', ''))
  /** set when the user is clicking on the map to add points */
  editing = $state(false)
  /** view only: nothing on screen may change the plan */
  locked = $state<boolean>(load('route.locked', false))

  get legs() {
    const w = this.waypoints
    return w.slice(1).map((b, i) => ({ from: w[i], to: b, index: i }))
  }

  get path(): [number, number][] {
    return this.waypoints.map((w) => [w.lon, w.lat])
  }

  add(p: Place, at?: number) {
    const wp: Waypoint = { ...p, uid: uid() }
    if (at === undefined || at >= this.waypoints.length) this.waypoints.push(wp)
    else this.waypoints.splice(at, 0, wp)
  }

  remove(uidToRemove: string) {
    this.waypoints = this.waypoints.filter((w) => w.uid !== uidToRemove)
  }

  move(from: number, to: number) {
    const w = [...this.waypoints]
    const [it] = w.splice(from, 1)
    w.splice(to, 0, it)
    this.waypoints = w
  }

  replace(list: Place[]) {
    this.waypoints = list.map((p) => ({ ...p, uid: uid() }))
  }

  reverse() {
    this.waypoints = [...this.waypoints].reverse()
  }

  clear() {
    this.waypoints = []
  }

  /** Make an intermediate airport a fuel stop, or not. */
  toggleFuelStop(i: number) {
    const w = this.waypoints[i]
    if (w && i > 0 && i < this.waypoints.length - 1 && w.kind === 'airport') w.fuelStop = !w.fuelStop
  }

  /** waypoint indices of the fuel stops */
  get fuelStops(): number[] {
    const n = this.waypoints.length
    return this.waypoints.flatMap((w, i) => (w.fuelStop && i > 0 && i < n - 1 ? [i] : []))
  }

  /** insert a point into whichever leg it is closest to (used for rubber-banding) */
  insertNearest(p: Place) {
    const w = this.waypoints
    if (w.length < 2) return this.add(p)
    let best = 1
    let bestCost = Infinity
    for (let i = 1; i < w.length; i++) {
      const cost = dist(w[i - 1], p) + dist(p, w[i]) - dist(w[i - 1], w[i])
      if (cost < bestCost) {
        bestCost = cost
        best = i
      }
    }
    this.add(p, best)
  }

  /** the route as typed: identifiers, with airway runs written as the airway */
  get text() {
    return compactIds(this.waypoints, (w) => w.id).join(' ')
  }

  /** the departure time ('' = now) */
  get departureTime(): Date {
    const d = this.departure ? new Date(this.departure) : null
    return d && !isNaN(d.getTime()) ? d : new Date()
  }
}

function dist(a: { lat: number; lon: number }, b: { lat: number; lon: number }) {
  const k = Math.cos(((a.lat + b.lat) / 2) * (Math.PI / 180))
  return Math.hypot(a.lat - b.lat, (a.lon - b.lon) * k)
}

/** "YYYY-MM-DDTHH:MM" in local time, the format route.departure stores. */
export function localStamp(d: Date) {
  const p = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}T${p(d.getHours())}:${p(d.getMinutes())}`
}

export const route = new RouteState()

/** Set the departure time; within 10 minutes of now means "now". */
export function setDeparture(t: Date) {
  route.departure = t.getTime() - Date.now() < 10 * 60000 ? '' : localStamp(t)
}

$effect.root(() => {
  $effect(() => save('route.wps', route.waypoints))
  $effect(() => save('route.cruise', route.cruise))
  $effect(() => save('route.departure', route.departure))
  $effect(() => save('route.stopMinutes', route.stopMinutes))
  $effect(() => save('route.aircraft', route.aircraftId))
  $effect(() => save('route.locked', route.locked))
  // view only stops map editing too
  $effect(() => {
    if (route.locked) route.editing = false
  })
})
