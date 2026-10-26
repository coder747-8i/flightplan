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
  import Timeline from './lib/ui/Timeline.svelte'
  import RulerLayer from './lib/chart/RulerLayer.svelte'
  import { route } from './lib/stores/route.svelte'
  import { history } from './lib/stores/history.svelte'
  import { toast } from './lib/stores/app.svelte'

  /** Ctrl+Z / Ctrl+Shift+Z (or Ctrl+Y) undo and redo plan changes; L locks the plan for viewing.
   *  Text fields keep their own undo while typing. */
  function keys(e: KeyboardEvent) {
    const t = document.activeElement?.tagName
    if (t === 'INPUT' || t === 'TEXTAREA' || t === 'SELECT' || (document.activeElement as HTMLElement | null)?.isContentEditable) return
    const cmd = e.ctrlKey || e.metaKey
    const z = e.key === 'z' || e.key === 'Z'
    if (cmd && z && !e.shiftKey) {
      e.preventDefault()
      if (route.locked) return toast('View only: press L to edit')
      toast(history.undo() ? 'Undone' : 'Nothing to undo')
    } else if (cmd && ((z && e.shiftKey) || e.key === 'y' || e.key === 'Y')) {
      e.preventDefault()
      if (route.locked) return toast('View only: press L to edit')
      toast(history.redo() ? 'Redone' : 'Nothing to redo')
    } else if ((e.key === 'l' || e.key === 'L') && !cmd && !e.altKey) {
      route.locked = !route.locked
      toast(route.locked ? 'View only: the plan is locked (L to edit)' : 'Editing on')
    }
  }

  const windyKey = $derived(config.windyMapKey)
  let windyOpen = $state(false)
  let windyLoaded = $state(false)

  $effect(() => {
    document.documentElement.dataset.theme = app.theme
  })
</script>

<svelte:window onkeydown={keys} />

<main style:--panel-offset={app.panel !== 'none' ? `${(['route', 'aircraft', 'settings'].includes(app.panel) ? 380 : 340) + 100}px` : '88px'}>
  <MapView />
  <WeatherLayer />
  <RouteLayer />
  <RulerLayer />
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
  <Timeline />
  <InfoCard />
  {#if app.panel === 'briefing'}<Briefing />{/if}
  {#if config.loaded && !onboarding.open && (!config.data || config.dataExpired)}
    <button class="data-banner glass" onclick={() => (app.panel = 'settings')}>
      <TriangleAlert size={14} />
      {config.data ? `Chart data expired ${config.data.expires}: update it in Settings` : 'No chart data installed: open Settings to install it'}
    </button>
  {/if}
  {#if app.toast}<div class="toast glass" role="status">{app.toast.text}</div>{/if}
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
  .toast {
    position: absolute;
    top: 56px;
    left: 50%;
    transform: translateX(-50%);
    z-index: 40;
    padding: 8px 16px;
    border-radius: 999px;
    font-weight: 600;
    font-size: 12.5px;
    pointer-events: none;
  }
  @media (max-width: 640px) {
    .coords {
      display: none;
    }
  }
</style>
