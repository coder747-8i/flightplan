// Flight plans saved in the app under a name: the route with its fuel stops, altitude and
// ground time, the aircraft, and a summary for the list. Saved on the server (shared by every
// device) with a copy in the browser.
import { api } from '../api'
import { aircraft } from './aircraft.svelte'
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

  private persist() {
    try {
      localStorage.setItem('fp.plans', JSON.stringify(this.list))
    } catch {
      /* ignore */
    }
    api.userPut('plans', $state.snapshot(this.list)).catch(() => {})
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
    this.persist()
    return n
  }

  /** Load a saved plan into the route (and select its aircraft if it's still in the fleet). */
  load(id: string) {
    const p = this.list.find((x) => x.id === id)
    if (!p) return null
    route.waypoints = structuredClone(p.route.waypoints)
    route.cruise = p.route.cruise
    route.departure = p.route.departure
    route.stopMinutes = p.route.stopMinutes || 45
    route.editing = false
    if (aircraft.list.some((a) => a.id === p.aircraftId)) aircraft.activeId = p.aircraftId
    return p
  }

  remove(id: string) {
    this.list = this.list.filter((x) => x.id !== id)
    this.persist()
  }
}

export const plans = new Plans()

api
  .userGet<SavedPlan[]>('plans')
  .then((v) => {
    if (Array.isArray(v)) {
      plans.list = v
      try {
        localStorage.setItem('fp.plans', JSON.stringify(v))
      } catch {
        /* ignore */
      }
    }
  })
  .catch(() => {})
