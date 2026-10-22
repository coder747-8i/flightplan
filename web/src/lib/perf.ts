// Dev-only timing: logs any wrapped computation slower than one frame.
export function timed<T>(name: string, fn: () => T): T {
  if (!import.meta.env.DEV) return fn()
  const t0 = performance.now()
  const out = fn()
  const ms = performance.now() - t0
  if (ms > 8) console.warn(`[perf] ${name} ${ms.toFixed(1)} ms`)
  ;((globalThis as any).__perf ??= {})[name] = Math.round(ms * 10) / 10
  return out
}
