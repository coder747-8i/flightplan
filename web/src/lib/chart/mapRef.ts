// Shared handle to the live map so panels (route, weather) can add their own layers.
import type { Map as MLMap, MapMouseEvent } from 'maplibre-gl'

export const mapRef: {
  map?: MLMap
  /** a panel can claim map clicks (e.g. "add waypoint" mode); return true to consume */
  onMapClick?: (e: MapMouseEvent) => boolean
} = {}
