// Derived flight analysis for the active route: terrain/airspace profile, winds aloft and
// the navlog. Re-fetches (debounced) whenever the route, altitude, time or aircraft changes.
import { api, type ProfileObstacle, type ProfileSample, type WindPoint } from '../api'
import { buildNavLog, type NavLog } from '../nav/navlog'
import { analyze, type Hazard } from '../nav/hazards'
import { wx } from '../wx/WeatherLayer.svelte'
import { aircraft } from './aircraft.svelte'
import { route } from './route.svelte'

export interface Crossing {
  kind: string
  name: string
  lo: number
  hi: number
  lo_agl: boolean
  times: string
  from: number
  to: number
}

class PlanState {
  samples = $state<ProfileSample[]>([])
  obstacles = $state<ProfileObstacle[]>([])
  airspace = $state<Crossing[]>([])
  winds = $state<(WindPoint | undefined)[]>([])
  windSource = $state('')
  loading = $state(false)
  error = $state('')
  /** field elevations at either end (from the terrain profile, refined by airport data) */
  depElev = $state(0)
  destElev = $state(0)
  /** METARs and runway headings for the departure and destination airports */
  endMetars = $state<Record<string, any>>({})
  endRunways = $state<Record<string, { id: string; hdg: number }[]>>({})

  get departureTime(): Date {
    return route.departure ? new Date(route.departure) : new Date()
  }

  get log(): NavLog | null {
    const ac = aircraft.active
    if (!ac || route.waypoints.length < 2) return null
    return buildNavLog({
      waypoints: route.waypoints,
      aircraft: ac,
      cruise: route.cruise,
      departure: this.departureTime,
      depElev: this.depElev,
      destElev: this.destElev,
      winds: this.winds,
    })
  }

  get hazards(): Hazard[] {
    return analyze({
      waypoints: route.waypoints,
      samples: this.samples,
      obstacles: this.obstacles,
      airspace: this.airspace,
      altitudePath: this.altitudePath,
      log: this.log,
      aircraft: aircraft.active,
      departure: this.departureTime,
      tfrs: wx.tfrs,
      sigmets: wx.sigmets,
      gairmets: wx.gairmets,
      metars: this.endMetars,
      runways: this.endRunways,
    })
  }

  /** planned altitude along the route: [along-track NM, ft MSL] */
  get altitudePath(): [number, number][] {
    const log = this.log
    if (!log || !log.rows.length) return []
    const toc = log.tocDist ?? 0
    const tod = log.todDist ?? log.totalDist
    const cruise = Math.max(route.cruise, this.depElev)
    return [
      [0, this.depElev],
      [toc, cruise],
      [tod, cruise],
      [log.totalDist, this.destElev + 1000], // arrive at pattern altitude
    ]
  }
}

export const plan = new PlanState()

let timer: ReturnType<typeof setTimeout>
let seq = 0

$effect.root(() => {
  // terrain + airspace profile depends only on geometry
  $effect(() => {
    const path = route.path
    clearTimeout(timer)
    if (path.length < 2) {
      plan.samples = []
      plan.obstacles = []
      plan.airspace = []
      return
    }
    const my = ++seq
    timer = setTimeout(async () => {
      plan.loading = true
      plan.error = ''
      try {
        const res = await api.profile(path)
        if (my !== seq) return
        plan.samples = res.samples
        plan.obstacles = res.obstacles
        plan.airspace = (res as any).airspace ?? []
        plan.depElev = res.samples[0]?.elev ?? 0
        plan.destElev = res.samples[res.samples.length - 1]?.elev ?? 0
        // airport elevations are more accurate than the DEM at the field
        const ends = [route.waypoints[0], route.waypoints[route.waypoints.length - 1]]
        await Promise.all(
          ends.map(async (w, i) => {
            if (w?.kind !== 'airport') return
            try {
              const a = await api.airport(w.id)
              if (my !== seq || a?.elev == null) return
              if (i === 0) plan.depElev = a.elev
              else plan.destElev = a.elev
              const rw: { id: string; hdg: number }[] = []
              for (const r of a.runways ?? []) for (const e of r.ends ?? []) if (e.trueHdg != null) rw.push({ id: e.id, hdg: e.trueHdg })
              plan.endRunways = { ...plan.endRunways, [w.id]: rw }
              const wxId = a.icao || (a.id.length === 3 ? `K${a.id}` : a.id)
              api.metar([wxId]).then((m) => m[0] && (plan.endMetars = { ...plan.endMetars, [w.id.length === 3 ? `K${w.id}` : w.id]: m[0] })).catch(() => {})
            } catch {
              /* keep DEM value */
            }
          }),
        )
      } catch (e) {
        if (my === seq) plan.error = String(e)
      } finally {
        if (my === seq) plan.loading = false
      }
    }, 250)
  })

  // winds at each waypoint
  $effect(() => {
    const pts = route.path
    if (pts.length < 2) {
      plan.winds = []
      return
    }
    const key = JSON.stringify(pts)
    const my = key
    api
      .winds(pts)
      .then((r) => {
        if (JSON.stringify(route.path) !== my) return
        plan.winds = r.points
        plan.windSource = r.source
      })
      .catch(() => {
        plan.winds = []
      })
  })
})
