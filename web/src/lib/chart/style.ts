// The vector "sectional": an OpenFreeMap base, terrain relief and our FAA layers on top.
import type { ExpressionSpecification, FilterSpecification, LayerSpecification, StyleSpecification } from 'maplibre-gl'
import type { Palette } from './palette'

export const TILE_BASE = `${location.origin}/tiles`
const FONT = ['Noto Sans Regular']
const FONT_BOLD = ['Noto Sans Bold']
const FONT_ITALIC = ['Noto Sans Italic']

// Layer groups the user can toggle. Each id maps to the style layers it controls.
export const LAYER_GROUPS = {
  terrain: ['relief', 'hillshade'],
  basemap: ['urban', 'roads-minor', 'roads-major', 'boundaries', 'place-city', 'place-town', 'water-label'],
  classB: ['as-B-fill', 'as-B-line'],
  classC: ['as-C-fill', 'as-C-line'],
  classD: ['as-D-line'],
  classE: ['as-E-surface', 'as-E5-fade', 'as-E5-msl'],
  classEhigh: ['as-E-1200'],
  modeC: ['as-modec'],
  sua: ['as-sua-fill', 'as-sua-line', 'as-sua-hatch'],
  airspaceLabels: ['as-labels-bc', 'as-labels-d', 'as-labels-sua', 'as-labels-e'],
  airportsPublic: ['apt-public', 'apt-public-label'],
  airportsPrivate: ['apt-private', 'apt-private-label'],
  heliports: ['apt-heli'],
  runways: ['runways-soft', 'runways-hard'],
  navaids: ['nav-rose', 'nav-symbol', 'nav-label'],
  vfrWaypoints: ['fix-vfr', 'fix-vfr-label'],
  ifrFixes: ['fix-ifr', 'fix-ifr-label'],
  airwaysLow: ['awy-low', 'awy-low-label'],
  airwaysHigh: ['awy-high', 'awy-high-label'],
  obstacles: ['obst', 'obst-label'],
} as const

export type LayerGroup = keyof typeof LAYER_GROUPS

// Base filters per airspace layer; the altitude band is AND-ed on top at runtime.
export const AIRSPACE_FILTERS: Record<string, FilterSpecification> = {
  'as-B-fill': ['==', ['get', 'kind'], 'B'],
  'as-B-line': ['==', ['get', 'kind'], 'B'],
  'as-C-fill': ['==', ['get', 'kind'], 'C'],
  'as-C-line': ['==', ['get', 'kind'], 'C'],
  'as-D-line': ['==', ['get', 'kind'], 'D'],
  'as-E-surface': ['in', ['get', 'kind'], ['literal', ['E2', 'E3', 'E4']]],
  'as-E5-fade': ['all', ['==', ['get', 'kind'], 'E5'], ['==', ['get', 'lo_agl'], 1], ['<=', ['get', 'lo'], 700]],
  'as-E5-msl': ['all', ['==', ['get', 'kind'], 'E5'], ['==', ['get', 'lo_agl'], 0]],
  'as-E-1200': ['all', ['in', ['get', 'kind'], ['literal', ['E5', 'E6']]], ['==', ['get', 'lo_agl'], 1], ['>', ['get', 'lo'], 700]],
  'as-modec': ['==', ['get', 'kind'], 'MODEC'],
  'as-sua-fill': ['in', ['get', 'kind'], ['literal', ['R', 'P', 'W', 'A', 'MOA']]],
  'as-sua-line': ['in', ['get', 'kind'], ['literal', ['R', 'P', 'W', 'A', 'MOA']]],
  'as-sua-hatch': ['in', ['get', 'kind'], ['literal', ['R', 'P']]],
  'as-labels-bc': ['in', ['get', 'kind'], ['literal', ['B', 'C']]],
  'as-labels-d': ['==', ['get', 'kind'], 'D'],
  'as-labels-sua': ['in', ['get', 'kind'], ['literal', ['R', 'P', 'W', 'A', 'MOA']]],
  'as-labels-e': ['==', ['get', 'kind'], 'E5'],
}

