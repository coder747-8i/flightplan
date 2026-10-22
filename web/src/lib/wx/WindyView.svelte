<script lang="ts">
  // Windy Map Forecast API (needs WINDY_MAP_KEY in .env, domain-locked key from api.windy.com).
  // Opens at the chart's current view with the route drawn on top. Windy allows one instance
  // per page, so it's created once and kept alive while hidden.
  import { X } from '@lucide/svelte'
  import { onMount } from 'svelte'
  import { mapRef } from '../chart/mapRef'
  import { route } from '../stores/route.svelte'

  let { open = $bindable(false), apiKey }: { open: boolean; apiKey: string } = $props()
  let status = $state('Loading Windy…')
  let windy = $state.raw<any>(null)
  // overlays/levels this Windy key actually allows (read from Windy once it loads)
  let allowedOverlays = $state<string[] | null>(null)
  let allowedLevels = $state<string[] | null>(null)
  let routeLine: any = null
  let overlay = $state('wind')
  let level = $state('850h')

  const OVERLAYS = [
    ['wind', 'Wind'], ['gust', 'Gusts'], ['rain', 'Rain, thunder'], ['clouds', 'Clouds'], ['lclouds', 'Low clouds'],
    ['mclouds', 'Medium clouds'], ['hclouds', 'High clouds'], ['cbase', 'Cloud base'], ['visibility', 'Visibility'],
    ['temp', 'Temperature'], ['dewpoint', 'Dew point'], ['rh', 'Humidity'], ['pressure', 'Pressure'], ['cape', 'CAPE (storms)'],
    ['rainAccu', 'Rain accumulation'], ['snowAccu', 'Snow accumulation'], ['snowcover', 'Snow depth'], ['thunder', 'Thunderstorms'],
    ['radar', 'Radar'], ['satellite', 'Satellite'], ['icing', 'Icing'], ['turbulence', 'Turbulence'], ['ccl', 'Thermals'],
  ] as const
  const LEVELS = [['surface', 'Surface'], ['950h', '2,000 ft'], ['900h', '3,000 ft'], ['850h', '5,000 ft'], ['800h', '6,400 ft'], ['700h', '10,000 ft'], ['600h', '14,000 ft'], ['500h', 'FL180'], ['300h', 'FL300']] as const

  function script(src: string) {
    return new Promise<void>((res, rej) => {
      if (document.querySelector(`script[src="${src}"]`)) return res()
      const s = document.createElement('script')
      s.src = src
      s.onload = () => res()
      s.onerror = () => rej(new Error(`failed to load ${src}`))
      document.head.appendChild(s)
    })
  }

  function drawRoute() {
    const L = (window as any).L
    if (!windy || !L) return
    routeLine?.remove()
    const pts = route.waypoints.map((w) => [w.lat, w.lon])
    if (pts.length < 2) return
    routeLine = L.layerGroup([
      L.polyline(pts, { color: '#ffffff', weight: 7, opacity: 0.8 }),
      L.polyline(pts, { color: '#d4148c', weight: 4 }),
      ...route.waypoints.map((w) => L.circleMarker([w.lat, w.lon], { radius: 5, color: '#fff', weight: 2, fillColor: '#d4148c', fillOpacity: 1 }).bindTooltip(w.id, { permanent: true, direction: 'right', className: 'windy-wp' })),
    ]).addTo(windy.map)
  }

  onMount(async () => {
    try {
      if (!(window as any).L) await script('https://unpkg.com/leaflet@1.4.0/dist/leaflet.js')
      await script('https://api.windy.com/assets/map-forecast/libBoot.js')
      const c = mapRef.map?.getCenter()
      ;(window as any).windyInit({ key: apiKey, lat: c?.lat ?? 33, lon: c?.lng ?? -112, zoom: Math.round(mapRef.map?.getZoom() ?? 7), overlay: 'wind', level: '850h' }, (api: any) => {
        windy = api
        status = ''
        drawRoute()
      })
    } catch (e) {
      status = `Windy failed to load: ${e}`
    }
  })

  $effect(() => {
    route.waypoints
    drawRoute()
  })
  // read the selections before the guard so the effect re-runs whenever they change
  $effect(() => {
    const o = overlay
    const l = level
    if (!windy) return
    if (windy.store.get('overlay') !== o) windy.store.set('overlay', o)
    if (windy.store.get('level') !== l) windy.store.set('level', l)
  })

  const NAMES: Record<string, string> = Object.fromEntries(OVERLAYS)
  const overlayOptions = $derived(
    allowedOverlays ? allowedOverlays.map((v) => [v, NAMES[v] ?? v.replace(/([A-Z])/g, ' $1').replace(/^./, (c) => c.toUpperCase())] as const) : OVERLAYS.map((o) => [o[0], o[1]] as const),
  )
  const LEVEL_NAMES: Record<string, string> = Object.fromEntries(LEVELS)
  const levelOptions = $derived(allowedLevels ? allowedLevels.map((v) => [v, LEVEL_NAMES[v] ?? v] as const) : LEVELS.map((l) => [l[0], l[1]] as const))
  $effect(() => {
    if (open && windy) {
      const c = mapRef.map?.getCenter()
      if (c) windy.map.setView([c.lat, c.lng], Math.round(mapRef.map!.getZoom()))
      setTimeout(() => windy.map.invalidateSize(), 50)
    }
  })
</script>

<svelte:head>
  <link rel="stylesheet" href="https://unpkg.com/leaflet@1.4.0/dist/leaflet.css" />
</svelte:head>

<div class="wrap" class:hidden={!open}>
  <div id="windy"></div>
  <div class="bar glass">
    <b>Windy</b>
    <select class="field" bind:value={overlay}>{#each overlayOptions as [v, l]}<option value={v}>{l}</option>{/each}</select>
    <select class="field" bind:value={level}>{#each levelOptions as [v, l]}<option value={v}>{l}</option>{/each}</select>
    <button class="btn" onclick={() => (open = false)}><X size={15} /> Back to chart</button>
  </div>
  {#if status}<div class="status glass">{status}</div>{/if}
</div>

<style>
  .wrap {
    position: absolute;
    inset: 0;
    z-index: 11;
  }
  .wrap.hidden {
    visibility: hidden;
    pointer-events: none;
  }
  #windy {
    position: absolute;
    inset: 0;
  }
  .bar {
    position: absolute;
    top: 60px;
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 8px 6px 14px;
    z-index: 1000;
    border-radius: 999px;
  }
  .bar .field {
    height: 30px;
    width: auto;
  }
  .status {
    position: absolute;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    padding: 12px 16px;
    z-index: 1000;
  }
  :global(.windy-wp) {
    font-weight: 700;
    font-family: var(--font);
  }
</style>
