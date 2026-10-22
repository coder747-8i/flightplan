<script lang="ts">
  import MapView from './lib/chart/MapView.svelte'
  import Rail from './lib/ui/Rail.svelte'
  import LayersPanel from './lib/ui/LayersPanel.svelte'
  import AltitudeFilter from './lib/ui/AltitudeFilter.svelte'
  import InfoCard from './lib/ui/InfoCard.svelte'
  import SearchBox from './lib/ui/SearchBox.svelte'
  import { app } from './lib/stores/app.svelte'

  $effect(() => {
    document.documentElement.dataset.theme = app.theme
  })
</script>

<main>
  <MapView />
  <Rail />
  <SearchBox />
  <AltitudeFilter />
  {#if app.panel === 'layers'}<LayersPanel />{/if}
  <InfoCard />
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
