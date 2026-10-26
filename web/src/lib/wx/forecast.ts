// Airport conditions at a planned time, up to about five days out: the METAR within the next
// hour, the TAF while it is valid, then the National Blend of Models text guidance (via the Iowa
// Environmental Mesonet API: NBS is 3-hourly with ceiling and visibility to 72 h, NBE is
// 12-hourly wind / sky / precipitation / thunder to ~10 days). The server picks the station
// (api.station); this reads what it sends.
import type { StationRaw } from '../api'
import { thousands } from '../nav/navlog'

export type Source = 'metar' | 'taf' | 'nbm' | 'nbmExtended'

export const SOURCE_LABEL: Record<Source, string> = { metar: 'METAR', taf: 'TAF', nbm: 'NBM forecast', nbmExtended: 'NBM extended forecast' }

/** Weather at one airport at one time, whatever the source. */
export interface Conditions {
  /** the station the numbers come from (may be a neighbour of the airport) */
  station: string
  source: Source
  valid: Date
  /** ceiling in ft AGL; null = no ceiling. Meaningless when `ceilingKnown` is false. */
  ceiling: number | null
  ceilingKnown: boolean
  /** statute miles; null = unknown */
  vis: number | null
  /** null = variable / calm */
  wdir: number | null
  wspd: number | null
  gust: number | null
  skyPct: number | null
  /** probability of precipitation, % */
  pop: number | null
  /** probability of thunder, % */
  tstm: number | null
  /** present / forecast weather, e.g. "-RA BR" or "TS" */
  wx: string
  tempC: number | null
  altimHpa: number | null
  /** worst TEMPO / PROB group covering the time: [category, short text] */
  tempo: [string, string] | null
}

const blank = (station: string, source: Source, valid: Date): Conditions => ({
  station, source, valid, ceiling: null, ceilingKnown: false, vis: null, wdir: null, wspd: null, gust: null, skyPct: null, pop: null, tstm: null, wx: '', tempC: null, altimHpa: null, tempo: null,
})

/** FAA flight category from ceiling (ft, null = none) and visibility (SM, null = unknown). */
export function category(ceiling: number | null, vis: number | null): string {
  const c = ceiling ?? Infinity
  const v = vis ?? Infinity
  if (c < 500 || v < 1) return 'LIFR'
  if (c < 1000 || v < 3) return 'IFR'
  if (c <= 3000 || v <= 5) return 'MVFR'
  return 'VFR'
}

/** VFR / MVFR / IFR / LIFR, when ceiling and visibility are both known. */
export function categoryOf(c: Conditions): string | null {
  if (!c.ceilingKnown && c.vis == null) return null
  return category(c.ceilingKnown ? c.ceiling : null, c.vis)
}

export const thunder = (c: Conditions) => c.wx.includes('TS') || !!c.tempo?.[1].includes('TS')

const trim = (v: number) => (Math.abs(v - Math.round(v)) < 0.05 ? String(Math.round(v)) : v.toFixed(1))

/** "Ceiling 2,500 ft · 4 SM · wind 240° 12G22 kt" */
export function summary(c: Conditions): string {
  const parts: string[] = []
  if (c.ceilingKnown) parts.push(c.ceiling != null ? `ceiling ${thousands(c.ceiling)} ft` : 'no ceiling')
  else if (c.skyPct != null) parts.push(`${Math.round(c.skyPct)}% sky cover`)
  if (c.vis != null) parts.push(c.vis >= 10 ? '10+ SM' : `${trim(c.vis)} SM`)
  const g = c.gust != null ? `G${Math.round(c.gust)}` : ''
  if (c.wspd != null) {
    if (c.wspd < 3 && c.gust == null) parts.push('calm')
    else if (c.wdir != null) parts.push(`wind ${String(Math.round(c.wdir)).padStart(3, '0')}° ${Math.round(c.wspd)}${g} kt`)
    else parts.push(`wind variable ${Math.round(c.wspd)}${g} kt`)
  }
  if (c.wx) parts.push(c.wx)
  if (c.tstm != null && c.tstm >= 10) parts.push(`${Math.round(c.tstm)}% thunder`)
  else if (c.pop != null && c.pop >= 30) parts.push(`${Math.round(c.pop)}% precip`)
  const s = parts.join(' · ')
  return s.charAt(0).toUpperCase() + s.slice(1)
}

