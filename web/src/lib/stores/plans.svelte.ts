// Flight plans saved in the app under a name: the route with its fuel stops, altitude and
// ground time, the aircraft, and a summary for the list. Saved on the server one by one, with a
// copy in the browser; the server's change stream (/api/events) brings in plans saved anywhere
// else (another browser, or the desktop app connected to this server) straight away.
import { api } from '../api'
import { aircraft, pullAircraft } from './aircraft.svelte'
import { route, type Waypoint } from './route.svelte'

export interface SavedPlan {
  id: string
  name: string
  /** ISO time of saving */
  savedAt: string
  route: { waypoints: Waypoint[]; cruise: number; departure: string; stopMinutes: number }
  /** the route as text, e.g. "KSDL PXR V327 FLG KGCN" */
  routeText: string
  /** fuel-stop airports, in order (also flagged on the route's waypoints) */
  fuelStops: string[]
  aircraftId: string
  aircraft: string
  distanceNm: number
  eteMin: number
  fuelGal: number
}

function load(): SavedPlan[] {
  try {
    const raw = localStorage.getItem('fp.plans')
    if (raw) return JSON.parse(raw)
  } catch {
    /* ignore */
  }
  return []
}

class Plans {
  list = $state<SavedPlan[]>(load())

  private keepCopy() {
    try {
      localStorage.setItem('fp.plans', JSON.stringify(this.list))
    } catch {
      /* ignore */
    }
  }

  /** The server's list (after a change anywhere). */
  async refresh() {
    try {
      const v = await api.plans()
      if (Array.isArray(v)) {
        this.list = v
        this.keepCopy()
      }
    } catch {
      /* offline: keep what we have */
    }
  }

  /** Save the current route as a named plan (replacing one with the same name). */
  save(name: string, summary: { distanceNm: number; eteMin: number; fuelGal: number }) {
    const n = name.trim() || route.text
    const ac = aircraft.active
    const p: SavedPlan = {
      id: this.list.find((x) => x.name === n)?.id ?? Math.random().toString(36).slice(2, 10),
      name: n,
      savedAt: new Date().toISOString(),
      route: { waypoints: $state.snapshot(route.waypoints), cruise: route.cruise, departure: route.departure, stopMinutes: route.stopMinutes },
      routeText: route.text,
      fuelStops: route.fuelStops.map((i) => route.waypoints[i].id),
      aircraftId: ac?.id ?? '',
      aircraft: ac ? (ac.tail ? `${ac.tail} · ${ac.name}` : ac.name) : '',
      ...summary,
    }
    this.list = [p, ...this.list.filter((x) => x.name !== n)]
    this.keepCopy()
    api.savePlan($state.snapshot(p)).catch(() => {})
    return n
  }

  /** Load a saved plan into the route (and select its aircraft if it's still in the fleet). */
  load(id: string) {
    const p = this.list.find((x) => x.id === id)
    if (!p) return null
    // a copy, not the list's own objects (structuredClone throws on Svelte's state proxies)
    route.waypoints = $state.snapshot(p.route.waypoints)
    route.cruise = p.route.cruise
    route.departure = p.route.departure
    route.stopMinutes = p.route.stopMinutes || 45
    route.editing = false
    if (aircraft.list.some((a) => a.id === p.aircraftId)) aircraft.activeId = p.aircraftId
    return p
  }

  remove(id: string) {
    this.list = this.list.filter((x) => x.id !== id)
    this.keepCopy()
    api.deletePlan(id).catch(() => {})
  }
}

export const plans = new Plans()

plans.refresh()

// live updates: the browser reconnects by itself if the stream drops
try {
  const events = new EventSource('/api/events')
  events.onmessage = (e) => {
    try {
      const what = JSON.parse(e.data).what
      if (what === 'plans' || what === 'all') plans.refresh()
      // aircraft saved in the desktop app (or another browser)
      if (what === 'aircraft' || what === 'all') pullAircraft()
    } catch {
      /* not ours */
    }
  }
} catch {
  /* no EventSource: the list loads at start only */
}
