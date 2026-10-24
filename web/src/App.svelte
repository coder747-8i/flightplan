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
  import SettingsPanel from './lib/ui/SettingsPanel.svelte'
  import { config } from './lib/stores/config.svelte'
  import HelpPanel from './lib/ui/HelpPanel.svelte'
  import Onboarding from './lib/setup/Onboarding.svelte'
  import Tour from './lib/setup/Tour.svelte'
  import { onboarding } from './lib/stores/onboarding.svelte'
  import { TriangleAlert } from '@lucide/svelte'

  const windyKey = $derived(config.windyMapKey)
  let windyOpen = $state(false)
  let windyLoaded = $state(false)

  $effect(() => {
    document.documentElement.dataset.theme = app.theme
  })
</script>

<main style:--panel-offset={app.panel !== 'none' ? `${(['route', 'aircraft', 'settings'].includes(app.panel) ? 380 : 340) + 100}px` : '88px'}>
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
  {#if app.panel === 'settings'}<SettingsPanel />{/if}
  {#if app.panel === 'help'}<HelpPanel />{/if}
  <ProfileDrawer />
  <InfoCard />
  {#if app.panel === 'briefing'}<Briefing />{/if}
  {#if config.loaded && !onboarding.open && (!config.data || config.dataExpired)}
    <button class="data-banner glass" onclick={() => (app.panel = 'settings')}>
      <TriangleAlert size={14} />
      {config.data ? `Chart data expired ${config.data.expires}: update it in Settings` : 'No chart data installed: open Settings to install it'}
    </button>
  {/if}
  {#if app.cursor}
    <div class="coords glass num">
      {Math.abs(app.cursor.lat).toFixed(4)}°{app.cursor.lat >= 0 ? 'N' : 'S'}
      {Math.abs(app.cursor.lon).toFixed(4)}°{app.cursor.lon >= 0 ? 'E' : 'W'}
    </div>
  {/if}
</main>
{#if onboarding.open}<Onboarding />{/if}
{#if onboarding.touring && !onboarding.open}<Tour />{/if}

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
  .data-banner {
    position: absolute;
    top: 12px;
    left: 50%;
    transform: translateX(-50%);
    z-index: 25;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 14px;
    border-radius: 999px;
    color: var(--warn);
    font-weight: 600;
    font-size: 12px;
    cursor: pointer;
  }
  @media (max-width: 640px) {
    .coords {
      display: none;
    }
  }
</style>
