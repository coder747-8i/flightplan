<script lang="ts">
  import RulerButton from '../chart/RulerButton.svelte'
  import { Mountain } from '@lucide/svelte'
  import { app } from '../stores/app.svelte'

  // Garmin-style altitude band: only airspace overlapping the band is drawn.
  const MAX = 18000
  const STEP = 500
  let open = $state(false)
  let track: HTMLDivElement | undefined = $state()
  let dragging: 'lo' | 'hi' | null = null

  const fmt = (ft: number) => (ft <= 0 ? 'SFC' : ft >= MAX ? 'FL180+' : ft >= 10000 ? `${(ft / 1000).toFixed(1)}k` : ft.toLocaleString())
  const pct = (ft: number) => (Math.min(ft, MAX) / MAX) * 100

  function valueAt(clientY: number) {
    const r = track!.getBoundingClientRect()
    const f = 1 - (clientY - r.top) / r.height
    return Math.round(Math.max(0, Math.min(1, f)) * (MAX / STEP)) * STEP
  }

  function down(e: PointerEvent) {
    const v = valueAt(e.clientY)
    dragging = Math.abs(v - app.altitude.lo) < Math.abs(v - app.altitude.hi) ? 'lo' : 'hi'
    ;(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId)
    move(e)
  }
  function move(e: PointerEvent) {
    if (!dragging) return
    const v = valueAt(e.clientY)
    app.altitude.enabled = true
    if (dragging === 'lo') app.altitude.lo = Math.min(v, app.altitude.hi - STEP)
    else app.altitude.hi = Math.max(v, app.altitude.lo + STEP) >= MAX ? 99999 : Math.max(v, app.altitude.lo + STEP)
  }
  function up() {
    dragging = null
  }

  const ticks = [0, 3000, 6000, 9000, 12000, 15000, 18000]
  const presets = [
    { label: 'Below 3k', lo: 0, hi: 3000 },
    { label: 'Below 10k', lo: 0, hi: 10000 },
    { label: '4.5k – 12.5k', lo: 4500, hi: 12500 },
  ]
</script>

<div class="alt">
  <div class="top">
    <RulerButton />
    <button class="pill glass" class:on={app.altitude.enabled} onclick={() => (open = !open)} title="Filter airspace by altitude" data-tour="altitude">
      <Mountain size={15} strokeWidth={2} />
      <span class="num">{app.altitude.enabled ? `${fmt(app.altitude.lo)} – ${fmt(app.altitude.hi)}` : 'All altitudes'}</span>
    </button>
  </div>

  {#if open}
    <div class="pop glass">
      <div class="head">
        <strong>Airspace altitude filter</strong>
        <label class="sw">
          <input type="checkbox" bind:checked={app.altitude.enabled} /> On
        </label>
      </div>
      <p class="faint">Shows only airspace that overlaps this band. Drag the handles.</p>
      <div class="body">
        <div class="scale">
          {#each ticks as t}
            <span style:bottom={`${pct(t)}%`}>{t === 0 ? 'SFC' : t >= MAX ? 'FL180' : `${t / 1000}k`}</span>
          {/each}
        </div>
        <div
          class="track"
          class:disabled={!app.altitude.enabled}
          bind:this={track}
          onpointerdown={down}
          onpointermove={move}
          onpointerup={up}
          role="slider"
          tabindex="0"
          aria-valuemin={0}
          aria-valuemax={MAX}
          aria-valuenow={app.altitude.hi}
          aria-label="Altitude band"
        >
          <div class="band" style:bottom={`${pct(app.altitude.lo)}%`} style:top={`${100 - pct(app.altitude.hi)}%`}></div>
          <div class="handle" style:bottom={`${pct(app.altitude.hi)}%`}><span class="num">{fmt(app.altitude.hi)}</span></div>
          <div class="handle" style:bottom={`${pct(app.altitude.lo)}%`}><span class="num">{fmt(app.altitude.lo)}</span></div>
        </div>
      </div>
      <div class="presets">
        {#each presets as pr}
          <button class="btn" onclick={() => (app.altitude = { enabled: true, lo: pr.lo, hi: pr.hi })}>{pr.label}</button>
        {/each}
      </div>
    </div>
  {/if}
</div>

<style>
  .alt {
    position: absolute;
    top: 12px;
    right: 12px;
    z-index: 12;
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 8px;
  }
  .top {
    display: flex;
    gap: 8px;
  }
  .pill {
    height: 36px;
    padding: 0 12px;
    display: flex;
    align-items: center;
    gap: 7px;
    cursor: pointer;
    font-weight: 600;
    border-radius: 999px;
    color: var(--text-2);
  }
  .pill.on {
    color: var(--accent);
  }
  .pop {
    width: 250px;
    padding: 14px;
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .sw {
    display: flex;
    gap: 5px;
    align-items: center;
    font-size: 12px;
    cursor: pointer;
  }
  p {
    margin: 4px 0 10px;
    font-size: 11.5px;
  }
  .body {
    display: flex;
    gap: 10px;
    height: 240px;
    padding: 8px 0;
  }
  .scale {
    position: relative;
    width: 38px;
    font-size: 10px;
    color: var(--text-3);
    text-align: right;
  }
  .scale span {
    position: absolute;
    right: 0;
    transform: translateY(50%);
  }
  .track {
    position: relative;
    width: 10px;
    border-radius: 999px;
    background: var(--surface-2);
    border: 1px solid var(--line);
    cursor: ns-resize;
    touch-action: none;
  }
  .track.disabled {
    opacity: 0.45;
  }
  .band {
    position: absolute;
    left: -1px;
    right: -1px;
    background: var(--accent);
    border-radius: 999px;
    opacity: 0.85;
  }
  .handle {
    position: absolute;
    left: 50%;
    width: 20px;
    height: 20px;
    margin-left: -10px;
    transform: translateY(50%);
    border-radius: 50%;
    background: var(--surface-solid);
    border: 2px solid var(--accent);
    box-shadow: 0 1px 4px rgba(0, 0, 0, 0.2);
  }
  .handle span {
    position: absolute;
    left: 28px;
    top: 50%;
    transform: translateY(-50%);
    font-size: 12px;
    font-weight: 600;
    white-space: nowrap;
  }
  .presets {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 6px;
  }
  .presets .btn {
    height: 28px;
    font-size: 11.5px;
    padding: 0 9px;
  }
  /* below the full-width search box on phones */
  @media (max-width: 640px) {
    .alt {
      top: 64px;
    }
  }
</style>
