<script lang="ts">
  // First-run screen: welcome, a walk through what the app does, chart data, optional API keys,
  // aircraft, then finish (optionally into the spotlight tour of the real screen).
  import {
    ArrowLeft, ArrowRight, Map as MapIcon, Route, AreaChart, CloudSun, Plane, FileText, Share2, CircleCheck, Circle, Sparkles, FileUp, Plus, TriangleAlert, Layers,
  } from '@lucide/svelte'
  import DataSetup from './DataSetup.svelte'
  import KeysForm from '../ui/KeysForm.svelte'
  import AircraftWizard from '../ui/AircraftWizard.svelte'
  import AircraftImport from '../ui/AircraftImport.svelte'
  import { onboarding } from '../stores/onboarding.svelte'
  import { aircraft, PRESETS } from '../stores/aircraft.svelte'
  import { config } from '../stores/config.svelte'

  const steps = ['Welcome', 'How it works', 'Chart data', 'Weather keys', 'Your aircraft', 'Finish']
  const step = $derived(onboarding.step)
  const go = (i: number) => (onboarding.step = Math.max(0, Math.min(steps.length - 1, i)))

  const installed = $derived(!!onboarding.status?.installed)
  let wizard = $state(false)
  let importing = $state(false)

  // the walkthrough slides
  const slides = [
    { icon: MapIcon, title: 'A sectional you can shape', points: ['Vector chart with airspace, airports, navaids, obstacles and terrain.', 'Layers turns groups on and off; the altitude filter shows only airspace in your altitude band, like a Garmin.', 'Click anything on the chart for details: airports open a card with runways, frequencies, weather and FBOs.'], tip: 'Press / to search for an airport, navaid or fix.' },
    { icon: Route, title: 'Build the route', points: ['Type a route (KNYL BZA 3300N11400W KPHX) or click the map to add waypoints.', 'Drag the magenta line to rubber-band a new waypoint in; drag a waypoint to move it.', 'Pick your aircraft, cruise altitude and departure time. The navlog uses winds aloft for that time.'], tip: 'The suggested VFR altitude follows the hemispheric rule and clears terrain by 1,000 ft.' },
    { icon: AreaChart, title: 'See it from the side', points: ['The profile shows terrain, obstacles and airspace along the route at your planned altitude.', 'Hover the profile and the map follows, and the other way round.', 'Climb and descent points (TOC/TOD) come from your aircraft’s performance.'], tip: 'Click an airspace band in the profile to highlight it on the map.' },
    { icon: CloudSun, title: 'Weather and hazards', points: ['Radar, METARs, TAFs, PIREPs, AIRMETs/SIGMETs, G-AIRMETs and TFRs on the chart.', 'Coded weather is decoded into plain English (switch to raw any time).', 'The hazard list flags airspace incursions, terrain, TFRs, crosswinds and fuel.'], tip: 'Add a Windy key for wind, cloud and icing maps along the route.' },
    { icon: Plane, title: 'Aircraft, weight & balance', points: ['Create airframes step by step, or import files from another pilot or your club.', 'Load seats, bags and fuel; the CG chart shows takeoff and landing against the envelope.', 'Performance numbers drive the navlog, fuel burn and reserves.'], tip: 'Export your fleet to a file to back it up or share it.' },
    { icon: Share2, title: 'Brief and file', points: ['The briefing gathers route, navlog, weather, NOTAM links and W&B on one printable page.', 'Export the route to FltPlan.com (fields ready to copy), Garmin .fpl or GPX.', 'For planning only: always get an official briefing before flight.'], tip: 'Everything you plan stays on this computer.' },
  ]
  let slide = $state(0)

  function next() {
    if (step === 1 && slide < slides.length - 1) return void slide++
    go(step + 1)
  }
  function back() {
    if (step === 1 && slide > 0) return void slide--
    go(step - 1)
  }

  const sample = $derived(aircraft.list.length === 1 && aircraft.list[0].tail === 'N12345')
  const keysSet = $derived(!!config.windyMapKey || config.windyPoint || config.notams)
