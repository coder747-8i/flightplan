// Thin client for the Rust API.
export interface Place {
  kind: 'airport' | 'navaid' | 'fix' | 'coord'
  id: string
  name: string
  detail: string
  lat: number
  lon: number
}

/** fetch for our own API: requests that change something carry the X-Flightplan header the server requires */
export function apiFetch(url: string, init: RequestInit = {}): Promise<Response> {
  const method = (init.method ?? 'GET').toUpperCase()
  if (method === 'GET' || method === 'HEAD') return fetch(url, init)
  const headers = new Headers(init.headers)
  headers.set('x-flightplan', '1')
  return fetch(url, { ...init, headers })
}

async function j<T>(url: string, init?: RequestInit): Promise<T> {
  const r = await apiFetch(url, init)
  if (!r.ok) {
    const text = await r.text()
    let msg = text.slice(0, 300)
    try {
      msg = JSON.parse(text).error ?? msg
    } catch {
      /* not JSON */
    }
    throw new Error(msg || `HTTP ${r.status}`)
  }
  return r.json()
}

const post = <T>(url: string, body: unknown) =>
  j<T>(url, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(body) })

export const api = {
  search: (q: string) => j<Place[]>(`/api/search?q=${encodeURIComponent(q)}`),
  resolve: (ids: string) => j<{ token: string; place: Place | null }[]>(`/api/resolve?ids=${encodeURIComponent(ids)}`),
  airport: (id: string) => j<any>(`/api/airport/${encodeURIComponent(id)}`),
  nearest: (lat: number, lon: number, r = 25) => j<{ place: Place; dist: number }[]>(`/api/nearest?lat=${lat}&lon=${lon}&r=${r}`),
  profile: (path: [number, number][], corridor = 1) =>
    j<{ samples: ProfileSample[]; obstacles: ProfileObstacle[] }>('/api/profile', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ path, corridor }),
    }),
  metar: (ids: string[]) => j<any[]>(`/api/wx/metar?ids=${ids.join(',')}`),
  metarBox: (b: [number, number, number, number]) => j<any[]>(`/api/wx/metar?bbox=${b.map((v) => v.toFixed(2)).join(',')}`),
  taf: (ids: string[]) => j<any[]>(`/api/wx/taf?ids=${ids.join(',')}`),
  pirep: (b: [number, number, number, number], age = 3) => j<any[]>(`/api/wx/pirep?bbox=${b.map((v) => v.toFixed(2)).join(',')}&age=${age}`),
  airsigmet: () => j<any[]>('/api/wx/airsigmet'),
  gairmet: () => j<any[]>('/api/wx/gairmet'),
  tfr: () => j<GeoJSON.FeatureCollection>('/api/tfr'),
  winds: (points: [number, number][]) =>
    j<{ source: string; points: WindPoint[] }>('/api/wx/winds', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ points }),
    }),
  config: () => j<ServerConfig>('/api/config'),
  setupStatus: () => j<SetupStatus>('/api/setup/status'),
  setupDownload: (url?: string) => post<{ started: boolean }>('/api/setup/download', { url }),
  setupFolder: (path?: string) => post<{ started?: boolean; cancelled?: boolean; path?: string }>('/api/setup/folder', { path }),
  setupCancel: () => post<{ ok: boolean }>('/api/setup/cancel', {}),
  /** stream a data-pack zip to the server; progress shows up in setupStatus().job */
  setupUpload: (file: File) =>
    j<{ started: boolean }>('/api/setup/upload', { method: 'POST', headers: { 'content-type': 'application/zip', 'x-file-name': encodeURIComponent(file.name) }, body: file }),
  /** a JSON document saved on the server (aircraft, prefs); null when never saved */
  userGet: <T>(key: 'aircraft' | 'prefs') => j<T | null>(`/api/user/${key}`),
  userPut: (key: 'aircraft' | 'prefs', value: unknown) => j<{ ok: boolean }>(`/api/user/${key}`, { method: 'PUT', headers: { 'content-type': 'application/json' }, body: JSON.stringify(value) }),
  hostSave: (name: string, label: string, body: Blob) =>
    j<{ path?: string; cancelled?: boolean }>(`/api/host/save?name=${encodeURIComponent(name)}&label=${encodeURIComponent(label)}`, { method: 'POST', body }),
  hostOpen: (url: string) => post<{ ok: boolean }>('/api/host/open', { url }),
  hostPrint: () => post<{ ok: boolean }>('/api/host/print', {}),
}

export interface DataInfo {
  cycle?: string | null
  expires?: string | null
  built?: string | null
  installedFrom?: string | null
  installedAt?: string | null
}

export interface ServerConfig {
  windyMapKey: string | null
  windyPoint: boolean
  notams: boolean
  /** running inside the desktop app (native dialogs available) */
  desktop: boolean
  data: DataInfo | null
  version: string
}

export interface SetupJob {
  state: 'idle' | 'downloading' | 'uploading' | 'copying' | 'extracting' | 'checking' | 'done' | 'error'
  done: number
  total: number
  message: string
}

export interface SetupStatus {
  installed: boolean
  info: DataInfo | null
  job: SetupJob
  defaultUrl: string
  dataDir: string
  canPickFolder: boolean
}

export interface ProfileSample {
  d: number
  lat: number
  lon: number
  elev: number
  max: number
  leg: number
}
export interface ProfileObstacle {
  d: number
  xt: number
  msl: number
  agl: number
  lit: boolean
  type: string
  lat: number
  lon: number
}
export interface WindPoint {
  lat: number
  lon: number
  error?: string
  levels: { p: number; alt: number; times: { t: number; dir: number; spd: number; temp: number | null; alt: number }[] }[]
}