function num(v: unknown): number | null {
  if (typeof v === 'number') return Number.isFinite(v) ? v : null
  if (typeof v === 'string') {
    const n = parseFloat(v.trim().replace(/\+$/, ''))
    return Number.isFinite(n) ? n : null
  }
  return null
}

const str = (v: unknown) => (v == null ? '' : String(v))

/** Ceiling (lowest BKN / OVC / OVX layer, or vertical visibility) from AWC cloud groups. */
function ceilingOf(clouds: any, vertVis: any): number | null {
  let c = Infinity
  for (const l of Array.isArray(clouds) ? clouds : []) {
    if (!['BKN', 'OVC', 'OVX'].includes(l?.cover)) continue
    const b = num(l.base)
    if (b != null) c = Math.min(c, b)
  }
  const vv = num(vertVis)
  if (vv != null) c = Math.min(c, vv * (vv < 100 ? 100 : 1))
  return Number.isFinite(c) ? c : null
}

const wdirOf = (v: unknown) => (v === 'VRB' ? null : num(v))

export function fromMetar(m: any): Conditions {
  const t = typeof m.obsTime === 'number' ? new Date(m.obsTime * 1000) : new Date()
  const c = blank(str(m.icaoId), 'metar', t)
  c.ceiling = ceilingOf(m.clouds, m.vertVis)
  c.ceilingKnown = true
  c.vis = num(m.visib)
  c.wdir = wdirOf(m.wdir)
  c.wspd = num(m.wspd)
  c.gust = num(m.wgst)
  c.wx = str(m.wxString)
  c.tempC = num(m.temp)
  c.altimHpa = num(m.altim)
  return c
}

function apply(c: Conditions, f: any) {
  if (Array.isArray(f.clouds) && f.clouds.length) c.ceiling = ceilingOf(f.clouds, f.vertVis)
  else if (f.vertVis != null) c.ceiling = ceilingOf(null, f.vertVis)
  if (f.visib != null) c.vis = num(f.visib)
  if (f.wspd != null) {
    c.wdir = wdirOf(f.wdir)
    c.wspd = num(f.wspd)
    c.gust = num(f.wgst)
  }
  if (f.wxString != null) c.wx = str(f.wxString)
}

const RANK = ['VFR', 'MVFR', 'IFR', 'LIFR']

/** TAF conditions at `when` (null outside the TAF's validity). */
export function fromTaf(taf: any, when: Date): Conditions | null {
  const t = when.getTime() / 1000
  const from = taf?.validTimeFrom
  const to = taf?.validTimeTo
  if (typeof from !== 'number' || typeof to !== 'number' || t < from || t >= to) return null
  const fc: any[] = Array.isArray(taf.fcsts) ? taf.fcsts : []
  let c = blank(str(taf.icaoId), 'taf', when)
  c.ceilingKnown = true
  let base = false
  // the prevailing group: the last initial / FM / BECMG group that has started
  for (const f of fc) {
    const change = str(f.fcstChange)
    if (!['', 'FM', 'BECMG'].includes(change) || !(typeof f.timeFrom === 'number' && f.timeFrom <= t)) continue
    if (change === 'BECMG' && base) apply(c, f) // BECMG only changes the elements it mentions
    else {
      c = blank(c.station, 'taf', when)
      c.ceilingKnown = true
      apply(c, f)
    }
    base = true
  }
  if (!base) return null
  // TEMPO / PROB groups covering the time: keep the worst as a caveat
  let worst: [string, string] | null = null
  const score = (cat: string, text: string) => Math.max(0, RANK.indexOf(cat)) + (text.includes('TS') ? 10 : 0)
  for (const f of fc) {
    const change = str(f.fcstChange)
    if (!(change === 'TEMPO' || change === 'PROB' || typeof f.probability === 'number')) continue
    if (typeof f.timeFrom !== 'number' || typeof f.timeTo !== 'number' || t < f.timeFrom || t >= f.timeTo) continue
    const g: Conditions = { ...c }
    apply(g, f)
    const cat = category(g.ceiling, g.vis)
    const wx = str(f.wxString)
    const label = change === 'TEMPO' ? 'TEMPO' : `PROB${f.probability ?? 30}`
    const text = `${label}: ${summary(g)}`
    const full = wx ? `${text} ${wx}` : text
    if (!worst || score(cat, wx) > score(worst[0], worst[1])) worst = [cat, full]
  }
  c.tempo = worst
  return c
}