</script>

<div class="onboarding" role="dialog" aria-modal="true" aria-label="Set up Flightplan">
  <aside>
    <div class="brand">
      <svg viewBox="0 0 24 24" width="26" height="26" aria-hidden="true">
        <path d="M3 17 L10 13 L14 15 L21 7" fill="none" stroke="var(--accent)" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" />
        <circle cx="3" cy="17" r="2.2" fill="var(--accent)" />
        <circle cx="21" cy="7" r="2.2" fill="var(--accent)" />
      </svg>
      <span>Flightplan</span>
    </div>
    <ol>
      {#each steps as s, i}
        <li class:on={i === step} class:done={i < step}>
          <button onclick={() => go(i)}>
            {#if i < step}<CircleCheck size={15} />{:else}<Circle size={15} />{/if}
            {s}
          </button>
        </li>
      {/each}
    </ol>
    <p class="faint foot">For planning only. Not a substitute for an official briefing, current charts, NOTAMs or the POH.</p>
  </aside>

  <section class="content scroll">
    <div class="inner">
      {#if step === 0}
        <div class="hero">
          <Sparkles size={28} />
          <h1>Welcome to Flightplan</h1>
          <p class="lead">
            A flight planner for the US: a sectional-style chart, route planning with a vertical profile, winds-aloft navlog, weather,
            weight & balance and a printable briefing.
          </p>
          <p class="muted">Setup takes a couple of minutes:</p>
          <ul class="checklist">
            <li><Layers size={15} /> A quick look at how the app works</li>
            <li><MapIcon size={15} /> Download the FAA chart data</li>
            <li><CloudSun size={15} /> Optionally add Windy and FAA NOTAM keys</li>
            <li><Plane size={15} /> Set up your aircraft</li>
          </ul>
        </div>
      {:else if step === 1}
        {@const sl = slides[slide]}
        <h2>How Flightplan works</h2>
        <div class="slide">
          <div class="art"><sl.icon size={44} strokeWidth={1.5} /></div>
          <div>
            <h3>{sl.title}</h3>
            <ul>{#each sl.points as p}<li>{p}</li>{/each}</ul>
            <p class="tip"><Sparkles size={13} /> {sl.tip}</p>
          </div>
        </div>
        <div class="dots">
          {#each slides as _, i}<button class:on={i === slide} onclick={() => (slide = i)} aria-label={`Slide ${i + 1}`}></button>{/each}
        </div>
      {:else if step === 2}
        <h2>Chart data</h2>
        <p class="lead">The chart, airport database, airspace and obstacles come from the FAA's 28-day cycle, packaged for Flightplan.</p>
        <DataSetup />
      {:else if step === 3}
        <h2>Weather & NOTAM keys</h2>
        <p class="lead">Optional. Everything else works without them. You can add or change keys later in Settings.</p>
        <KeysForm />
      {:else if step === 4}
        <h2>Your aircraft</h2>
        <p class="lead">Performance drives the navlog and fuel; weight & balance data drives the loading chart.</p>
        {#if sample}
          <p class="warn"><TriangleAlert size={14} /> N12345 is a sample Cessna 172S with typical numbers. Add your own airplane, then delete the sample in the Aircraft panel.</p>
        {/if}
        <div class="actions">
          <button class="big" onclick={() => (wizard = true)}>
            <Plus size={20} />
            <b>Create an aircraft</b>
            <span class="faint">Step by step: start from a template or from scratch, with checks along the way</span>
          </button>
          <button class="big" onclick={() => (importing = true)}>
            <FileUp size={20} />
            <b>Import aircraft files</b>
            <span class="faint">From another pilot, your club, or a backup (.fpaircraft.json)</span>
          </button>
        </div>
        <div class="section-title">In your hangar</div>
        <ul class="fleet">
          {#each aircraft.list as a (a.id)}
            <li><Plane size={14} /> <b>{a.tail || 'No tail'}</b> <span class="faint">{a.name} · {a.perf.tas} kt · {a.usableFuel} gal</span></li>
          {/each}
        </ul>
        <p class="faint small">Templates available: {PRESETS.filter((p) => p.type !== 'ZZZZ').map((p) => p.name).join(', ')}.</p>
      {:else}
        <div class="hero">
          <CircleCheck size={28} />
          <h1>You're set</h1>
          <ul class="summary">
            <li class:ok={installed}>
              {#if installed}<CircleCheck size={15} />{:else}<TriangleAlert size={15} />{/if}
              {installed ? `Chart data installed${onboarding.status?.info?.cycle ? ` (cycle ${onboarding.status.info.cycle})` : ''}` : 'No chart data yet: the chart and search stay empty until you install it'}
              {#if !installed}<button class="link" onclick={() => go(2)}>Install</button>{/if}
            </li>
            <li class:ok={keysSet}>
              {#if keysSet}<CircleCheck size={15} />{:else}<Circle size={15} />{/if}
              {keysSet ? 'Weather / NOTAM keys added' : 'No API keys (optional)'}
            </li>
            <li class:ok={!sample}>
              {#if !sample}<CircleCheck size={15} />{:else}<Circle size={15} />{/if}
              {aircraft.list.length} aircraft{sample ? ' (the sample only)' : ''}
            </li>
          </ul>
          <p class="muted">Take a 1-minute tour of the screen, or jump straight in. Help (bottom left) brings this setup and the tour back any time.</p>
          <div class="finish">
            <button class="btn primary big-btn" onclick={() => onboarding.finish(true)}><Sparkles size={15} /> Take the tour</button>
            <button class="btn big-btn" onclick={() => onboarding.finish(false)}>Start planning</button>
          </div>
        </div>
      {/if}
    </div>

    {#if step < steps.length - 1}
      <footer>
        {#if step > 0}<button class="btn" onclick={back}><ArrowLeft size={14} /> Back</button>{/if}
        <span class="spacer"></span>
        {#if step === 2 && !installed}
          <button class="btn ghost" onclick={() => go(3)}>Skip for now</button>
        {:else if step === 3}
          <button class="btn ghost" onclick={() => go(4)}>Skip</button>
        {/if}
        <button class="btn primary" onclick={next} disabled={step === 2 && !installed}>
          {step === 0 ? 'Get started' : 'Next'} <ArrowRight size={14} />
        </button>
      </footer>
    {/if}
  </section>
</div>

{#if wizard}<AircraftWizard onclose={() => (wizard = false)} onimport={() => (importing = true)} />{/if}
{#if importing}<AircraftImport onclose={() => (importing = false)} />{/if}

<style>
  .onboarding {
    position: fixed;
    inset: 0;
    z-index: 50;
    display: grid;
    grid-template-columns: 260px 1fr;
    background: var(--bg);
  }
  aside {
    display: flex;
    flex-direction: column;
    padding: 28px 20px;
    border-right: 1px solid var(--line);
    background: var(--surface-solid);
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    font-weight: 700;
    font-size: 17px;
    letter-spacing: -0.01em;
    margin-bottom: 28px;
  }
  aside ol {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  aside li button {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 9px;
    border: 0;
    background: none;
    padding: 9px 10px;
    border-radius: 9px;
    color: var(--text-3);
    font-weight: 550;
    cursor: pointer;
    text-align: left;
  }
  aside li.done button {
    color: var(--text-2);
  }
  aside li.done :global(svg) {
    color: var(--ok);
  }
  aside li.on button {
    background: var(--accent-soft);
    color: var(--accent);
  }
  .foot {
    margin-top: auto;
    font-size: 11px;
    line-height: 1.45;
  }
  .content {
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .inner {
    flex: 1;
    width: 100%;
    max-width: 760px;
    margin: 0 auto;
    padding: 48px 32px 24px;
  }
  footer {
    position: sticky;
    bottom: 0;
    display: flex;
    gap: 8px;
    align-items: center;
    max-width: 760px;
    width: 100%;
    margin: 0 auto;
    padding: 16px 32px 24px;
    background: linear-gradient(transparent, var(--bg) 30%);
  }
  .spacer {
    flex: 1;
  }
  h1 {
    font-size: 28px;
    letter-spacing: -0.02em;
    margin: 12px 0 10px;
  }
  h2 {
    font-size: 22px;
    letter-spacing: -0.015em;
    margin: 0 0 8px;
  }
  .lead {
    font-size: 14.5px;
    line-height: 1.55;
    color: var(--text-2);
    margin: 0 0 18px;
  }
  .hero :global(svg:first-child) {
    color: var(--accent);
  }
  .checklist,
  .summary {
    list-style: none;
    padding: 0;
    margin: 12px 0 20px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    font-size: 14px;
  }
  .checklist li,
  .summary li {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .checklist :global(svg) {
    color: var(--accent);
  }
  .summary li {
    color: var(--text-2);
  }
  .summary li.ok :global(svg) {
    color: var(--ok);
  }
  .slide {
    display: grid;
    grid-template-columns: 140px 1fr;
    gap: 28px;
    align-items: start;
    margin-top: 22px;
    min-height: 240px;
  }
  .art {
    width: 140px;
    height: 140px;
    border-radius: 28px;
    display: grid;
    place-items: center;
    background: var(--accent-soft);
    color: var(--accent);
  }
  .slide h3 {
    margin: 4px 0 10px;
    font-size: 18px;
  }
  .slide ul {
    margin: 0;
    padding-left: 18px;
    line-height: 1.6;
    font-size: 14px;
    color: var(--text-2);
  }
  .tip {
    display: flex;
    gap: 7px;
    align-items: center;
    margin-top: 14px;
    font-size: 12.5px;
    color: var(--text);
    background: var(--surface-2);
    padding: 8px 10px;
    border-radius: 9px;
  }
  .dots {
    display: flex;
    gap: 6px;
    justify-content: center;
    margin-top: 18px;
  }
  .dots button {
    width: 8px;
    height: 8px;
    border-radius: 99px;
    border: 0;
    padding: 0;
    background: var(--line-strong);
    cursor: pointer;
  }
  .dots button.on {
    width: 22px;
    background: var(--accent);
  }
  .actions {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }
  .big {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 5px;
    text-align: left;
    padding: 16px;
    border-radius: 12px;
    border: 1px solid var(--line-strong);
    background: var(--surface-solid);
    cursor: pointer;
  }
  .big:hover {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .big :global(svg) {
    color: var(--accent);
  }
  .big span {
    font-size: 12px;
    line-height: 1.4;
  }
  .fleet {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .fleet li {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .warn {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    color: var(--warn);
    background: rgba(212, 138, 11, 0.1);
    padding: 9px 11px;
    border-radius: 9px;
    font-size: 12.5px;
    line-height: 1.45;
  }
  .warn :global(svg) {
    flex: none;
    margin-top: 2px;
  }
  .small {
    font-size: 11.5px;
  }
  .link {
    border: 0;
    background: none;
    color: var(--accent);
    font-weight: 600;
    cursor: pointer;
    padding: 0;
  }
  .finish {
    display: flex;
    gap: 10px;
    margin-top: 18px;
  }
  .big-btn {
    height: 40px;
    padding: 0 18px;
    font-size: 14px;
  }
  .btn:disabled {
    opacity: 0.5;
    cursor: default;
  }
  @media (max-width: 760px) {
    .onboarding {
      grid-template-columns: 1fr;
    }
    aside {
      display: none;
    }
    .inner {
      padding: 28px 16px 16px;
    }
    footer {
      padding: 12px 16px 16px;
    }
    .slide,
    .actions {
      grid-template-columns: 1fr;
    }
  }
</style>
