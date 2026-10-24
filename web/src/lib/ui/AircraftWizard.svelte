<script lang="ts">
  // Step-by-step airframe editor: identity, performance, weights, loading stations, CG envelope,
  // then a review that runs the same checks as importing a file. Used to create aircraft and
  // to edit existing ones.
  import { Plus, Trash2, ClipboardPaste, FileUp, CircleCheck, TriangleAlert, OctagonAlert, ArrowLeft, ArrowRight, Plane } from '@lucide/svelte'
  import { untrack } from 'svelte'
  import Modal from './Modal.svelte'
  import WBChart from './WBChart.svelte'
  import { aircraft, computeWB, PRESETS, type Aircraft, type Station } from '../stores/aircraft.svelte'
  import { normalize, parseTable, DEFAULT_PERF } from '../io/aircraftFile'

  let { edit: editProp, onclose, onimport }: { edit?: Aircraft; onclose: () => void; onimport?: () => void } = $props()
  // the aircraft being edited, as it was when the wizard opened
  const edit = untrack(() => editProp)

  type Row = { id: string; name: string; kind: Station['kind']; arm: number | null; max: number | null }
  type Pt = { arm: number | null; weight: number | null }

  const steps = edit ? ['Identity', 'Performance', 'Weights', 'Stations', 'Envelope', 'Review'] : ['Start', 'Identity', 'Performance', 'Weights', 'Stations', 'Envelope', 'Review']
  let step = $state(0)
  const current = $derived(steps[step])

  // the draft, in form-friendly shapes (numbers may be null while a field is empty)
  let name = $state('')
  let tail = $state('')
  let type = $state('')
  let fuelType = $state<Aircraft['fuelType']>('100LL')
  let lbPerGal = $state<number | null>(6)
  let usableFuel = $state<number | null>(null)
  let perf = $state<Record<keyof Aircraft['perf'], number | null>>({ ...DEFAULT_PERF })
  let emptyWeight = $state<number | null>(null)
  let emptyArm = $state<number | null>(null)
  let maxRamp = $state<number | null>(null)
  let maxTakeoff = $state<number | null>(null)
  let maxLanding = $state<number | null>(null)
  let rows = $state<Row[]>([])
  let env = $state<Pt[]>([])
  let notes = $state('')
  let startedFrom = $state('')

  // moment helper for weighing records that list moment instead of arm
  let moment = $state<number | null>(null)
  let momentPer1000 = $state(true)

  const newId = () => Math.random().toString(36).slice(2, 8)

  function load(src: Omit<Aircraft, 'id' | 'load' | 'tail'>, keepIdentity = false) {
    if (!keepIdentity) {
      name = src.name
      type = src.type
    }
    fuelType = src.fuelType
    lbPerGal = src.lbPerGal
    usableFuel = src.usableFuel
    perf = { ...src.perf }
    emptyWeight = src.wb.emptyWeight
    emptyArm = src.wb.emptyArm
    maxRamp = src.wb.maxRamp
    maxTakeoff = src.wb.maxTakeoff
    maxLanding = src.wb.maxLanding
    rows = src.wb.stations.map((s) => ({ id: s.id, name: s.name, kind: s.kind, arm: s.arm, max: s.max ?? null }))
    env = src.wb.envelope.map(([arm, weight]) => ({ arm, weight }))
    notes = src.notes ?? ''
  }

  if (edit) {
    load($state.snapshot(edit) as Aircraft)
    tail = edit.tail
  }

  function startFrom(i: number | null) {
    if (i === null) {
      startedFrom = 'blank'
      name = ''
      type = ''
      perf = { ...DEFAULT_PERF }
      emptyWeight = emptyArm = maxRamp = maxTakeoff = maxLanding = usableFuel = null
      rows = [
        { id: 'front', name: 'Front seats', kind: 'seat', arm: null, max: null },
        { id: 'rear', name: 'Rear seats', kind: 'seat', arm: null, max: null },
        { id: 'bag', name: 'Baggage', kind: 'baggage', arm: null, max: null },
        { id: 'fuel', name: 'Fuel (usable)', kind: 'fuel', arm: null, max: null },
      ]
      env = [{ arm: null, weight: null }, { arm: null, weight: null }, { arm: null, weight: null }, { arm: null, weight: null }]
    } else {
      startedFrom = PRESETS[i].name
      load(structuredClone(PRESETS[i]))
      // a template's empty weight is never yours: make the user enter their own
      emptyWeight = null
      emptyArm = null
    }
    step = 1
  }

  /** the draft as an aircraft-file object, for the shared validator */
  const draft = $derived({
    name,
    tail,
    type,
    fuelType,
    lbPerGal,
    usableFuel,
    perf,
    notes,
    wb: {
      emptyWeight,
      emptyArm,
      maxRamp: maxRamp ?? maxTakeoff,
      maxTakeoff,
      maxLanding: maxLanding ?? maxTakeoff,
      stations: rows.filter((r) => r.name.trim() || r.arm !== null).map((r) => ({ id: r.id, name: r.name, kind: r.kind, arm: r.arm, ...(r.max ? { max: r.max } : {}) })),
      envelope: env.filter((p) => p.arm !== null && p.weight !== null).map((p) => [p.arm, p.weight]),
    },
  })
  const check = $derived(normalize($state.snapshot(draft)))

  // chart preview with a sample loading (170 lb in the first seat, full fuel, landing with half)
  const preview = $derived.by(() => {
    const pts = env.filter((p) => p.arm && p.weight) as { arm: number; weight: number }[]
    if (pts.length < 3) return null
    const stations = rows.filter((r) => r.arm !== null).map((r) => ({ id: r.id, name: r.name, kind: r.kind, arm: r.arm!, max: r.max ?? undefined }))
    const loadMap: Record<string, number> = {}
    const seat = stations.find((s) => s.kind === 'seat')
    for (const s of stations) loadMap[s.id] = s.kind === 'fuel' ? (s.max ?? usableFuel ?? 0) : s === seat ? 170 : 0
    const ac: Aircraft = {
      id: 'preview', name, tail, type, fuelType, lbPerGal: lbPerGal ?? 6,
      perf: { ...DEFAULT_PERF, ...Object.fromEntries(Object.entries(perf).filter(([, v]) => v !== null)) },
      usableFuel: usableFuel ?? 0,
      wb: { emptyWeight: emptyWeight ?? 0, emptyArm: emptyArm ?? 0, maxRamp: maxRamp ?? maxTakeoff ?? 0, maxTakeoff: maxTakeoff ?? 0, maxLanding: maxLanding ?? maxTakeoff ?? 0, stations, envelope: pts.map((p) => [p.arm, p.weight]) },
      load: loadMap,
    }
    return { ac, wb: computeWB(ac, (usableFuel ?? 0) / 2) }
  })

  // problems that belong to the step on screen, so Next can say what's missing
  const stepIssues = $derived.by(() => {
    const e: string[] = []
    if (current === 'Identity') {
      if (!name.trim()) e.push('Give the aircraft a name')
      if (!usableFuel || usableFuel <= 0) e.push('Enter the usable fuel')
    }
    if (current === 'Performance' && (!perf.tas || perf.tas <= 0)) e.push('Enter the cruise true airspeed')
    if (current === 'Weights') {
      if (!emptyWeight) e.push('Enter the basic empty weight from your weight & balance record')
      if (emptyArm === null) e.push('Enter the empty-weight arm (or its moment)')
      if (!maxTakeoff) e.push('Enter the maximum takeoff weight')
    }
    if (current === 'Stations') {
      if (!rows.some((r) => r.kind === 'fuel' && r.arm !== null)) e.push('Add a fuel station with its arm')
      if (rows.some((r) => r.name.trim() && r.arm === null)) e.push('Every station needs an arm')
    }
    if (current === 'Envelope' && env.filter((p) => p.arm !== null && p.weight !== null).length < 3) e.push('The envelope needs at least 3 points')
    return e
  })

  function applyMoment() {
    if (moment && emptyWeight) emptyArm = Math.round(((moment * (momentPer1000 ? 1000 : 1)) / emptyWeight) * 100) / 100
  }

  let pasteStations = $state('')
  let pasteEnv = $state('')
  let showPasteStations = $state(false)
  let showPasteEnv = $state(false)

  function importStations() {
    const out: Row[] = []
    for (const cells of parseTable(pasteStations)) {
      const nums = cells.map((c) => Number(c.replace(/,/g, ''))).map((n, i) => (cells[i] !== '' && Number.isFinite(n) ? n : null))
      const label = cells.find((_, i) => nums[i] === null) ?? `Station ${out.length + 1}`
      const values = nums.filter((n): n is number => n !== null)
      if (!values.length) continue
      const kind: Station['kind'] = /fuel|tank/i.test(label) ? 'fuel' : /bag|cargo/i.test(label) ? 'baggage' : /seat|pilot|pass|row|crew/i.test(label) ? 'seat' : 'other'
      out.push({ id: newId(), name: label, kind, arm: values[0], max: values[1] ?? null })
    }
    if (out.length) {
      rows = out
      pasteStations = ''
      showPasteStations = false
    }
  }

  function importEnvelope() {
    const out: Pt[] = []
    for (const cells of parseTable(pasteEnv)) {
      const v = cells.map((c) => Number(c.replace(/,/g, ''))).filter((n) => Number.isFinite(n))
      if (v.length < 2) continue
      // arms are tens of inches, weights hundreds+ of pounds: accept either column order
      const [a, w] = v[0] > v[1] ? [v[1], v[0]] : [v[0], v[1]]
      out.push({ arm: a, weight: w })
    }
    if (out.length >= 3) {
      env = out
      pasteEnv = ''
      showPasteEnv = false
    }
  }

  function save() {
    const r = check
    if (!r.aircraft) return
    const a = r.aircraft
    if (edit) {
      a.id = edit.id
      // keep this flight's loading for stations that still exist
      for (const s of a.wb.stations) if (edit.load[s.id] !== undefined) a.load[s.id] = edit.load[s.id]
    }
    aircraft.upsert(a)
    onclose()
  }

  const perfFields: { k: keyof Aircraft['perf']; label: string; unit: string; step?: number; hint?: string }[] = [
    { k: 'tas', label: 'Cruise true airspeed', unit: 'kt', hint: 'At your usual cruise power and altitude' },
    { k: 'gph', label: 'Cruise fuel flow', unit: 'gph', step: 0.1 },
    { k: 'climbFpm', label: 'Climb rate', unit: 'fpm', hint: 'Average to cruise altitude, not sea-level best' },
    { k: 'climbTas', label: 'Climb speed', unit: 'kt TAS' },
    { k: 'climbGph', label: 'Climb fuel flow', unit: 'gph', step: 0.1 },
    { k: 'descentFpm', label: 'Descent rate', unit: 'fpm' },
    { k: 'descentTas', label: 'Descent speed', unit: 'kt TAS' },
    { k: 'descentGph', label: 'Descent fuel flow', unit: 'gph', step: 0.1 },
    { k: 'taxiGal', label: 'Start, taxi & runup fuel', unit: 'gal', step: 0.1 },
    { k: 'reserveMin', label: 'Fuel reserve', unit: 'min', step: 5, hint: '30 min day VFR, 45 min night VFR / IFR (FAR 91.151/91.167)' },
    { k: 'maxXwind', label: 'Max demonstrated crosswind', unit: 'kt' },
  ]
