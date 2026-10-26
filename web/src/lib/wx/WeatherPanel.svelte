<script lang="ts">
  import { Wind, RefreshCw } from '@lucide/svelte'
  import PanelShell from '../ui/PanelShell.svelte'
  import Toggle from '../ui/Toggle.svelte'
  import { api } from '../api'
  import { route } from '../stores/route.svelte'
  import { app } from '../stores/app.svelte'
  import { wx, WIND_LEVELS, speedColor } from './WeatherLayer.svelte'
  import { fltCatColor, fmtAge } from './format'
  import { DBZ, frameLabel } from './radar.svelte'
  import { plan } from '../stores/plan.svelte'
  import { categoryOf, conditionsAt, summary, SOURCE_LABEL, type StationWx } from './forecast'
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

  // the flight at the planned time: departure at ETD, destination at ETA
  const dep = $derived(plan.departureTime)
  const eta = $derived(plan.log ? new Date(dep.getTime() + plan.log.totalTime * 60000) : dep)
  const atTime = (s: StationWx | null, when: Date) => {
    if (!s?.station) return null
    const c = conditionsAt(s, when)
    return { s, c, cat: c ? categoryOf(c) : null }
  }
  const flight = $derived(
    route.waypoints.length >= 2
      ? [
          { role: 'Departure', when: dep, r: atTime(plan.dep, dep) },
          { role: 'Arrival', when: eta, r: atTime(plan.dest, eta) },
        ]
      : [],
  )
  const wkhm = (d: Date) => d.toLocaleString([], { weekday: 'short', hour: '2-digit', minute: '2-digit', hour12: false })
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
  <Toggle
    bind:checked={wx.show.radar}
    label="Radar"
    hint={wx.frame ? frameLabel(wx.frame) : 'Live NEXRAD now; on the timeline, HRRR forecast radar to about 48 h, then the WPC forecast chart (days 1-3) and precipitation forecast (days 4-5)'}
    swatch={DBZ[2][1]}
  />
  {#if wx.show.radar}
    <label class="slider"><span class="faint">Opacity</span><input type="range" min="0.2" max="1" step="0.05" bind:value={wx.radarOpacity} /></label>
    {#if wx.frame?.kind === 'observed' || wx.frame?.kind === 'hrrr'}
      <div class="dbz">{#each DBZ as [, c]}<i style:background={c}></i>{/each}<span class="faint">light → heavy</span></div>
    {/if}
  {/if}
  <Toggle bind:checked={wx.show.spc} label="Convective outlook (SPC)" hint="The Storm Prediction Center's thunderstorm outlook for the timeline's day, days 1-8" swatch="#55bb55" />
  <Toggle bind:checked={wx.show.winds} label="Winds aloft" hint="NWS winds-aloft forecast (FB) at its stations, for the timeline's time (about 24 hours ahead)" swatch={speedColor(30)} />
  {#if wx.show.winds}
    <label class="slider">
      <span class="faint">Level</span>
      <select class="field" bind:value={wx.windLevel}>{#each WIND_LEVELS as [, label], i}<option value={i}>{label}</option>{/each}</select>
    </label>
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

  {#if flight.length}
    <div class="section-title">Your flight</div>
    {#if plan.forecastLoading && !plan.dep}<p class="faint small">Fetching forecasts…</p>{/if}
    {#each flight as f}
      {#if f.r}
        <div class="at">
          <div class="head"><b>{f.role} {f.r.s.airport}</b><span class="faint">{wkhm(f.when)}</span></div>
          {#if f.r.c}
            <div class="line">
              {#if f.r.cat}<span class="cat" style:background={fltCatColor(f.r.cat)}>{f.r.cat}</span>{/if}
              <span>{summary(f.r.c)}</span>
            </div>
            <div class="faint small">{SOURCE_LABEL[f.r.c.source]}{f.r.c.station !== f.r.s.airport && f.r.s.offsetNm > 1 ? ` from ${f.r.c.station} (${Math.round(f.r.s.offsetNm)} NM)` : ''}</div>
            {#if f.r.c.tempo}<div class="faint small">{f.r.c.tempo[1]}</div>{/if}
          {:else}
            <div class="faint small">No forecast covers this time yet</div>
          {/if}
        </div>
      {/if}
    {/each}
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
  <p class="faint small src">Sources: aviationweather.gov, FAA TFR, Iowa Environmental Mesonet (NEXRAD, HRRR, NBM), NOAA WPC and SPC, NWS winds aloft (FB). Check an official briefing (1800wxbrief) before flight.</p>
</PanelShell>

<style>
  .dbz {
    display: flex;
    align-items: center;
    gap: 3px;
    margin: 0 0 6px 2px;
    font-size: 11px;
  }
  .dbz i {
    width: 22px;
    height: 6px;
    border-radius: 2px;
  }
  .dbz span {
    margin-left: 6px;
  }
  .at {
    padding: 8px 10px;
    border-radius: 9px;
    background: var(--surface-2);
    margin-bottom: 6px;
  }
  .at .head {
    display: flex;
    gap: 8px;
    align-items: baseline;
  }
  .at .line {
    display: flex;
    gap: 6px;
    align-items: baseline;
    font-size: 12px;
    margin-top: 3px;
  }
  .at .cat {
    flex: none;
  }
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
