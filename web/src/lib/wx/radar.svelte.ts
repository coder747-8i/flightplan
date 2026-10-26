// The weather picture for any time in the next five days, always an official product shown as
// published. Now: the observed NEXRAD mosaic. Up to ~48 h: HRRR simulated reflectivity (Iowa
// Environmental Mesonet tiles; 15-minute steps from the latest run to 18 h, hourly from the
// latest extended run as far as IEM serves it). Then the WPC National Forecast Chart (days 1-3)
// and the WPC precipitation forecast (days 4-5) from NOAA's map service. The SPC convective
// outlook is its own layer (days 1-8). Nothing here is computed: it picks which product covers
// a time.
import { api } from '../api'

export const BASE = 'https://mapservices.weather.noaa.gov/vector/rest/services'
export const WPC_CHART = 'outlooks/natl_fcst_wx_chart'
export const WPC_QPF = 'precip/wpc_qpf'
export const SPC = 'outlooks/SPC_wx_outlks'
const IEM_TILES = 'https://mesonet.agron.iastate.edu/cache/tile.py/1.0.0'

/** The WPC chart's weather layers for a day (fronts and areas; no highs/lows or fire weather). */
export const wpcChartLayers = (day: number) => [2, 3, 4, 5, 6, 7, 8, 10, 11].map((l) => l + (day - 1) * 12)

/** SPC layer for a day: the categorical outlook for days 1-3, the probabilistic one for 4-8. */
export const spcLayer = (day: number) => (day === 1 ? 1 : day === 2 ? 9 : day === 3 ? 17 : 21 + (day - 4))

/** A MapLibre tile URL for some layers of a NOAA map service (it fills in the tile's bbox). */
export const exportUrl = (service: string, layers: number[]) =>
  `${BASE}/${service}/MapServer/export?bbox={bbox-epsg-3857}&bboxSR=3857&imageSR=3857&size=512,512&format=png32&transparent=true&layers=show:${layers.join(',')}&dpi=96&f=image`

/** When the current products' day 1 ends (unix s; they run 12Z to 12Z) and the HRRR runs. */
class Products {
  wpcDay1End = $state<number | null>(null)
  spcDay1End = $state<number | null>(null)
  hrrrShort = $state<number | null>(null)
  hrrrExtended = $state<number | null>(null)
  /** last forecast minute of the extended run IEM actually serves */
  hrrrExtendedLast = $state(1080)
  private outlooksAt = 0
  private runsAt = 0

  /** Refresh what is stale (outlooks every 30 min, HRRR runs every 10). */
  refresh() {
    const now = Date.now()
    if (now - this.outlooksAt > 1800_000) {
      this.outlooksAt = now
      api.outlooks().then((o) => {
        this.wpcDay1End = o.wpcDay1End
        this.spcDay1End = o.spcDay1End
      }).catch(() => (this.outlooksAt = 0))
    }
    if (now - this.runsAt > 600_000) {
      this.runsAt = now
      api.hrrr().then((r) => {
        this.hrrrShort = r.short
        this.hrrrExtended = r.extended
        this.hrrrExtendedLast = r.extendedLast
      }).catch(() => (this.runsAt = 0))
    }
  }

  /** The last time HRRR covers (ms). */
  get reach(): number | null {
    const ext = this.hrrrExtended != null ? (this.hrrrExtended + this.hrrrExtendedLast * 60) * 1000 : null
    const short = this.hrrrShort != null ? (this.hrrrShort + 18 * 3600) * 1000 : null
    if (ext == null) return short
    if (short == null) return ext
    return Math.max(ext, short)
  }
}

export const products = new Products()

/** The next 12Z after `now`, the usual end of a day-1 product. */
function next12z(now: Date): number {
  const t = Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), now.getUTCDate(), 12)
  return (now.getUTCHours() < 12 ? t : t + 86400000) / 1000
}

function day(end1: number, t: Date) {
  const s = t.getTime() / 1000
  if (s <= end1) return 1
  return Math.min(99, Math.max(1, 1 + Math.ceil((s - end1) / 86400)))
}

/** WPC chart day (1-3) covering `t`. */
export const wpcDay = (t: Date) => day(products.wpcDay1End ?? next12z(new Date()), t)
/** SPC outlook day (1-8) covering `t`. */
export const spcDay = (t: Date) => day(products.spcDay1End ?? next12z(new Date()), t)

