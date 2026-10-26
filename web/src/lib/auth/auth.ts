// Sign-in for the web app (server/src/auth.rs). Loaded before the app itself, so it uses plain
// fetch rather than the API client (which pulls in the stores).

export interface AuthStatus {
  /** false inside the desktop app: no sign-in there */
  enforced: boolean
  /** a password has been set */
  configured: boolean
  signedIn: boolean
  name: string
  hasImage: boolean
  /** shortest password the server accepts */
  minPassword: number
}

async function post(url: string, body: unknown, method = 'POST') {
  const r = await fetch(url, { method, headers: { 'content-type': 'application/json', 'x-flightplan': '1' }, body: JSON.stringify(body) })
  if (!r.ok) {
    let msg = `HTTP ${r.status}`
    try {
      msg = (await r.json()).error ?? msg
    } catch {
      /* not JSON */
    }
    throw new Error(msg)
  }
  return r.json()
}

export const auth = {
  status: async (): Promise<AuthStatus> => (await fetch('/api/auth/status')).json(),
  login: (password: string) => post('/api/auth/login', { password }),
  /** check the one-time setup code the server printed */
  setupCheck: (code: string) => post('/api/auth/setup/check', { code }),
  setup: (code: string, name: string, password: string) => post('/api/auth/setup', { code, name, password }),
  logout: () => post('/api/auth/logout', {}),
  /** sign every device out, this one included */
  logoutAll: () => post('/api/auth/logout-all', {}),
  changePassword: (current: string, next: string) => post('/api/auth/password', { current, new: next }),
  setName: (name: string) => post('/api/auth/name', { name }),
  setImage: async (file: File) => {
    const r = await fetch('/api/auth/image', { method: 'PUT', headers: { 'content-type': file.type, 'x-flightplan': '1' }, body: file })
    if (!r.ok) throw new Error((await r.json().catch(() => null))?.error ?? `HTTP ${r.status}`)
  },
  removeImage: () => post('/api/auth/image', {}, 'DELETE'),
}

export type Phase = 'morning' | 'afternoon' | 'evening' | 'night'

export function phaseOf(d = new Date()): Phase {
  const h = d.getHours()
  return h >= 5 && h < 12 ? 'morning' : h >= 12 && h < 17 ? 'afternoon' : h >= 17 && h < 22 ? 'evening' : 'night'
}

/** "Good morning" / "Good afternoon" / "Good evening" (late at night too). */
export const greeting = (p: Phase) => `Good ${p === 'night' ? 'evening' : p}`

/** Rough strength for the meter: 0 weak, 1 fair, 2 good, 3 strong. The server has the final say
 *  (length, common passwords, your name). */
export function strength(pw: string): number {
  if (pw.length < 10) return 0
  const classes = [/[a-z]/, /[A-Z]/, /\d/, /[^A-Za-z0-9]/].filter((r) => r.test(pw)).length
  const words = pw.trim().split(/\s+/).length
  const unique = new Set(pw.toLowerCase()).size
  let s = pw.length >= 16 || words >= 3 ? 2 : 1
  if (classes >= 3 || words >= 4 || pw.length >= 20) s++
  if (unique < 5) s = Math.min(s, 1)
  return Math.min(3, s)
}
