<script lang="ts">
  import { Wind, RefreshCw } from '@lucide/svelte'
  import PanelShell from '../ui/PanelShell.svelte'
  import Toggle from '../ui/Toggle.svelte'
  import { api } from '../api'
  import { route } from '../stores/route.svelte'
  import { app } from '../stores/app.svelte'
  import { wx } from './WeatherLayer.svelte'
  import { fltCatColor, fmtAge } from './format'
  import WxText from './WxText.svelte'

  let { windyAvailable, onWindy }: { windyAvailable: boolean; onWindy: () => void } = $props()

  let routeWx = $state<{ id: string; metar: any; taf: any }[]>([])
  let loading = $state(false)
  let showAllSig = $state(false)

  async function loadRouteWx() {
    const ids = route.waypoints.filter((w) => w.kind === 'airport').map((w) => (w.id.length === 3 ? `K${w.id}` : w.id))
    if (!ids.length) return (routeWx = [])
    loading = true
    try {
      const [m, t] = await Promise.all([api.metar(ids), api.taf(ids)])
      routeWx = ids.map((id) => ({ id, metar: m.find((x) => x.icaoId === id), taf: t.find((x) => x.icaoId === id) }))
    } finally {
      loading = false
    }
  }
  $effect(() => {
    route.waypoints
    loadRouteWx()
  })

  const hours = $derived([...new Set(wx.gairmets.map((g) => g.forecastHour))].sort((a, b) => a - b))
  const activeSig = $derived(wx.sigmets.filter((s) => s.validTimeTo * 1000 > Date.now()))
</script>

