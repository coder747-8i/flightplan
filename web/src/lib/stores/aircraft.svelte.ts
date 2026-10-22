// Aircraft profiles: performance for the navlog, weight & balance data for the loading sheet.
import { save } from './app.svelte'

export interface Station {
  id: string
  name: string
  arm: number
  kind: 'seat' | 'baggage' | 'fuel' | 'other'
  max?: number // lb (or gal for fuel)
}

export interface Aircraft {
  id: string
  name: string
  tail: string
  type: string
  fuelType: '100LL' | 'Jet A' | 'UL94' | 'Mogas'
  lbPerGal: number
  perf: {
    tas: number
    gph: number
    climbFpm: number
    climbTas: number
    climbGph: number
    descentFpm: number
    descentTas: number
    descentGph: number
    taxiGal: number
    reserveMin: number
    maxXwind: number
  }
  usableFuel: number
  wb: {
    emptyWeight: number
    emptyArm: number
    maxRamp: number
    maxTakeoff: number
    maxLanding: number
    stations: Station[]
    /** CG envelope polygon, [arm, weight] points in order */
    envelope: [number, number][]
    utility?: [number, number][]
  }
  /** loading for the current flight: station id -> lb (fuel station: gallons) */
  load: Record<string, number>
  notes?: string
}

const uid = () => Math.random().toString(36).slice(2, 10)

// Typical POH figures. Every aircraft differs: always enter your own weighing data.
export const PRESETS: Omit<Aircraft, 'id' | 'tail' | 'load'>[] = [
  {
    name: 'Cessna 172S Skyhawk',
    type: 'C172',
    fuelType: '100LL',
    lbPerGal: 6,
    perf: { tas: 120, gph: 8.5, climbFpm: 700, climbTas: 75, climbGph: 10.5, descentFpm: 500, descentTas: 120, descentGph: 7, taxiGal: 1.1, reserveMin: 30, maxXwind: 15 },
    usableFuel: 53,
    wb: {
      emptyWeight: 1663,
      emptyArm: 41.0,
      maxRamp: 2558,
      maxTakeoff: 2550,
      maxLanding: 2550,
      stations: [
        { id: 'front', name: 'Pilot & front passenger', arm: 37.0, kind: 'seat' },
        { id: 'rear', name: 'Rear passengers', arm: 73.0, kind: 'seat' },
        { id: 'bagA', name: 'Baggage area A', arm: 95.0, kind: 'baggage', max: 120 },
        { id: 'bagB', name: 'Baggage area B', arm: 123.0, kind: 'baggage', max: 50 },
        { id: 'fuel', name: 'Fuel (usable)', arm: 48.0, kind: 'fuel', max: 53 },
      ],
      envelope: [[35.0, 1500], [35.0, 1950], [41.0, 2550], [47.3, 2550], [47.3, 1500]],
      utility: [[35.0, 1500], [35.0, 1950], [37.5, 2200], [40.5, 2200], [40.5, 1500]],
    },
  },
  {
    name: 'Piper PA-28-181 Archer',
    type: 'P28A',
    fuelType: '100LL',
    lbPerGal: 6,
    perf: { tas: 125, gph: 9.5, climbFpm: 650, climbTas: 76, climbGph: 11, descentFpm: 500, descentTas: 125, descentGph: 7.5, taxiGal: 1, reserveMin: 30, maxXwind: 17 },
    usableFuel: 48,
    wb: {
      emptyWeight: 1650,
      emptyArm: 86.0,
      maxRamp: 2558,
      maxTakeoff: 2550,
      maxLanding: 2550,
      stations: [
        { id: 'front', name: 'Pilot & front passenger', arm: 80.5, kind: 'seat' },
        { id: 'rear', name: 'Rear passengers', arm: 118.1, kind: 'seat' },
        { id: 'bag', name: 'Baggage', arm: 142.8, kind: 'baggage', max: 200 },
        { id: 'fuel', name: 'Fuel (usable)', arm: 95.0, kind: 'fuel', max: 48 },
      ],
      envelope: [[82.0, 1200], [82.0, 2050], [88.6, 2550], [93.0, 2550], [93.0, 1200]],
    },
  },
  {
    name: 'Custom aircraft',
    type: 'ZZZZ',
    fuelType: '100LL',
    lbPerGal: 6,
    perf: { tas: 110, gph: 8, climbFpm: 600, climbTas: 75, climbGph: 9, descentFpm: 500, descentTas: 110, descentGph: 6, taxiGal: 1, reserveMin: 30, maxXwind: 15 },
    usableFuel: 40,
    wb: {
      emptyWeight: 1500,
      emptyArm: 40,
      maxRamp: 2300,
      maxTakeoff: 2300,
      maxLanding: 2300,
      stations: [
        { id: 'front', name: 'Front seats', arm: 37, kind: 'seat' },
        { id: 'rear', name: 'Rear seats', arm: 73, kind: 'seat' },
        { id: 'bag', name: 'Baggage', arm: 95, kind: 'baggage', max: 120 },
        { id: 'fuel', name: 'Fuel (usable)', arm: 48, kind: 'fuel', max: 40 },
      ],
      envelope: [[35, 1500], [35, 1900], [40, 2300], [47, 2300], [47, 1500]],
    },
  },
]

