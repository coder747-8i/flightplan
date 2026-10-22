<script lang="ts">
  // CG envelope with the loading's takeoff -> landing path.
  import type { Aircraft, WBResult } from '../stores/aircraft.svelte'

  let { ac, wb, height = 220 }: { ac: Aircraft; wb: WBResult; height?: number } = $props()
  let width = $state(320)
  const P = { l: 44, r: 12, t: 10, b: 28 }

  const pts = $derived([...ac.wb.envelope, ...(ac.wb.utility ?? [])])
  const arms = $derived([...pts.map((p) => p[0]), wb.takeoff.cg, wb.landing.cg, wb.zeroFuel.cg].filter((v) => v > 0))
  const wts = $derived([...pts.map((p) => p[1]), wb.takeoff.weight, wb.landing.weight, wb.zeroFuel.weight].filter((v) => v > 0))
  const x0 = $derived(Math.floor(Math.min(...arms) - 1))
  const x1 = $derived(Math.ceil(Math.max(...arms) + 1))
  const y0 = $derived(Math.floor((Math.min(...wts) - 100) / 100) * 100)
  const y1 = $derived(Math.ceil((Math.max(...wts) + 100) / 100) * 100)
  const X = (a: number) => P.l + ((a - x0) / (x1 - x0)) * (width - P.l - P.r)
  const Y = (w: number) => P.t + (1 - (w - y0) / (y1 - y0)) * (height - P.t - P.b)
  const poly = (p: [number, number][]) => p.map(([a, w]) => `${X(a)},${Y(w)}`).join(' ')
  const xt = $derived(Array.from({ length: x1 - x0 + 1 }, (_, i) => x0 + i).filter((v) => (x1 - x0 > 12 ? v % 2 === 0 : true)))
  const yt = $derived(Array.from({ length: Math.floor((y1 - y0) / 200) + 1 }, (_, i) => y0 + i * 200))
  const ok = $derived(!wb.problems.some((p) => p.includes('envelope')))
</script>

<div class="wrap" bind:clientWidth={width}>
  <svg {width} {height} role="img" aria-label="CG envelope">
    {#each yt as t}
      <line x1={P.l} x2={width - P.r} y1={Y(t)} y2={Y(t)} class="grid" />
      <text x={P.l - 6} y={Y(t) + 3} text-anchor="end" class="ax">{t}</text>
    {/each}
    {#each xt as t}
      <text x={X(t)} y={height - 10} text-anchor="middle" class="ax">{t}</text>
    {/each}
    <polygon points={poly(ac.wb.envelope)} class="env" />
    {#if ac.wb.utility}<polygon points={poly(ac.wb.utility)} class="util" />{/if}
    <line x1={X(wb.takeoff.cg)} y1={Y(wb.takeoff.weight)} x2={X(wb.landing.cg)} y2={Y(wb.landing.weight)} class="path" class:bad={!ok} />
    <line x1={X(wb.landing.cg)} y1={Y(wb.landing.weight)} x2={X(wb.zeroFuel.cg)} y2={Y(wb.zeroFuel.weight)} class="path faint" class:bad={!ok} />
    {#each [[wb.takeoff, 'TO'], [wb.landing, 'LDG'], [wb.zeroFuel, 'ZF']] as [pt, l]}
      {@const p = pt as { cg: number; weight: number }}
      <circle cx={X(p.cg)} cy={Y(p.weight)} r="4.5" class="pt" class:bad={!ok} />
      <text x={X(p.cg) + 7} y={Y(p.weight) + 4} class="lbl">{l}</text>
    {/each}
  </svg>
  <div class="axis-title faint">CG (inches aft of datum) · weight (lb)</div>
</div>

<style>
  .wrap {
    width: 100%;
  }
  svg {
    display: block;
  }
  .grid {
    stroke: var(--line);
  }
  .ax {
    font-size: 10px;
    fill: var(--text-3);
  }
  .env {
    fill: rgba(31, 157, 106, 0.12);
    stroke: var(--ok);
    stroke-width: 1.6;
  }
  .util {
    fill: none;
    stroke: var(--ok);
    stroke-dasharray: 4 3;
    stroke-width: 1;
  }
  .path {
    stroke: var(--accent);
    stroke-width: 2;
  }
  .path.faint {
    stroke-dasharray: 3 3;
    opacity: 0.6;
  }
  .pt {
    fill: var(--accent);
    stroke: white;
    stroke-width: 1.5;
  }
  .bad {
    stroke: var(--bad);
  }
  .pt.bad {
    fill: var(--bad);
  }
  .lbl {
    font-size: 10px;
    font-weight: 700;
    fill: var(--text-2);
  }
  .axis-title {
    font-size: 10.5px;
    text-align: center;
  }
</style>
