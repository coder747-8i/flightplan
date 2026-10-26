<script lang="ts">
  // Route panel: the go / no-go call, route entry, aircraft / altitude / departure, waypoints with
  // leg info and fuel stops, totals; NOTAMs and saved plans; export / import.
  import { ArrowDownUp, Trash2, MousePointerClick, X, GripVertical, AreaChart, Plane, TriangleAlert, Fuel, Clock, Ruler, Share2, FileUp, Save, Lock, ChevronDown, ChevronRight, Loader, ChevronUp } from '@lucide/svelte'
  import PanelShell from './PanelShell.svelte'
  import { api } from '../api'
  import { app, toast } from '../stores/app.svelte'
  import { route, setDeparture } from '../stores/route.svelte'
  import { plan } from '../stores/plan.svelte'
  import { aircraft } from '../stores/aircraft.svelte'
  import { config } from '../stores/config.svelte'
  import { plans } from '../stores/plans.svelte'
  import { fmtTime, deg } from '../nav/navlog'
  import { bearing, distanceNm } from '../nav/geo'
  import { variation } from '../nav/navlog'
  import { mapRef } from '../chart/mapRef'
  import VerdictCard from './VerdictCard.svelte'
  import NotamList from './NotamList.svelte'
  import RouteExport from './RouteExport.svelte'
  import { parseRouteFile } from '../io/flightplan'
  import { pickFiles } from '../io/files'
  import { important } from '../wx/notams'

  let text = $state(route.text)
  let unresolved = $state<string[]>([])
  let dragFrom: number | null = null
  let exporting = $state(false)
  let importError = $state('')
  let saving = $state(false)
  let planName = $state('')
  let notamsOpen = $state(false)
  let plansOpen = $state(!route.waypoints.length)

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

  /** Resolve the typed route; airways ("PXR V327 FLG") expand to their fixes. */
  async function applyText() {
    try {
      const res = await api.route(text.toUpperCase())
      unresolved = res.missing
      route.replace(res.places)
      fit()
    } catch (e) {
      unresolved = [(e as Error).message]
    }
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
    if (route.locked || dragFrom == null || dragFrom === i) return
    route.move(dragFrom, i)
    dragFrom = null
  }

  function openSave() {
    const w = route.waypoints
    planName = w.length ? `${w[0].id} to ${w[w.length - 1].id}` : ''
    saving = !saving
  }
  function savePlan() {
    const name = plans.save(planName, { distanceNm: log?.totalDist ?? 0, eteMin: log?.totalTime ?? 0, fuelGal: log?.tripFuel ?? 0 })
    saving = false
    toast(`Saved "${name}"`)
  }
  function loadPlan(id: string) {
    const p = plans.load(id)
    if (p) {
      fit()
      toast(`Loaded "${p.name}"`)
    }
  }

  const depLabel = $derived(route.departure ? plan.departureTime.toLocaleString([], { weekday: 'short', hour: '2-digit', minute: '2-digit', hour12: false }) : 'Now')
  const notamTitle = $derived.by(() => {
    if (!config.notams) return 'NOTAMs'
    if (plan.notamsLoading && !plan.notamsDone) return 'NOTAMs (loading…)'
    const imp = plan.notams.filter(important).length
    return imp ? `NOTAMs (${plan.notams.length} · ${imp} important)` : `NOTAMs (${plan.notams.length})`
  })
  const altPresets = [3500, 4500, 5500, 6500, 7500, 8500, 9500, 10500, 11500, 12500]
  const hasRoute = $derived(route.waypoints.length >= 2)
</script>

