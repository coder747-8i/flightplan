// The chart feature under the pointer as a route point: airport > navaid > VFR waypoint > fix,
// else a lat/long user waypoint. Used by route editing and the ruler.
import type { Map as MLMap } from 'maplibre-gl'
import type { Place } from '../api'
import { coordId } from '../io/flightplan'

export function snapPlace(map: MLMap, e: { point: { x: number; y: number }; lngLat: { lng: number; lat: number } }): Place {
  const r = 14
  const hits = map.queryRenderedFeatures([[e.point.x - r, e.point.y - r], [e.point.x + r, e.point.y + r]], {
    layers: ['apt-public', 'apt-private', 'apt-heli', 'nav-symbol', 'fix-vfr', 'fix-ifr'].filter((l) => map.getLayer(l)),
  })
  const order = ['airports', 'navaids', 'fixes']
  hits.sort((a, b) => order.indexOf(a.sourceLayer!) - order.indexOf(b.sourceLayer!))
  const h = hits[0]
  if (h && h.geometry.type === 'Point') {
    const pr = h.properties as any
    const [lon, lat] = h.geometry.coordinates
    if (h.sourceLayer === 'airports') return { kind: 'airport', id: pr.icao || pr.id, name: pr.name, detail: '', lat, lon }
    if (h.sourceLayer === 'navaids') return { kind: 'navaid', id: pr.id, name: pr.name, detail: `${pr.type} ${pr.freq}`, lat, lon }
    return { kind: 'fix', id: pr.id, name: pr.id, detail: pr.vfr ? 'VFR waypoint' : 'Fix', lat, lon }
  }
  const { lng, lat } = e.lngLat
  const id = coordId(lat, lng)
  return { kind: 'coord', id, name: 'User waypoint', detail: `${lat.toFixed(4)}, ${lng.toFixed(4)}`, lat, lon: lng }
}
