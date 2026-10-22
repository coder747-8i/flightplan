<script lang="ts">
  // Windy Map Forecast API (key set in Settings; domain-locked on api.windy.com).
  // Opens at the chart's current view with the route drawn on top. Windy allows one instance
  // per page, so it's created once and kept alive while hidden.
  //
  // Overlay changes go straight to windy.store on tap. Many overlays (clouds, rain, gusts...)
  // only exist at the surface, so after each change the level is moved to one Windy says is
  // available for that overlay; forcing a level it doesn't have makes Windy refuse the overlay.
  import { X, Layers3 } from '@lucide/svelte'
  import { onMount } from 'svelte'
  import { mapRef } from '../chart/mapRef'
  import { route } from '../stores/route.svelte'

  let { open = $bindable(false), apiKey }: { open: boolean; apiKey: string } = $props()
  let status = $state('Loading Windy…')
  let note = $state('')
  let windy = $state.raw<any>(null)
  let overlay = $state('wind')
  let level = $state('surface')
  let allowedOverlays = $state<string[]>([])
  let availLevels = $state<string[]>(['surface'])
  let routeLine: any = null
  let pending: string | null = null
  let pendingTimer: ReturnType<typeof setTimeout> | undefined
  // Windy's free Testing keys only serve wind, temperature and pressure
  const testingKey = $derived(allowedOverlays.length > 0 && allowedOverlays.length <= 3)

  const NAMES: Record<string, string> = {
    wind: 'Wind', gust: 'Gusts', rain: 'Rain & thunder', thunder: 'Thunderstorms', clouds: 'Clouds', lclouds: 'Low clouds',
    mclouds: 'Mid clouds', hclouds: 'High clouds', cbase: 'Cloud base', visibility: 'Visibility', temp: 'Temperature',
    dewpoint: 'Dew point', rh: 'Humidity', pressure: 'Pressure', cape: 'CAPE (storms)', rainAccu: 'Rain total', snowAccu: 'Snow total',
    snowcover: 'Snow depth', radar: 'Radar', satellite: 'Satellite', icing: 'Icing', turbulence: 'Turbulence', ccl: 'Thermals',
    gustAccu: 'Max gusts', fog: 'Fog', cloudtop: 'Cloud tops', freezing: 'Freezing level', waves: 'Waves', swell1: 'Swell',
    sst: 'Sea temp', so2sm: 'SO₂', dustsm: 'Dust', cosc: 'CO', aqi: 'Air quality', uvindex: 'UV index',
  }
  // what pilots reach for first; anything else Windy allows follows
  const PREFERRED = ['wind', 'gust', 'rain', 'thunder', 'clouds', 'lclouds', 'cbase', 'cloudtop', 'visibility', 'fog', 'cape', 'icing', 'turbulence', 'freezing', 'temp', 'pressure', 'radar', 'satellite']
  const LEVEL_NAMES: Record<string, string> = {
    surface: 'Surface', '100m': '330 ft', '975h': '1,000 ft', '950h': '2,000 ft', '925h': '2,500 ft', '900h': '3,000 ft', '850h': '5,000 ft',
    '800h': '6,400 ft', '700h': '10,000 ft', '600h': '14,000 ft', '500h': 'FL180', '400h': 'FL240', '300h': 'FL300', '250h': 'FL340', '200h': 'FL390', '150h': 'FL450',
  }
  const LEVEL_ORDER = Object.keys(LEVEL_NAMES)

  const overlays = $derived.by(() => {
    const list = allowedOverlays.length ? allowedOverlays : PREFERRED
    return [...PREFERRED.filter((o) => list.includes(o)), ...list.filter((o) => !PREFERRED.includes(o) && NAMES[o])]
  })
  const levels = $derived([...availLevels].sort((a, b) => LEVEL_ORDER.indexOf(a) - LEVEL_ORDER.indexOf(b)))

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

  function asList(v: unknown): string[] {
    return Array.isArray(v) ? v.map(String) : typeof v === 'string' && v ? v.split(',') : []
  }

  /** read back what Windy is actually showing */
  function sync() {
    if (!windy) return
    overlay = windy.store.get('overlay')
    level = windy.store.get('level')
    availLevels = asList(windy.store.get('availLevels')).length ? asList(windy.store.get('availLevels')) : asList(windy.store.getAllowed('availLevels'))
    if (!availLevels.length) availLevels = ['surface']
    if (pending && overlay === pending) {
      pending = null
      note = ''
    }
  }

  function pickOverlay(v: string) {
    if (!windy) return
    note = ''
    const s = windy.store
    s.set('overlay', v)
    if (s.get('overlay') !== v) s.set('overlay', v, { forceChange: true })
    // keep the level only if this overlay has it
    const avail = asList(s.get('availLevels'))
    if (avail.length && !avail.includes(s.get('level'))) s.set('level', avail.includes('surface') ? 'surface' : avail[0])
    sync()
    // Windy applies changes asynchronously; only complain if it still hasn't switched shortly after
    pending = overlay === v ? null : v
    clearTimeout(pendingTimer)
    if (pending) {
      pendingTimer = setTimeout(() => {
        sync()
        if (pending === v && overlay !== v) note = `Windy didn't switch to ${NAMES[v] ?? v}; your Windy key may not include it.`
      }, 1500)
    }
  }

  function pickLevel(l: string) {
    if (!windy) return
    windy.store.set('level', l)
    sync()
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
      ;(window as any).windyInit({ key: apiKey, lat: c?.lat ?? 33, lon: c?.lng ?? -112, zoom: Math.round(mapRef.map?.getZoom() ?? 7), overlay: 'wind', level: 'surface' }, (api: any) => {
        windy = api
        status = ''
        try {
          allowedOverlays = asList(api.store.getAllowed('overlay'))
        } catch {
          allowedOverlays = []
        }
        // follow changes made through Windy's own controls too
        api.store.on('overlay', sync)
        api.store.on('level', sync)
        api.store.on('availLevels', sync)
        sync()
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

  $effect(() => {
    if (open && windy) {
      const c = mapRef.map?.getCenter()
      if (c) windy.map.setView([c.lat, c.lng], Math.round(mapRef.map!.getZoom()))
      setTimeout(() => windy.map.invalidateSize(), 50)
    }
  })

  // keep taps on our controls from reaching Leaflet/Windy underneath
  function stop(e: Event) {
    e.stopPropagation()
  }
</script>

<svelte:head>
  <link rel="stylesheet" href="https://unpkg.com/leaflet@1.4.0/dist/leaflet.css" />
</svelte:head>

<div class="wrap" class:hidden={!open}>
  <div id="windy"></div>
  <div class="bar glass" onpointerdown={stop} ontouchstart={stop} onwheel={stop} role="toolbar" aria-label="Windy layers" tabindex="-1">
    <div class="top">
      <span class="title"><Layers3 size={15} /> Windy</span>
      <button class="btn back" onclick={() => (open = false)}><X size={15} /> Back to chart</button>
    </div>
    <div class="chips" aria-label="Layer">
      {#each overlays as o}
        <button class="chip" class:on={overlay === o} onclick={() => pickOverlay(o)}>{NAMES[o] ?? o}</button>
      {/each}
    </div>
    {#if levels.length > 1}
      <div class="chips levels" aria-label="Altitude">
        {#each levels as l}
          <button class="chip" class:on={level === l} onclick={() => pickLevel(l)}>{LEVEL_NAMES[l] ?? l}</button>
        {/each}
      </div>
    {/if}
    {#if note}<div class="note">{note}</div>{/if}
    {#if testingKey}
      <div class="note">
        This Windy key is a <b>Testing</b> key, which Windy limits to wind, temperature and pressure. Clouds, rain, gusts,
        visibility and the rest need a <b>Professional</b> Map Forecast API key from
        <a href="https://api.windy.com/keys" target="_blank" rel="noopener noreferrer">api.windy.com/keys</a>
        (a windy.com Premium subscription doesn't include the API). Paste the new key in Settings.
      </div>
    {/if}
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
    width: min(720px, calc(100% - 24px));
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 8px 10px;
    /* above Windy's own UI, which lives in the same stacking context */
    z-index: 100000;
    border-radius: 14px;
    touch-action: manipulation;
  }
  .top {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .title {
    display: flex;
    gap: 6px;
    align-items: center;
    font-weight: 650;
  }
  .back {
    height: 32px;
  }
  .chips {
    display: flex;
    gap: 6px;
    overflow-x: auto;
    -webkit-overflow-scrolling: touch;
    scrollbar-width: none;
    padding-bottom: 2px;
  }
  .chips::-webkit-scrollbar {
    display: none;
  }
  .chip {
    flex: none;
    height: 34px;
    padding: 0 12px;
    font-size: 12.5px;
    border: 1px solid var(--line-strong);
    background: var(--surface-solid);
    color: var(--text);
    cursor: pointer;
  }
  .chip.on {
    background: var(--accent);
    border-color: var(--accent);
    color: white;
  }
  .levels .chip {
    height: 30px;
    font-size: 12px;
  }
  .note {
    font-size: 12px;
    color: var(--warn);
    line-height: 1.4;
  }
  .note a {
    color: var(--blue);
  }
  .status {
    position: absolute;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    padding: 12px 16px;
    z-index: 100000;
  }
  :global(.windy-wp) {
    font-weight: 700;
    font-family: var(--font);
  }
</style>
