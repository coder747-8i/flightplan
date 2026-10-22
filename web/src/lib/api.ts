// Thin client for the Rust API.
export interface Place {
  kind: 'airport' | 'navaid' | 'fix' | 'coord'
  id: string
  name: string
  detail: string
  lat: number
  lon: number
}

async function j<T>(url: string, init?: RequestInit): Promise<T> {
  const r = await fetch(url, init)
  if (!r.ok) throw new Error(`${r.status} ${(await r.text()).slice(0, 200)}`)
  return r.json()
}

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
  config: () => j<{ windyMapKey: string | null; windyPoint: boolean }>('/api/config'),
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