/** One row of NBM text guidance (IEM `mos.json`). */
export interface NbmRow {
  t: Date
  wdir: number | null
  wspd: number | null
  gust: number | null
  /** ft; Infinity = unlimited (NBS -88); only meaningful for NBS */
  cig: number | null
  /** SM */
  vis: number | null
  sky: number | null
  pop: number | null
  tstm: number | null
  tempF: number | null
}

export function parseNbm(v: any): NbmRow[] {
  const out: NbmRow[] = []
  for (const r of Array.isArray(v?.data) ? v.data : []) {
    const m = /^(\d{4})-(\d{2})-(\d{2}) (\d{2}):(\d{2})/.exec(str(r.ftime))
    if (!m) continue
    const t = new Date(Date.UTC(+m[1], +m[2] - 1, +m[3], +m[4], +m[5]))
    const n = (k: string) => {
      const x = num(r[k])
      return x != null && x > -80 ? x : null
    }
    const c = num(r.cig)
    out.push({
      t,
      wdir: n('wdr'),
      wspd: n('wsp'),
      gust: n('gst'),
      cig: c == null ? null : c <= -80 ? Infinity : c * 100,
      vis: n('vis') != null ? n('vis')! / 10 : null,
      sky: n('sky'),
      pop: n('p06') ?? n('p12'),
      tstm: n('t03') ?? n('t06_1') ?? n('t12_1'),
      tempF: n('tmp'),
    })
  }
  return out
}

function nbmAt(rows: NbmRow[], when: Date, maxGapH: number, station: string, source: Source): Conditions | null {
  if (!rows.length) return null
  const gapH = (r: NbmRow) => Math.abs(r.t.getTime() - when.getTime()) / 3600000
  const best = rows.reduce((a, b) => (gapH(b) < gapH(a) ? b : a))
  if (Math.trunc(gapH(best)) > maxGapH) return null
  const c = blank(station, source, when)
  if (source === 'nbm') {
    c.ceilingKnown = best.cig != null
    c.ceiling = best.cig != null && Number.isFinite(best.cig) ? best.cig : null
    c.vis = best.vis
  }
  c.wdir = best.wdir
  c.wspd = best.wspd
  c.gust = best.gust != null && (best.wspd == null || best.gust > best.wspd + 5) ? best.gust : null
  c.skyPct = best.sky
  // probabilities: the worst of the rows around the time (they cover 3-12 h windows)
  const near = rows.filter((r) => Math.trunc(gapH(r)) <= maxGapH)
  const max = (xs: (number | null)[]) => xs.reduce<number | null>((a, b) => (b == null ? a : a == null ? b : Math.max(a, b)), null)
  c.pop = max(near.map((r) => r.pop))
  c.tstm = max(near.map((r) => r.tstm))
  c.tempC = best.tempF != null ? ((best.tempF - 32) * 5) / 9 : null
  return c
}

/** Everything we know about one airport's weather. */
export interface StationWx {
  airport: string
  station: string
  offsetNm: number
  metar: any | null
  taf: any | null
  nbs: NbmRow[]
  nbe: NbmRow[]
}

export function stationWx(r: StationRaw): StationWx {
  return { airport: r.airport, station: r.station, offsetNm: r.offset_nm, metar: r.metar, taf: r.taf, nbs: r.nbs ? parseNbm(r.nbs) : [], nbe: r.nbe ? parseNbm(r.nbe) : [] }
}

/** Conditions at `when`, from the best source that covers it. */
export function conditionsAt(s: StationWx, when: Date): Conditions | null {
  const now = new Date()
  const lead = (when.getTime() - now.getTime()) / 60000
  if (lead <= 75 && s.metar) {
    const c = fromMetar(s.metar)
    // a TAF still adds its TEMPO caveats
    const t = s.taf ? fromTaf(s.taf, when > now ? when : now) : null
    if (t) c.tempo = t.tempo
    c.valid = when
    return c
  }
  const t = s.taf ? fromTaf(s.taf, when) : null
  if (t) return t
  return nbmAt(s.nbs, when, 2, s.station, 'nbm') ?? nbmAt(s.nbe, when, 7, s.station, 'nbmExtended')
}

/** A lead-time phrase: "now", "in 3 h", "in 2 days". */
export function leadText(when: Date): string {
  const m = Math.round((when.getTime() - Date.now()) / 60000)
  if (m < 20) return 'now'
  if (m < 90) return `in ${m} min`
  if (m < 48 * 60) return `in ${Math.round(m / 60)} h`
  return `in ${(m / 1440).toFixed(1)} days`
}
