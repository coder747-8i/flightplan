// Chart symbols drawn on a canvas at load time, so they stay crisp at any pixel ratio
// and follow the active theme's colours.
import type { Map as MLMap } from 'maplibre-gl'
import type { Palette } from './palette'

const PR = 2 // draw at 2x and register with pixelRatio 2

type Draw = (c: CanvasRenderingContext2D, s: number) => void

function make(map: MLMap, name: string, size: number, draw: Draw) {
  const px = size * PR
  const canvas = document.createElement('canvas')
  canvas.width = canvas.height = px
  const c = canvas.getContext('2d')!
  c.scale(PR, PR)
  c.lineCap = 'round'
  c.lineJoin = 'round'
  draw(c, size)
  const data = c.getImageData(0, 0, px, px)
  if (map.hasImage(name)) map.removeImage(name)
  map.addImage(name, data, { pixelRatio: PR })
}

function airport(color: string, halo: string, opts: { hard: boolean; fuel: boolean; pvt?: boolean; mil?: boolean }): Draw {
  return (c, s) => {
    const m = s / 2
    const r = s * 0.27
    // service ticks (fuel available), like the sectional's "cogwheel"
    if (opts.fuel) {
      c.strokeStyle = color
      c.lineWidth = 2.2
      for (let i = 0; i < 4; i++) {
        const a = (Math.PI / 2) * i + Math.PI / 4
        c.beginPath()
        c.moveTo(m + Math.cos(a) * (r + 1.5), m + Math.sin(a) * (r + 1.5))
        c.lineTo(m + Math.cos(a) * (r + 4.5), m + Math.sin(a) * (r + 4.5))
        c.stroke()
      }
    }
    c.fillStyle = halo
    c.beginPath()
    c.arc(m, m, r + 1.5, 0, Math.PI * 2)
    c.fill()
    if (opts.hard) {
      c.strokeStyle = color
      c.lineWidth = 2
      c.beginPath()
      c.arc(m, m, r, 0, Math.PI * 2)
      c.stroke()
      // runway bar, pointing "up" (icon-rotate turns it to the true heading)
      c.lineWidth = 3
      c.beginPath()
      c.moveTo(m, m - r - 3.5)
      c.lineTo(m, m + r + 3.5)
      c.stroke()
    } else {
      c.fillStyle = color
      c.beginPath()
      c.arc(m, m, r, 0, Math.PI * 2)
      c.fill()
    }
    if (opts.pvt) {
      c.fillStyle = opts.hard ? color : halo
      c.font = `700 ${s * 0.32}px Inter Variable, sans-serif`
      c.textAlign = 'center'
      c.textBaseline = 'middle'
      c.fillText('R', m, m + 0.5)
    }
    if (opts.mil) {
      c.strokeStyle = color
      c.lineWidth = 1.4
      c.beginPath()
      c.arc(m, m, r - 3, 0, Math.PI * 2)
      c.stroke()
    }
  }
}

function heliport(color: string, halo: string): Draw {
  return (c, s) => {
    const m = s / 2
    c.fillStyle = halo
    c.beginPath()
    c.arc(m, m, s * 0.3, 0, Math.PI * 2)
    c.fill()
    c.strokeStyle = color
    c.lineWidth = 1.6
    c.stroke()
    c.fillStyle = color
    c.font = `700 ${s * 0.36}px Inter Variable, sans-serif`
    c.textAlign = 'center'
    c.textBaseline = 'middle'
    c.fillText('H', m, m + 0.5)
  }
}

function hexagon(c: CanvasRenderingContext2D, m: number, r: number) {
  c.beginPath()
  for (let i = 0; i < 6; i++) {
    const a = (Math.PI / 3) * i
    const x = m + Math.cos(a) * r
    const y = m + Math.sin(a) * r
    i ? c.lineTo(x, y) : c.moveTo(x, y)
  }
  c.closePath()
}

