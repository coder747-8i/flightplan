<script lang="ts">
  // Spotlight tour over the real screen: dims everything except one control and explains it.
  // Targets are elements marked with data-tour="...".
  import { ArrowLeft, ArrowRight, X } from '@lucide/svelte'
  import { onMount, tick } from 'svelte'
  import { app, type Panel } from '../stores/app.svelte'
  import { onboarding } from '../stores/onboarding.svelte'

  const steps: { target: string; title: string; text: string; panel?: Panel }[] = [
    { target: 'search', title: 'Search', text: 'Find any airport, navaid or fix. Press / from anywhere to jump here.' },
    { target: 'rail-route', title: 'Route', text: 'Type a route or click the map to build one. Your aircraft, altitude and departure time live here too.', panel: 'route' },
    { target: 'route-entry', title: 'Route entry', text: 'Enter identifiers, airways and lat/longs separated by spaces, then Load route. Or turn on "Click map to add".', panel: 'route' },
    { target: 'route-verdict', title: 'Go / no-go', text: 'A green, yellow or red call for the flight at your departure time: weather at both ends against your personal minimums, wind and crosswind, official WPC / SPC forecasts along the route, terrain, airspace, fuel and weight & balance. Set your minimums in Settings.', panel: 'route' },
    { target: 'timeline', title: 'Timeline', text: 'Drag to plan up to five days ahead. Radar (then HRRR forecast radar, then WPC forecast charts), winds, forecasts and the go / no-go call follow the time you pick; green stretches are good windows to fly. Press play to animate.' },
    { target: 'route-export', title: 'Export', text: "Send the route to FltPlan Go with a QR code (nothing gets filed), fill FltPlan.com's form, or save a Garmin .fpl, GPX or KML file. The button next to it opens .fpl/.gpx files.", panel: 'route' },
    { target: 'rail-layers', title: 'Layers', text: 'Switch chart layers on and off: airspace classes, special use airspace, airports, navaids, airways, obstacles.', panel: 'layers' },
    { target: 'ruler', title: 'Measure', text: 'A ruler for distance and course: click points along a line (they snap to airports and navaids), press Space for another line, up to three. Press R to toggle it.' },
    { target: 'altitude', title: 'Altitude filter', text: 'Show only the airspace that matters in your altitude band.' },
    { target: 'rail-weather', title: 'Weather', text: 'Radar (live, then forecast), flight categories, winds aloft, TFRs, AIRMETs / SIGMETs, G-AIRMETs and PIREPs, plus the forecast at your departure and arrival, and Windy if you added a key.', panel: 'weather' },
    { target: 'rail-aircraft', title: 'Aircraft', text: 'Load the airplane and check weight & balance. Create, edit, import and export airframes here.', panel: 'aircraft' },
    { target: 'aircraft-new', title: 'New aircraft', text: 'The guided editor walks through performance, stations and the CG envelope.', panel: 'aircraft' },
    { target: 'profile', title: 'Profile', text: 'Terrain, obstacles and airspace along the route at your planned altitude. Shows up once a route has two points.' },
    { target: 'rail-briefing', title: 'Briefing', text: 'Everything for the flight on one printable page.' },
    { target: 'rail-help', title: 'Help', text: 'Bring back this tour or the setup screen, and update chart data, any time.' },
  ]

  let i = $state(0)
  let rect = $state<DOMRect | null>(null)

  async function place() {
    const s = steps[i]
    if (s.panel && app.panel !== s.panel) app.panel = s.panel
    else if (!s.panel && ['route', 'layers', 'aircraft'].includes(app.panel)) app.panel = 'none'
    await tick()
    // panels animate in; measure after they settle
    setTimeout(() => {
      const el = document.querySelector(`[data-tour="${s.target}"]`) as HTMLElement | null
      const r = el?.getBoundingClientRect()
      rect = r && r.width > 0 ? r : null
    }, 220)
  }

  function go(n: number) {
    if (n < 0) return
    if (n >= steps.length) return end()
    i = n
    place()
  }

  function end() {
    onboarding.touring = false
    app.panel = 'none'
  }

  onMount(() => {
    place()
    const onResize = () => place()
    window.addEventListener('resize', onResize)
    return () => window.removeEventListener('resize', onResize)
  })

  const pad = 6
  const card = $derived.by(() => {
    const w = 300
    if (!rect) return { left: window.innerWidth / 2 - w / 2, top: window.innerHeight / 2 - 80 }
    // beside the target when there's room, otherwise below / above
    let left = rect.right + 16
    let top = rect.top
    if (left + w > window.innerWidth - 12) {
      left = Math.max(12, Math.min(window.innerWidth - w - 12, rect.left))
      top = rect.bottom + 14
      if (top + 170 > window.innerHeight) top = Math.max(12, rect.top - 184)
    }
    return { left, top: Math.max(12, Math.min(top, window.innerHeight - 190)) }
  })
</script>

<svelte:window
  onkeydown={(e) => {
    if (e.key === 'Escape') end()
    if (e.key === 'ArrowRight') go(i + 1)
    if (e.key === 'ArrowLeft') go(i - 1)
  }}
/>

<div class="tour" role="dialog" aria-label="Tour">
  {#if rect}
    <div class="hole" style:left={`${rect.left - pad}px`} style:top={`${rect.top - pad}px`} style:width={`${rect.width + pad * 2}px`} style:height={`${rect.height + pad * 2}px`}></div>
  {:else}
    <div class="dim"></div>
  {/if}
  <div class="card glass" style:left={`${card.left}px`} style:top={`${card.top}px`}>
    <div class="head">
      <span class="faint small">{i + 1} of {steps.length}</span>
      <button class="btn ghost x" onclick={end} aria-label="End tour"><X size={14} /></button>
    </div>
    <h3>{steps[i].title}</h3>
    <p>{steps[i].text}</p>
    <div class="row">
      {#if i > 0}<button class="btn" onclick={() => go(i - 1)}><ArrowLeft size={14} /></button>{/if}
      <span class="spacer"></span>
      <button class="btn primary" onclick={() => go(i + 1)}>{i === steps.length - 1 ? 'Done' : 'Next'} {#if i < steps.length - 1}<ArrowRight size={14} />{/if}</button>
    </div>
  </div>
</div>

<style>
  .tour {
    position: fixed;
    inset: 0;
    z-index: 55;
  }
  .dim {
    position: absolute;
    inset: 0;
    background: rgba(10, 14, 20, 0.45);
  }
  .hole {
    position: absolute;
    border-radius: 12px;
    box-shadow: 0 0 0 9999px rgba(10, 14, 20, 0.45);
    outline: 2px solid var(--accent);
    transition: all 0.25s ease;
    pointer-events: none;
  }
  .card {
    position: absolute;
    width: 300px;
    padding: 14px 16px;
    background: var(--surface-solid);
    transition: left 0.25s ease, top 0.25s ease;
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .x {
    width: 26px;
    height: 26px;
    padding: 0;
    justify-content: center;
  }
  h3 {
    margin: 2px 0 6px;
    font-size: 15px;
  }
  p {
    margin: 0 0 12px;
    color: var(--text-2);
    line-height: 1.5;
  }
  .row {
    display: flex;
    gap: 6px;
  }
  .spacer {
    flex: 1;
  }
  .small {
    font-size: 11px;
  }
</style>
