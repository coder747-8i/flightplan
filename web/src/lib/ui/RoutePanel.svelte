<script lang="ts">
  import { ArrowDownUp, Trash2, MousePointerClick, X, GripVertical, AreaChart, Plane, TriangleAlert, Fuel, Clock, Ruler, Share2, FileUp } from '@lucide/svelte'
  import PanelShell from './PanelShell.svelte'
  import { api } from '../api'
  import { app } from '../stores/app.svelte'
  import { route } from '../stores/route.svelte'
  import { plan } from '../stores/plan.svelte'
  import { aircraft } from '../stores/aircraft.svelte'
  import { fmtTime, deg } from '../nav/navlog'
  import { bearing, distanceNm } from '../nav/geo'
  import { variation } from '../nav/navlog'
  import { mapRef } from '../chart/mapRef'
  import HazardList from './HazardList.svelte'
  import RouteExport from './RouteExport.svelte'
  import { parseRouteFile } from '../io/flightplan'
  import { pickFiles } from '../io/files'

  let text = $state(route.text)
  let unresolved = $state<string[]>([])
  let dragFrom: number | null = null
  let exporting = $state(false)
  let importError = $state('')

  /** Load a Garmin .fpl or GPX file. Named points are matched to the database when one with that
   *  id lies within 3 NM of the file's position; anything else stays a lat/long waypoint. */
  async function importFile() {
    importError = ''
    const [file] = await pickFiles('.fpl,.gpx,.xml')
    if (!file) return
    try {
      const pts = parseRouteFile(await file.text())
      const named = pts.map((p) => p.id).filter((id) => /^[A-Z0-9]{2,5}$/i.test(id))
      if (named.length) {
        const res = await api.resolve(named.join(' ').toUpperCase()).catch(() => [])
        const found = new Map(res.filter((r) => r.place).map((r) => [r.token.toUpperCase(), r.place!]))
        for (let i = 0; i < pts.length; i++) {
          const db = found.get(pts[i].id.toUpperCase())
          if (db && distanceNm(db.lat, db.lon, pts[i].lat, pts[i].lon) < 3) pts[i] = db
        }
      }
      route.replace(pts)
      fit()
    } catch (e) {
      importError = `${file.name}: ${(e as Error).message}`
    }
  }

  // keep the text box in sync when the route changes elsewhere (map edits)
  $effect(() => {
    text = route.text
  })

  async function applyText() {
    const res = await api.resolve(text.toUpperCase())
    unresolved = res.filter((r) => !r.place).map((r) => r.token)
    route.replace(res.filter((r) => r.place).map((r) => r.place!))
    fit()
  }

  function fit() {
    const w = route.waypoints
    if (!w.length || !mapRef.map) return
    if (w.length === 1) return mapRef.map.flyTo({ center: [w[0].lon, w[0].lat], zoom: 10 })
    const lons = w.map((p) => p.lon)
    const lats = w.map((p) => p.lat)
    mapRef.map.fitBounds([[Math.min(...lons), Math.min(...lats)], [Math.max(...lons), Math.max(...lats)]], { padding: { top: 80, bottom: app.profileOpen ? 300 : 80, left: 440, right: 80 }, maxZoom: 11 })
  }

  const log = $derived(plan.log)
  const ac = $derived(aircraft.active)

  // hemispheric rule (VFR above 3,000 AGL): east = odd + 500, west = even + 500
  const suggested = $derived.by(() => {
    const w = route.waypoints
    if (w.length < 2) return null
    const a = w[0]
    const b = w[w.length - 1]
    const mc = (bearing(a.lat, a.lon, b.lat, b.lon) - variation(a.lat, a.lon) + 360) % 360
    const east = mc < 180
    const base = Math.max(3500, Math.ceil(((Math.max(...plan.samples.map((s) => s.max), 0) + 1000) - 500) / 1000) * 1000 + 500)
    let alt = base
    const thousands = Math.floor(alt / 1000)
    if (east !== (thousands % 2 === 1)) alt += 1000
    return { alt, east, mc }
  })

  function legInfo(i: number) {
    const w = route.waypoints
    const a = w[i]
    const b = w[i + 1]
    const d = distanceNm(a.lat, a.lon, b.lat, b.lon)
    const mc = bearing(a.lat, a.lon, b.lat, b.lon) - variation(a.lat, a.lon)
    let start = 0
    for (let k = 1; k <= i; k++) start += distanceNm(w[k - 1].lat, w[k - 1].lon, w[k].lat, w[k].lon)
    // navlog rows are split at TOC/TOD; add up the pieces that fall inside this leg
    const ete = (log?.rows ?? []).filter((r) => r.cumDist - r.dist >= start - 0.01 && r.cumDist <= start + d + 0.01).reduce((t, r) => t + r.ete, 0)
    return { d, mc: deg(mc), ete }
  }

  function onDrop(i: number) {
    if (dragFrom == null || dragFrom === i) return
    route.move(dragFrom, i)
    dragFrom = null
  }

  const altPresets = [3500, 4500, 5500, 6500, 7500, 8500, 9500, 10500, 11500, 12500]