function vor(color: string, halo: string, kind: 'VOR' | 'VORTAC' | 'VOR/DME' | 'DME' | 'TACAN'): Draw {
  return (c, s) => {
    const m = s / 2
    const r = s * 0.24
    c.strokeStyle = color
    c.fillStyle = halo
    c.lineWidth = 1.6
    if (kind === 'VORTAC' || kind === 'TACAN') {
      // three filled "ears" on alternate hexagon sides
      c.fillStyle = color
      for (let i = 0; i < 3; i++) {
        const a = (Math.PI * 2 * i) / 3 + Math.PI / 2
        c.save()
        c.translate(m + Math.cos(a) * r * 1.05, m + Math.sin(a) * r * 1.05)
        c.rotate(a + Math.PI / 2)
        c.fillRect(-r * 0.5, -2.5, r, 5)
        c.restore()
      }
      c.fillStyle = halo
    }
    if (kind === 'VOR/DME' || kind === 'DME') {
      c.strokeRect(m - r * 1.15, m - r * 1.15, r * 2.3, r * 2.3)
    }
    if (kind !== 'DME') {
      hexagon(c, m, r)
      c.fill()
      c.stroke()
    }
    c.fillStyle = color
    c.beginPath()
    c.arc(m, m, 1.6, 0, Math.PI * 2)
    c.fill()
  }
}

function ndb(color: string): Draw {
  return (c, s) => {
    const m = s / 2
    c.fillStyle = color
    for (const [ring, n, dot] of [[s * 0.18, 10, 0.9], [s * 0.3, 16, 0.8], [s * 0.42, 22, 0.7]] as const) {
      for (let i = 0; i < n; i++) {
        const a = (Math.PI * 2 * i) / n
        c.beginPath()
        c.arc(m + Math.cos(a) * ring, m + Math.sin(a) * ring, dot, 0, Math.PI * 2)
        c.fill()
      }
    }
    c.beginPath()
    c.arc(m, m, 2.4, 0, Math.PI * 2)
    c.fill()
  }
}

function compassRose(color: string): Draw {
  return (c, s) => {
    const m = s / 2
    const r = s / 2 - 2
    c.strokeStyle = color
    c.globalAlpha = 0.75
    c.lineWidth = 1
    c.beginPath()
    c.arc(m, m, r, 0, Math.PI * 2)
    c.stroke()
    for (let deg = 0; deg < 360; deg += 5) {
      const a = ((deg - 90) * Math.PI) / 180
      const len = deg % 30 === 0 ? 9 : deg % 10 === 0 ? 6 : 3
      c.lineWidth = deg % 30 === 0 ? 1.4 : 1
      c.beginPath()
      c.moveTo(m + Math.cos(a) * r, m + Math.sin(a) * r)
      c.lineTo(m + Math.cos(a) * (r - len), m + Math.sin(a) * (r - len))
      c.stroke()
    }
    // magnetic north arrow
    c.fillStyle = color
    c.beginPath()
    c.moveTo(m, m - r - 1)
    c.lineTo(m - 4, m - r + 9)
    c.lineTo(m + 4, m - r + 9)
    c.closePath()
    c.fill()
    c.font = `600 9px Inter Variable, sans-serif`
    c.textAlign = 'center'
    c.textBaseline = 'middle'
    for (const [deg, label] of [[90, '9'], [180, '18'], [270, '27']] as const) {
      const a = ((deg - 90) * Math.PI) / 180
      c.save()
      c.translate(m + Math.cos(a) * (r - 16), m + Math.sin(a) * (r - 16))
      c.rotate((deg * Math.PI) / 180)
      c.fillText(label, 0, 0)
      c.restore()
    }
  }
}