<PanelShell title="Route" subtitle={hasRoute && log ? `${Math.round(log.totalDist)} NM · ${fmtTime(log.totalTime)} · ${log.tripFuel.toFixed(1)} gal` : 'Type a route or click the map'} width={380}>
  {#snippet actions()}
    <button class="btn ghost ico" title="Export to FltPlan Go, FltPlan.com, Garmin .fpl, GPX or KML" onclick={() => (exporting = true)} disabled={!route.waypoints.length} data-tour="route-export"><Share2 size={15} /></button>
    <button class="btn ghost ico" title="Save this flight plan in the app" onclick={openSave} disabled={!hasRoute}><Save size={15} /></button>
    <button class="btn ghost ico" title="Open a Garmin .fpl or GPX file" onclick={importFile} disabled={route.locked}><FileUp size={15} /></button>
    <button class="btn ghost ico" title="Reverse route" onclick={() => route.reverse()} disabled={!hasRoute || route.locked}><ArrowDownUp size={15} /></button>
    <button class="btn ghost ico" title="Clear route" onclick={() => route.clear()} disabled={!route.waypoints.length || route.locked}><Trash2 size={15} /></button>
  {/snippet}

  {#if saving}
    <form class="save" onsubmit={(e) => { e.preventDefault(); savePlan() }}>
      <b>Save flight plan</b>
      <input class="field" bind:value={planName} placeholder="Name" />
      {#if route.fuelStops.length}<span class="faint small"><Fuel size={12} /> Saves the {route.fuelStops.length} fuel stop{route.fuelStops.length === 1 ? '' : 's'} too</span>{/if}
      <div class="row"><button class="btn primary" type="submit"><Save size={14} /> Save</button><button class="btn" type="button" onclick={() => (saving = false)}>Cancel</button></div>
    </form>
  {/if}

  {#if hasRoute}
    <div class="verdict">
      {#if plan.verdict}
        <VerdictCard v={plan.verdict} />
      {:else}
        <p class="faint checking"><Loader size={14} class="spin" /> Checking weather, terrain and airspace…</p>
      {/if}
      {#if plan.forecastLoading}<p class="faint small">Fetching forecasts…</p>{/if}
    </div>
  {/if}

  {#if route.locked}
    <p class="locked"><Lock size={13} /> View only. Press L or the lock in the rail to edit.</p>
  {/if}

  <fieldset disabled={route.locked}>
    <div class="entry" data-tour="route-entry">
      <textarea
        class="field"
        rows="2"
        bind:value={text}
        placeholder="KSDL PXR V327 FLG KGCN"
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
      <p class="faint tip">Airways work too (PXR V327 FLG). Drag the magenta line to add a waypoint, or drag a waypoint to move it.</p>
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
        <span class="faint">Departure (local) · {depLabel} · or drag the timeline under the chart</span>
        <input class="field" type="datetime-local" bind:value={route.departure} />
      </label>
    </div>
    <div class="quick">
      <button class="chip" class:on={!route.departure} onclick={() => (route.departure = '')}>Now</button>
      {#each [['+1 h', 1], ['+3 h', 3], ['+1 day', 24]] as const as [label, h]}
        <button class="chip" onclick={() => setDeparture(new Date(plan.departureTime.getTime() + h * 3600_000))}>{label}</button>
      {/each}
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
        {@const canStop = w.kind === 'airport' && i > 0 && i < route.waypoints.length - 1}
        <li
          draggable={!route.locked}
          ondragstart={() => (dragFrom = i)}
          ondragover={(e) => e.preventDefault()}
          ondrop={() => onDrop(i)}
        >
          <div class="wp" class:stop={w.fuelStop}>
            <GripVertical size={14} class="grip" />
            <button class="id" onclick={() => mapRef.map?.flyTo({ center: [w.lon, w.lat], zoom: 11 })} title="Show on the chart">
              <b>{w.id}</b>
              {#if w.fuelStop}<span class="fuel"><Fuel size={11} /> Fuel stop</span>{/if}
              <span class="faint">{w.kind === 'airport' ? w.name : w.detail || w.name}</span>
            </button>
            <span class="acts">
              {#if canStop}
                <button class="btn ghost ico pump" class:on={w.fuelStop} title={w.fuelStop ? 'Not a fuel stop' : 'Make this a fuel stop: land, top off, go on'} onclick={() => route.toggleFuelStop(i)}><Fuel size={13} /></button>
              {/if}
              <button class="btn ghost ico" title="Move up" disabled={i === 0} onclick={() => route.move(i, i - 1)}><ChevronUp size={13} /></button>
              <button class="btn ghost ico" title="Move down" disabled={i === route.waypoints.length - 1} onclick={() => route.move(i, i + 1)}><ChevronDown size={13} /></button>
              <button class="btn ghost ico" onclick={() => route.remove(w.uid)} aria-label="Remove" title="Remove"><X size={14} /></button>
            </span>
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

    {#if log && log.segments.length > 1}
      <div class="segs">
        {#each log.segments as seg, k}
          {@const from = k === 0 ? route.waypoints[0].id : log.segments[k - 1].to}
          <div class:bad={seg.fuelAtLanding < log.reserveFuel}>
            {from} → {seg.to}: {Math.round(seg.endD - seg.startD)} NM · {fmtTime((seg.arrive.getTime() - seg.depart.getTime()) / 60000)} · lands with {seg.fuelAtLanding.toFixed(1)} gal
          </div>
        {/each}
        <label class="ground">
          <span class="faint"><Fuel size={12} /> Top off at each stop, on the ground</span>
          <input class="field num" type="number" min="10" max="240" step="5" bind:value={route.stopMinutes} /> <span class="faint">min</span>
        </label>
      </div>
    {:else if route.waypoints.length > 2 && log && log.fuelAtLanding < log.reserveFuel}
      <p class="hint"><Fuel size={13} /> Not enough fuel non-stop? Tap the pump next to an airport in the list to make it a fuel stop.</p>
    {/if}
  </fieldset>

  {#if log}
    <div class="summary">
      <div><Ruler size={14} /><b class="num">{log.totalDist.toFixed(1)}</b><span class="faint">NM</span></div>
      <div><Clock size={14} /><b class="num">{fmtTime(log.totalTime)}</b><span class="faint">{log.segments.length > 1 ? 'time incl. stops' : 'ETE'}</span></div>
      <div><Fuel size={14} /><b class="num">{log.tripFuel.toFixed(1)}</b><span class="faint">gal burn</span></div>
      <div><Plane size={14} /><b class="num" class:bad={log.fuelAtLanding < log.reserveFuel}>{log.fuelAtLanding.toFixed(1)}</b><span class="faint">gal at landing</span></div>
    </div>
    <div class="row end">
      <button class="btn" onclick={() => (app.profileOpen = !app.profileOpen)}><AreaChart size={15} /> {app.profileOpen ? 'Hide' : 'Show'} profile</button>
      <button class="btn" onclick={() => (app.panel = 'briefing')}>Navlog & briefing</button>
    </div>
    <p class="faint small">Winds aloft: {plan.windSource === 'nws-fb' ? 'NWS winds-aloft forecast (FB)' : 'loading…'} · Magnetic variation: WMM-2025</p>
  {/if}

  {#if hasRoute}
    <button class="fold" onclick={() => (notamsOpen = !notamsOpen)}>
      {#if notamsOpen}<ChevronDown size={14} />{:else}<ChevronRight size={14} />{/if} {notamTitle}
    </button>
    {#if notamsOpen}
      <NotamList items={plan.notams} configured={config.notams} errors={plan.notamErrors} loading={plan.notamsLoading} />
      {#if config.notams}<p class="faint small">Airports and navaids on the route, a 10 NM corridor and the ARTCCs crossed.</p>{/if}
    {/if}
  {/if}

  {#if plans.list.length}
    <button class="fold" onclick={() => (plansOpen = !plansOpen)}>
      {#if plansOpen}<ChevronDown size={14} />{:else}<ChevronRight size={14} />{/if} Saved plans ({plans.list.length})
    </button>
    {#if plansOpen}
      {#each plans.list as p (p.id)}
        <div class="plan">
          <div class="pinfo">
            <b>{p.name}</b>
            <span class="mono rt">{p.routeText}</span>
            <span class="faint small">
              {[
                p.distanceNm > 0 ? `${Math.round(p.distanceNm)} NM · ${fmtTime(p.eteMin)}` : '',
                p.fuelStops.length ? `fuel at ${p.fuelStops.join(', ')}` : '',
                p.aircraft,
                `saved ${new Date(p.savedAt).toLocaleString([], { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit', hour12: false })}`,
              ].filter(Boolean).join(' · ')}
            </span>
          </div>
          <button class="btn" disabled={route.locked} title={route.locked ? 'View only: press L to edit' : 'Load this plan'} onclick={() => loadPlan(p.id)}>Load</button>
          <button class="btn ghost ico" title="Delete" onclick={() => plans.remove(p.id)}><Trash2 size={13} /></button>
        </div>
      {/each}
    {/if}
  {/if}

  {#if plan.loading}<p class="faint small">Updating the profile…</p>{/if}
  {#if plan.error}<p class="err">{plan.error}</p>{/if}
</PanelShell>

{#if exporting}<RouteExport onclose={() => (exporting = false)} />{/if}

<style>
  fieldset {
    border: 0;
    margin: 0;
    padding: 0;
    min-width: 0;
  }
  fieldset:disabled {
    opacity: 0.75;
  }
  .verdict {
    margin-bottom: 8px;
  }
  .checking {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
  }
  .verdict :global(.spin) {
    animation: spin 1s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  .locked {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11.5px;
    color: var(--text-2);
    margin: 0 0 8px;
  }
  .save {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 10px;
    border-radius: 10px;
    background: var(--surface-2);
    margin-bottom: 10px;
  }
  .save .row {
    margin-top: 2px;
  }
  .quick {
    display: flex;
    gap: 6px;
    margin-top: 8px;
  }
  .chip {
    border: 1px solid var(--line-strong);
    background: var(--surface-solid);
    border-radius: 999px;
    padding: 2px 10px;
    font-size: 11.5px;
    cursor: pointer;
  }
  .chip.on {
    border-color: var(--accent);
    color: var(--accent);
    background: var(--accent-soft);
  }
  .wp.stop {
    border-color: var(--accent);
  }
  .fuel {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    color: var(--accent) !important;
    font-weight: 600;
    font-size: 11px !important;
    flex: none;
  }
  .acts {
    display: flex;
    gap: 0;
    opacity: 0;
    transition: opacity 0.1s;
  }
  .wp:hover .acts,
  .wp:focus-within .acts,
  .acts:has(.pump.on) {
    opacity: 1;
  }
  .acts .ico {
    width: 24px;
    height: 24px;
  }
  .pump {
    color: var(--text-3);
  }
  .pump.on,
  .pump:hover {
    color: var(--accent);
  }
  @media (hover: none) {
    .acts {
      opacity: 1;
    }
  }
  .segs {
    margin-top: 8px;
    padding: 8px 10px;
    border-radius: 9px;
    background: var(--accent-soft);
    font-size: 11.5px;
    color: var(--text-2);
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .segs .bad {
    color: var(--bad);
  }
  .ground {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 2px;
  }
  .ground span:first-child {
    flex: 1;
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .ground input {
    width: 64px;
    height: 26px;
  }
  .hint {
    display: flex;
    gap: 6px;
    font-size: 11.5px;
    color: var(--text-2);
    margin: 8px 0 0;
  }
  .fold {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    border: 0;
    border-top: 1px solid var(--line);
    background: none;
    padding: 10px 0 8px;
    margin-top: 10px;
    cursor: pointer;
    font-weight: 600;
    font-size: 12.5px;
    text-align: left;
  }
  .plan {
    display: flex;
    gap: 6px;
    align-items: center;
    padding: 7px 10px;
    border-radius: 9px;
    background: var(--surface-2);
    margin-bottom: 4px;
  }
  .pinfo {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .rt {
    font-size: 11px;
    color: var(--text-2);
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .err {
    color: var(--bad);
    font-size: 12px;
  }
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
