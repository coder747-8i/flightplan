// The pilot's personal minimums for the go / no-go call. Saved in the browser and on the server
// (like the aircraft), so every device that opens the app judges by the same limits.
import { api } from '../api'
import { DEFAULT_MINIMUMS, type Minimums } from '../nav/verdict'
import { save } from './app.svelte'

function load(): Minimums {
  try {
    const raw = localStorage.getItem('fp.minimums')
    if (raw) return { ...DEFAULT_MINIMUMS, ...JSON.parse(raw) }
  } catch {
    /* ignore */
  }
  return { ...DEFAULT_MINIMUMS }
}

export const minimums = $state<Minimums>(load())

let fromServer = false
let ready = false
let timer: ReturnType<typeof setTimeout>

api
  .userGet<Partial<Minimums>>('minimums')
  .then((m) => {
    if (m) {
      fromServer = true
      Object.assign(minimums, { ...DEFAULT_MINIMUMS, ...m })
    }
  })
  .catch(() => {})
  .finally(() => (ready = true))

$effect.root(() => {
  $effect(() => {
    JSON.stringify(minimums)
    // thunder no-go can't sit below the caution level
    if (minimums.tstmNogo < minimums.tstmCaution) minimums.tstmNogo = minimums.tstmCaution
    save('minimums', minimums)
    if (fromServer) {
      fromServer = false
      return
    }
    if (!ready) return
    clearTimeout(timer)
    timer = setTimeout(() => api.userPut('minimums', $state.snapshot(minimums)).catch(() => {}), 800)
  })
})
