// Server-side configuration the UI depends on (which API keys are set).
import { api, type DataInfo } from '../api'

class ConfigState {
  windyMapKey = $state<string | null>(null)
  windyPoint = $state(false)
  notams = $state(false)
  desktop = $state(false)
  data = $state<DataInfo | null>(null)
  version = $state('')
  /** set once the first answer from the server has arrived */
  loaded = $state(false)

  async reload() {
    try {
      const c = await api.config()
      this.windyMapKey = c.windyMapKey
      this.windyPoint = c.windyPoint
      this.notams = c.notams ?? false
      this.desktop = c.desktop ?? false
      this.data = c.data ?? null
      this.version = c.version ?? ''
      this.loaded = true
    } catch {
      /* server unreachable; keep previous values */
    }
  }

  /** chart data past its 28-day cycle */
  get dataExpired() {
    const e = this.data?.expires
    return !!e && new Date(`${e}T09:01:00Z`).getTime() < Date.now()
  }
}

export const config = new ConfigState()
config.reload()
