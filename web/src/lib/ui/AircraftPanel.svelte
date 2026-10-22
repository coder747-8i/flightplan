<script lang="ts">
  import { Plus, Trash2, ChevronDown, CircleCheck, OctagonAlert } from '@lucide/svelte'
  import PanelShell from './PanelShell.svelte'
  import WBChart from './WBChart.svelte'
  import { aircraft, computeWB, PRESETS } from '../stores/aircraft.svelte'
  import { plan } from '../stores/plan.svelte'

  const ac = $derived(aircraft.active)
  const wb = $derived(ac ? computeWB(ac, plan.log?.tripFuel ?? 0) : null)
  let editPerf = $state(false)
  let editWB = $state(false)
  let addOpen = $state(false)

  let envText = $state('')
  $effect(() => {
    if (ac) envText = ac.wb.envelope.map(([a, w]) => `${a}, ${w}`).join('\n')
  })
  function applyEnvelope() {
    if (!ac) return
    const pts = envText
      .split('\n')
      .map((l) => l.split(/[,\s]+/).map(Number))
      .filter((p) => p.length >= 2 && p.every((v) => Number.isFinite(v))) as [number, number][]
    if (pts.length >= 3) ac.wb.envelope = pts.map((p) => [p[0], p[1]])
  }
</script>

