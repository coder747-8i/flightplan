// Sunrise / sunset / civil twilight (NOAA solar position approximation, ±1-2 min).
const rad = Math.PI / 180

function julian(date: Date) {
  return date.getTime() / 86400000 + 2440587.5
}

/** times for the UTC day containing `date`; `zenith` 90.833 = sunrise/set, 96 = civil twilight */
export function sunTimes(lat: number, lon: number, date: Date, zenith = 90.833): { rise: Date | null; set: Date | null } {
  const day = new Date(Date.UTC(date.getUTCFullYear(), date.getUTCMonth(), date.getUTCDate()))
  const n = Math.round(julian(day) - 2451545.0 + 0.0008)
  const jStar = n - lon / 360
  const M = (357.5291 + 0.98560028 * jStar) % 360
  const C = 1.9148 * Math.sin(M * rad) + 0.02 * Math.sin(2 * M * rad) + 0.0003 * Math.sin(3 * M * rad)
  const lambda = (M + C + 180 + 102.9372) % 360
  const jTransit = 2451545.0 + jStar + 0.0053 * Math.sin(M * rad) - 0.0069 * Math.sin(2 * lambda * rad)
  const decl = Math.asin(Math.sin(lambda * rad) * Math.sin(23.4397 * rad))
  const cosH = (Math.cos(zenith * rad) - Math.sin(lat * rad) * Math.sin(decl)) / (Math.cos(lat * rad) * Math.cos(decl))
  if (cosH > 1 || cosH < -1) return { rise: null, set: null }
  const H = Math.acos(cosH) / rad
  const toDate = (j: number) => new Date((j - 2440587.5) * 86400000)
  return { rise: toDate(jTransit - H / 360), set: toDate(jTransit + H / 360) }
}
