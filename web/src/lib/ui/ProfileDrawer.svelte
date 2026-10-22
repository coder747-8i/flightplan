<script lang="ts">
  // Vertical profile: terrain (centerline + corridor high point), obstacles, airspace volumes
  // the route passes through, planned climb/cruise/descent, freezing level. Drag the magenta
  // cruise line to change altitude; hover to read clearance and see the spot on the map.
  import { ChevronDown, Mountain, Snowflake } from '@lucide/svelte'
  import type { GeoJSONSource } from 'maplibre-gl'
  import { onDestroy } from 'svelte'
  import { app } from '../stores/app.svelte'
  import { plan } from '../stores/plan.svelte'
  import { route } from '../stores/route.svelte'
  import { mapRef } from '../chart/mapRef'
  import { pointAlong, distanceNm } from '../nav/geo'

  let width = $state(800)
  let showE = $state(false)
  const H = 210
  const PAD = { l: 52, r: 16, t: 14, b: 26 }

  const total = $derived(plan.samples.length ? plan.samples[plan.samples.length - 1].d : 0)
  const terrainMax = $derived(Math.max(0, ...plan.samples.map((s) => s.max)))
  const yMax = $derived.by(() => {
    const top = Math.max(route.cruise + 2500, terrainMax + 3000, 4000)
    return Math.ceil(top / 1000) * 1000
  })
  const x = (d: number) => PAD.l + (d / Math.max(total, 0.01)) * (width - PAD.l - PAD.r)
  const y = (ft: number) => PAD.t + (1 - Math.min(ft, yMax) / yMax) * (H - PAD.t - PAD.b)
  const dAt = (px: number) => ((px - PAD.l) / (width - PAD.l - PAD.r)) * total

  const terrainPath = $derived.by(() => {
    const s = plan.samples
    if (!s.length) return ''
    return `M${x(0)},${y(0)} ` + s.map((p) => `L${x(p.d).toFixed(1)},${y(p.elev).toFixed(1)}`).join(' ') + ` L${x(total)},${y(0)} Z`
  })
  const corridorPath = $derived(plan.samples.map((p, i) => `${i ? 'L' : 'M'}${x(p.d).toFixed(1)},${y(p.max).toFixed(1)}`).join(' '))
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

  const volumes = $derived(
    plan.airspace
      .filter((c) => KIND_STYLE[c.kind] && (showE || !c.kind.startsWith('E')))
      .filter((c) => c.to - c.from > 0.05)
      .map((c) => {
        const st = KIND_STYLE[c.kind]
        // AGL floors follow the terrain
        let d: string
        const hi = Math.min(c.hi, yMax)
        if (c.lo_agl) {
          const pts = plan.samples.filter((s) => s.d >= c.from && s.d <= c.to)
          d = `M${x(c.from)},${y(hi)} L${x(c.to)},${y(hi)} ` + [...pts].reverse().map((s) => `L${x(s.d)},${y(s.elev + c.lo)}`).join(' ') + ' Z'
        } else {
          d = `M${x(c.from)},${y(hi)} L${x(c.to)},${y(hi)} L${x(c.to)},${y(c.lo)} L${x(c.from)},${y(c.lo)} Z`
        }
        const fmt = (v: number) => (v >= 99999 ? 'UNL' : v >= 17999 ? 'FL180' : v === 0 ? 'SFC' : `${Math.round(v / 100)}`)
        return { ...c, st, d, label: `${st.label} ${fmt(c.hi)}/${c.lo_agl ? c.lo + 'AGL' : fmt(c.lo)}`, cx: (x(c.from) + x(c.to)) / 2, wide: x(c.to) - x(c.from) > 46 }
      }),
  )

  // waypoint ticks
  const ticks = $derived.by(() => {
    const w = route.waypoints
    let acc = 0
    return w.map((p, i) => {
      if (i) acc += distanceNm(w[i - 1].lat, w[i - 1].lon, p.lat, p.lon)
      return { id: p.id, d: acc }
    })
  })

  // freezing level per waypoint (from the winds/temps aloft data, nearest forecast hour)
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

  // clearance warnings along the planned path
  function plannedAlt(d: number) {
    const p = plan.altitudePath
    for (let i = 1; i < p.length; i++) {
      if (d <= p[i][0]) {
        const f = (d - p[i - 1][0]) / Math.max(p[i][0] - p[i - 1][0], 1e-6)
        return p[i - 1][1] + (p[i][1] - p[i - 1][1]) * f
      }
    }
    return p[p.length - 1]?.[1] ?? 0
  }
  const conflicts = $derived.by(() => {
    const out: { from: number; to: number; level: 'warn' | 'bad' }[] = []
    if (!plan.altitudePath.length) return out
    const s = plan.samples
    for (const p of s) {
      // ignore the first/last 5 NM (departure climb and pattern entry)
      if (p.d < 5 || p.d > total - 5) continue
      const clr = plannedAlt(p.d) - p.max
      const level = clr < 500 ? 'bad' : clr < 1000 ? 'warn' : null
      if (!level) continue
      const last = out[out.length - 1]
      if (last && last.level === level && p.d - last.to < 0.6) last.to = p.d
      else out.push({ from: p.d, to: p.d, level })
    }
    return out
  })

  const yTicks = $derived(Array.from({ length: Math.floor(yMax / 2000) + 1 }, (_, i) => i * 2000))

  // hover
  let hover = $state<{ px: number; d: number } | null>(null)
  const hoverInfo = $derived.by(() => {
    if (!hover || !plan.samples.length) return null
    const s = plan.samples.reduce((a, b) => (Math.abs(b.d - hover!.d) < Math.abs(a.d - hover!.d) ? b : a))
    const alt = plannedAlt(s.d)
    const inside = plan.airspace.filter((c) => s.d >= c.from && s.d <= c.to && (showE || !c.kind.startsWith('E')))
    return { s, alt, clr: alt - s.max, inside }
  })
  $effect(() => {
    const src = mapRef.map?.getSource('route-hover') as GeoJSONSource | undefined
    if (!mapRef.map) return
    if (!src && mapRef.map.isStyleLoaded()) {
      mapRef.map.addSource('route-hover', { type: 'geojson', data: { type: 'FeatureCollection', features: [] } })
      mapRef.map.addLayer({ id: 'route-hover', type: 'circle', source: 'route-hover', paint: { 'circle-radius': 7, 'circle-color': '#fff', 'circle-stroke-color': '#d4148c', 'circle-stroke-width': 3 } })
    }
    const s = hoverInfo?.s
    ;(mapRef.map.getSource('route-hover') as GeoJSONSource | undefined)?.setData(
      s ? { type: 'Feature', properties: {}, geometry: { type: 'Point', coordinates: [s.lon, s.lat] } } : { type: 'FeatureCollection', features: [] },
    )
  })
  onDestroy(() => {
    ;(mapRef.map?.getSource('route-hover') as GeoJSONSource | undefined)?.setData({ type: 'FeatureCollection', features: [] })
  })

  // drag the cruise line
  let dragging = false
  let svg = $state<SVGSVGElement>()!
  function svgY(e: PointerEvent) {
    const r = svg.getBoundingClientRect()
    return e.clientY - r.top
  }
  function down(e: PointerEvent) {
    const yy = svgY(e)
    if (Math.abs(yy - y(route.cruise)) < 10) {
      dragging = true
      svg.setPointerCapture(e.pointerId)
    }
  }
  function move(e: PointerEvent) {
    const r = svg.getBoundingClientRect()
    const px = e.clientX - r.left
    hover = px >= PAD.l && px <= width - PAD.r ? { px, d: dAt(px) } : null
    if (dragging) {
      const ft = (1 - (svgY(e) - PAD.t) / (H - PAD.t - PAD.b)) * yMax
      route.cruise = Math.max(500, Math.round(ft / 500) * 500)
    }
  }
  function up() {
    dragging = false
  }