<PanelShell title="Weather" subtitle={wx.updated ? `Advisories updated ${wx.updated.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}` : 'Loading…'}>
  {#if windyAvailable}
    <button class="windy" onclick={onWindy}>
      <Wind size={18} />
      <span><b>Open Windy</b><small>Wind, clouds, rain, icing and more, with your route drawn on top</small></span>
    </button>
  {:else}
    <button class="note link" onclick={() => (app.panel = 'settings')}>Add your Windy Map Forecast key in <b>Settings</b> to turn on the Windy view →</button>
  {/if}

  <div class="section-title">On the chart</div>
  <Toggle bind:checked={wx.show.metars} label="METAR flight category" hint="Dots: VFR, MVFR, IFR, LIFR" />
  <div class="cats">
    {#each ['VFR', 'MVFR', 'IFR', 'LIFR'] as c}<span><i style:background={fltCatColor(c)}></i>{c}</span>{/each}
  </div>
  <Toggle bind:checked={wx.show.radar} label="Radar (NEXRAD)" hint="Composite reflectivity, ~5 min" />
  {#if wx.show.radar}
    <label class="slider"><span class="faint">Opacity</span><input type="range" min="0.2" max="1" step="0.05" bind:value={wx.radarOpacity} /></label>
  {/if}
  <Toggle bind:checked={wx.show.tfr} label="TFRs" hint={`${wx.tfrs.features.length} active shapes`} swatch="#d23a3a" />
  <Toggle bind:checked={wx.show.sigmet} label="AIRMETs & SIGMETs" hint={`${activeSig.length} in effect`} swatch="#e0452b" />
  <Toggle bind:checked={wx.show.gairmet} label="G-AIRMETs" hint="IFR, mountain obscuration, turbulence, icing, freezing level" swatch="#7b4fc9" />
  {#if wx.show.gairmet && hours.length > 1}
    <div class="hours">
      {#each hours as h}<button class="chip" class:on={wx.gairmetHour === h} onclick={() => (wx.gairmetHour = h)}>+{h} h</button>{/each}
    </div>
  {/if}
  <Toggle bind:checked={wx.show.pireps} label="PIREPs" hint="Last 3 hours; blue icing, orange turbulence" swatch="#d48a0b" />

  {#if activeSig.length}
    <div class="section-title">AIRMETs & SIGMETs in effect</div>
    {#each activeSig.slice(0, showAllSig ? 99 : 4) as sg}
      <div class="adv">
        <WxText kind="sigmet" data={sg} compact />
      </div>
    {/each}
    {#if activeSig.length > 4}<button class="more" onclick={() => (showAllSig = !showAllSig)}>{showAllSig ? 'Show fewer' : `Show all ${activeSig.length}`}</button>{/if}
  {/if}

  <div class="section-title row">
    <span>Along your route</span>
    <button class="btn ghost tiny" onclick={loadRouteWx} title="Refresh"><RefreshCw size={13} class={loading ? 'spin' : ''} /></button>
  </div>
  {#if !routeWx.length}
    <p class="muted">Add airports to your route to see their METARs and TAFs here.</p>
  {/if}
  {#each routeWx as r}
    <div class="stn">
      <div class="head">
        <b>{r.id}</b>
        {#if r.metar?.fltCat}<span class="cat" style:background={fltCatColor(r.metar.fltCat)}>{r.metar.fltCat}</span>{/if}
        {#if r.metar}<span class="faint">{fmtAge(r.metar.obsTime)}</span>{/if}
      </div>
      {#if r.metar}<WxText kind="metar" raw={r.metar.rawOb} />{:else}<p class="faint small">No METAR</p>{/if}
      {#if r.taf}<div class="tafw"><WxText kind="taf" raw={r.taf.rawTAF} /></div>{/if}
    </div>
  {/each}
  <p class="faint small src">Sources: aviationweather.gov, FAA TFR, Iowa Environmental Mesonet. Check an official briefing (1800wxbrief) before flight.</p>
</PanelShell>

<style>
  .windy {
    width: 100%;
    display: flex;
    gap: 12px;
    align-items: center;
    text-align: left;
    padding: 12px;
    border-radius: 12px;
    border: 0;
    cursor: pointer;
    color: white;
    background: linear-gradient(120deg, #2c63b8, #7b4fc9 60%, #d4148c);
    margin-top: 4px;
  }
  .windy span {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .windy small {
    opacity: 0.85;
    font-size: 11.5px;
  }
  .note.link {
    width: 100%;
    text-align: left;
    border: 0;
    cursor: pointer;
  }
  .note {
    font-size: 12px;
    color: var(--text-2);
    background: var(--surface-2);
    padding: 10px;
    border-radius: 9px;
  }
  .cats {
    display: flex;
    gap: 12px;
    font-size: 11px;
    padding: 0 0 6px 2px;
    color: var(--text-2);
  }
  .cats span {
    display: flex;
    gap: 5px;
    align-items: center;
  }
  .cats i {
    width: 9px;
    height: 9px;
    border-radius: 50%;
    display: inline-block;
  }
  .slider {
    display: flex;
    gap: 10px;
    align-items: center;
    padding: 0 2px 6px;
    font-size: 11.5px;
  }
  .slider input {
    flex: 1;
    accent-color: var(--accent);
  }
  .hours {
    display: flex;
    gap: 5px;
    padding-bottom: 6px;
  }
  .hours .chip {
    border: 0;
    cursor: pointer;
  }
  .hours .chip.on {
    background: var(--accent);
    color: white;
  }
  .row {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .tiny {
    height: 24px;
    padding: 0 6px;
  }
  .stn {
    padding: 8px 0;
    border-bottom: 1px solid var(--line);
  }
  .head {
    display: flex;
    gap: 8px;
    align-items: center;
    margin-bottom: 4px;
  }
  .cat {
    color: white;
    font-size: 10px;
    font-weight: 700;
    padding: 1px 6px;
    border-radius: 5px;
  }
  .adv {
    padding: 8px 10px;
    border-radius: 9px;
    background: var(--surface-2);
    margin-bottom: 6px;
  }
  .more {
    border: 0;
    background: none;
    color: var(--accent);
    font-weight: 600;
    cursor: pointer;
    padding: 2px 0 6px;
  }
  .tafw {
    margin-top: 6px;
  }
  .small {
    font-size: 11px;
  }
  .src {
    margin-top: 14px;
  }
  :global(.spin) {
    animation: spin 0.9s linear infinite;
  }
  @keyframes spin {
    to { transform: rotate(360deg); }
  }
</style>
