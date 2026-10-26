<script lang="ts">
  // The planning timeline along the bottom of the chart: now to five days out. Dragging it sets
  // the departure time, so the radar (observed, HRRR forecast, then the WPC charts), winds
  // aloft, forecasts and the go / no-go call all follow. With a route, the track is coloured by
  // the call for departures every 3 hours: the green stretches are your windows to fly.
  import { Play, Pause, Loader } from '@lucide/svelte'
  import { app } from '../stores/app.svelte'
  import { plan } from '../stores/plan.svelte'
  import { route, setDeparture } from '../stores/route.svelte'
  import { wx } from '../wx/WeatherLayer.svelte'
  import { frameLabel, products } from '../wx/radar.svelte'
  import { leadText } from '../wx/forecast'
  import { CALL_WORD } from '../nav/verdict'

  const HOURS = 120
  let now = $state(Date.now())
  $effect(() => {
    const id = setInterval(() => (now = Date.now()), 60_000)
    return () => clearInterval(id)
  })

  const t0 = $derived(Math.floor(now / 900_000) * 900_000)
  const t1 = $derived(t0 + HOURS * 3600_000)
  const hasRoute = $derived(route.waypoints.length >= 2)
  const visible = $derived(hasRoute || wx.show.radar || wx.show.winds)
  const dep = $derived(Math.max(plan.departureTime.getTime(), t0))
  const isNow = $derived(!route.departure)

  /** Snap to 15 minutes within the HRRR's 18 hours, whole hours after that. */
  function snap(t: number) {
    const step = t - Date.now() < 18 * 3600_000 ? 900_000 : 3600_000
    return Math.round(t / step) * step
  }

  let track = $state<HTMLDivElement>()
  const frac = (t: number) => Math.min(1, Math.max(0, (t - t0) / (t1 - t0)))
  const pct = (t: number) => `${(frac(t) * 100).toFixed(3)}%`
  function timeAt(clientX: number) {
    const r = track!.getBoundingClientRect()
    return snap(t0 + Math.min(1, Math.max(0, (clientX - r.left) / r.width)) * (t1 - t0))
  }

  let dragging = false
  function down(e: PointerEvent) {
    dragging = true
    playing = false
    track!.setPointerCapture(e.pointerId)
    setDeparture(new Date(timeAt(e.clientX)))
  }
  function moveDrag(e: PointerEvent) {
    hoverT = timeAt(e.clientX)
    if (dragging) setDeparture(new Date(hoverT))
  }
  let hoverT = $state<number | null>(null)

  // play: step through time, an hour at a time (3 h beyond two days)
  let playing = $state(false)
  $effect(() => {
    if (!playing) return
    if (!wx.show.radar) wx.show.radar = true
    const id = setInterval(() => {
      if (wx.radarLoading) return
      const d = plan.departureTime.getTime()
      const step = d - Date.now() < 48 * 3600_000 ? 1 : 3
      const next = snap(d + step * 3600_000)
      setDeparture(new Date(next > t1 ? Date.now() : next))
    }, 700)
    return () => clearInterval(id)
  })

  // go / no-go windows as coloured stretches (each 3-hourly call covers ±1.5 h)
  const bands = $derived(
    hasRoute
      ? plan.windows.map(([t, call], i) => {
          const a = Math.max(t - 5400_000, t0)
          const b = Math.min(plan.windows[i + 1] ? plan.windows[i + 1][0] - 5400_000 : t + 5400_000, t1)
          return { left: pct(a), width: `${((frac(b) - frac(a)) * 100).toFixed(3)}%`, call }
        })
      : [],
  )

  // local midnights, and day labels at local noon
  const days = $derived.by(() => {
    const out: { line: string | null; mid: string; label: string }[] = []
    const d = new Date(t0)
    d.setHours(0, 0, 0, 0)
    while (d.getTime() < t1) {
      const start = d.getTime()
      const noon = Math.min(Math.max(start + 12 * 3600_000, t0), t1)
      out.push({ line: start > t0 ? pct(start) : null, mid: pct(noon), label: d.toLocaleDateString([], { weekday: 'short', day: 'numeric' }) })
      d.setDate(d.getDate() + 1)
    }
    return out.filter((x) => parseFloat(x.mid) > 2 && parseFloat(x.mid) < 98)
  })

  const reach = $derived(products.reach ?? now + 18 * 3600_000)
  const fmtTitle = (t: number) => new Date(t).toLocaleString([], isNow ? { weekday: 'short', hour: '2-digit', minute: '2-digit', hour12: false } : { weekday: 'short', day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit', hour12: false })
  const hoverText = $derived.by(() => {
    if (hoverT == null) return ''
    const w = hasRoute && plan.windows.length ? plan.windows.reduce((a, b) => (Math.abs(b[0] - hoverT!) < Math.abs(a[0] - hoverT!) ? b : a)) : null
    return `${new Date(hoverT).toLocaleString([], { weekday: 'short', day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit', hour12: false })}${w ? ` · ${CALL_WORD[w[1]]}` : ''}`
  })
  let innerWidth = $state(1024)
  // the profile drawer sits higher on phones (above the rail)
  const base = $derived(innerWidth <= 640 ? 72 : 12)
  const bottom = $derived(hasRoute && app.drawerHeight ? base + app.drawerHeight + 10 : base)
</script>

<svelte:window bind:innerWidth />

{#if visible}
  <div class="timeline glass" class:shifted={hasRoute && !app.profileOpen} style:bottom={`${bottom}px`} data-tour="timeline">
    <div class="head">
      <button class="play" title="Play through the forecast" onclick={() => (playing = !playing)}>
        {#if playing}<Pause size={15} />{:else}<Play size={15} />{/if}
      </button>
      <b>{isNow ? `Now · ${fmtTitle(dep)}` : fmtTitle(dep)}</b>
      {#if !isNow}<span class="faint">{leadText(new Date(dep))}</span>{/if}
      <span class="spacer"></span>
      {#if wx.radarLoading}<Loader size={12} class="spin" />{/if}
      {#if wx.frame}<span class="faint src">{frameLabel(wx.frame)}</span>{:else if wx.show.radar}<span class="faint src">No official forecast this far out</span>{/if}
      {#if !isNow}<button class="btn now" onclick={() => { playing = false; route.departure = '' }}>Now</button>{/if}
    </div>
    <div
      class="track"
      bind:this={track}
      role="slider"
      tabindex="0"
      aria-label="Departure time"
      aria-valuemin={t0}
      aria-valuemax={t1}
      aria-valuenow={dep}
      aria-valuetext={fmtTitle(dep)}
      title={hoverText}
      onpointerdown={down}
      onpointermove={moveDrag}
      onpointerup={() => (dragging = false)}
      onpointerleave={() => (hoverT = null)}
      onkeydown={(e) => {
        const step = e.shiftKey ? 24 : 1
        if (e.key === 'ArrowRight') setDeparture(new Date(snap(dep + step * 3600_000)))
        else if (e.key === 'ArrowLeft') setDeparture(new Date(Math.max(Date.now(), snap(dep - step * 3600_000))))
      }}
    >
      <div class="bar">
        {#each bands as b}<span class="band {b.call}" style:left={b.left} style:width={b.width}></span>{/each}
        {#each days as d}{#if d.line}<span class="day" style:left={d.line}></span>{/if}{/each}
        <span class="reach" style:left={pct(reach)} title="Forecast radar (HRRR) reaches about here; after it, the WPC forecast charts"></span>
        <span class="handle" style:left={pct(dep)}></span>
      </div>
      {#each days as d}<span class="label" style:left={d.mid}>{d.label}</span>{/each}
    </div>
  </div>
{/if}

<style>
  .timeline {
    position: absolute;
    left: calc(var(--panel-offset, 88px) + (100vw - var(--panel-offset, 88px) - 12px - min(760px, 100vw - var(--panel-offset, 88px) - 24px)) / 2);
    width: min(760px, calc(100vw - var(--panel-offset, 88px) - 24px));
    z-index: 12;
    padding: 6px 14px 4px 10px;
    border-radius: 16px;
  }
  .timeline.shifted {
    left: max(calc(var(--panel-offset, 88px) + 232px), calc(var(--panel-offset, 88px) + (100vw - var(--panel-offset, 88px) - 12px - min(760px, 100vw - var(--panel-offset, 88px) - 24px)) / 2));
    width: min(760px, calc(100vw - var(--panel-offset, 88px) - 256px));
  }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 24px;
    font-size: 13px;
  }
  .head .faint {
    font-size: 11.5px;
  }
  .src {
    font-size: 11px !important;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    min-width: 0;
  }
  .spacer {
    flex: 1;
  }
  .play {
    border: 0;
    background: none;
    color: var(--accent);
    cursor: pointer;
    display: flex;
    padding: 2px;
  }
  .now {
    height: 22px;
    font-size: 11.5px;
    padding: 0 8px;
  }
  .head :global(.spin) {
    animation: spin 1s linear infinite;
    color: var(--text-3);
    flex: none;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  .track {
    position: relative;
    height: 26px;
    margin: 2px 6px 0;
    cursor: pointer;
    touch-action: none;
    outline: none;
  }
  .bar {
    position: absolute;
    left: 0;
    right: 0;
    top: 3px;
    height: 8px;
    border-radius: 4px;
    background: var(--surface-2);
  }
  .band {
    position: absolute;
    top: 0;
    bottom: 0;
    opacity: 0.8;
  }
  .band.go {
    background: var(--ok);
  }
  .band.caution {
    background: var(--warn);
  }
  .band.nogo {
    background: var(--bad);
  }
  .day {
    position: absolute;
    top: -2px;
    bottom: -2px;
    width: 1px;
    background: var(--line-strong);
  }
  .reach {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 1px;
    background: var(--text-3);
    opacity: 0.6;
  }
  .handle {
    position: absolute;
    top: 50%;
    width: 14px;
    height: 14px;
    margin: -7px 0 0 -7px;
    border-radius: 50%;
    background: white;
    border: 2.5px solid var(--accent);
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.25);
  }
  .track:focus-visible .handle {
    box-shadow: 0 0 0 4px var(--accent-soft);
  }
  .label {
    position: absolute;
    top: 13px;
    transform: translateX(-50%);
    font-size: 10.5px;
    color: var(--text-3);
    white-space: nowrap;
    pointer-events: none;
  }
  @media (max-width: 640px) {
    .timeline,
    .timeline.shifted {
      left: 8px;
      right: 8px;
      width: auto;
    }
  }
</style>
