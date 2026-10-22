<script lang="ts">
  import MapView from './lib/chart/MapView.svelte'
  import Rail from './lib/ui/Rail.svelte'
  import LayersPanel from './lib/ui/LayersPanel.svelte'
  import AltitudeFilter from './lib/ui/AltitudeFilter.svelte'
  import InfoCard from './lib/ui/InfoCard.svelte'
  import SearchBox from './lib/ui/SearchBox.svelte'
  import RoutePanel from './lib/ui/RoutePanel.svelte'
  import ProfileDrawer from './lib/ui/ProfileDrawer.svelte'
  import RouteLayer from './lib/chart/RouteLayer.svelte'
  import WeatherLayer from './lib/wx/WeatherLayer.svelte'
  import WeatherPanel from './lib/wx/WeatherPanel.svelte'
  import WindyView from './lib/wx/WindyView.svelte'
  import AircraftPanel from './lib/ui/AircraftPanel.svelte'
  import Briefing from './lib/ui/Briefing.svelte'
  import { app } from './lib/stores/app.svelte'
  import { api } from './lib/api'

  let windyKey = $state<string | null>(null)
  let windyOpen = $state(false)
  let windyLoaded = $state(false)
  api.config().then((c) => (windyKey = c.windyMapKey)).catch(() => {})

  $effect(() => {
    document.documentElement.dataset.theme = app.theme
  })
</script>

<main style:--panel-offset={app.panel !== 'none' ? `${(app.panel === 'route' || app.panel === 'aircraft' ? 380 : 340) + 100}px` : '88px'}>
  <MapView />
  <WeatherLayer />
  <RouteLayer />
  {#if windyKey && windyLoaded}<WindyView bind:open={windyOpen} apiKey={windyKey} />{/if}
  <Rail />
  <SearchBox />
  <AltitudeFilter />
  {#if app.panel === 'layers'}<LayersPanel />{/if}
  {#if app.panel === 'route'}<RoutePanel />{/if}
  {#if app.panel === 'weather'}<WeatherPanel windyAvailable={!!windyKey} onWindy={() => { windyLoaded = true; windyOpen = true; app.panel = 'none' }} />{/if}
  {#if app.panel === 'aircraft'}<AircraftPanel />{/if}
  <ProfileDrawer />
  <InfoCard />
  {#if app.panel === 'briefing'}<Briefing />{/if}
  {#if app.cursor}
    <div class="coords glass num">
      {Math.abs(app.cursor.lat).toFixed(4)}°{app.cursor.lat >= 0 ? 'N' : 'S'}
      {Math.abs(app.cursor.lon).toFixed(4)}°{app.cursor.lon >= 0 ? 'E' : 'W'}
    </div>
  {/if}
</main>

<style>
  main {
    position: absolute;
    inset: 0;
  }
  .coords {
    position: absolute;
    left: 88px;
    bottom: 12px;
    padding: 4px 10px;
    font-size: 11px;
    border-radius: 999px;
    color: var(--text-2);
    z-index: 5;
    pointer-events: none;
  }
  @media (max-width: 640px) {
    .coords {
      display: none;
    }
  }
</style>
