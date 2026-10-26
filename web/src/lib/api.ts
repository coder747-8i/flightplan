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

let signingOut = false

async function j<T>(url: string, init?: RequestInit): Promise<T> {
  const r = await apiFetch(url, init)
  // the session ended (signed out elsewhere, password changed, expired): back to sign-in
  if (r.status === 401 && !signingOut) {
    signingOut = true
    location.reload()
  }
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
  /** a whole route string, airways expanded to their fixes */
  route: (text: string) => j<{ places: Place[]; missing: string[] }>(`/api/route?text=${encodeURIComponent(text)}`),
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
  /** every NWS winds-aloft (FB) station, for the winds layer */
  fbPoints: () => j<{ source: string; points: (WindPoint & { station: string })[] }>('/api/wx/fb'),
  /** METAR, TAF and NBM guidance for an airport (or the nearest station that has them) */
  station: (airport: string, icao: string, lat: number, lon: number) =>
    j<StationRaw>(`/api/wx/station?airport=${encodeURIComponent(airport)}&icao=${encodeURIComponent(icao)}&lat=${lat}&lon=${lon}`),
  /** WPC forecast chart and SPC outlook areas a route crosses, all days */
  official: (path: [number, number][]) => post<OfficialHit[]>('/api/wx/official', { path }),
  /** when the WPC chart's and SPC outlook's day 1 ends (unix s) */
  outlooks: () => j<{ wpcDay1End: number | null; spcDay1End: number | null }>('/api/wx/outlooks'),
  /** HRRR runs with tiles at IEM (unix s) and the last extended-run minute served */
  hrrr: () => j<{ short: number | null; extended: number | null; extendedLast: number }>('/api/wx/hrrr'),
  notams: (req: { locations?: string[]; points?: [number, number][]; radius?: number; artccs?: string[] }) => post<NotamResponse>('/api/notams', req),
  config: () => j<ServerConfig>('/api/config'),
  setupStatus: () => j<SetupStatus>('/api/setup/status'),
  setupDownload: (url?: string) => post<{ started: boolean }>('/api/setup/download', { url }),
  setupFolder: (path?: string) => post<{ started?: boolean; cancelled?: boolean; path?: string }>('/api/setup/folder', { path }),
  setupCancel: () => post<{ ok: boolean }>('/api/setup/cancel', {}),
  /** stream a data-pack zip to the server; progress shows up in setupStatus().job */
  setupUpload: (file: File) =>
    j<{ started: boolean }>('/api/setup/upload', { method: 'POST', headers: { 'content-type': 'application/zip', 'x-file-name': encodeURIComponent(file.name) }, body: file }),
  /** saved flight plans, one by one (shared with a connected desktop app) */
  plans: () => j<any[]>('/api/plans'),
  savePlan: (plan: { id: string }) => j<{ ok: boolean }>(`/api/plans/${encodeURIComponent(plan.id)}`, { method: 'PUT', headers: { 'content-type': 'application/json' }, body: JSON.stringify(plan) }),
  deletePlan: (id: string) => j<{ ok: boolean }>(`/api/plans/${encodeURIComponent(id)}`, { method: 'DELETE' }),
  /** a JSON document saved on the server (aircraft, prefs); null when never saved */
  userGet: <T>(key: UserDoc) => j<T | null>(`/api/user/${key}`),
  userPut: (key: UserDoc, value: unknown) => j<{ ok: boolean }>(`/api/user/${key}`, { method: 'PUT', headers: { 'content-type': 'application/json' }, body: JSON.stringify(value) }),
  hostSave: (name: string, label: string, body: Blob) =>
    j<{ path?: string; cancelled?: boolean }>(`/api/host/save?name=${encodeURIComponent(name)}&label=${encodeURIComponent(label)}`, { method: 'POST', body }),
  hostOpen: (url: string) => post<{ ok: boolean }>('/api/host/open', { url }),
  hostPrint: () => post<{ ok: boolean }>('/api/host/print', {}),
}

export type UserDoc = 'aircraft' | 'prefs' | 'minimums'

export interface StationRaw {
  airport: string
  station: string
  offset_nm: number
  metar: any | null
  taf: any | null
  /** IEM mos.json documents */
  nbs: any | null
  nbe: any | null
}

export interface OfficialHit {
  product: string
  day: number
  label: string
  detail: string
}

export interface NotamResponse {
  configured: boolean
  service?: string
  items: any[]
  errors?: string[]
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
  /** NM to the FB station the forecast comes from */
  station_nm?: number
  /** each time carries the bulletin's "for use" window (unix s) */
  levels: { p: number; alt: number; times: { t: number; dir: number; spd: number; temp: number | null; alt: number; from?: number; to?: number }[] }[]
}
