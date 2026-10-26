// Undo / redo for the plan: the route (waypoints, fuel stops, altitude, departure, ground time)
// and the fleet (aircraft edits and loading). Snapshots are taken by watching for changes, so
// every way of editing is covered; a burst of changes (dragging a number, typing) within a
// moment of each other is one step.
import { aircraft, type Aircraft } from './aircraft.svelte'
import { route, type Waypoint } from './route.svelte'

const LIMIT = 100
/** changes closer together than this are one undo step */
const BURST = 800

interface Snap {
  waypoints: Waypoint[]
  cruise: number
  departure: string
  stopMinutes: number
  fleet: Aircraft[]
  active: string
}

const take = (): Snap => ({
  waypoints: $state.snapshot(route.waypoints),
  cruise: route.cruise,
  departure: route.departure,
  stopMinutes: route.stopMinutes,
  fleet: $state.snapshot(aircraft.list),
  active: aircraft.activeId,
})

const key = (s: Snap) => JSON.stringify(s)

class History {
  private undoStack: Snap[] = []
  private redoStack: Snap[] = []
  private last: { snap: Snap; key: string } | null = null
  private changedAt = 0
  /** set while restoring, so the restore isn't recorded as a new step */
  private restoring = false
  canUndo = $state(false)
  canRedo = $state(false)

  observe(now: Snap) {
    const k = key(now)
    if (this.restoring) {
      this.restoring = false
      this.last = { snap: now, key: k }
      return
    }
    if (!this.last) {
      this.last = { snap: now, key: k }
      return
    }
    if (this.last.key === k) return
    if (Date.now() - this.changedAt >= BURST) {
      this.undoStack.push(this.last.snap)
      if (this.undoStack.length > LIMIT) this.undoStack.shift()
      this.redoStack = []
    }
    this.changedAt = Date.now()
    this.last = { snap: now, key: k }
    this.sync()
  }

  private sync() {
    this.canUndo = this.undoStack.length > 0
    this.canRedo = this.redoStack.length > 0
  }

  private restore(s: Snap) {
    this.restoring = true
    this.changedAt = 0
    route.waypoints = structuredClone(s.waypoints)
    route.cruise = s.cruise
    route.departure = s.departure
    route.stopMinutes = s.stopMinutes
    route.editing = false
    aircraft.list = structuredClone(s.fleet)
    aircraft.activeId = s.active
    this.last = { snap: s, key: key(s) }
    this.sync()
  }

  undo(): boolean {
    const prev = this.undoStack.pop()
    if (!prev) return false
    this.redoStack.push(take())
    this.restore(prev)
    return true
  }

  redo(): boolean {
    const next = this.redoStack.pop()
    if (!next) return false
    this.undoStack.push(take())
    this.restore(next)
    return true
  }
}

export const history = new History()

$effect.root(() => {
  $effect(() => {
    history.observe(take())
  })
})
