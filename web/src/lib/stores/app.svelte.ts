// App-wide state (Svelte 5 runes). Viewer conveniences persist in localStorage.
import type { LayerGroup } from '../chart/style'

export type Panel = 'none' | 'layers' | 'route' | 'weather' | 'aircraft' | 'briefing' | 'help' | 'settings'

const DEFAULT_LAYERS: Record<LayerGroup, boolean> = {
  terrain: true,
  basemap: true,
  classB: true,
  classC: true,
  classD: true,
  classE: true,
  classEhigh: false,
  modeC: false,
  restricted: true,
  moa: true,
  warningAlert: true,
  airspaceLabels: true,
  airportsPublic: true,
  airportsPrivate: true,
  heliports: false,
  runways: true,
  navaids: true,
  vfrWaypoints: true,
  ifrFixes: false,
  airwaysLow: false,
  airwaysHigh: false,
  obstacles: true,
}

function load<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(`fp.${key}`)
    if (raw) return { ...fallback, ...JSON.parse(raw) }
  } catch {
    /* private mode or corrupted entry */
  }
  return fallback
}

export function save(key: string, value: unknown) {
  try {
    localStorage.setItem(`fp.${key}`, JSON.stringify(value))
  } catch {
    /* ignore */
  }
}

class AppState {
  theme = $state<'day' | 'night'>(load('prefs', { theme: 'day' as 'day' | 'night' }).theme)
  // keep only known groups (an old "sua" switch became restricted / moa / warningAlert)
  layers = $state<Record<LayerGroup, boolean>>(
    Object.fromEntries(Object.entries(load('layers', DEFAULT_LAYERS)).filter(([k]) => k in DEFAULT_LAYERS)) as Record<LayerGroup, boolean>,
  )
  altitude = $state(load('altitude', { enabled: false, lo: 0, hi: 12500 }))
  panel = $state<Panel>('none')
  profileOpen = $state(load('prefs2', { profileOpen: true }).profileOpen)
  /** show coded weather/remarks as raw text instead of plain English */
  rawWx = $state(load('prefs3', { rawWx: false }).rawWx)
  /** what the info card shows: an airport id, or a list of things under a map click */
  selection = $state<null | { kind: 'airport'; id: string } | { kind: 'point'; lngLat: [number, number]; items: PointItem[] }>(null)
  cursor = $state<{ lat: number; lon: number; zoom: number } | null>(null)
  tourStep = $state<number | null>(null)
}

export interface PointItem {
  type: 'airspace' | 'airport' | 'navaid' | 'fix' | 'obstacle'
  title: string
  subtitle: string
  color?: string
  id?: string
  /** coded text to render decoded (METAR, PIREP, SIGMET ...) */
  decode?: { kind: 'metar' | 'taf' | 'pirep' | 'sigmet' | 'gairmet' | 'remark'; raw?: string; data?: any }
}

export const app = new AppState()

$effect.root(() => {
  $effect(() => save('layers', app.layers))
  $effect(() => save('altitude', app.altitude))
  $effect(() => save('prefs', { theme: app.theme }))
  $effect(() => save('prefs2', { profileOpen: app.profileOpen }))
  $effect(() => save('prefs3', { rawWx: app.rawWx }))
})
