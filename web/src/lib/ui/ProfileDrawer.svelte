<script lang="ts">
  // Vertical profile: terrain (centerline + corridor high point), obstacles, airspace volumes
  // the route passes through, planned climb/cruise/descent, freezing level, plus a strip of
  // warnings (terrain clearance, airspace incursions) you can tap. Hover/touch syncs with the
  // map; drag the magenta cruise line to change altitude; tap a volume to see that airspace.
  import { ChevronDown, Mountain, Snowflake, TriangleAlert, OctagonAlert, X } from '@lucide/svelte'
  import { app } from '../stores/app.svelte'
  import { plan, type Crossing } from '../stores/plan.svelte'
  import { route } from '../stores/route.svelte'
  import { distanceNm } from '../nav/geo'

  let width = $state(800)
  let showE = $state(false)
  const H = 230
  const STRIP = 20 // warning strip above the chart
  const PAD = { l: 52, r: 16, t: 14 + STRIP, b: 26 }

  const total = $derived(plan.total)
  const terrainMax = $derived(plan.samples.reduce((m, s) => Math.max(m, s.max), 0))
  const yMax = $derived(Math.ceil(Math.max(route.cruise + 2500, terrainMax + 3000, 4000) / 1000) * 1000)
  const plotW = $derived(Math.max(1, width - PAD.l - PAD.r))
  const x = (d: number) => PAD.l + (d / Math.max(total, 0.01)) * plotW
  const y = (ft: number) => PAD.t + (1 - Math.min(Math.max(ft, 0), yMax) / yMax) * (H - PAD.t - PAD.b)
  const dAt = (px: number) => Math.max(0, Math.min(total, ((px - PAD.l) / plotW) * total))

  // thin the samples to ~1 per 2 px for drawing
  const drawSamples = $derived.by(() => {
    const s = plan.samples
    const step = Math.max(1, Math.floor(s.length / (plotW / 2)))
    return step === 1 ? s : s.filter((_, i) => i % step === 0 || i === s.length - 1)
  })
  const terrainPath = $derived(
    drawSamples.length ? `M${x(0)},${y(0)} ` + drawSamples.map((p) => `L${x(p.d).toFixed(1)},${y(p.elev).toFixed(1)}`).join(' ') + ` L${x(total)},${y(0)} Z` : '',
  )
  const corridorPath = $derived(drawSamples.map((p, i) => `${i ? 'L' : 'M'}${x(p.d).toFixed(1)},${y(p.max).toFixed(1)}`).join(' '))
  const altPath = $derived(plan.altitudePath.map(([d, a], i) => `${i ? 'L' : 'M'}${x(d).toFixed(1)},${y(a).toFixed(1)}`).join(' '))

  const KIND_STYLE: Record<string, { color: string; dash?: string; fill: number; label: string }> = {
    B: { color: 'var(--blue)', fill: 0.16, label: 'B' },
    C: { color: 'var(--magenta)', fill: 0.14, label: 'C' },
    D: { color: 'var(--blue)', dash: '5 3', fill: 0.08, label: 'D' },
    E2: { color: 'var(--magenta)', dash: '5 3', fill: 0.05, label: 'E' },
    E3: { color: 'var(--magenta)', dash: '5 3', fill: 0.04, label: 'E' },
    E4: { color: 'var(--magenta)', dash: '5 3', fill: 0.04, label: 'E' },
    E5: { color: 'var(--magenta)', dash: '2 3', fill: 0.03, label: 'E' },
    R: { color: '#2c63b8', fill: 0.18, label: 'R' },
    P: { color: '#2c63b8', fill: 0.28, label: 'P' },
    W: { color: '#2c63b8', fill: 0.1, label: 'W' },
    A: { color: 'var(--magenta)', fill: 0.08, label: 'A' },
    MOA: { color: 'var(--magenta)', fill: 0.1, label: 'MOA' },
  }
  const fmtAlt = (v: number) => (v >= 99999 ? 'UNL' : v >= 17999 ? `FL${Math.round((v + 1) / 100)}` : v === 0 ? 'SFC' : `${Math.round(v / 100)}`)
  const rangeText = (c: Crossing) =>
    `${c.lo_agl ? `${c.lo.toLocaleString()} AGL` : c.lo === 0 ? 'SFC' : c.lo.toLocaleString()} – ${c.hi >= 99999 ? 'unlimited' : c.hi === 17999 ? 'FL180' : c.hi.toLocaleString()}`

  const volumes = $derived(
    plan.airspace
      .filter((c) => KIND_STYLE[c.kind] && (showE || !c.kind.startsWith('E')) && c.to - c.from > 0.05)
      .map((c) => {
        const st = KIND_STYLE[c.kind]
        const hi = Math.min(c.hi, yMax)
        let d: string
        if (c.lo_agl) {
          // AGL floors follow the terrain
          const pts = drawSamples.filter((s) => s.d >= c.from && s.d <= c.to)
          d = `M${x(c.from)},${y(hi)} L${x(c.to)},${y(hi)} ` + [...pts].reverse().map((s) => `L${x(s.d)},${y(s.elev + c.lo)}`).join(' ') + ' Z'
        } else {
          d = `M${x(c.from)},${y(hi)} L${x(c.to)},${y(hi)} L${x(c.to)},${y(c.lo)} L${x(c.from)},${y(c.lo)} Z`
        }
        return { c, st, d, label: `${st.label} ${fmtAlt(c.hi)}/${c.lo_agl ? c.lo + 'AGL' : fmtAlt(c.lo)}`, cx: (x(c.from) + x(c.to)) / 2, wide: x(c.to) - x(c.from) > 46 }
      }),
  )

  const ticks = $derived.by(() => {
    const w = route.waypoints
    let acc = 0
    return w.map((p, i) => {
      if (i) acc += distanceNm(w[i - 1].lat, w[i - 1].lon, p.lat, p.lon)
      return { id: p.id, d: acc }
    })
  })

  // freezing level per waypoint from the temperatures aloft (nearest forecast hour)
  const freezing = $derived.by(() => {
    const out: { d: number; ft: number }[] = []
    const t = plan.departureTime.getTime() / 1000
    ticks.forEach((tk, i) => {
      const wp = plan.winds[i]
      if (!wp?.levels) return
      const lv = wp.levels
        .map((l) => {
          if (!l.times.length) return null
          const near = l.times.reduce((a, b) => (Math.abs(b.t - t) < Math.abs(a.t - t) ? b : a))
          return near.temp == null ? null : { alt: near.alt ?? l.alt, temp: near.temp }
        })
        .filter(Boolean) as { alt: number; temp: number }[]
      lv.sort((a, b) => a.alt - b.alt)
      if (!lv.length) return
      if (lv[0].temp <= 0) return out.push({ d: tk.d, ft: 0 })
      for (let k = 1; k < lv.length; k++) {
        const lo = lv[k - 1]
        const hi = lv[k]
        if (lo.temp > 0 && hi.temp <= 0) {
          out.push({ d: tk.d, ft: lo.alt + ((hi.alt - lo.alt) * lo.temp) / (lo.temp - hi.temp) })
          break
        }
      }
    })
    return out
  })
  const freezePath = $derived(freezing.map((f, i) => `${i ? 'L' : 'M'}${x(f.d)},${y(f.ft)}`).join(' '))

  // warning strip: terrain clearance + airspace incursions at the planned altitude
  const marks = $derived([
    ...plan.terrainConflicts.map((t) => ({ kind: 'terrain' as const, from: t.from, to: t.to, level: t.level === 'bad' ? 'warning' : 'caution', title: t.level === 'bad' ? 'Under 500 ft above terrain' : 'Under 1,000 ft above terrain', c: null as Crossing | null })),
    ...plan.incursions.map((i) => ({ kind: 'airspace' as const, from: i.from, to: i.to, level: i.level, title: `Inside ${i.c.name}`, c: i.c })),
  ])
  const yTicks = $derived(Array.from({ length: Math.floor(yMax / 2000) + 1 }, (_, i) => i * 2000))

  // hover/touch (shared with the map through plan.hover)
  const hoverD = $derived(plan.hover?.d ?? null)
  const hoverInfo = $derived.by(() => {
    if (hoverD == null || !plan.samples.length) return null
    let s = plan.samples[0]
    for (const p of plan.samples) if (Math.abs(p.d - hoverD) < Math.abs(s.d - hoverD)) s = p
    const alt = plan.altAt(s.d)
    const inside = plan.airspace.filter((c) => s.d >= c.from && s.d <= c.to && (showE || !c.kind.startsWith('E')))
    const atAlt = inside.filter((c) => {
      const floor = c.lo_agl ? s.elev + c.lo : c.lo
      return alt >= floor && alt <= c.hi
    })
    return { s, alt, clr: alt - s.max, inside, atAlt }
  })

  let dragging = false
  let moved = false
  let svg = $state<SVGSVGElement>()
  const local = (e: PointerEvent) => {
    const r = svg!.getBoundingClientRect()
    return { px: e.clientX - r.left, py: e.clientY - r.top }
  }
  function down(e: PointerEvent) {
    const { py } = local(e)
    moved = false
    if (!route.locked && Math.abs(py - y(route.cruise)) < 12) {
      dragging = true
      svg!.setPointerCapture(e.pointerId)
    }
    move(e)
  }
  function move(e: PointerEvent) {
    const { px, py } = local(e)
    if (px >= PAD.l && px <= width - PAD.r) plan.hover = { d: dAt(px), from: 'profile' }
    if (dragging) {
      moved = true
      const ft = (1 - (py - PAD.t) / (H - PAD.t - PAD.b)) * yMax
      route.cruise = Math.max(500, Math.min(45000, Math.round(ft / 500) * 500))
    }
  }
  function up(e: PointerEvent) {
    const wasDrag = dragging && moved
    dragging = false
    if (wasDrag) return
    // a tap: pick the topmost volume under the finger, or a warning in the strip
    const { px, py } = local(e)
    const d = dAt(px)
    if (py < PAD.t) {
      const m = marks.find((m) => d >= m.from - total * 0.01 && d <= m.to + total * 0.01)
      if (m?.c) plan.picked = m.c
      return
    }
    const ft = (1 - (py - PAD.t) / (H - PAD.t - PAD.b)) * yMax
    const hit = [...volumes].reverse().find((v) => {
      if (d < v.c.from || d > v.c.to) return false
      const s = plan.samples.find((x) => x.d >= d) ?? plan.samples[plan.samples.length - 1]
      const floor = v.c.lo_agl ? (s?.elev ?? 0) + v.c.lo : v.c.lo
      return ft >= floor && ft <= v.c.hi
    })
    plan.picked = hit?.c ?? null
  }
  function leave(e: PointerEvent) {
    if (e.pointerType === 'mouse' && plan.hover?.from === 'profile') plan.hover = null
  }
