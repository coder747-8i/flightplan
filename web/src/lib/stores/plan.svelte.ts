// Derived flight analysis for the active route: terrain/airspace profile, winds aloft and
// the navlog. Re-fetches (debounced) whenever the route, altitude, time or aircraft changes.
import { api, type ProfileObstacle, type ProfileSample, type WindPoint } from '../api'
import { buildNavLog, type NavLog } from '../nav/navlog'
import { analyze, type Hazard } from '../nav/hazards'
import { wx } from '../wx/WeatherLayer.svelte'
import { aircraft } from './aircraft.svelte'
import { route } from './route.svelte'
import { timed } from '../perf'

export interface Crossing {
  kind: string
  name: string
  lo: number
  hi: number
  lo_agl: boolean
  times: string
  from: number
  to: number
  /** simplified outline (polygons -> rings -> [lon, lat]) for highlighting on the map */
  geom?: [number, number][][][]
}

/** a stretch of the route where the planned altitude is inside a volume */
export interface Incursion {
  c: Crossing
  from: number
  to: number
  level: 'warning' | 'caution' | 'info'
}

export interface TerrainConflict {
  from: number
  to: number
  level: 'warn' | 'bad'
}

const INCURSION_LEVEL: Record<string, Incursion['level'] | undefined> = {
  P: 'warning', R: 'warning', B: 'caution', C: 'caution', MOA: 'caution', D: 'info', W: 'info', A: 'info',
}

// Bulk data (samples, airspace, weather) is stored with $state.raw: it is always replaced whole,
// and deep proxies made every coordinate read in the geometry loops go through reactivity.
class PlanState {
  samples = $state.raw<ProfileSample[]>([])
  obstacles = $state.raw<ProfileObstacle[]>([])
  airspace = $state.raw<Crossing[]>([])
  winds = $state.raw<(WindPoint | undefined)[]>([])
  windSource = $state('')
  loading = $state(false)
  error = $state('')
  /** field elevations at either end (from the terrain profile, refined by airport data) */
  depElev = $state(0)
  destElev = $state(0)
  /** METARs and runway headings for the departure and destination airports */
  endMetars = $state.raw<Record<string, any>>({})
  endRunways = $state.raw<Record<string, { id: string; hdg: number }[]>>({})
  /** along-track NM under the pointer, shared by the map and the profile */
  hover = $state<{ d: number; from: 'map' | 'profile' } | null>(null)
  /** airspace picked in the profile (highlighted on the map) */
  picked = $state.raw<Crossing | null>(null)

  departureTime = $derived(route.departure ? new Date(route.departure) : new Date())

  // Everything below is memoised with $derived: these are read per terrain sample and on every
  // pointer move, so recomputing them on each read froze the page.
  log = $derived.by((): NavLog | null => {
    const ac = aircraft.active
    if (!ac || route.waypoints.length < 2) return null
    return timed('navlog', () => buildNavLog({
      waypoints: route.waypoints,
      aircraft: ac,
      cruise: route.cruise,
      departure: this.departureTime,
      depElev: this.depElev,
      destElev: this.destElev,
      winds: this.winds,
    }))
  })

  /** planned altitude along the route: [along-track NM, ft MSL] */
  altitudePath = $derived.by((): [number, number][] => {
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
  })

  total = $derived(this.samples.length ? this.samples[this.samples.length - 1].d : 0)

  /** planned altitude at along-track distance d */
  altAt(d: number) {
    const p = this.altitudePath
    for (let i = 1; i < p.length; i++) {
      if (d <= p[i][0]) {
        const f = (d - p[i - 1][0]) / Math.max(p[i][0] - p[i - 1][0], 1e-6)
        return p[i - 1][1] + (p[i][1] - p[i - 1][1]) * f
      }
    }
    return p[p.length - 1]?.[1] ?? 0
  }

  /** stretches with under 1,000 / 500 ft clearance over the highest terrain within 1 NM */
  terrainConflicts = $derived.by((): TerrainConflict[] => {
    const out: TerrainConflict[] = []
    if (!this.altitudePath.length) return out
    const total = this.total
    for (const s of this.samples) {
      if (s.d < 5 || s.d > total - 5) continue // departure climb and pattern entry
      const clr = this.altAt(s.d) - s.max
      const level = clr < 500 ? 'bad' : clr < 1000 ? 'warn' : null
      if (!level) continue
      const last = out[out.length - 1]
      if (last && last.level === level && s.d - last.to < 0.6) last.to = s.d
      else out.push({ from: s.d, to: s.d, level })
    }
    return out
  })

  /** where the planned altitude actually enters controlled / special-use airspace */
  incursions = $derived.by((): Incursion[] => timed('incursions', () => {
    const out: Incursion[] = []
    for (const c of this.airspace) {
      const level = INCURSION_LEVEL[c.kind]
      if (!level) continue
      let cur: Incursion | null = null
      for (const s of this.samples) {
        if (s.d < c.from || s.d > c.to) continue
        const alt = this.altAt(s.d)
        const floor = c.lo_agl ? s.elev + c.lo : c.lo
        const inside = alt >= floor && alt <= c.hi
        if (inside && !cur) cur = { c, from: s.d, to: s.d, level }
        else if (inside && cur) cur.to = s.d
        else if (!inside && cur) {
          out.push(cur)
          cur = null
        }
      }
      if (cur) out.push(cur)
    }
    return out.sort((a, b) => a.from - b.from)
  }))

  hazards = $derived.by((): Hazard[] =>
    timed('hazards', () => analyze({
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
    })),
  )
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