</script>

<PanelShell title="Route" subtitle={route.waypoints.length >= 2 && log ? `${Math.round(log.totalDist)} NM · ${fmtTime(log.totalTime)} · ${log.tripFuel.toFixed(1)} gal` : 'Type a route or click the map'} width={380}>
  {#snippet actions()}
    <button class="btn ghost ico" title="Export to FltPlan.com, Garmin .fpl or GPX" onclick={() => (exporting = true)} disabled={!route.waypoints.length} data-tour="route-export"><Share2 size={15} /></button>
    <button class="btn ghost ico" title="Open a Garmin .fpl or GPX file" onclick={importFile}><FileUp size={15} /></button>
    <button class="btn ghost ico" title="Reverse route" onclick={() => route.reverse()} disabled={route.waypoints.length < 2}><ArrowDownUp size={15} /></button>
    <button class="btn ghost ico" title="Clear route" onclick={() => route.clear()} disabled={!route.waypoints.length}><Trash2 size={15} /></button>
  {/snippet}

  <div class="entry" data-tour="route-entry">
    <textarea
      class="field"
      rows="2"
      bind:value={text}
      placeholder="KNYL BZA 3300N11400W KPHX"
      spellcheck="false"
      onkeydown={(e) => {
        if (e.key === 'Enter' && !e.shiftKey) {
          e.preventDefault()
          applyText()
        }
      }}
    ></textarea>
    <div class="row">
      <button class="btn primary" onclick={applyText}>Load route</button>
      <button class="btn" class:active={route.editing} onclick={() => (route.editing = !route.editing)} title="Click the map to add waypoints">
        <MousePointerClick size={15} /> {route.editing ? 'Adding… (click map)' : 'Click map to add'}
      </button>
    </div>
    {#if unresolved.length}<p class="warn"><TriangleAlert size={13} /> Not found: {unresolved.join(', ')}</p>{/if}
    {#if importError}<p class="warn"><TriangleAlert size={13} /> {importError}</p>{/if}
    <p class="faint tip">Tip: drag the magenta line to add a waypoint (rubber-band), or drag a waypoint to move it.</p>
  </div>

  <div class="section-title">Flight</div>
  <div class="grid">
    <label>
      <span class="faint">Aircraft</span>
      <select class="field" bind:value={aircraft.activeId}>
        {#each aircraft.list as a}<option value={a.id}>{a.tail || a.type} · {a.name}</option>{/each}
      </select>
    </label>
    <label>
      <span class="faint">Cruise altitude</span>
      <input class="field num" type="number" step="500" min="500" max="45000" bind:value={route.cruise} list="alts" />
      <datalist id="alts">{#each altPresets as a}<option value={a}></option>{/each}</datalist>
    </label>
    <label class="span2">
      <span class="faint">Departure (local) · empty = now</span>
      <input class="field" type="datetime-local" bind:value={route.departure} />
    </label>
  </div>
  {#if suggested && route.cruise !== suggested.alt}
    <button class="suggest" onclick={() => (route.cruise = suggested.alt)}>
      Suggested VFR altitude <b class="num">{suggested.alt.toLocaleString()}</b> ({suggested.east ? 'eastbound, odd + 500' : 'westbound, even + 500'}, clears terrain by 1,000 ft+)
    </button>
  {/if}

  <div class="section-title">Waypoints</div>
  {#if !route.waypoints.length}
    <p class="muted">No waypoints yet. Type a route above, search with <kbd>/</kbd>, or turn on "Click map to add".</p>
  {/if}
  <ol class="wps">
    {#each route.waypoints as w, i (w.uid)}
      <li
        draggable="true"
        ondragstart={() => (dragFrom = i)}
        ondragover={(e) => e.preventDefault()}
        ondrop={() => onDrop(i)}
      >
        <div class="wp">
          <GripVertical size={14} class="grip" />
          <button class="id" onclick={() => mapRef.map?.flyTo({ center: [w.lon, w.lat], zoom: 11 })}>
            <b>{w.id}</b>
            <span class="faint">{w.kind === 'airport' ? w.name : w.detail || w.name}</span>
          </button>
          <button class="btn ghost ico" onclick={() => route.remove(w.uid)} aria-label="Remove"><X size={14} /></button>
        </div>
        {#if i < route.waypoints.length - 1}
          {@const li = legInfo(i)}
          <div class="leg faint num">
            <span>{li.mc}°M</span><span>{li.d.toFixed(1)} NM</span>{#if li.ete}<span>{fmtTime(li.ete)}</span>{/if}
          </div>
        {/if}
      </li>
    {/each}
  </ol>

  {#if log}
    <div class="summary">
      <div><Ruler size={14} /><b class="num">{log.totalDist.toFixed(1)}</b><span class="faint">NM</span></div>
      <div><Clock size={14} /><b class="num">{fmtTime(log.totalTime)}</b><span class="faint">ETE</span></div>
      <div><Fuel size={14} /><b class="num">{log.tripFuel.toFixed(1)}</b><span class="faint">gal trip</span></div>
      <div><Plane size={14} /><b class="num" class:bad={log.fuelAtLanding < log.reserveFuel}>{log.fuelAtLanding.toFixed(1)}</b><span class="faint">gal at landing</span></div>
    </div>
    <div class="section-title">Hazard advisor</div>
    <HazardList hazards={plan.hazards} compact />
    <div class="row end">
      <button class="btn" onclick={() => (app.profileOpen = !app.profileOpen)}><AreaChart size={15} /> {app.profileOpen ? 'Hide' : 'Show'} profile</button>
      <button class="btn" onclick={() => (app.panel = 'briefing')}>Navlog & briefing</button>
    </div>
    <p class="faint small">Winds: {plan.windSource === 'windy' ? 'Windy (GFS)' : plan.windSource === 'open-meteo' ? 'Open-Meteo GFS' : 'loading…'} · Magnetic variation: WMM-2025</p>
  {/if}
</PanelShell>

{#if exporting}<RouteExport onclose={() => (exporting = false)} />{/if}

<style>
  .entry textarea {
    height: auto;
    padding: 8px 10px;
    resize: none;
    font-family: var(--mono);
    font-size: 13px;
    text-transform: uppercase;
  }
  .row {
    display: flex;
    gap: 6px;
    margin-top: 8px;
  }
  .row.end {
    margin-top: 12px;
  }
  .row .btn {
    flex: 1;
    justify-content: center;
  }
  .btn.active {
    border-color: var(--accent);
    color: var(--accent);
    background: var(--accent-soft);
  }
  .ico {
    width: 30px;
    padding: 0;
    justify-content: center;
  }
  .tip {
    font-size: 11.5px;
    margin: 8px 0 0;
  }
  .warn {
    display: flex;
    gap: 6px;
    align-items: flex-start;
    color: var(--warn);
    font-size: 12px;
    margin: 8px 0 0;
  }
  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }
  .grid label {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 11.5px;
    min-width: 0;
  }
  .span2 {
    grid-column: span 2;
  }
  .suggest {
    margin-top: 8px;
    width: 100%;
    text-align: left;
    border: 1px dashed var(--line-strong);
    background: var(--surface-2);
    border-radius: 8px;
    padding: 7px 10px;
    font-size: 11.5px;
    color: var(--text-2);
    cursor: pointer;
  }
  .wps {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .wp {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 4px;
    border-radius: 8px;
    background: var(--surface-solid);
    border: 1px solid var(--line);
  }
  .wp :global(.grip) {
    color: var(--text-3);
    cursor: grab;
    flex: none;
  }
  .id {
    flex: 1;
    min-width: 0;
    display: flex;
    gap: 8px;
    align-items: baseline;
    text-align: left;
    border: 0;
    background: none;
    cursor: pointer;
    padding: 0;
  }
  .id span {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    font-size: 12px;
  }
  .leg {
    display: flex;
    gap: 12px;
    padding: 5px 0 5px 34px;
    font-size: 11.5px;
    border-left: 2px solid var(--accent);
    margin-left: 14px;
  }
  .summary {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 6px;
    margin-top: 14px;
  }
  .summary div {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
    padding: 8px;
    background: var(--surface-2);
    border-radius: 9px;
    font-size: 10.5px;
    color: var(--text-2);
  }
  .summary b {
    font-size: 15px;
    color: var(--text);
  }
  .summary b.bad {
    color: var(--bad);
  }
  .small {
    font-size: 11px;
  }
  kbd {
    font-family: var(--mono);
    border: 1px solid var(--line-strong);
    border-radius: 4px;
    padding: 0 4px;
  }
</style>
