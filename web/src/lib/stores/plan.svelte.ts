// Derived flight analysis for the active route: terrain/airspace profile, winds aloft and the
// navlog, hazards, departure / fuel-stop / destination forecasts, the official outlooks the route
// crosses, NOTAMs and the go / no-go verdict, plus the call for departures every 3 h over the
// next five days (the timeline's windows). Refetches (debounced) when the route changes;
// recomputes the cheap parts whenever any input changes.
import { untrack } from 'svelte'
import { api, type OfficialHit, type ProfileObstacle, type ProfileSample, type WindPoint } from '../api'
import { buildNavLog, type NavLog } from '../nav/navlog'
import { analyze, type Hazard } from '../nav/hazards'
import { evaluate, type Call, type End, type Verdict } from '../nav/verdict'
import { distanceNm, pointAlong } from '../nav/geo'
import { conditionsAt, stationWx, type StationWx } from '../wx/forecast'
import { parse as parseNotams, type Notam } from '../wx/notams'
import { products, spcDay, wpcDay } from '../wx/radar.svelte'
import { wx } from '../wx/WeatherLayer.svelte'
import { aircraft, type Aircraft } from './aircraft.svelte'
import { config } from './config.svelte'
import { minimums } from './minimums.svelte'
import { route, wxId } from './route.svelte'
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

/** what the airport database says about an airport on the route */
interface AirportInfo {
  elev: number | null
  runways: { id: string; hdg: number }[]
  artcc: string
}

const INCURSION_LEVEL: Record<string, Incursion['level'] | undefined> = {
  P: 'warning', R: 'warning', B: 'caution', C: 'caution', MOA: 'caution', D: 'info', W: 'info', A: 'info',
}