</script>

{#if route.waypoints.length >= 2}
  <section class="drawer glass" class:closed={!app.profileOpen} data-tour="profile" bind:clientHeight={app.drawerHeight}>
    <header>
      <button class="title" onclick={() => (app.profileOpen = !app.profileOpen)}>
        <Mountain size={15} /> Vertical profile
        <ChevronDown size={15} style={`transform: rotate(${app.profileOpen ? 0 : 180}deg)`} />
      </button>
      {#if app.profileOpen}
        <div class="legend">
          <span><i class="sw terrain"></i>Terrain</span>
          <span><i class="sw corridor"></i>Highest within 1 NM</span>
          <span><i class="sw plan"></i>Planned{route.locked ? '' : ' (drag)'}</span>
          {#if freezing.length}<span><Snowflake size={12} /> 0°C</span>{/if}
          <label><input type="checkbox" bind:checked={showE} /> Class E</label>
        </div>
        {#if marks.some((m) => m.level === 'warning')}
          <span class="alert bad"><OctagonAlert size={12} /> {marks.filter((m) => m.level === 'warning').length} warning{marks.filter((m) => m.level === 'warning').length > 1 ? 's' : ''}</span>
        {:else if marks.some((m) => m.level === 'caution')}
          <span class="alert"><TriangleAlert size={12} /> {marks.filter((m) => m.level === 'caution').length} caution{marks.filter((m) => m.level === 'caution').length > 1 ? 's' : ''}</span>
        {/if}
        {#if plan.loading}<span class="faint">Updating…</span>{/if}
      {/if}
    </header>
    {#if app.profileOpen}
      <div class="chart" bind:clientWidth={width}>
        <svg bind:this={svg} {width} height={H} onpointerdown={down} onpointermove={move} onpointerup={up} onpointerleave={leave} role="img" aria-label="Vertical profile">
          <defs>
            <linearGradient id="terr" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0" stop-color="#b59a76" />
              <stop offset="1" stop-color="#d9c8a9" />
            </linearGradient>
          </defs>
          {#each yTicks as t}
            <line x1={PAD.l} x2={width - PAD.r} y1={y(t)} y2={y(t)} class="grid" />
            <text x={PAD.l - 8} y={y(t) + 4} class="axis" text-anchor="end">{t === 0 ? 'SFC' : `${t / 1000}k`}</text>
          {/each}

          {#each volumes as v}
            <path d={v.d} fill={v.st.color} fill-opacity={plan.picked === v.c ? v.st.fill + 0.2 : v.st.fill} stroke={v.st.color} stroke-width={plan.picked === v.c ? 2.6 : 1.3} stroke-dasharray={v.st.dash ?? ''} class="vol-shape" />
            {#if v.wide}
              <text x={v.cx} y={Math.max(y(Math.min(v.c.hi, yMax)) + 13, PAD.t + 12)} class="vol" fill={v.st.color} text-anchor="middle">{v.label}</text>
            {/if}
          {/each}

          <path d={terrainPath} fill="url(#terr)" />
          <path d={corridorPath} fill="none" stroke="#8a6d48" stroke-width="1" stroke-dasharray="2 2" />

          {#each plan.obstacles as o}
            <path d={`M${x(o.d) - 4},${y(o.msl) + 9} L${x(o.d)},${y(o.msl)} L${x(o.d) + 4},${y(o.msl) + 9}`} class="obst" />
          {/each}

          {#if freezePath}<path d={freezePath} fill="none" stroke="#3a8fd6" stroke-width="1.6" stroke-dasharray="6 4" />{/if}

          <!-- warning strip -->
          <rect x={PAD.l} y={4} width={plotW} height={STRIP - 6} rx="3" class="strip" />
          {#each marks as m}
            <rect
              x={x(m.from) - 2}
              y={4}
              width={Math.max(6, x(m.to) - x(m.from) + 4)}
              height={STRIP - 6}
              rx="3"
              class={`mark ${m.level}`}
              class:picked={m.c && plan.picked === m.c}
            ><title>{m.title}</title></rect>
            {#if m.kind === 'terrain'}
              <rect x={x(m.from) - 1} width={Math.max(3, x(m.to) - x(m.from) + 2)} y={PAD.t} height={H - PAD.t - PAD.b} fill={m.level === 'warning' ? 'var(--bad)' : 'var(--warn)'} opacity="0.1" />
            {:else}
              <line x1={x(m.from)} x2={x(m.to)} y1={y(plan.altAt((m.from + m.to) / 2))} y2={y(plan.altAt((m.from + m.to) / 2))} stroke={m.level === 'warning' ? 'var(--bad)' : m.level === 'caution' ? 'var(--warn)' : 'var(--blue)'} stroke-width="9" stroke-linecap="round" opacity="0.35" />
            {/if}
          {/each}

          <path d={altPath} fill="none" stroke="#ffffff" stroke-width="6" stroke-linejoin="round" opacity="0.9" />
          <path d={altPath} fill="none" stroke="var(--accent)" stroke-width="3" stroke-linejoin="round" />
          <line x1={x(plan.log?.tocDist ?? 0)} x2={x(plan.log?.todDist ?? total)} y1={y(route.cruise)} y2={y(route.cruise)} class="grab" />
          <text x={x(((plan.log?.tocDist ?? 0) + (plan.log?.todDist ?? total)) / 2)} y={y(route.cruise) - 8} text-anchor="middle" class="cruise">{route.cruise.toLocaleString()} ft</text>

          {#each ticks as t}
            <line x1={x(t.d)} x2={x(t.d)} y1={PAD.t} y2={H - PAD.b} class="wpline" />
            <text x={x(t.d)} y={H - 8} text-anchor="middle" class="wp">{t.id}</text>
          {/each}

          {#if hoverInfo}
            <line x1={x(hoverInfo.s.d)} x2={x(hoverInfo.s.d)} y1={PAD.t} y2={H - PAD.b} class="cross" />
            <circle cx={x(hoverInfo.s.d)} cy={y(hoverInfo.alt)} r="5" fill="var(--accent)" stroke="white" stroke-width="2" />
          {/if}
        </svg>

        {#if hoverInfo && !plan.picked}
          <div class="tip glass" style:left={`${Math.min(x(hoverInfo.s.d) + 12, width - 240)}px`}>
            <div class="num"><b>{hoverInfo.s.d.toFixed(1)} NM</b> · planned {Math.round(hoverInfo.alt).toLocaleString()} ft</div>
            <div class="num">Terrain {Math.round(hoverInfo.s.elev).toLocaleString()} ft · highest nearby {Math.round(hoverInfo.s.max).toLocaleString()} ft</div>
            <div class="num" class:bad={hoverInfo.clr < 500} class:warn={hoverInfo.clr >= 500 && hoverInfo.clr < 1000}>Clearance {Math.round(hoverInfo.clr).toLocaleString()} ft</div>
            {#each hoverInfo.inside as c}<div class:in={hoverInfo.atAlt.includes(c)} class="faint">{hoverInfo.atAlt.includes(c) ? '● ' : ''}{c.name} · {rangeText(c)}</div>{/each}
          </div>
        {/if}

        {#if plan.picked}
          {@const c = plan.picked}
          {@const inc = plan.incursions.filter((i) => i.c === c)}
          <div class="picked glass" style:left={`${Math.min(Math.max(x(c.from), PAD.l), width - 300)}px`}>
            <div class="ph">
              <b>{c.name}</b>
              <button class="btn ghost x" onclick={() => (plan.picked = null)} aria-label="Close"><X size={14} /></button>
            </div>
            <div class="num">{rangeText(c)}{c.lo_agl ? '' : ' MSL'}</div>
            <div class="faint num">Route inside from {c.from.toFixed(1)} to {c.to.toFixed(1)} NM</div>
            {#if c.times}<div class="faint">Times: {c.times}</div>{/if}
            {#if inc.length}
              <div class="incl" class:bad={inc[0].level === 'warning'}>Your planned altitude is inside it from {inc[0].from.toFixed(1)} to {inc[inc.length - 1].to.toFixed(1)} NM</div>
            {:else}
              <div class="ok">Your planned altitude stays outside it</div>
            {/if}
            <div class="faint small">Highlighted on the map</div>
          </div>
        {/if}
      </div>
    {/if}
  </section>
{/if}

<style>
  .drawer {
    position: absolute;
    left: var(--panel-offset, 88px);
    right: 12px;
    bottom: 12px;
    z-index: 13;
    padding: 6px 12px 8px;
  }
  .drawer.closed {
    right: auto;
  }
  header {
    display: flex;
    align-items: center;
    gap: 14px;
    min-height: 30px;
    flex-wrap: wrap;
  }
  .title {
    display: flex;
    align-items: center;
    gap: 6px;
    border: 0;
    background: none;
    font-weight: 650;
    cursor: pointer;
    padding: 0;
  }
  .legend {
    display: flex;
    gap: 12px;
    font-size: 11px;
    color: var(--text-2);
    align-items: center;
  }
  .legend span,
  .legend label {
    display: flex;
    gap: 5px;
    align-items: center;
  }
  .sw {
    width: 14px;
    height: 8px;
    border-radius: 2px;
    display: inline-block;
  }
  .sw.terrain {
    background: #c9b08b;
  }
  .sw.corridor {
    border-top: 1.5px dashed #8a6d48;
    height: 0;
  }
  .sw.plan {
    background: var(--accent);
    height: 3px;
  }
  .alert {
    display: inline-flex;
    gap: 4px;
    align-items: center;
    font-size: 11px;
    font-weight: 650;
    color: var(--warn);
    background: rgba(212, 138, 11, 0.12);
    padding: 3px 8px;
    border-radius: 6px;
  }
  .alert.bad {
    color: var(--bad);
    background: rgba(210, 58, 58, 0.12);
  }
  .chart {
    position: relative;
  }
  svg {
    display: block;
    touch-action: none;
    user-select: none;
    -webkit-user-select: none;
  }
  .grid {
    stroke: var(--line);
  }
  .axis {
    font-size: 10px;
    fill: var(--text-3);
  }
  .vol {
    font-size: 10px;
    font-weight: 700;
    pointer-events: none;
  }
  .vol-shape {
    cursor: pointer;
  }
  .obst {
    fill: none;
    stroke: var(--text);
    stroke-width: 1.4;
  }
  .strip {
    fill: var(--surface-2);
  }
  .mark {
    cursor: pointer;
  }
  .mark.warning {
    fill: var(--bad);
  }
  .mark.caution {
    fill: var(--warn);
  }
  .mark.info {
    fill: var(--blue);
    opacity: 0.6;
  }
  .mark.picked {
    stroke: var(--text);
    stroke-width: 2;
  }
  .grab {
    stroke: transparent;
    stroke-width: 18;
    cursor: ns-resize;
  }
  .cruise {
    font-size: 11px;
    font-weight: 700;
    fill: var(--accent);
    paint-order: stroke;
    stroke: var(--surface-solid);
    stroke-width: 3px;
    pointer-events: none;
  }
  .wpline {
    stroke: var(--line-strong);
    stroke-dasharray: 2 3;
  }
  .wp {
    font-size: 10.5px;
    font-weight: 650;
    fill: var(--text-2);
  }
  .cross {
    stroke: var(--accent);
    stroke-width: 1.2;
    pointer-events: none;
  }
  .tip,
  .picked {
    position: absolute;
    top: 26px;
    padding: 8px 10px;
    font-size: 11.5px;
    width: 228px;
    border-radius: 10px;
    line-height: 1.5;
  }
  .tip {
    pointer-events: none;
  }
  .picked {
    width: 288px;
  }
  .ph {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 6px;
  }
  .x {
    height: 24px;
    width: 24px;
    padding: 0;
    justify-content: center;
  }
  .in {
    color: var(--text);
    font-weight: 600;
  }
  .incl {
    color: var(--warn);
    font-weight: 650;
  }
  .incl.bad,
  .bad {
    color: var(--bad);
    font-weight: 650;
  }
  .ok {
    color: var(--ok);
    font-weight: 600;
  }
  .warn {
    color: var(--warn);
    font-weight: 650;
  }
  .small {
    font-size: 10.5px;
  }
  @media (max-width: 640px) {
    .drawer {
      left: 8px;
      right: 8px;
      bottom: 72px;
    }
    .legend {
      display: none;
    }
  }
</style>
