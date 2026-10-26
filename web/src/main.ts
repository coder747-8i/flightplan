import { mount, unmount } from 'svelte'
import './app.css'
import Login from './lib/auth/Login.svelte'
import { auth, type AuthStatus } from './lib/auth/auth'

const target = document.getElementById('app')!

// the chart theme the viewer chose last time, so the sign-in screen matches it
try {
  const theme = JSON.parse(localStorage.getItem('fp.prefs') ?? '{}').theme
  if (theme) document.documentElement.dataset.theme = theme
} catch {
  /* ignore */
}

/** The app itself loads only once signed in: its stores start fetching as soon as they load. */
async function startApp() {
  const { default: App } = await import('./App.svelte')
  mount(App, { target })
  if (import.meta.env.DEV) {
    // handles for debugging in the console
    Promise.all([import('./lib/stores/route.svelte'), import('./lib/stores/plan.svelte'), import('./lib/stores/app.svelte')]).then(([r, p, a]) => {
      ;(window as any).__fp = { route: r.route, plan: p.plan, app: a.app }
    })
  }
}

async function start() {
  let status: AuthStatus | null = null
  try {
    status = await auth.status()
  } catch {
    /* server unreachable: the app shows its own errors */
  }
  if (!status || status.signedIn) return startApp()
  const login = mount(Login, {
    target,
    props: {
      status,
      onsignedin: () => {
        unmount(login)
        startApp()
      },
    },
  })
}

start()