export function altitudeFilter(base: FilterSpecification, lo: number, hi: number): FilterSpecification {
  // keep airspace whose vertical extent overlaps [lo, hi]
  return ['all', base, ['<=', ['get', 'lo'], hi], ['>=', ['get', 'hi'], lo]] as FilterSpecification
}

const zoomWidth = (pairs: [number, number][]): ExpressionSpecification =>
  ['interpolate', ['exponential', 1.5], ['zoom'], ...pairs.flat()] as unknown as ExpressionSpecification

function suaColor(p: Palette): ExpressionSpecification {
  return ['match', ['get', 'kind'], 'MOA', p.moa, 'A', p.alert, 'W', p.warning, p.restricted] as ExpressionSpecification
}

export function buildStyle(p: Palette): StyleSpecification {
  const night = p.name === 'night'
  const layers: LayerSpecification[] = [
    { id: 'background', type: 'background', paint: { 'background-color': p.background } },
    {
      id: 'relief', type: 'color-relief', source: 'dem',
      paint: {
        'color-relief-color': ['interpolate', ['linear'], ['elevation'], ...p.relief.flat()] as unknown as ExpressionSpecification,
        'color-relief-opacity': 1,
      },
    } as LayerSpecification,
    {
      id: 'hillshade', type: 'hillshade', source: 'dem',
      paint: {
        'hillshade-shadow-color': p.hillShadow,
        'hillshade-highlight-color': p.hillHighlight,
        'hillshade-accent-color': p.hillShadow,
        'hillshade-exaggeration': night ? 0.5 : 0.32,
        'hillshade-illumination-direction': 315,
      },
    },
    {
      id: 'urban', type: 'fill', source: 'base', 'source-layer': 'landuse', minzoom: 6,
      filter: ['in', ['get', 'class'], ['literal', ['residential', 'suburb', 'neighbourhood', 'commercial', 'industrial', 'retail']]],
      paint: { 'fill-color': p.urban, 'fill-opacity': ['interpolate', ['linear'], ['zoom'], 6, 0.55, 12, 0.35] },
    },
    { id: 'water', type: 'fill', source: 'base', 'source-layer': 'water', paint: { 'fill-color': p.water } },
    {
      id: 'waterway', type: 'line', source: 'base', 'source-layer': 'waterway', minzoom: 7,
      filter: ['in', ['get', 'class'], ['literal', ['river', 'canal']]],
      paint: { 'line-color': p.waterLine, 'line-width': zoomWidth([[7, 0.5], [12, 1.6]]) },
    },
    {
      id: 'roads-minor', type: 'line', source: 'base', 'source-layer': 'transportation', minzoom: 10,
      filter: ['in', ['get', 'class'], ['literal', ['primary', 'secondary', 'tertiary']]],
      paint: { 'line-color': p.road, 'line-width': zoomWidth([[10, 0.6], [14, 2.5]]) },
    },
    {
      id: 'roads-major', type: 'line', source: 'base', 'source-layer': 'transportation', minzoom: 6,
      filter: ['in', ['get', 'class'], ['literal', ['motorway', 'trunk']]],
      paint: { 'line-color': p.roadMajor, 'line-width': zoomWidth([[6, 0.6], [12, 2.2]]) },
    },
    {
      id: 'boundaries', type: 'line', source: 'base', 'source-layer': 'boundary',
      filter: ['all', ['<=', ['get', 'admin_level'], 4], ['!=', ['get', 'maritime'], 1]],
      paint: { 'line-color': p.border, 'line-width': zoomWidth([[3, 0.6], [10, 1.4]]), 'line-dasharray': [3, 2], 'line-opacity': 0.6 },
    },

    // ---- airspace -----------------------------------------------------------
    {
      id: 'as-E-1200', type: 'line', source: 'airspace', 'source-layer': 'airspace', layout: { visibility: 'none' },
      paint: { 'line-color': p.classB, 'line-width': 1, 'line-opacity': 0.35, 'line-dasharray': [1, 2] },
    },
    {
      id: 'as-E5-fade', type: 'line', source: 'airspace', 'source-layer': 'airspace',
      paint: {
        'line-color': p.classE, 'line-opacity': night ? 0.22 : 0.18,
        'line-width': zoomWidth([[7, 4], [10, 10], [13, 22]]),
        'line-offset': zoomWidth([[7, 2], [10, 5], [13, 11]]), 'line-blur': zoomWidth([[7, 3], [10, 7], [13, 14]]),
      },
    },
    {
      id: 'as-E5-msl', type: 'line', source: 'airspace', 'source-layer': 'airspace',
      paint: {
        'line-color': p.classB, 'line-opacity': 0.22,
        'line-width': zoomWidth([[7, 4], [10, 10], [13, 22]]),
        'line-offset': zoomWidth([[7, 2], [10, 5], [13, 11]]), 'line-blur': zoomWidth([[7, 3], [10, 7], [13, 14]]),
      },
    },
    {
      id: 'as-E-surface', type: 'line', source: 'airspace', 'source-layer': 'airspace',
      paint: { 'line-color': p.classE, 'line-width': zoomWidth([[7, 1], [12, 2]]), 'line-dasharray': [4, 2.5], 'line-opacity': 0.85 },
    },
    {
      id: 'as-modec', type: 'line', source: 'airspace', 'source-layer': 'airspace',
      paint: { 'line-color': p.classC, 'line-width': 1, 'line-opacity': 0.5 },
    },
    {
      id: 'as-sua-fill', type: 'fill', source: 'airspace', 'source-layer': 'airspace',
      paint: { 'fill-color': suaColor(p), 'fill-opacity': ['match', ['get', 'kind'], 'P', 0.12, 'R', 0.07, 0.04] },
    },
    {
      id: 'as-sua-hatch', type: 'line', source: 'airspace', 'source-layer': 'airspace',
      paint: {
        'line-color': p.restricted, 'line-opacity': 0.35,
        'line-width': zoomWidth([[5, 3], [10, 9], [13, 16]]), 'line-offset': zoomWidth([[5, 1.5], [10, 4.5], [13, 8]]),
        'line-dasharray': [0.25, 0.25],
      },
    },
    {
      id: 'as-sua-line', type: 'line', source: 'airspace', 'source-layer': 'airspace',
      paint: { 'line-color': suaColor(p), 'line-width': zoomWidth([[5, 1], [12, 2]]), 'line-opacity': 0.9 },
    },
    {
      id: 'as-D-line', type: 'line', source: 'airspace', 'source-layer': 'airspace',
      paint: { 'line-color': p.classD, 'line-width': zoomWidth([[6, 1], [12, 2.2]]), 'line-dasharray': [3, 2] },
    },
    {
      id: 'as-C-fill', type: 'line', source: 'airspace', 'source-layer': 'airspace',
      paint: {
        'line-color': p.classC, 'line-opacity': 0.14,
        'line-width': zoomWidth([[5, 3], [10, 10], [13, 20]]), 'line-offset': zoomWidth([[5, 1.5], [10, 5], [13, 10]]),
      },
    },
    {
      id: 'as-C-line', type: 'line', source: 'airspace', 'source-layer': 'airspace',
      paint: { 'line-color': p.classC, 'line-width': zoomWidth([[5, 1.2], [12, 3]]) },
    },
    {
      id: 'as-B-fill', type: 'line', source: 'airspace', 'source-layer': 'airspace',
      paint: {
        'line-color': p.classB, 'line-opacity': 0.14,
        'line-width': zoomWidth([[4, 3], [10, 12], [13, 24]]), 'line-offset': zoomWidth([[4, 1.5], [10, 6], [13, 12]]),
      },
    },
    {
      id: 'as-B-line', type: 'line', source: 'airspace', 'source-layer': 'airspace',
      paint: { 'line-color': p.classB, 'line-width': zoomWidth([[4, 1.2], [12, 3.2]]) },
    },

    // ---- airways, runways --------------------------------------------------
    {
      id: 'awy-low', type: 'line', source: 'aero', 'source-layer': 'airways', layout: { visibility: 'none' },
      filter: ['==', ['get', 'level'], 'low'],
      paint: { 'line-color': p.airway, 'line-width': zoomWidth([[7, 0.8], [12, 2]]), 'line-opacity': 0.55 },
    },
    {
      id: 'awy-high', type: 'line', source: 'aero', 'source-layer': 'airways', layout: { visibility: 'none' },
      filter: ['==', ['get', 'level'], 'high'],
      paint: { 'line-color': p.military, 'line-width': zoomWidth([[6, 0.7], [12, 1.6]]), 'line-opacity': 0.5 },
    },
    {
      id: 'runways-soft', type: 'line', source: 'aero', 'source-layer': 'runways', minzoom: 11,
      filter: ['==', ['get', 'hard'], 0],
      paint: { 'line-color': p.runwaySoft, 'line-width': zoomWidth([[11, 2], [14, 8], [16, 26]]) },
    },
    {
      id: 'runways-hard', type: 'line', source: 'aero', 'source-layer': 'runways', minzoom: 11,
      filter: ['==', ['get', 'hard'], 1],
      paint: { 'line-color': p.runway, 'line-width': zoomWidth([[11, 2.5], [14, 10], [16, 34]]) },
    },

    // ---- labels for airspace -------------------------------------------------
    {
      id: 'as-labels-e', type: 'symbol', source: 'airspace', 'source-layer': 'airspace_labels', minzoom: 9,
      filter: ['all', ['==', ['get', 'kind'], 'E5'], ['==', ['get', 'lo_agl'], 0]],
      layout: { 'text-field': ['get', 'label'], 'text-font': FONT, 'text-size': 11, 'text-allow-overlap': false },
      paint: { 'text-color': p.classB, 'text-halo-color': p.labelHalo, 'text-halo-width': 1.5 },
    },
    {
      id: 'as-labels-sua', type: 'symbol', source: 'airspace', 'source-layer': 'airspace_labels', minzoom: 7,
      layout: {
        'text-field': ['format', ['get', 'label'], { 'font-scale': 1 }, '\n', {}, ['get', 'range'], { 'font-scale': 0.85 }],
        'text-font': FONT_ITALIC, 'text-size': 11, 'text-max-width': 12,
      },
      paint: { 'text-color': suaColor(p), 'text-halo-color': p.labelHalo, 'text-halo-width': 1.5 },
    },
    {
      id: 'as-labels-d', type: 'symbol', source: 'airspace', 'source-layer': 'airspace_labels', minzoom: 8,
      layout: { 'text-field': ['get', 'label'], 'text-font': FONT_BOLD, 'text-size': 12 },
      paint: { 'text-color': p.classD, 'text-halo-color': p.labelHalo, 'text-halo-width': 1.8 },
    },
    {
      id: 'as-labels-bc', type: 'symbol', source: 'airspace', 'source-layer': 'airspace_labels', minzoom: 7,
      layout: {
        // ceiling over floor, sectional style: "100" / "SFC"
        'text-field': ['concat', ['slice', ['get', 'label'], 0, ['index-of', '/', ['get', 'label']]], '\n',
          ['slice', ['get', 'label'], ['+', ['index-of', '/', ['get', 'label']], 1]]],
        'text-font': FONT_BOLD, 'text-size': ['interpolate', ['linear'], ['zoom'], 7, 11, 11, 15], 'text-line-height': 1.05,
      },
      paint: {
        'text-color': ['match', ['get', 'kind'], 'B', p.classB, p.classC],
        'text-halo-color': p.labelHalo, 'text-halo-width': 2,
      },
    },

    // ---- obstacles -----------------------------------------------------------
    {
      id: 'obst', type: 'symbol', source: 'obstacles', 'source-layer': 'obstacles', minzoom: 9,
      layout: { 'icon-image': ['case', ['==', ['get', 'lit'], 1], 'obst-lit', 'obst'], 'icon-allow-overlap': true, 'icon-size': ['interpolate', ['linear'], ['get', 'agl'], 200, 0.75, 1000, 1.1] },
    },
    {
      id: 'obst-label', type: 'symbol', source: 'obstacles', 'source-layer': 'obstacles', minzoom: 11,
      layout: {
        'text-field': ['concat', ['to-string', ['get', 'msl']], '\n(', ['to-string', ['get', 'agl']], ')'],
        'text-font': FONT, 'text-size': 10, 'text-offset': [0, 1.4], 'text-anchor': 'top', 'text-line-height': 1.05,
      },
      paint: { 'text-color': p.obstacle, 'text-halo-color': p.labelHalo, 'text-halo-width': 1.3 },
    },

    // ---- fixes and navaids ---------------------------------------------------
    {
      id: 'fix-ifr', type: 'symbol', source: 'aero', 'source-layer': 'fixes', minzoom: 9, layout: { visibility: 'none', 'icon-image': 'fix-ifr' },
      filter: ['==', ['get', 'vfr'], 0],
    },
    {
      id: 'fix-ifr-label', type: 'symbol', source: 'aero', 'source-layer': 'fixes', minzoom: 10,
      filter: ['==', ['get', 'vfr'], 0],
      layout: { visibility: 'none', 'text-field': ['get', 'id'], 'text-font': FONT, 'text-size': 10, 'text-offset': [0, 1], 'text-anchor': 'top' },
      paint: { 'text-color': p.navaid, 'text-halo-color': p.labelHalo, 'text-halo-width': 1.3 },
    },
    {
      id: 'fix-vfr', type: 'symbol', source: 'aero', 'source-layer': 'fixes', minzoom: 9,
      filter: ['==', ['get', 'vfr'], 1], layout: { 'icon-image': 'fix-vfr' },
    },
    {
      id: 'fix-vfr-label', type: 'symbol', source: 'aero', 'source-layer': 'fixes', minzoom: 10,
      filter: ['==', ['get', 'vfr'], 1],
      layout: { 'text-field': ['get', 'id'], 'text-font': FONT_BOLD, 'text-size': 10, 'text-offset': [0, 1.1], 'text-anchor': 'top' },
      paint: { 'text-color': p.nontowered, 'text-halo-color': p.labelHalo, 'text-halo-width': 1.4 },
    },
    {
      id: 'nav-rose', type: 'symbol', source: 'aero', 'source-layer': 'navaids', minzoom: 9,
      filter: ['in', ['get', 'type'], ['literal', ['VOR', 'VORTAC', 'VOR/DME']]],
      layout: {
        'icon-image': 'rose', 'icon-rotate': ['-', 0, ['get', 'var']], 'icon-rotation-alignment': 'map',
        'icon-allow-overlap': true, 'icon-ignore-placement': true,
        'icon-size': ['interpolate', ['exponential', 2], ['zoom'], 9, 0.55, 11, 1.4, 13, 3],
      },
    },
    {
      id: 'nav-symbol', type: 'symbol', source: 'aero', 'source-layer': 'navaids',
      layout: { 'icon-image': ['concat', 'nav-', ['get', 'type']], 'icon-allow-overlap': true, 'icon-size': ['interpolate', ['linear'], ['zoom'], 6, 0.65, 10, 1] },
    },
    {
      id: 'nav-label', type: 'symbol', source: 'aero', 'source-layer': 'navaids', minzoom: 7,
      layout: {
        'text-field': ['format', ['upcase', ['get', 'name']], { 'font-scale': 0.9 }, '\n', {}, ['get', 'freq'], { 'text-font': ['literal', FONT_BOLD] }, ' ', {}, ['get', 'id'], { 'text-font': ['literal', FONT_BOLD] }],
        'text-font': FONT, 'text-size': 10.5, 'text-anchor': 'top', 'text-offset': [0, 1.3], 'text-max-width': 14,
      },
      paint: { 'text-color': ['match', ['get', 'type'], ['NDB', 'NDB/DME'], p.ndb, p.navaid], 'text-halo-color': p.labelHalo, 'text-halo-width': 1.6 },
    },

    // ---- airways labels --------------------------------------------------------
    {
      id: 'awy-low-label', type: 'symbol', source: 'aero', 'source-layer': 'airways', minzoom: 8,
      filter: ['==', ['get', 'level'], 'low'],
      layout: { visibility: 'none', 'symbol-placement': 'line-center', 'text-field': ['get', 'id'], 'text-font': FONT_BOLD, 'text-size': 10 },
      paint: { 'text-color': p.airway, 'text-halo-color': p.labelHalo, 'text-halo-width': 1.5 },
    },
    {
      id: 'awy-high-label', type: 'symbol', source: 'aero', 'source-layer': 'airways', minzoom: 7,
      filter: ['==', ['get', 'level'], 'high'],
      layout: { visibility: 'none', 'symbol-placement': 'line-center', 'text-field': ['get', 'id'], 'text-font': FONT_BOLD, 'text-size': 10 },
      paint: { 'text-color': p.military, 'text-halo-color': p.labelHalo, 'text-halo-width': 1.5 },
    },

    // ---- places ------------------------------------------------------------------
    {
      id: 'water-label', type: 'symbol', source: 'base', 'source-layer': 'water_name', minzoom: 6,
      layout: { 'text-field': ['get', 'name:en'], 'text-font': FONT_ITALIC, 'text-size': 11 },
      paint: { 'text-color': p.waterLine, 'text-halo-color': p.labelHalo, 'text-halo-width': 1 },
    },
    {
      id: 'place-town', type: 'symbol', source: 'base', 'source-layer': 'place', minzoom: 9,
      filter: ['in', ['get', 'class'], ['literal', ['town', 'village']]],
      layout: { 'text-field': ['coalesce', ['get', 'name:en'], ['get', 'name']], 'text-font': FONT, 'text-size': 11 },
      paint: { 'text-color': p.label, 'text-opacity': 0.75, 'text-halo-color': p.labelHalo, 'text-halo-width': 1.2 },
    },
    {
      id: 'place-city', type: 'symbol', source: 'base', 'source-layer': 'place', minzoom: 5,
      filter: ['==', ['get', 'class'], 'city'],
      layout: { 'text-field': ['coalesce', ['get', 'name:en'], ['get', 'name']], 'text-font': FONT, 'text-size': ['interpolate', ['linear'], ['zoom'], 5, 11, 10, 15], 'text-transform': 'uppercase', 'text-letter-spacing': 0.08 },
      paint: { 'text-color': p.label, 'text-opacity': 0.7, 'text-halo-color': p.labelHalo, 'text-halo-width': 1.4 },
    },

    // ---- airports (on top) ----------------------------------------------------
    {
      id: 'apt-heli', type: 'symbol', source: 'aero', 'source-layer': 'airports', minzoom: 10, layout: { visibility: 'none', 'icon-image': ['case', ['==', ['get', 'twr'], 1], 'heli-twr', 'heli'] },
      filter: ['==', ['get', 'type'], 'H'],
    },
    {
      id: 'apt-private', type: 'symbol', source: 'aero', 'source-layer': 'airports', minzoom: 9,
      filter: ['all', ['!=', ['get', 'type'], 'H'], ['==', ['get', 'pvt'], 1], ['==', ['get', 'mil'], 0]],
      layout: {
        'icon-image': ['case', ['==', ['get', 'hard'], 1], 'apt-pvt-h', 'apt-pvt-s'],
        'icon-rotate': ['case', ['>=', ['get', 'hdg'], 0], ['get', 'hdg'], 0], 'icon-rotation-alignment': 'map',
        'icon-size': 0.8, 'symbol-sort-key': ['get', 'rank'],
      },
    },
    {
      id: 'apt-private-label', type: 'symbol', source: 'aero', 'source-layer': 'airports', minzoom: 10,
      filter: ['all', ['!=', ['get', 'type'], 'H'], ['==', ['get', 'pvt'], 1], ['==', ['get', 'mil'], 0]],
      layout: { 'text-field': ['get', 'id'], 'text-font': FONT, 'text-size': 10, 'text-offset': [0, 1.2], 'text-anchor': 'top', 'text-optional': true },
      paint: { 'text-color': p.nontowered, 'text-halo-color': p.labelHalo, 'text-halo-width': 1.3 },
    },
    {
      id: 'apt-public', type: 'symbol', source: 'aero', 'source-layer': 'airports',
      filter: ['all', ['!=', ['get', 'type'], 'H'], ['any', ['==', ['get', 'pvt'], 0], ['==', ['get', 'mil'], 1]]],
      layout: {
        'icon-image': ['concat', 'apt-',
          ['case', ['==', ['get', 'mil'], 1], 'mil', ['==', ['get', 'twr'], 1], 'twr', 'ntw'], '-',
          ['case', ['==', ['get', 'hard'], 1], 'h', 's'], '-', ['case', ['==', ['get', 'fuel'], 1], 'f', 'n']],
        'icon-rotate': ['case', ['>=', ['get', 'hdg'], 0], ['get', 'hdg'], 0], 'icon-rotation-alignment': 'map',
        'icon-size': ['interpolate', ['linear'], ['zoom'], 4, 0.65, 9, 0.9, 12, 1.05],
        'icon-allow-overlap': ['step', ['zoom'], false, 8, true],
        'symbol-sort-key': ['get', 'rank'],
      },
    },
    {
      id: 'apt-public-label', type: 'symbol', source: 'aero', 'source-layer': 'airports', minzoom: 6,
      filter: ['all', ['!=', ['get', 'type'], 'H'], ['any', ['==', ['get', 'pvt'], 0], ['==', ['get', 'mil'], 1]]],
      layout: {
        'text-field': ['step', ['zoom'],
          ['coalesce', ['get', 'icao'], ['get', 'id']],
          9, ['format',
            ['upcase', ['get', 'name']], { 'font-scale': 0.82 }, '\n', {},
            ['concat', ['case', ['!=', ['get', 'icao'], ''], ['get', 'icao'], ['get', 'id']]], { 'text-font': ['literal', FONT_BOLD] },
            ['case', ['!=', ['get', 'ctaf'], ''], ['concat', '  ', ['get', 'ctaf']], ''], {},
            '\n', {},
            ['concat', ['to-string', ['get', 'elev']], ' ', ['case', ['==', ['get', 'lit'], 1], 'L ', ''], ['to-string', ['round', ['/', ['get', 'rwy'], 100]]]], { 'font-scale': 0.85 }]],
        'text-font': FONT, 'text-size': ['interpolate', ['linear'], ['zoom'], 6, 10, 10, 12],
        'text-anchor': 'top', 'text-offset': [0, 1.15], 'text-max-width': 12, 'text-optional': true,
        'symbol-sort-key': ['get', 'rank'],
      },
      paint: {
        'text-color': ['case', ['==', ['get', 'mil'], 1], p.military, ['==', ['get', 'twr'], 1], p.towered, p.nontowered],
        'text-halo-color': p.labelHalo, 'text-halo-width': 1.6,
      },
    },
  ]

  return {
    version: 8,
    glyphs: 'https://tiles.openfreemap.org/fonts/{fontstack}/{range}.pbf',
    sources: {
      base: { type: 'vector', url: 'https://tiles.openfreemap.org/planet', attribution: '© OpenStreetMap · OpenFreeMap' },
      dem: {
        type: 'raster-dem', encoding: 'terrarium', tileSize: 256, maxzoom: 12,
        tiles: ['https://s3.amazonaws.com/elevation-tiles-prod/terrarium/{z}/{x}/{y}.png'],
        attribution: 'Terrain: Mapzen/AWS',
      },
      aero: { type: 'vector', url: `pmtiles://${TILE_BASE}/aero.pmtiles`, attribution: 'FAA NASR' },
      airspace: { type: 'vector', url: `pmtiles://${TILE_BASE}/airspace.pmtiles` },
      obstacles: { type: 'vector', url: `pmtiles://${TILE_BASE}/obstacles.pmtiles` },
    },
    layers,
  }
}
