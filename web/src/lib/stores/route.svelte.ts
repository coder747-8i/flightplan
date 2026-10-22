// The active flight plan: ordered waypoints plus planning settings.
import type { Place } from '../api'
import { save } from './app.svelte'

export interface Waypoint extends Place {
  uid: string
  /** optional altitude override for the leg starting here */
  alt?: number
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

class RouteState {
  waypoints = $state<Waypoint[]>(load('route.wps', []))
  cruise = $state<number>(load('route.cruise', 6500))
  departure = $state<string>(load('route.departure', '')) // ISO local datetime; '' = now
  aircraftId = $state<string>(load('route.aircraft', ''))
  /** set when the user is clicking on the map to add points */
  editing = $state(false)

  get legs() {
    const w = this.waypoints
    return w.slice(1).map((b, i) => ({ from: w[i], to: b, index: i }))
  }

  get path(): [number, number][] {
    return this.waypoints.map((w) => [w.lon, w.lat])
  }

  add(p: Place, at?: number) {
    const wp = { ...p, uid: uid() }
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

  get text() {
    return this.waypoints.map((w) => w.id).join(' ')
  }
}

function dist(a: { lat: number; lon: number }, b: { lat: number; lon: number }) {
  const k = Math.cos(((a.lat + b.lat) / 2) * (Math.PI / 180))
  return Math.hypot(a.lat - b.lat, (a.lon - b.lon) * k)
}

export const route = new RouteState()

$effect.root(() => {
  $effect(() => save('route.wps', route.waypoints))
  $effect(() => save('route.cruise', route.cruise))
  $effect(() => save('route.departure', route.departure))
  $effect(() => save('route.aircraft', route.aircraftId))
})