export type FrameKind = 'observed' | 'hrrr' | 'wpcChart' | 'wpcQpf' | 'spc'

/** What the weather layer shows for a requested time. */
export interface Frame {
  kind: FrameKind
  /** WPC chart / SPC day */
  day?: number
  /** a key for the tile source: the IEM layer name or the NOAA product and issue */
  key: string
  tiles: string
  /** time the picture is valid for, ms (forecast charts: the end of their day) */
  valid: number
  /** model run, for HRRR (ms) */
  run?: number
}

const SOURCE: Record<FrameKind, string> = {
  observed: 'Radar (NEXRAD)',
  hrrr: 'Forecast radar (HRRR)',
  wpcChart: 'WPC forecast chart',
  wpcQpf: 'WPC precipitation forecast',
  spc: 'SPC convective outlook',
}

const fmt = (ms: number) => new Date(ms).toLocaleString([], { weekday: 'short', hour: '2-digit', minute: '2-digit', hour12: false })

/** "Forecast radar (HRRR) · Sun 14:00 · 03Z run" */
export function frameLabel(f: Frame): string {
  switch (f.kind) {
    case 'observed':
      return `${SOURCE.observed} · live`
    case 'hrrr':
      return `${SOURCE.hrrr} · ${fmt(f.valid)}${f.run ? ` · ${String(new Date(f.run).getUTCHours()).padStart(2, '0')}Z run` : ''}`
    case 'wpcChart':
      return `${SOURCE.wpcChart} · day ${f.day}, through ${fmt(f.valid)}`
    case 'wpcQpf':
      return `${SOURCE.wpcQpf} · days 4-5`
    case 'spc':
      return `${SOURCE.spc} · day ${f.day}`
  }
}

const hrrr = (m: number, run: number): Frame => {
  const layer = `hrrr::REFD-F${String(m).padStart(4, '0')}-0`
  return { kind: 'hrrr', key: layer, tiles: `${IEM_TILES}/${layer}/{z}/{x}/{y}.png`, valid: (run + m * 60) * 1000, run: run * 1000 }
}

/** The frame to show for `when` (null past what the forecasts cover). */
export function frameFor(when: Date, now = new Date()): Frame | null {
  const p = products
  if ((when.getTime() - now.getTime()) / 60000 < 8) {
    // the mosaic updates every ~5 minutes
    const bucket = Math.floor(now.getTime() / 300000) * 300000
    return { kind: 'observed', key: `nexrad-${bucket}`, tiles: `${IEM_TILES}/nexrad-n0q-900913/{z}/{x}/{y}.png?${bucket}`, valid: bucket }
  }
  const s = when.getTime() / 1000
  if (p.hrrrShort != null) {
    const m = Math.round((s - p.hrrrShort) / 60 / 15) * 15
    if (m >= 0 && m <= 1080) return hrrr(m, p.hrrrShort)
  }
  if (p.hrrrExtended != null) {
    const m = Math.round((s - p.hrrrExtended) / 3600) * 60
    if (m >= 1140 && m <= p.hrrrExtendedLast) return hrrr(m, p.hrrrExtended)
  }
  const d = wpcDay(when)
  const end1 = p.wpcDay1End ?? now.getTime() / 1000
  if (d <= 3) return { kind: 'wpcChart', day: d, key: `wpc-chart-${d}-${end1}`, tiles: exportUrl(WPC_CHART, wpcChartLayers(d)), valid: (end1 + (d - 1) * 86400) * 1000 }
  if (d <= 5) return { kind: 'wpcQpf', key: `wpc-qpf45-${end1}`, tiles: exportUrl(WPC_QPF, [4]), valid: (end1 + 4 * 86400) * 1000 }
  return null
}

/** The SPC convective outlook covering `when` (days 1-8). */
export function spcFrame(when: Date): Frame | null {
  const d = spcDay(when)
  const end1 = products.spcDay1End ?? Date.now() / 1000
  return d <= 8 ? { kind: 'spc', day: d, key: `spc-${d}-${end1}`, tiles: exportUrl(SPC, [spcLayer(d)]), valid: (end1 + (d - 1) * 86400) * 1000 } : null
}

/** NWS reflectivity colours (dBZ), for the radar legend. */
export const DBZ = [
  [10, '#6ec9e8'], [20, '#2f9e5a'], [30, '#1d8f23'], [40, '#e8a21d'], [50, '#c81e1e'], [60, '#a8145c'],
] as const
