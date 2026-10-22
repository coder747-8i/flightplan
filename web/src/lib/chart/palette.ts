// Chart colours. Day follows the sectional's conventions (blue = towered / Class B & D,
// magenta = non-towered / Class C & E) in a calmer, flatter tone; Night is the same map inverted.
export interface Palette {
  name: 'day' | 'night'
  background: string
  water: string
  waterLine: string
  urban: string
  road: string
  roadMajor: string
  border: string
  label: string
  labelHalo: string
  hillShadow: string
  hillHighlight: string
  relief: [number, string][] // metres -> colour
  towered: string
  nontowered: string
  military: string
  navaid: string
  ndb: string
  obstacle: string
  classB: string
  classC: string
  classD: string
  classE: string
  restricted: string
  moa: string
  warning: string
  alert: string
  airway: string
  runway: string
  runwaySoft: string
  symbolHalo: string
  route: string
}

export const DAY: Palette = {
  name: 'day',
  background: '#f4f2ea',
  water: '#bcd8ec',
  waterLine: '#8fbbdb',
  urban: '#f5e7a8',
  road: '#d9d1c4',
  roadMajor: '#c4b5a5',
  border: '#9a8f9e',
  label: '#3d3a36',
  labelHalo: 'rgba(250,248,242,0.9)',
  hillShadow: '#5c5246',
  hillHighlight: '#ffffff',
  relief: [
    [-50, '#eaf0dd'],
    [0, '#e8efd9'],
    [300, '#eef1d6'],
    [700, '#f2eed2'],
    [1200, '#efe4c4'],
    [1800, '#e8d7b4'],
    [2500, '#dcc6a1'],
    [3200, '#d2b994'],
    [4200, '#e6ddd2'],
  ],
  towered: '#2c63b8',
  nontowered: '#a33a86',
  military: '#5b5f66',
  navaid: '#2c63b8',
  ndb: '#a33a86',
  obstacle: '#2b2b2b',
  classB: '#2c63b8',
  classC: '#a33a86',
  classD: '#2c63b8',
  classE: '#a33a86',
  restricted: '#2c63b8',
  moa: '#a33a86',
  warning: '#2c63b8',
  alert: '#a33a86',
  airway: '#6f8fbf',
  runway: '#4b4b4b',
  runwaySoft: '#8a7a5c',
  symbolHalo: '#ffffff',
  route: '#d4148c',
}

export const NIGHT: Palette = {
  name: 'night',
  background: '#151a21',
  water: '#1d3247',
  waterLine: '#2a4a68',
  urban: '#2c2a22',
  road: '#2b3139',
  roadMajor: '#3b434e',
  border: '#56506a',
  label: '#c9ced6',
  labelHalo: 'rgba(16,20,26,0.9)',
  hillShadow: '#000000',
  hillHighlight: '#6d7a8a',
  relief: [
    [-50, '#161c22'],
    [0, '#171e24'],
    [700, '#1b2126'],
    [1500, '#211f1d'],
    [2500, '#272320'],
    [4200, '#2e2c2b'],
  ],
  towered: '#6ea3ff',
  nontowered: '#e77fd0',
  military: '#a3a8b0',
  navaid: '#6ea3ff',
  ndb: '#e77fd0',
  obstacle: '#e8e8e8',
  classB: '#6ea3ff',
  classC: '#e77fd0',
  classD: '#6ea3ff',
  classE: '#e77fd0',
  restricted: '#6ea3ff',
  moa: '#e77fd0',
  warning: '#6ea3ff',
  alert: '#e77fd0',
  airway: '#5a77a3',
  runway: '#c9c9c9',
  runwaySoft: '#9c8d6e',
  symbolHalo: '#151a21',
  route: '#ff3fb4',
}
