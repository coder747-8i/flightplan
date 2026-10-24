// Aircraft files (.fpaircraft.json): share an airframe, back up your fleet, or bring in data
// typed up elsewhere. Imports are checked field by field: missing performance numbers fall
// back to defaults (with a warning), missing weight & balance essentials are errors.
// The format is documented in docs/aircraft-format.md.
import { uid, type Aircraft, type Station } from '../stores/aircraft.svelte'

export const AIRCRAFT_FORMAT = 'flightplan-aircraft'
export const AIRCRAFT_EXT = '.fpaircraft.json'

/** an aircraft as written to a file: no internal id, no loading for a particular flight */
export type AircraftDef = Omit<Aircraft, 'id' | 'load'>

export interface AircraftFileDoc {
  format: typeof AIRCRAFT_FORMAT
  version: 1
  exported: string
  aircraft: AircraftDef[]
}

export interface ImportResult {
  /** what the user would recognise this entry by */
  label: string
  aircraft?: Aircraft
  errors: string[]
  warnings: string[]
}

const FUELS = ['100LL', 'Jet A', 'UL94', 'Mogas'] as const
const KINDS = ['seat', 'baggage', 'fuel', 'other'] as const

export const DEFAULT_PERF: Aircraft['perf'] = {
  tas: 110, gph: 8, climbFpm: 600, climbTas: 75, climbGph: 9, descentFpm: 500, descentTas: 110, descentGph: 6, taxiGal: 1, reserveMin: 30, maxXwind: 15,
}

const PERF_LABELS: Record<keyof Aircraft['perf'], string> = {
  tas: 'cruise TAS', gph: 'cruise fuel flow', climbFpm: 'climb rate', climbTas: 'climb speed', climbGph: 'climb fuel flow',
  descentFpm: 'descent rate', descentTas: 'descent speed', descentGph: 'descent fuel flow', taxiGal: 'taxi fuel', reserveMin: 'reserve', maxXwind: 'max crosswind',
}

export function toFile(list: Aircraft[]): string {
  const doc: AircraftFileDoc = {
    format: AIRCRAFT_FORMAT,
    version: 1,
    exported: new Date().toISOString(),
    aircraft: list.map(({ id: _id, load: _load, ...rest }) => structuredClone(rest)),
  }
  return JSON.stringify(doc, null, 2)
}

/** A blank file to fill in by hand, with every field present. */
export function templateFile(base: AircraftDef): string {
  return JSON.stringify({ format: AIRCRAFT_FORMAT, version: 1, exported: new Date().toISOString(), aircraft: [base] }, null, 2)
}

/** Read an aircraft file (ours, a bare aircraft object, or a list of them). */
export function parseAircraftFile(text: string, fileName = 'file'): ImportResult[] {
  let raw: unknown
  try {
    raw = JSON.parse(text)
  } catch (e) {
    return [{ label: fileName, errors: [`${fileName} isn't valid JSON (${(e as Error).message})`], warnings: [] }]
  }
  const items: unknown[] = Array.isArray(raw) ? raw : isObj(raw) && Array.isArray(raw.aircraft) ? raw.aircraft : [raw]
  if (!items.length) return [{ label: fileName, errors: ['The file has no aircraft in it'], warnings: [] }]
  return items.map((it, i) => normalize(it, items.length > 1 ? `${fileName} #${i + 1}` : fileName))
}

const isObj = (v: unknown): v is Record<string, any> => typeof v === 'object' && v !== null && !Array.isArray(v)
const num = (v: unknown): number | undefined => {
  const n = typeof v === 'string' ? Number(v.replace(/,/g, '')) : v
  return typeof n === 'number' && Number.isFinite(n) ? n : undefined
}

/** [arm, weight] pairs from [[a, w], ...] or [{arm, weight}, ...] */
function points(v: unknown): [number, number][] | undefined {
  if (!Array.isArray(v)) return undefined
  const pts: [number, number][] = []
  for (const p of v) {
    const a = Array.isArray(p) ? num(p[0]) : isObj(p) ? num(p.arm ?? p.cg ?? p.x) : undefined
    const w = Array.isArray(p) ? num(p[1]) : isObj(p) ? num(p.weight ?? p.w ?? p.y) : undefined
    if (a === undefined || w === undefined) return undefined
    pts.push([a, w])
  }
  return pts
}

