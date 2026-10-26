// The measuring tool: each click adds a point to the current line (the next leg follows the
// pointer); Space starts another line, up to three at once (the oldest goes); Backspace removes
// the last point; Esc clears, and Esc again closes the tool. R toggles it.
import type { Place } from '../api'

export const RULER_LINES = 3

export interface RulerPoint {
  lat: number
  lon: number
  /** airport / navaid / fix id when the point snapped to one */
  label: string
}

class Ruler {
  on = $state(false)
  /** the last line is the one being drawn */
  lines = $state<RulerPoint[][]>([])

  toggle() {
    this.on = !this.on
    this.lines = []
  }

  add(p: Place) {
    if (!this.lines.length) this.lines.push([])
    this.lines[this.lines.length - 1].push({ lat: p.lat, lon: p.lon, label: p.kind === 'coord' ? '' : p.id })
  }

  newLine() {
    if (!this.lines[this.lines.length - 1]?.length) return
    this.lines.push([])
    while (this.lines.length > RULER_LINES) this.lines.shift()
  }

  undo() {
    this.lines[this.lines.length - 1]?.pop()
  }

  /** Esc: clear, or close when already clear */
  escape() {
    if (this.lines.some((l) => l.length)) this.lines = []
    else this.on = false
  }
}

export const ruler = new Ruler()