function waypoint(color: string, halo: string, vfr: boolean): Draw {
  return (c, s) => {
    const m = s / 2
    if (vfr) {
      // VFR waypoint: four-point star in a ring
      c.strokeStyle = color
      c.lineWidth = 1.4
      c.fillStyle = halo
      c.beginPath()
      c.arc(m, m, s * 0.36, 0, Math.PI * 2)
      c.fill()
      c.stroke()
      c.fillStyle = color
      c.beginPath()
      const R = s * 0.32
      const r = s * 0.09
      for (let i = 0; i < 8; i++) {
        const a = (Math.PI / 4) * i - Math.PI / 2
        const rad = i % 2 ? r : R
        const x = m + Math.cos(a) * rad
        const y = m + Math.sin(a) * rad
        i ? c.lineTo(x, y) : c.moveTo(x, y)
      }
      c.closePath()
      c.fill()
    } else {
      c.fillStyle = halo
      c.strokeStyle = color
      c.lineWidth = 1.4
      c.beginPath()
      c.moveTo(m, m - s * 0.3)
      c.lineTo(m + s * 0.28, m + s * 0.22)
      c.lineTo(m - s * 0.28, m + s * 0.22)
      c.closePath()
      c.fill()
      c.stroke()
    }
  }
}

function obstacle(color: string, halo: string, lit: boolean): Draw {
  return (c, s) => {
    const m = s / 2
    c.strokeStyle = halo
    c.lineWidth = 4
    const path = () => {
      c.beginPath()
      c.moveTo(m - s * 0.22, m + s * 0.3)
      c.lineTo(m, m - s * 0.25)
      c.lineTo(m + s * 0.22, m + s * 0.3)
    }
    path()
    c.stroke()
    c.strokeStyle = color
    c.lineWidth = 1.6
    path()
    c.stroke()
    c.fillStyle = color
    c.beginPath()
    c.arc(m, m + s * 0.3, 1.4, 0, Math.PI * 2)
    c.fill()
    if (lit) {
      c.lineWidth = 1.1
      for (const a of [-60, -30, 0, 30, 60]) {
        const rad = ((a - 90) * Math.PI) / 180
        c.beginPath()
        c.moveTo(m + Math.cos(rad) * s * 0.33, m - s * 0.25 + 2 + Math.sin(rad) * s * 0.18)
        c.lineTo(m + Math.cos(rad) * s * 0.45, m - s * 0.25 + 2 + Math.sin(rad) * s * 0.28)
        c.stroke()
      }
    }
  }
}

function routeNode(color: string, halo: string): Draw {
  return (c, s) => {
    const m = s / 2
    c.fillStyle = halo
    c.beginPath()
    c.arc(m, m, s * 0.42, 0, Math.PI * 2)
    c.fill()
    c.fillStyle = color
    c.beginPath()
    c.arc(m, m, s * 0.28, 0, Math.PI * 2)
    c.fill()
  }
}

export function registerIcons(map: MLMap, p: Palette) {
  const halo = p.symbolHalo
  for (const [key, color] of [['twr', p.towered], ['ntw', p.nontowered], ['mil', p.military]] as const) {
    for (const hard of [true, false]) {
      for (const fuel of [true, false]) {
        make(map, `apt-${key}-${hard ? 'h' : 's'}-${fuel ? 'f' : 'n'}`, 26, airport(color, halo, { hard, fuel, mil: key === 'mil' }))
      }
    }
  }
  make(map, 'apt-pvt-h', 24, airport(p.nontowered, halo, { hard: true, fuel: false, pvt: true }))
  make(map, 'apt-pvt-s', 24, airport(p.nontowered, halo, { hard: false, fuel: false, pvt: true }))
  make(map, 'heli', 22, heliport(p.nontowered, halo))
  make(map, 'heli-twr', 22, heliport(p.towered, halo))
  for (const k of ['VOR', 'VORTAC', 'VOR/DME', 'DME', 'TACAN'] as const) make(map, `nav-${k}`, 26, vor(p.navaid, halo, k))
  make(map, 'nav-NDB', 26, ndb(p.ndb))
  make(map, 'nav-NDB/DME', 26, ndb(p.ndb))
  make(map, 'rose', 220, compassRose(p.navaid))
  make(map, 'fix-vfr', 18, waypoint(p.nontowered, halo, true))
  make(map, 'fix-ifr', 14, waypoint(p.navaid, halo, false))
  make(map, 'obst', 18, obstacle(p.obstacle, halo, false))
  make(map, 'obst-lit', 18, obstacle(p.obstacle, halo, true))
  make(map, 'route-node', 18, routeNode(p.route, '#ffffff'))
}
