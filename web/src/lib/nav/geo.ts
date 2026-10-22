// Great-circle helpers (NM, degrees true).
const R = 3440.065
const rad = (d: number) => (d * Math.PI) / 180
const degr = (r: number) => (r * 180) / Math.PI

export function distanceNm(lat1: number, lon1: number, lat2: number, lon2: number) {
  const dp = rad(lat2 - lat1)
  const dl = rad(lon2 - lon1)
  const a = Math.sin(dp / 2) ** 2 + Math.cos(rad(lat1)) * Math.cos(rad(lat2)) * Math.sin(dl / 2) ** 2
  return 2 * R * Math.asin(Math.sqrt(a))
}

export function bearing(lat1: number, lon1: number, lat2: number, lon2: number) {
  const y = Math.sin(rad(lon2 - lon1)) * Math.cos(rad(lat2))
  const x = Math.cos(rad(lat1)) * Math.sin(rad(lat2)) - Math.sin(rad(lat1)) * Math.cos(rad(lat2)) * Math.cos(rad(lon2 - lon1))
  return (degr(Math.atan2(y, x)) + 360) % 360
}

export function interpolate(lat1: number, lon1: number, lat2: number, lon2: number, f: number): [number, number] {
  const d = distanceNm(lat1, lon1, lat2, lon2) / R
  if (d < 1e-9) return [lat1, lon1]
  const a = Math.sin((1 - f) * d) / Math.sin(d)
  const b = Math.sin(f * d) / Math.sin(d)
  const x = a * Math.cos(rad(lat1)) * Math.cos(rad(lon1)) + b * Math.cos(rad(lat2)) * Math.cos(rad(lon2))
  const y = a * Math.cos(rad(lat1)) * Math.sin(rad(lon1)) + b * Math.cos(rad(lat2)) * Math.sin(rad(lon2))
  const z = a * Math.sin(rad(lat1)) + b * Math.sin(rad(lat2))
  return [degr(Math.atan2(z, Math.hypot(x, y))), degr(Math.atan2(y, x))]
}

/** densify a route into great-circle segments for drawing */
export function greatCircle(path: [number, number][], stepNm = 10): [number, number][] {
  const out: [number, number][] = []
  for (let i = 1; i < path.length; i++) {
    const [lon1, lat1] = path[i - 1]
    const [lon2, lat2] = path[i]
    const n = Math.max(1, Math.ceil(distanceNm(lat1, lon1, lat2, lon2) / stepNm))
    for (let k = i === 1 ? 0 : 1; k <= n; k++) {
      const [la, lo] = interpolate(lat1, lon1, lat2, lon2, k / n)
      out.push([lo, la])
    }
  }
  return out
}

/** point at along-track distance d (NM) on a waypoint path */
export function pointAlong(path: [number, number][], d: number): [number, number] {
  let acc = 0
  for (let i = 1; i < path.length; i++) {
    const [lon1, lat1] = path[i - 1]
    const [lon2, lat2] = path[i]
    const len = distanceNm(lat1, lon1, lat2, lon2)
    if (acc + len >= d) {
      const [la, lo] = interpolate(lat1, lon1, lat2, lon2, (d - acc) / Math.max(len, 1e-6))
      return [lo, la]
    }
    acc += len
  }
  return path[path.length - 1]
}