</script>

<Modal title={edit ? `Edit ${edit.tail || edit.name}` : 'New aircraft'} subtitle={startedFrom && startedFrom !== 'blank' ? `Starting from the ${startedFrom} template` : 'Performance and weight & balance, step by step'} width={640} {onclose}>
  <ol class="steps">
    {#each steps as s, i}
      <li class:on={i === step} class:done={i < step}>
        <button disabled={i > step && !(edit || startedFrom)} onclick={() => (step = i)}>{s}</button>
      </li>
    {/each}
  </ol>

  {#if current === 'Start'}
    <p class="lead">Pick the closest template. You'll replace its numbers with your own airplane's in the next steps.</p>
    <div class="cards">
      {#each PRESETS as p, i}
        {#if p.type !== 'ZZZZ'}
          <button class="card" onclick={() => startFrom(i)}>
            <Plane size={18} />
            <b>{p.name}</b>
            <span class="faint">{p.type} · {p.perf.tas} kt · {p.usableFuel} gal · MTOW {p.wb.maxTakeoff} lb</span>
          </button>
        {/if}
      {/each}
      <button class="card" onclick={() => startFrom(null)}>
        <Plus size={18} />
        <b>Blank</b>
        <span class="faint">Enter everything from the POH</span>
      </button>
      {#if onimport}
        <button class="card" onclick={() => { onclose(); onimport?.() }}>
          <FileUp size={18} />
          <b>Import a file</b>
          <span class="faint">A Flightplan aircraft file from another pilot, club or backup</span>
        </button>
      {/if}
    </div>
  {:else if current === 'Identity'}
    <div class="grid2">
      <label><span>Tail number</span><input class="field" bind:value={tail} placeholder="N12345" spellcheck="false" /></label>
      <label><span>ICAO type</span><input class="field" bind:value={type} placeholder="C172" spellcheck="false" /></label>
      <label class="span2"><span>Name</span><input class="field" bind:value={name} placeholder="Cessna 172S Skyhawk" /></label>
      <label>
        <span>Fuel</span>
        <select class="field" bind:value={fuelType} onchange={() => (lbPerGal = fuelType === 'Jet A' ? 6.7 : 6)}>
          <option>100LL</option><option>UL94</option><option>Mogas</option><option>Jet A</option>
        </select>
      </label>
      <label><span>Fuel weight <small>lb/gal</small></span><input class="field num" type="number" step="0.1" bind:value={lbPerGal} /></label>
      <label><span>Usable fuel <small>gal</small></span><input class="field num" type="number" step="0.5" bind:value={usableFuel} /></label>
      <label class="span2"><span>Notes</span><textarea class="field notes" rows="2" bind:value={notes} placeholder="Equipment, speeds, anything you want on hand"></textarea></label>
    </div>
  {:else if current === 'Performance'}
    <p class="lead">Numbers for the navlog. Use what your airplane actually does at the power settings you fly.</p>
    <div class="grid2">
      {#each perfFields as f}
        <label title={f.hint ?? ''}>
          <span>{f.label} <small>{f.unit}</small></span>
          <input class="field num" type="number" step={f.step ?? 1} min="0" bind:value={perf[f.k]} />
          {#if f.hint}<small class="hint">{f.hint}</small>{/if}
        </label>
      {/each}
    </div>
  {:else if current === 'Weights'}
    <p class="lead">From the airplane's current weight & balance record (not the POH sample). Arms are inches aft of the datum.</p>
    <div class="grid2">
      <label><span>Basic empty weight <small>lb</small></span><input class="field num" type="number" bind:value={emptyWeight} /></label>
      <label><span>Empty-weight arm <small>in</small></span><input class="field num" type="number" step="0.01" bind:value={emptyArm} /></label>
    </div>
    <div class="moment">
      <span class="faint">Record shows a moment instead?</span>
      <input class="field num" type="number" step="0.01" bind:value={moment} placeholder="moment" />
      <label class="inline"><input type="checkbox" bind:checked={momentPer1000} /> ÷1000</label>
      <button class="btn small" disabled={!moment || !emptyWeight} onclick={applyMoment}>Work out arm</button>
    </div>
    <div class="grid3">
      <label><span>Max ramp <small>lb</small></span><input class="field num" type="number" bind:value={maxRamp} placeholder={maxTakeoff ? String(maxTakeoff) : ''} /></label>
      <label><span>Max takeoff <small>lb</small></span><input class="field num" type="number" bind:value={maxTakeoff} /></label>
      <label><span>Max landing <small>lb</small></span><input class="field num" type="number" bind:value={maxLanding} placeholder={maxTakeoff ? String(maxTakeoff) : ''} /></label>
    </div>
  {:else if current === 'Stations'}
    <p class="lead">Everything you load: seats, baggage areas and fuel. Max is pounds, or gallons for fuel.</p>
    <div class="table">
      <div class="th"><span>Station</span><span>Kind</span><span>Arm in</span><span>Max</span><span></span></div>
      {#each rows as r, i (r.id)}
        <div class="tr">
          <input class="field" bind:value={r.name} placeholder="Station name" />
          <select class="field" bind:value={r.kind}>
            <option value="seat">Seat</option><option value="baggage">Baggage</option><option value="fuel">Fuel</option><option value="other">Other</option>
          </select>
          <input class="field num" type="number" step="0.1" bind:value={r.arm} />
          <input class="field num" type="number" bind:value={r.max} placeholder={r.kind === 'fuel' ? 'gal' : 'lb'} />
          <button class="btn ghost ico" onclick={() => rows.splice(i, 1)} aria-label="Remove station"><Trash2 size={14} /></button>
        </div>
      {/each}
    </div>
    <div class="row">
      <button class="btn small" onclick={() => rows.push({ id: newId(), name: '', kind: 'baggage', arm: null, max: null })}><Plus size={13} /> Add station</button>
      <button class="btn small ghost" onclick={() => (showPasteStations = !showPasteStations)}><ClipboardPaste size={13} /> Paste from a spreadsheet</button>
    </div>
    {#if showPasteStations}
      <div class="paste">
        <textarea class="field mono" rows="5" bind:value={pasteStations} placeholder={'Front seats\t37\nRear seats\t73\nBaggage A\t95\t120\nFuel\t48\t53'}></textarea>
        <div class="row">
          <span class="faint small">One station per line: name, arm, max (optional). Tabs, commas or semicolons. Replaces the list above.</span>
          <button class="btn small primary" onclick={importStations} disabled={!pasteStations.trim()}>Use these</button>
        </div>
      </div>
    {/if}
  {:else if current === 'Envelope'}
    <p class="lead">Corner points of the normal-category CG envelope, in order around the outline (POH section 6).</p>
    <div class="env">
      <div>
        <div class="table pts">
          <div class="th"><span>Arm in</span><span>Weight lb</span><span></span></div>
          {#each env as p, i}
            <div class="tr">
              <input class="field num" type="number" step="0.1" bind:value={p.arm} />
              <input class="field num" type="number" bind:value={p.weight} />
              <button class="btn ghost ico" onclick={() => env.splice(i, 1)} aria-label="Remove point"><Trash2 size={14} /></button>
            </div>
          {/each}
        </div>
        <div class="row">
          <button class="btn small" onclick={() => env.push({ arm: null, weight: null })}><Plus size={13} /> Add point</button>
          <button class="btn small ghost" onclick={() => (showPasteEnv = !showPasteEnv)}><ClipboardPaste size={13} /> Paste</button>
        </div>
        {#if showPasteEnv}
          <div class="paste">
            <textarea class="field mono" rows="5" bind:value={pasteEnv} placeholder={'35.0, 1500\n35.0, 1950\n41.0, 2550\n47.3, 2550\n47.3, 1500'}></textarea>
            <div class="row">
              <span class="faint small">Arm and weight per line, either order.</span>
              <button class="btn small primary" onclick={importEnvelope} disabled={!pasteEnv.trim()}>Use these</button>
            </div>
          </div>
        {/if}
      </div>
      <div class="chart">
        {#if preview}
          <WBChart ac={preview.ac} wb={preview.wb} height={240} />
          <p class="faint small">Dots: a sample loading (170 lb pilot, full tanks) at takeoff, and landing after burning half the fuel.</p>
        {:else}
          <p class="faint small empty">The envelope appears here once it has 3 points.</p>
        {/if}
      </div>
    </div>
  {:else if current === 'Review'}
    {#if check.errors.length}
      <div class="status bad"><OctagonAlert size={16} /><div>{#each check.errors as e}<div>{e}</div>{/each}</div></div>
    {:else}
      <div class="status ok"><CircleCheck size={16} /><div>Ready to save</div></div>
    {/if}
    {#if check.warnings.length}
      <div class="status warn"><TriangleAlert size={16} /><div>{#each check.warnings as w}<div>{w}</div>{/each}</div></div>
    {/if}
    <dl class="summary">
      <dt>Aircraft</dt><dd>{tail || 'No tail'} · {name} ({type || 'no type'})</dd>
      <dt>Cruise</dt><dd>{perf.tas ?? '?'} kt at {perf.gph ?? '?'} gph · {usableFuel ?? '?'} gal usable ({perf.gph && usableFuel ? `${(usableFuel / perf.gph).toFixed(1)} h` : '?'} endurance)</dd>
      <dt>Empty</dt><dd>{emptyWeight ?? '?'} lb at {emptyArm ?? '?'} in</dd>
      <dt>Max weights</dt><dd>ramp {maxRamp ?? maxTakeoff ?? '?'} · takeoff {maxTakeoff ?? '?'} · landing {maxLanding ?? maxTakeoff ?? '?'} lb</dd>
      <dt>Stations</dt><dd>{rows.map((r) => r.name).filter(Boolean).join(', ')}</dd>
      <dt>Envelope</dt><dd>{env.filter((p) => p.arm && p.weight).length} points</dd>
    </dl>
    {#if preview}<WBChart ac={preview.ac} wb={preview.wb} height={200} />{/if}
    <p class="faint small">Your aircraft's weighing record and POH govern. Double-check every number before relying on it.</p>
  {/if}

  {#snippet footer()}
    {#if stepIssues.length && current !== 'Start' && current !== 'Review'}<span class="issue faint">{stepIssues[0]}</span>{/if}
    {#if step > 0}<button class="btn" onclick={() => step--}><ArrowLeft size={14} /> Back</button>{/if}
    {#if current === 'Review'}
      <button class="btn primary" disabled={!check.aircraft} onclick={save}><CircleCheck size={15} /> {edit ? 'Save changes' : 'Add aircraft'}</button>
    {:else if current !== 'Start'}
      <button class="btn primary" onclick={() => step++}>Next <ArrowRight size={14} /></button>
    {/if}
  {/snippet}
</Modal>

<style>
  .steps {
    display: flex;
    gap: 4px;
    list-style: none;
    padding: 0;
    margin: 0 0 14px;
    flex-wrap: wrap;
  }
  .steps button {
    border: 0;
    background: var(--surface-2);
    color: var(--text-3);
    font-size: 11px;
    font-weight: 600;
    padding: 4px 9px;
    border-radius: 999px;
    cursor: pointer;
  }
  .steps button:disabled {
    cursor: default;
  }
  .steps .done button {
    color: var(--text-2);
  }
  .steps .on button {
    background: var(--accent-soft);
    color: var(--accent);
  }
  .lead {
    margin: 0 0 12px;
    color: var(--text-2);
    line-height: 1.45;
  }
  .cards {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
    gap: 8px;
  }
  .card {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 4px;
    text-align: left;
    padding: 12px;
    border-radius: 11px;
    border: 1px solid var(--line-strong);
    background: var(--surface-solid);
    cursor: pointer;
  }
  .card:hover {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .card span {
    font-size: 11px;
    line-height: 1.35;
  }
  .grid2,
  .grid3 {
    display: grid;
    gap: 10px;
  }
  .grid2 {
    grid-template-columns: 1fr 1fr;
  }
  .grid3 {
    grid-template-columns: 1fr 1fr 1fr;
    margin-top: 10px;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 12px;
    color: var(--text-2);
    min-width: 0;
  }
  label small {
    color: var(--text-3);
  }
  .hint {
    font-size: 10.5px;
    line-height: 1.3;
  }
  .span2 {
    grid-column: span 2;
  }
  .notes {
    height: auto;
    padding: 8px 10px;
  }
  .moment {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 10px 0 4px;
    font-size: 12px;
    flex-wrap: wrap;
  }
  .moment .field {
    width: 110px;
  }
  .inline {
    flex-direction: row;
    align-items: center;
    gap: 4px;
  }
  .table {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }
  .th,
  .tr {
    display: grid;
    grid-template-columns: 1fr 104px 80px 74px 30px;
    gap: 5px;
    align-items: center;
  }
  .pts .th,
  .pts .tr {
    grid-template-columns: 1fr 1fr 30px;
  }
  .th {
    font-size: 11px;
    font-weight: 600;
    color: var(--text-3);
  }
  .row {
    display: flex;
    gap: 8px;
    align-items: center;
    justify-content: space-between;
    margin-top: 8px;
    flex-wrap: wrap;
  }
  .paste {
    margin-top: 8px;
    padding: 10px;
    border-radius: 10px;
    background: var(--surface-2);
  }
  .paste textarea {
    height: auto;
    padding: 8px;
    font-size: 12px;
  }
  .env {
    display: grid;
    grid-template-columns: 220px 1fr;
    gap: 16px;
  }
  .chart {
    min-width: 0;
  }
  .empty {
    padding: 40px 10px;
    text-align: center;
    border: 1px dashed var(--line-strong);
    border-radius: 10px;
  }
  .ico {
    width: 30px;
    padding: 0;
    justify-content: center;
  }
  .btn.small {
    height: 28px;
    font-size: 12px;
  }
  .small {
    font-size: 11px;
  }
  .status {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    padding: 9px 10px;
    border-radius: 9px;
    margin-bottom: 8px;
    font-weight: 600;
    font-size: 12px;
    line-height: 1.4;
  }
  .status.ok {
    background: rgba(31, 157, 106, 0.09);
    color: var(--ok);
  }
  .status.bad {
    background: rgba(210, 58, 58, 0.09);
    color: var(--bad);
  }
  .status.warn {
    background: rgba(212, 138, 11, 0.1);
    color: var(--warn);
    font-weight: 500;
  }
  .summary {
    display: grid;
    grid-template-columns: 100px 1fr;
    gap: 5px 10px;
    font-size: 12.5px;
    margin: 12px 0;
  }
  .summary dt {
    color: var(--text-3);
  }
  .summary dd {
    margin: 0;
  }
  .issue {
    margin-right: auto;
    font-size: 12px;
  }
  @media (max-width: 640px) {
    .grid2,
    .grid3,
    .env {
      grid-template-columns: 1fr;
    }
    .span2 {
      grid-column: auto;
    }
  }
</style>