function fromPreset(i: number, tail = ''): Aircraft {
  const p = structuredClone(PRESETS[i])
  const load: Record<string, number> = {}
  for (const s of p.wb.stations) load[s.id] = s.kind === 'fuel' ? (s.max ?? 0) : s.id === 'front' ? 170 : 0
  return { ...p, id: uid(), tail, load }
}

function loadList(): Aircraft[] {
  try {
    const raw = localStorage.getItem('fp.aircraft')
    if (raw) return JSON.parse(raw)
  } catch {
    /* ignore */
  }
  return [fromPreset(0, 'N12345')]
}

class AircraftState {
  list = $state<Aircraft[]>(loadList())
  activeId = $state<string>('')

  constructor() {
    try {
      this.activeId = localStorage.getItem('fp.aircraft.active') || this.list[0]?.id || ''
    } catch {
      this.activeId = this.list[0]?.id ?? ''
    }
  }

  get active(): Aircraft | undefined {
    return this.list.find((a) => a.id === this.activeId) ?? this.list[0]
  }

  addPreset(i: number) {
    const a = fromPreset(i)
    this.list.push(a)
    this.activeId = a.id
    return a
  }

  remove(id: string) {
    this.list = this.list.filter((a) => a.id !== id)
    if (this.activeId === id) this.activeId = this.list[0]?.id ?? ''
  }
}

export const aircraft = new AircraftState()

$effect.root(() => {
  $effect(() => save('aircraft', aircraft.list))
  $effect(() => {
    try {
      localStorage.setItem('fp.aircraft.active', aircraft.activeId)
    } catch {
      /* ignore */
    }
  })
})

export interface WBResult {
  rows: { name: string; weight: number; arm: number; moment: number }[]
  ramp: { weight: number; cg: number }
  takeoff: { weight: number; cg: number }
  landing: { weight: number; cg: number }
  zeroFuel: { weight: number; cg: number }
  problems: string[]
}

export function inPolygon(x: number, y: number, poly: [number, number][]) {
  let inside = false
  for (let i = 0, j = poly.length - 1; i < poly.length; j = i++) {
    const [xi, yi] = poly[i]
    const [xj, yj] = poly[j]
    if (yi > y !== yj > y && x < ((xj - xi) * (y - yi)) / (yj - yi) + xi) inside = !inside
  }
  return inside
}

/** Weight & balance for ramp, takeoff (after taxi fuel), zero-fuel and landing (after trip fuel). */
export function computeWB(a: Aircraft, tripGal = 0): WBResult {
  const rows: WBResult['rows'] = [{ name: 'Basic empty weight', weight: a.wb.emptyWeight, arm: a.wb.emptyArm, moment: a.wb.emptyWeight * a.wb.emptyArm }]
  let fuelArm = 0
  let fuelGal = 0
  const problems: string[] = []
  for (const s of a.wb.stations) {
    const v = a.load[s.id] ?? 0
    if (s.kind === 'fuel') {
      fuelArm = s.arm
      fuelGal += v
      if (s.max && v > s.max) problems.push(`${s.name}: ${v} gal exceeds ${s.max} gal capacity`)
      continue
    }
    if (s.max && v > s.max) problems.push(`${s.name}: ${v} lb exceeds the ${s.max} lb limit`)
    if (v) rows.push({ name: s.name, weight: v, arm: s.arm, moment: v * s.arm })
  }
  const sum = (r: typeof rows) => {
    const w = r.reduce((t, x) => t + x.weight, 0)
    const m = r.reduce((t, x) => t + x.moment, 0)
    return { weight: w, cg: w ? m / w : 0 }
  }
  const zf = sum(rows)
  const withFuel = (gal: number) => {
    const w = gal * a.lbPerGal
    return sum([...rows, { name: 'Fuel', weight: w, arm: fuelArm, moment: w * fuelArm }])
  }
  const ramp = withFuel(fuelGal)
  const takeoff = withFuel(Math.max(0, fuelGal - a.perf.taxiGal))
  const landing = withFuel(Math.max(0, fuelGal - a.perf.taxiGal - tripGal))
  rows.push({ name: `Fuel ${fuelGal} gal`, weight: fuelGal * a.lbPerGal, arm: fuelArm, moment: fuelGal * a.lbPerGal * fuelArm })
  if (ramp.weight > a.wb.maxRamp) problems.push(`Ramp weight ${Math.round(ramp.weight)} lb is over the ${a.wb.maxRamp} lb maximum`)
  if (takeoff.weight > a.wb.maxTakeoff) problems.push(`Takeoff weight ${Math.round(takeoff.weight)} lb is over the ${a.wb.maxTakeoff} lb maximum`)
  if (landing.weight > a.wb.maxLanding) problems.push(`Landing weight ${Math.round(landing.weight)} lb is over the ${a.wb.maxLanding} lb maximum`)
  for (const [label, pt] of [['Takeoff', takeoff], ['Landing', landing], ['Zero fuel', zf]] as const) {
    if (!inPolygon(pt.cg, pt.weight, a.wb.envelope)) problems.push(`${label} CG ${pt.cg.toFixed(2)} in at ${Math.round(pt.weight)} lb is outside the envelope`)
  }
  return { rows, ramp, takeoff, landing, zeroFuel: zf, problems }
}