</script>

{#if route.waypoints.length >= 2}
  <section class="drawer glass" class:closed={!app.profileOpen} data-tour="profile">
    <header>
      <button class="title" onclick={() => (app.profileOpen = !app.profileOpen)}>
        <Mountain size={15} /> Vertical profile
        <ChevronDown size={15} style={`transform: rotate(${app.profileOpen ? 0 : 180}deg)`} />
      </button>
      {#if app.profileOpen}
        <div class="legend">
          <span><i class="sw terrain"></i>Terrain</span>
          <span><i class="sw corridor"></i>Highest within 1 NM</span>
          <span><i class="sw plan"></i>Planned (drag to change)</span>
          {#if freezing.length}<span><Snowflake size={12} /> Freezing level</span>{/if}
          <label><input type="checkbox" bind:checked={showE} /> Class E</label>
        </div>
        {#if conflicts.length}
          <span class="alert" class:bad={conflicts.some((c) => c.level === 'bad')}>
            {conflicts.some((c) => c.level === 'bad') ? 'Under 500 ft terrain clearance' : 'Under 1,000 ft terrain clearance'}
          </span>
        {/if}
        {#if plan.loading}<span class="faint">Updating…</span>{/if}
      {/if}
    </header>
    {#if app.profileOpen}
      <div class="chart" bind:clientWidth={width}>
        <svg bind:this={svg} {width} height={H} onpointerdown={down} onpointermove={move} onpointerup={up} onpointerleave={() => (hover = null)} role="img" aria-label="Vertical profile">
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
            <path d={v.d} fill={v.st.color} fill-opacity={v.st.fill} stroke={v.st.color} stroke-width="1.3" stroke-dasharray={v.st.dash ?? ''} />
            {#if v.wide}
              <text x={v.cx} y={Math.max(y(Math.min(v.hi, yMax)) + 13, PAD.t + 12)} class="vol" fill={v.st.color} text-anchor="middle">{v.label}</text>
            {/if}
          {/each}

          <path d={terrainPath} fill="url(#terr)" />
          <path d={corridorPath} fill="none" stroke="#8a6d48" stroke-width="1" stroke-dasharray="2 2" />

          {#each plan.obstacles as o}
            <g transform={`translate(${x(o.d)},${y(o.msl)})`}>
              <path d="M-4,0 L0,-9 L4,0" class="obst" />
            </g>
          {/each}

          {#if freezePath}<path d={freezePath} fill="none" stroke="#3a8fd6" stroke-width="1.6" stroke-dasharray="6 4" />{/if}

          {#each conflicts as c}
            <rect x={x(c.from) - 1} width={Math.max(3, x(c.to) - x(c.from) + 2)} y={PAD.t} height={H - PAD.t - PAD.b} fill={c.level === 'bad' ? 'var(--bad)' : 'var(--warn)'} opacity="0.12" />
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
            <line x1={hover!.px} x2={hover!.px} y1={PAD.t} y2={H - PAD.b} class="cross" />
            <circle cx={x(hoverInfo.s.d)} cy={y(hoverInfo.alt)} r="4" fill="var(--accent)" />
          {/if}
        </svg>
        {#if hoverInfo}
          <div class="tip glass" style:left={`${Math.min(hover!.px + 12, width - 230)}px`}>
            <div class="num"><b>{hoverInfo.s.d.toFixed(1)} NM</b> · planned {Math.round(hoverInfo.alt).toLocaleString()} ft</div>
            <div class="num">Terrain {Math.round(hoverInfo.s.elev).toLocaleString()} ft · highest nearby {Math.round(hoverInfo.s.max).toLocaleString()} ft</div>
            <div class="num" class:bad={hoverInfo.clr < 500} class:warn={hoverInfo.clr >= 500 && hoverInfo.clr < 1000}>Clearance {Math.round(hoverInfo.clr).toLocaleString()} ft</div>
            {#each hoverInfo.inside as c}<div class="faint">{c.name} · {c.lo_agl ? `${c.lo} AGL` : c.lo === 0 ? 'SFC' : c.lo.toLocaleString()}–{c.hi >= 99999 ? 'UNL' : c.hi.toLocaleString()}</div>{/each}
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
    height: 30px;
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
  }
  .obst {
    fill: none;
    stroke: var(--text);
    stroke-width: 1.4;
  }
  .grab {
    stroke: transparent;
    stroke-width: 14;
    cursor: ns-resize;
  }
  .cruise {
    font-size: 11px;
    font-weight: 700;
    fill: var(--accent);
    paint-order: stroke;
    stroke: var(--surface-solid);
    stroke-width: 3px;
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
    stroke: var(--text-2);
    stroke-width: 1;
  }
  .tip {
    position: absolute;
    top: 8px;
    padding: 8px 10px;
    font-size: 11.5px;
    pointer-events: none;
    width: 218px;
    border-radius: 10px;
    line-height: 1.5;
  }
  .bad {
    color: var(--bad);
    font-weight: 650;
  }
  .warn {
    color: var(--warn);
    font-weight: 650;
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