export function normalize(input: unknown, fileLabel = 'aircraft'): ImportResult {
  const errors: string[] = []
  const warnings: string[] = []
  if (!isObj(input)) return { label: fileLabel, errors: ['Expected an aircraft object'], warnings }

  const str = (v: unknown) => (typeof v === 'string' ? v.trim() : '')
  const tail = str(input.tail).toUpperCase()
  const type = str(input.type).toUpperCase() || 'ZZZZ'
  const name = str(input.name) || (type !== 'ZZZZ' ? type : 'Imported aircraft')
  const label = [tail, name].filter(Boolean).join(' · ') || fileLabel

  let fuelType = FUELS.find((f) => f.toLowerCase() === str(input.fuelType).toLowerCase())
  if (!fuelType) {
    if (input.fuelType) warnings.push(`Unknown fuel type "${input.fuelType}", using 100LL`)
    fuelType = '100LL'
  }
  const lbPerGal = num(input.lbPerGal) ?? (fuelType === 'Jet A' ? 6.7 : 6)

  const perfIn = isObj(input.perf) ? input.perf : {}
  const perf = { ...DEFAULT_PERF }
  const defaulted: string[] = []
  for (const k of Object.keys(DEFAULT_PERF) as (keyof Aircraft['perf'])[]) {
    const v = num(perfIn[k])
    if (v === undefined || v < 0) defaulted.push(PERF_LABELS[k])
    else perf[k] = v
  }
  if (defaulted.length) warnings.push(`Using default values for ${defaulted.join(', ')}. Check them under Performance.`)
  if (perf.tas <= 0) errors.push('Cruise TAS must be more than 0')

  const wbIn = isObj(input.wb) ? input.wb : {}
  const emptyWeight = num(wbIn.emptyWeight)
  const emptyArm = num(wbIn.emptyArm)
  if (!emptyWeight || emptyWeight <= 0) errors.push('Missing basic empty weight (wb.emptyWeight, lb)')
  if (emptyArm === undefined) errors.push('Missing empty-weight arm (wb.emptyArm, inches)')
  const maxTakeoff = num(wbIn.maxTakeoff)
  if (!maxTakeoff) errors.push('Missing maximum takeoff weight (wb.maxTakeoff, lb)')
  const maxRamp = num(wbIn.maxRamp) ?? maxTakeoff ?? 0
  const maxLanding = num(wbIn.maxLanding) ?? maxTakeoff ?? 0
  if (emptyWeight && maxTakeoff && emptyWeight >= maxTakeoff) errors.push(`Empty weight ${emptyWeight} lb isn't below max takeoff ${maxTakeoff} lb`)

  const stations: Station[] = []
  const seen = new Set<string>()
  if (!Array.isArray(wbIn.stations) || !wbIn.stations.length) errors.push('No loading stations (wb.stations): add at least seats and fuel')
  else
    wbIn.stations.forEach((st: unknown, i: number) => {
      if (!isObj(st)) return errors.push(`Station ${i + 1} isn't an object`)
      const arm = num(st.arm)
      const sname = str(st.name) || `Station ${i + 1}`
      if (arm === undefined) return errors.push(`Station "${sname}" has no arm`)
      let kind = KINDS.find((k) => k === str(st.kind).toLowerCase())
      if (!kind) kind = /fuel|tank/i.test(sname) ? 'fuel' : /bag|cargo/i.test(sname) ? 'baggage' : /seat|pilot|pass|row/i.test(sname) ? 'seat' : 'other'
      let id = str(st.id) || sname.toLowerCase().replace(/\W+/g, '').slice(0, 12) || uid()
      while (seen.has(id)) id = `${id}_${i}`
      seen.add(id)
      const max = num(st.max)
      stations.push({ id, name: sname, arm, kind, ...(max ? { max } : {}) })
    })
  const fuelStations = stations.filter((s) => s.kind === 'fuel')
  if (stations.length && !fuelStations.length) errors.push('No fuel station: add one with kind "fuel" and its arm')
  if (fuelStations.length > 1) warnings.push('More than one fuel station: weight & balance uses the arm of the last one for fuel burn')

  let usableFuel = num(input.usableFuel)
  if (!usableFuel) {
    usableFuel = fuelStations.reduce((t, s) => t + (s.max ?? 0), 0) || undefined
    if (usableFuel) warnings.push(`Usable fuel taken from the fuel station capacity (${usableFuel} gal)`)
    else errors.push('Missing usable fuel (usableFuel, gallons)')
  }

  const envelope = points(wbIn.envelope)
  if (!envelope || envelope.length < 3) errors.push('CG envelope (wb.envelope) needs at least 3 [arm, weight] points')
  const utility = wbIn.utility === undefined ? undefined : points(wbIn.utility)
  if (wbIn.utility !== undefined && (!utility || utility.length < 3)) warnings.push('Ignored the utility envelope: it needs at least 3 [arm, weight] points')

  // plausibility: the envelope should reach max takeoff weight and contain the empty CG's arm range
  if (envelope && envelope.length >= 3 && maxTakeoff) {
    const topW = Math.max(...envelope.map((p) => p[1]))
    if (Math.abs(topW - maxTakeoff) > maxTakeoff * 0.05) warnings.push(`The envelope tops out at ${topW} lb but max takeoff is ${maxTakeoff} lb. Check both.`)
    const arms = envelope.map((p) => p[0])
    const [lo, hi] = [Math.min(...arms), Math.max(...arms)]
    const far = stations.filter((s) => s.arm < lo - 120 || s.arm > hi + 120)
    if (far.length) warnings.push(`Station arms look out of range for this envelope (${far.map((s) => s.name).join(', ')}). Are all arms in inches from the same datum?`)
  }

  if (errors.length) return { label, errors, warnings }

  const load: Record<string, number> = {}
  for (const st of stations) load[st.id] = st.kind === 'fuel' ? (st.max ?? usableFuel ?? 0) : 0
  const firstSeat = stations.find((st) => st.kind === 'seat')
  if (firstSeat) load[firstSeat.id] = 170

  return {
    label,
    errors,
    warnings,
    aircraft: {
      id: uid(),
      name,
      tail,
      type,
      fuelType,
      lbPerGal,
      perf,
      usableFuel: usableFuel!,
      wb: { emptyWeight: emptyWeight!, emptyArm: emptyArm!, maxRamp, maxTakeoff: maxTakeoff!, maxLanding, stations, envelope: envelope!, ...(utility && utility.length >= 3 ? { utility } : {}) },
      load,
      ...(str(input.notes) ? { notes: str(input.notes) } : {}),
    },
  }
}

/**
 * Rows of numbers pasted from a spreadsheet or POH table: tab, comma, semicolon or space
 * separated. Rows that don't start with numbers are skipped (headers), text cells are kept.
 */
export function parseTable(text: string): string[][] {
  return text
    .split(/\r?\n/)
    .map((l) => l.trim())
    .filter(Boolean)
    .map((l) => (l.includes('\t') ? l.split('\t') : l.includes(';') ? l.split(';') : l.includes(',') ? l.split(',') : l.split(/\s+/)).map((c) => c.trim()))
}
