// Server-side configuration the UI depends on (which API keys are set).
import { api } from '../api'

class ConfigState {
  windyMapKey = $state<string | null>(null)
  windyPoint = $state(false)
  notams = $state(false)

  async reload() {
    try {
      const c = await api.config()
      this.windyMapKey = c.windyMapKey
      this.windyPoint = c.windyPoint
      this.notams = (c as any).notams ?? false
    } catch {
      /* server unreachable; keep previous values */
    }
  }
}

export const config = new ConfigState()
config.reload()