<PanelShell title="Aircraft" subtitle="Performance and weight & balance" width={380}>
  {#snippet actions()}
    <button class="btn ghost ico" title="Add aircraft" onclick={() => (addOpen = !addOpen)}><Plus size={16} /></button>
  {/snippet}

  {#if addOpen}
    <div class="add">
      <div class="faint small">Start from a template (typical POH numbers: replace them with your own aircraft's weighing record):</div>
      {#each PRESETS as p, i}
        <button class="btn" onclick={() => { aircraft.addPreset(i); addOpen = false }}>{p.name}</button>
      {/each}
    </div>
  {/if}

  <div class="pick">
    <select class="field" bind:value={aircraft.activeId}>
      {#each aircraft.list as a}<option value={a.id}>{a.tail || 'No tail'} · {a.name}</option>{/each}
    </select>
    {#if ac && aircraft.list.length > 1}
      <button class="btn ghost ico" title="Delete this aircraft" onclick={() => ac && aircraft.remove(ac.id)}><Trash2 size={15} /></button>
    {/if}
  </div>

  {#if ac && wb}
    <div class="grid2">
      <label><span class="faint">Tail number</span><input class="field" bind:value={ac.tail} /></label>
      <label><span class="faint">Type (ICAO)</span><input class="field" bind:value={ac.type} /></label>
      <label class="span2"><span class="faint">Name</span><input class="field" bind:value={ac.name} /></label>
    </div>

    <div class="section-title">Loading for this flight</div>
    {#each ac.wb.stations as s}
      <label class="load">
        <span>{s.name}{#if s.max}<small class="faint"> · max {s.max} {s.kind === 'fuel' ? 'gal' : 'lb'}</small>{/if}</span>
        <span class="inp">
          <input class="field num" type="number" min="0" step={s.kind === 'fuel' ? 1 : 5} bind:value={ac.load[s.id]} />
          <span class="unit faint">{s.kind === 'fuel' ? 'gal' : 'lb'}</span>
        </span>
      </label>
    {/each}

    <div class="status" class:bad={wb.problems.length}>
      {#if wb.problems.length}
        <OctagonAlert size={16} />
        <div>{#each wb.problems as p}<div>{p}</div>{/each}</div>
      {:else}
        <CircleCheck size={16} /> <div>Within limits at takeoff and landing</div>
      {/if}
    </div>

    <WBChart {ac} {wb} />

    <table class="wb">
      <thead><tr><th></th><th>Weight</th><th>CG</th></tr></thead>
      <tbody>
        <tr><td>Ramp</td><td class="num">{Math.round(wb.ramp.weight)}</td><td class="num">{wb.ramp.cg.toFixed(2)}</td></tr>
        <tr><td>Takeoff</td><td class="num"><b>{Math.round(wb.takeoff.weight)}</b> <small class="faint">/ {ac.wb.maxTakeoff}</small></td><td class="num"><b>{wb.takeoff.cg.toFixed(2)}</b></td></tr>
        <tr><td>Landing <small class="faint">(−{(plan.log?.tripFuel ?? 0).toFixed(1)} gal)</small></td><td class="num">{Math.round(wb.landing.weight)}</td><td class="num">{wb.landing.cg.toFixed(2)}</td></tr>
        <tr><td>Zero fuel</td><td class="num">{Math.round(wb.zeroFuel.weight)}</td><td class="num">{wb.zeroFuel.cg.toFixed(2)}</td></tr>
      </tbody>
    </table>

    <button class="disclose" onclick={() => (editPerf = !editPerf)}>
      <ChevronDown size={14} style={`transform: rotate(${editPerf ? 0 : -90}deg)`} /> Performance
    </button>
    {#if editPerf}
      <div class="grid3">
        <label><span class="faint">Cruise TAS kt</span><input class="field num" type="number" bind:value={ac.perf.tas} /></label>
        <label><span class="faint">Cruise GPH</span><input class="field num" type="number" step="0.1" bind:value={ac.perf.gph} /></label>
        <label><span class="faint">Usable gal</span><input class="field num" type="number" bind:value={ac.usableFuel} /></label>
        <label><span class="faint">Climb FPM</span><input class="field num" type="number" bind:value={ac.perf.climbFpm} /></label>
        <label><span class="faint">Climb TAS</span><input class="field num" type="number" bind:value={ac.perf.climbTas} /></label>
        <label><span class="faint">Climb GPH</span><input class="field num" type="number" step="0.1" bind:value={ac.perf.climbGph} /></label>
        <label><span class="faint">Descent FPM</span><input class="field num" type="number" bind:value={ac.perf.descentFpm} /></label>
        <label><span class="faint">Descent TAS</span><input class="field num" type="number" bind:value={ac.perf.descentTas} /></label>
        <label><span class="faint">Descent GPH</span><input class="field num" type="number" step="0.1" bind:value={ac.perf.descentGph} /></label>
        <label><span class="faint">Taxi gal</span><input class="field num" type="number" step="0.1" bind:value={ac.perf.taxiGal} /></label>
        <label><span class="faint">Reserve min</span><input class="field num" type="number" step="5" bind:value={ac.perf.reserveMin} /></label>
        <label><span class="faint">Max xwind kt</span><input class="field num" type="number" bind:value={ac.perf.maxXwind} /></label>
        <label><span class="faint">Fuel</span>
          <select class="field" bind:value={ac.fuelType} onchange={() => ac && (ac.lbPerGal = ac.fuelType === 'Jet A' ? 6.7 : 6)}>
            <option>100LL</option><option>UL94</option><option>Mogas</option><option>Jet A</option>
          </select>
        </label>
        <label><span class="faint">lb / gal</span><input class="field num" type="number" step="0.1" bind:value={ac.lbPerGal} /></label>
      </div>
    {/if}

    <button class="disclose" onclick={() => (editWB = !editWB)}>
      <ChevronDown size={14} style={`transform: rotate(${editWB ? 0 : -90}deg)`} /> Weight & balance data
    </button>
    {#if editWB}
      <div class="grid3">
        <label><span class="faint">Empty weight</span><input class="field num" type="number" bind:value={ac.wb.emptyWeight} /></label>
        <label><span class="faint">Empty arm</span><input class="field num" type="number" step="0.01" bind:value={ac.wb.emptyArm} /></label>
        <label><span class="faint">Max ramp</span><input class="field num" type="number" bind:value={ac.wb.maxRamp} /></label>
        <label><span class="faint">Max takeoff</span><input class="field num" type="number" bind:value={ac.wb.maxTakeoff} /></label>
        <label><span class="faint">Max landing</span><input class="field num" type="number" bind:value={ac.wb.maxLanding} /></label>
      </div>
      <div class="section-title">Stations</div>
      {#each ac.wb.stations as s, i}
        <div class="station">
          <input class="field" bind:value={s.name} />
          <input class="field num" type="number" step="0.1" bind:value={s.arm} title="Arm (in)" />
          <input class="field num" type="number" bind:value={s.max} placeholder="max" title="Max lb (gal for fuel)" />
          <button class="btn ghost ico" onclick={() => ac && ac.wb.stations.splice(i, 1)} aria-label="Remove station"><Trash2 size={13} /></button>
        </div>
      {/each}
      <button class="btn small" onclick={() => ac && ac.wb.stations.splice(ac.wb.stations.length - 1, 0, { id: Math.random().toString(36).slice(2, 7), name: 'New station', arm: 0, kind: 'baggage' })}><Plus size={13} /> Add station</button>
      <div class="section-title">Normal category envelope (arm, weight per line)</div>
      <textarea class="field mono env" rows="6" bind:value={envText} onblur={applyEnvelope}></textarea>
    {/if}
    <p class="faint small">Your aircraft's actual weighing record and POH govern. Templates are typical values only.</p>
  {/if}
</PanelShell>

<style>
  .ico {
    width: 30px;
    padding: 0;
    justify-content: center;
  }
  .add {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 10px;
    background: var(--surface-2);
    border-radius: 10px;
    margin-bottom: 10px;
  }
  .pick {
    display: flex;
    gap: 6px;
    margin-bottom: 10px;
  }
  .grid2,
  .grid3 {
    display: grid;
    gap: 8px;
  }
  .grid2 {
    grid-template-columns: 1fr 1fr;
  }
  .grid3 {
    grid-template-columns: 1fr 1fr 1fr;
    margin-top: 6px;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 3px;
    font-size: 11px;
    min-width: 0;
  }
  .span2 {
    grid-column: span 2;
  }
  .load {
    flex-direction: row;
    justify-content: space-between;
    align-items: center;
    gap: 10px;
    font-size: 12.5px;
    padding: 3px 0;
  }
  .inp {
    display: flex;
    align-items: center;
    gap: 5px;
  }
  .inp .field {
    width: 84px;
    text-align: right;
  }
  .unit {
    width: 20px;
    font-size: 11px;
  }
  .status {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    padding: 9px 10px;
    border-radius: 9px;
    margin: 10px 0;
    background: rgba(31, 157, 106, 0.09);
    color: var(--ok);
    font-weight: 600;
    font-size: 12px;
  }
  .status.bad {
    background: rgba(210, 58, 58, 0.09);
    color: var(--bad);
  }
  .wb {
    width: 100%;
    border-collapse: collapse;
    margin-top: 8px;
    font-size: 12px;
  }
  .wb th {
    text-align: right;
    font-weight: 600;
    color: var(--text-3);
    font-size: 11px;
  }
  .wb td {
    padding: 4px 0;
    border-bottom: 1px solid var(--line);
  }
  .wb td:not(:first-child) {
    text-align: right;
  }
  .disclose {
    display: flex;
    gap: 6px;
    align-items: center;
    width: 100%;
    border: 0;
    background: none;
    padding: 12px 0 4px;
    font-weight: 650;
    cursor: pointer;
  }
  .station {
    display: grid;
    grid-template-columns: 1fr 70px 64px 30px;
    gap: 5px;
    margin-bottom: 5px;
  }
  .small {
    font-size: 11px;
  }
  .btn.small {
    height: 28px;
    font-size: 12px;
  }
  .env {
    height: auto;
    padding: 8px;
    font-size: 12px;
  }
</style>
