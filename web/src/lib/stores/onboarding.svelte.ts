// First-run setup: shown until it has been finished once, and again whenever chart data is
// missing. The "finished" flag is kept on the server (shared by every device) and locally.
import { api, type SetupStatus } from '../api'

interface Prefs {
  onboarded?: boolean
  /** data pack address, when the user changed it */
  dataUrl?: string
}

const LOCAL = 'fp.onboarded'

class OnboardingState {
  open = $state(false)
  step = $state(0)
  status = $state<SetupStatus | null>(null)
  prefs = $state<Prefs>({})
  /** the spotlight tour over the real UI */
  touring = $state(false)
  /** chart data was (re)installed in this session: the map must reload to pick it up */
  dataChanged = $state(false)

  async init() {
    let local = false
    try {
      local = localStorage.getItem(LOCAL) === '1'
    } catch {
      /* ignore */
    }
    const [prefs, status] = await Promise.all([api.userGet<Prefs>('prefs').catch(() => null), this.refresh()])
    this.prefs = prefs ?? {}
    const done = local || !!this.prefs.onboarded
    // a server that builds its own data fills it in by itself: nothing for returning users to do
    if (!done || (status && !status.installed && !status.builder)) {
      // returning users with missing data go straight to the data step
      this.step = done ? 2 : 0
      this.open = true
    }
  }

  async refresh() {
    try {
      this.status = await api.setupStatus()
    } catch {
      /* server unreachable */
    }
    return this.status
  }

  savePrefs(patch: Partial<Prefs>) {
    this.prefs = { ...this.prefs, ...patch }
    api.userPut('prefs', $state.snapshot(this.prefs)).catch(() => {})
  }

  finish(tour = false) {
    try {
      localStorage.setItem(LOCAL, '1')
    } catch {
      /* ignore */
    }
    this.savePrefs({ onboarded: true })
    this.open = false
    if (this.dataChanged) {
      // fresh tiles need a fresh map; come back into the tour if one was asked for
      if (tour) sessionStorage.setItem('fp.tourAfterReload', '1')
      location.reload()
      return
    }
    this.touring = tour
  }

  reopen(step = 0) {
    this.step = step
    this.open = true
    this.refresh()
  }
}

export const onboarding = new OnboardingState()
onboarding.init()
try {
  if (sessionStorage.getItem('fp.tourAfterReload')) {
    sessionStorage.removeItem('fp.tourAfterReload')
    onboarding.touring = true
  }
} catch {
  /* ignore */
}
