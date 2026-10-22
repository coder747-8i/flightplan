// Weather display helpers.
export function fltCatColor(cat?: string) {
  switch (cat) {
    case 'VFR': return 'var(--vfr)'
    case 'MVFR': return 'var(--mvfr)'
    case 'IFR': return 'var(--ifr)'
    case 'LIFR': return 'var(--lifr)'
    default: return 'var(--text-3)'
  }
}

export const FLT_CAT_HEX: Record<string, string> = { VFR: '#1f9d4a', MVFR: '#2c63b8', IFR: '#d23a3a', LIFR: '#b03ab8' }

/** "12 min ago" from a unix-seconds timestamp */
export function fmtAge(unix: number) {
  const m = Math.round((Date.now() / 1000 - unix) / 60)
  if (m < 1) return 'just now'
  if (m < 60) return `${m} min ago`
  const h = Math.floor(m / 60)
  return `${h} h ${m % 60} min ago`
}

export function zulu(unix: number) {
  const d = new Date(unix * 1000)
  return `${String(d.getUTCDate()).padStart(2, '0')}/${String(d.getUTCHours()).padStart(2, '0')}${String(d.getUTCMinutes()).padStart(2, '0')}Z`
}