function altOn(p: [number, number][], d: number) {
  for (let i = 1; i < p.length; i++) {
    if (d <= p[i][0]) {
      const f = (d - p[i - 1][0]) / Math.max(p[i][0] - p[i - 1][0], 1e-6)
      return p[i - 1][1] + (p[i][1] - p[i - 1][1]) * f
    }
  }
  return p[p.length - 1]?.[1] ?? 0
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
  /** route geometry the profile in hand belongs to */
  profileKey = $state('')
  /** field elevations at either end (from the terrain profile, refined by airport data) */
  depElev = $state(0)
  destElev = $state(0)
  /** every airport on the route: elevation, runways (true headings), ARTCC */
  airports = $state.raw<Record<string, AirportInfo>>({})
  /** along-track NM under the pointer, shared by the map and the profile */
  hover = $state<{ d: number; from: 'map' | 'profile' } | null>(null)
  /** airspace picked in the profile (highlighted on the map) */
  picked = $state.raw<Crossing | null>(null)

  /** departure, fuel-stop and destination weather (METAR / TAF / NBM) */
  dep = $state.raw<StationWx | null>(null)
  dest = $state.raw<StationWx | null>(null)
  stops = $state.raw<StationWx[]>([])
  /** WPC forecast chart and SPC outlook areas the route crosses, all days */
  official = $state.raw<OfficialHit[]>([])
  forecastLoading = $state(false)
  /** route NOTAMs */
  notams = $state.raw<Notam[]>([])
  notamErrors = $state.raw<string[]>([])
  notamsLoading = $state(false)
  /** a NOTAM fetch has finished for the current route */
  notamsDone = $state(false)
  /** moves every 5 minutes, so calls that depend on "now" are made again */
  tick = $state(Math.floor(Date.now() / 300000))

  departureTime = $derived.by(() => {
    this.tick
    return route.departureTime
  })

  /** fuel stops with their field elevation (the database's, else the terrain there) */
  stopElevs = $derived.by((): [number, number][] => {
    const w = route.waypoints
    let along = 0
    const out: [number, number][] = []
    for (let i = 0; i < w.length; i++) {
      if (i > 0) along += distanceNm(w[i - 1].lat, w[i - 1].lon, w[i].lat, w[i].lon)
      if (!w[i].fuelStop || i === 0 || i === w.length - 1) continue
      let elev = this.airports[w[i].id]?.elev ?? null
      if (elev == null && this.samples.length) elev = this.samples.reduce((a, b) => (Math.abs(b.d - along) < Math.abs(a.d - along) ? b : a)).elev
      out.push([i, elev ?? 0])
    }
    return out
  })

  /** Navlog for a departure time. */
  logAt(ac: Aircraft, departure: Date): NavLog {
    return buildNavLog({
      waypoints: route.waypoints,
      aircraft: ac,
      cruise: route.cruise,
      departure,
      depElev: this.depElev,
      destElev: this.destElev,
      winds: this.winds,
      stops: this.stopElevs,
      groundMin: route.stopMinutes,
    })
  }

  /** Hazards for a navlog. */
  hazardsFor(log: NavLog, departure: Date): Hazard[] {
    const altitudePath = log.rows.length ? log.altPath : []
    return analyze({
      waypoints: route.waypoints,
      samples: this.samples,
      obstacles: this.obstacles,
      airspace: this.airspace,
      altitudePath,
      log,
      aircraft: aircraft.active,
      departure,
      tfrs: wx.tfrs,
      sigmets: wx.sigmets,
      gairmets: wx.gairmets,
      stopDists: log.segments.slice(0, -1).map((s) => s.endD),
    })
  }

  // Everything below is memoised with $derived: these are read per terrain sample and on every
  // pointer move, so recomputing them on each read froze the page.
  log = $derived.by((): NavLog | null => {
    const ac = aircraft.active
    if (!ac || route.waypoints.length < 2) return null
    return timed('navlog', () => this.logAt(ac, this.departureTime))
  })

  /** planned altitude along the route: [along-track NM, ft MSL] */
  altitudePath = $derived.by((): [number, number][] => {
    const log = this.log
    return log && log.rows.length ? log.altPath : []
  })

  total = $derived(this.samples.length ? this.samples[this.samples.length - 1].d : 0)

  /** planned altitude at along-track distance d */
  altAt(d: number) {
    return altOn(this.altitudePath, d)
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

  hazards = $derived.by((): Hazard[] => {
    const log = this.log
    return log ? timed('hazards', () => this.hazardsFor(log, this.departureTime)) : []
  })

  /** judge only with the profile for this route and the first forecasts in hand: a call made
   *  without terrain or weather would read as a false "go" */
  ready = $derived(this.profileKey === JSON.stringify(route.path) && !this.loading && this.samples.length > 0 && !(this.forecastLoading && !this.dep))

  /** The go / no-go call for the plan at a departure time. */
  judge(ac: Aircraft, log: NavLog, hazards: Hazard[], departure: Date): Verdict {
    const w = route.waypoints
    const n = w.length
    const eta = new Date(departure.getTime() + log.totalTime * 60000)
    const info = (id: string) => this.airports[id]
    // departure, each fuel stop at its arrival time, destination
    const points: { role: End['role']; i: number; stn: StationWx | null; elev: number; when: Date }[] = [{ role: 'Departure', i: 0, stn: this.dep, elev: this.depElev, when: departure }]
    for (const seg of log.segments) {
      if (seg.toIndex >= n - 1) continue
      const id = w[seg.toIndex].id
      points.push({ role: 'Fuel stop', i: seg.toIndex, stn: this.stops.find((s) => s.airport === id) ?? null, elev: info(id)?.elev ?? 0, when: seg.arrive })
    }
    points.push({ role: 'Destination', i: n - 1, stn: this.dest, elev: this.destElev, when: eta })
    const ends: End[] = points.map((p) => {
      const wp = w[p.i]
      return {
        role: p.role,
        airport: wp.id,
        isAirport: wp.kind === 'airport' && (!!p.stn || !this.forecastLoading),
        cond: p.stn ? conditionsAt(p.stn, p.when) : null,
        runways: info(wp.id)?.runways ?? [],
        elev: p.elev,
        offsetNm: p.stn?.offsetNm ?? 0,
        when: p.when,
      }
    })
    // the official outlook days the flight spans
    const [spcA, spcB] = [spcDay(departure), spcDay(eta)]
    const [wpcA, wpcB] = [wpcDay(departure), wpcDay(eta)]
    const official = this.official.filter((h) => (h.product.startsWith('SPC') ? h.day >= spcA && h.day <= spcB : h.day >= wpcA && h.day <= wpcB))
    return evaluate({
      hazards,
      ends,
      official,
      officialKnown: !this.forecastLoading,
      notams: config.notams && this.notamsDone ? this.notams : null,
      routeNavaids: w.filter((x) => x.kind === 'navaid').map((x) => x.id),
      log,
      maxTerrainFt: this.samples.reduce((m, s) => Math.max(m, s.max), 0),
      maxXwind: ac.perf.maxXwind ?? 15,
      minimums,
      departure,
      now: new Date(),
      dataExpired: config.dataExpired,
    })
  }

  verdict = $derived.by((): Verdict | null => {
    const ac = aircraft.active
    const log = this.log
    if (!ac || !log || !this.ready) return null
    JSON.stringify(minimums) // track every minimum
    this.products()
    return timed('verdict', () => this.judge(ac, log, this.hazards, this.departureTime))
  })

  /** the call for departures every 3 h over the next five days: [ms, call]. Computed in the
   *  background once the inputs settle (41 full checks are too slow for every keystroke). */
  windows = $state.raw<[number, Call][]>([])

  computeWindows(): [number, Call][] {
    const ac = aircraft.active
    if (!ac || route.waypoints.length < 2 || !this.ready || this.forecastLoading) return []
    return timed('windows', () => {
      const start = (Math.floor(Date.now() / 3600000) + 1) * 3600000
      const out: [number, Call][] = []
      for (let i = 0; i <= 40; i++) {
        const t = new Date(start + i * 3 * 3600000)
        const log = this.logAt(ac, t)
        out.push([t.getTime(), this.judge(ac, log, this.hazardsFor(log, t), t).call])
      }
      return out
    })
  }

  /** read the outlook day boundaries so the calls redo when they arrive */
  private products() {
    return [products.wpcDay1End, products.spcDay1End]
  }
}

export const plan = new PlanState()

let timer: ReturnType<typeof setTimeout>
let seq = 0

async function airportInfo(id: string): Promise<AirportInfo | null> {
  try {
    const a = await api.airport(id)
    const runways: AirportInfo['runways'] = []
    for (const r of a?.runways ?? []) for (const e of r.ends ?? []) if (e.trueHdg != null) runways.push({ id: e.id, hdg: e.trueHdg })
    return { elev: a?.elev ?? null, runways, artcc: a?.artcc ?? '' }
  } catch {
    return null
  }
}

let forecastKey = ''
let forecastAt = 0
let forecastSeq = 0
let notamKey = ''
let notamAt = 0
let notamSeq = 0

/** Departure / fuel-stop / destination forecasts and the official outlooks the route crosses,
 *  refreshed when the route changes and every 30 minutes. */
function refreshForecast(force = false) {
  const w = route.waypoints
  const n = w.length
  const key = w.map((x) => `${x.id}${x.lat.toFixed(3)}${x.lon.toFixed(3)}${x.fuelStop ? 'F' : ''}`).join(' ')
  if (!force && key === forecastKey && Date.now() - forecastAt < 1800_000) return
  forecastKey = key
  forecastAt = Date.now()
  const my = ++forecastSeq
  if (n < 2) {
    plan.dep = null
    plan.dest = null
    plan.stops = []
    plan.official = []
    plan.forecastLoading = false
    return
  }
  plan.forecastLoading = true
  const ends = w.filter((x, i) => i === 0 || i === n - 1 || (x.fuelStop && x.kind === 'airport'))
  const stations = Promise.all(
    ends.map((x) =>
      x.kind === 'airport'
        ? api.station(x.id, wxId(x.id), x.lat, x.lon).then(stationWx).catch(() => null)
        : Promise.resolve(null),
    ),
  )
  const official = api.official(w.map((x) => [x.lon, x.lat])).catch(() => [] as OfficialHit[])
  Promise.all([stations, official]).then(([list, hits]) => {
    if (my !== forecastSeq) return
    plan.dep = list[0]
    plan.dest = list.length > 1 ? list[list.length - 1] : null
    plan.stops = list.slice(1, -1).filter((s): s is StationWx => !!s)
    plan.official = hits
    plan.forecastLoading = false
  })
}

/** NOTAMs for the route: airports and navaids by id, the route every ~25 NM and the ARTCCs
 *  crossed. Refreshed when the route changes and every 15 minutes. */
function refreshNotams(force = false) {
  const w = route.waypoints
  const configured = config.notams
  const key = `${configured}${w.map((x) => x.id).join(' ')}`
  if (!force && key === notamKey && Date.now() - notamAt < 900_000) return
  notamKey = key
  notamAt = Date.now()
  const my = ++notamSeq
  plan.notamsDone = false
  if (w.length < 2 || !configured) {
    plan.notams = []
    plan.notamErrors = []
    plan.notamsDone = true
    plan.notamsLoading = false
    return
  }
  const locations = w.filter((x) => x.kind === 'airport' || x.kind === 'navaid').map((x) => (x.kind === 'airport' ? wxId(x.id) : x.id))
  const path = w.map((x) => [x.lon, x.lat] as [number, number])
  let total = 0
  for (let i = 1; i < w.length; i++) total += distanceNm(w[i - 1].lat, w[i - 1].lon, w[i].lat, w[i].lon)
  const k = Math.min(30, Math.max(1, Math.ceil(total / 25)))
  const points = Array.from({ length: k + 1 }, (_, i) => pointAlong(path, (total * i) / k))
  plan.notamsLoading = true
  // ARTCCs come from the airport records (fetched with the profile)
  const artccs = () => [...new Set(w.filter((x) => x.kind === 'airport').map((x) => plan.airports[x.id]?.artcc).filter((s): s is string => !!s))].sort()
  const go = (a: string[]) =>
    api
      .notams({ locations, points, radius: 10, artccs: a })
      .then((v) => {
        if (my !== notamSeq) return
        const p = parseNotams(v)
        plan.notams = p.items
        plan.notamErrors = p.errors
      })
      .catch((e) => my === notamSeq && (plan.notamErrors = [String(e.message ?? e)]))
      .finally(() => {
        if (my !== notamSeq) return
        plan.notamsLoading = false
        plan.notamsDone = true
      })
  // give the airport records a moment to arrive so the ARTCCs are known
  setTimeout(() => go(artccs()), Object.keys(plan.airports).length ? 0 : 1500)
}

$effect.root(() => {
  // terrain + airspace profile depends only on geometry
  $effect(() => {
    const path = route.path
    // ids and kinds only: toggling a fuel stop doesn't change the geometry
    const w = route.waypoints.map((x) => ({ id: x.id, kind: x.kind }))
    clearTimeout(timer)
    if (path.length < 2) {
      plan.samples = []
      plan.obstacles = []
      plan.airspace = []
      plan.profileKey = ''
      return
    }
    const my = ++seq
    const key = JSON.stringify(path)
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
        // every airport: departure / destination elevations (more accurate than the DEM at the
        // field), runways for crosswind, field elevations for fuel stops, ARTCC for NOTAMs
        const ids = [...new Set(w.filter((x) => x.kind === 'airport').map((x) => x.id))]
        const infos = await Promise.all(ids.map(airportInfo))
        if (my !== seq) return
        const airports: Record<string, AirportInfo> = {}
        ids.forEach((id, i) => infos[i] && (airports[id] = infos[i]!))
        plan.airports = airports
        const first = w[0]
        const last = w[w.length - 1]
        if (first.kind === 'airport' && airports[first.id]?.elev != null) plan.depElev = airports[first.id].elev!
        if (last.kind === 'airport' && airports[last.id]?.elev != null) plan.destElev = airports[last.id].elev!
        plan.profileKey = key
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
    const my = JSON.stringify(pts)
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

  $effect(() => {
    JSON.stringify(route.waypoints)
    refreshForecast()
  })

  // the 5-day windows: everything the calls depend on, then recompute once it's quiet
  let windowsTimer: ReturnType<typeof setTimeout>
  $effect(() => {
    JSON.stringify(aircraft.active)
    JSON.stringify(route.waypoints)
    JSON.stringify(minimums)
    void [route.cruise, route.stopMinutes, plan.samples, plan.obstacles, plan.airspace, plan.winds, plan.airports, plan.depElev, plan.destElev]
    void [plan.dep, plan.dest, plan.stops, plan.official, plan.notams, plan.notamsDone, plan.ready, plan.forecastLoading, plan.tick]
    void [products.wpcDay1End, products.spcDay1End, wx.tfrs, wx.sigmets, wx.gairmets, config.notams, config.dataExpired]
    clearTimeout(windowsTimer)
    windowsTimer = setTimeout(() => (plan.windows = untrack(() => plan.computeWindows())), 500)
  })
  $effect(() => {
    config.notams
    route.waypoints.map((x) => x.id).join(' ')
    refreshNotams()
  })
})

// keep "now" moving and the slow refreshes going
products.refresh()
setInterval(() => {
  plan.tick = Math.floor(Date.now() / 300000)
  products.refresh()
  refreshForecast()
  refreshNotams()
}, 60_000)
