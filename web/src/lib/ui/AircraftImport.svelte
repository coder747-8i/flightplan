<script lang="ts">
  // Import aircraft from files (or pasted text): shows what each entry contains, what was
  // filled in with defaults, and what's missing, before anything is added.
  import { FileUp, CircleCheck, TriangleAlert, OctagonAlert, ClipboardPaste, Download } from '@lucide/svelte'
  import { untrack } from 'svelte'
  import Modal from './Modal.svelte'
  import { aircraft, PRESETS } from '../stores/aircraft.svelte'
  import { parseAircraftFile, templateFile, AIRCRAFT_EXT, type ImportResult } from '../io/aircraftFile'
  import { pickFiles, saveFile } from '../io/files'

  let { initial = [], onclose }: { initial?: ImportResult[]; onclose: () => void } = $props()

  let results = $state<ImportResult[]>(untrack(() => [...initial]))
  let pasted = $state('')
  let dragging = $state(false)
  let added = $state(0)

  const ok = $derived(results.filter((r) => r.aircraft))

  /** an aircraft already in the hangar with the same tail number: importing updates it */
  const existing = (r: ImportResult) => {
    const tail = r.aircraft?.tail.trim().toUpperCase()
    return tail ? aircraft.list.find((a) => a.tail.trim().toUpperCase() === tail) : undefined
  }
  const updates = $derived(ok.filter((r) => existing(r)).length)

  async function readFiles(files: File[]) {
    for (const f of files) results.push(...parseAircraftFile(await f.text(), f.name))
  }

  async function choose() {
    await readFiles(await pickFiles(`${AIRCRAFT_EXT},.json,application/json`, true))
  }

  function usePaste() {
    results.push(...parseAircraftFile(pasted, 'pasted text'))
    pasted = ''
  }

  function addAll() {
    for (const r of ok) {
      const a = r.aircraft!
      const old = existing(r)
      if (old) {
        a.id = old.id
        for (const st of a.wb.stations) if (old.load[st.id] !== undefined) a.load[st.id] = old.load[st.id]
      }
      aircraft.upsert(a)
    }
    added = ok.length
    results = results.filter((r) => !r.aircraft)
    if (!results.length) onclose()
  }

  async function downloadTemplate() {
    const { name: _n, ...rest } = structuredClone(PRESETS[0])
    await saveFile(`aircraft-template${AIRCRAFT_EXT}`, templateFile({ ...rest, name: 'My aircraft', tail: 'N12345', type: 'C172', notes: 'Replace every number with your own airplane’s.' }), 'application/json', 'Flightplan aircraft')
  }
</script>

<Modal title="Import aircraft" subtitle="Flightplan aircraft files (.fpaircraft.json) or JSON in the same format" width={560} {onclose}>
  <div
    class="drop"
    class:dragging
    role="region"
    aria-label="Drop aircraft files here"
    ondragover={(e) => {
      e.preventDefault()
      dragging = true
    }}
    ondragleave={() => (dragging = false)}
    ondrop={(e) => {
      e.preventDefault()
      dragging = false
      readFiles(Array.from(e.dataTransfer?.files ?? []))
    }}
  >
    <FileUp size={22} />
    <div>Drop files here or <button class="link" onclick={choose}>choose files</button></div>
    <div class="faint small">You can pick several at once, or a file with a whole fleet.</div>
  </div>

  <details class="paste">
    <summary><ClipboardPaste size={13} /> Paste aircraft JSON instead</summary>
    <textarea class="field mono" rows="5" bind:value={pasted} placeholder={'{ "format": "flightplan-aircraft", "aircraft": [ … ] }'}></textarea>
    <button class="btn small" disabled={!pasted.trim()} onclick={usePaste}>Read pasted text</button>
  </details>

  {#if added}<div class="status ok"><CircleCheck size={16} /> Added {added} aircraft</div>{/if}

  {#each results as r, i}
    <div class="item" class:bad={!r.aircraft}>
      <div class="head">
        {#if r.aircraft}<CircleCheck size={15} class="okc" />{:else}<OctagonAlert size={15} class="badc" />{/if}
        <b>{r.label}</b>
        {#if r.aircraft}<span class="faint small">{r.aircraft.type} · {r.aircraft.perf.tas} kt · {r.aircraft.wb.stations.length} stations</span>{/if}
        {#if existing(r)}<span class="chip">updates yours</span>{/if}
        <button class="btn ghost small x" onclick={() => results.splice(i, 1)}>Remove</button>
      </div>
      {#each r.errors as e}<div class="msg err">{e}</div>{/each}
      {#each r.warnings as w}<div class="msg warn"><TriangleAlert size={12} /> {w}</div>{/each}
    </div>
  {/each}

  <p class="faint small">
    Need to type up an airframe by hand? <button class="link" onclick={downloadTemplate}><Download size={11} /> Get a template file</button>, fill it in, then import it.
    The format is described in <code>docs/aircraft-format.md</code>.
  </p>

  {#snippet footer()}
    <button class="btn" onclick={onclose}>Close</button>
    <button class="btn primary" disabled={!ok.length} onclick={addAll}>
      {updates === ok.length && updates ? `Update ${updates} aircraft` : `Add ${ok.length || ''} aircraft`}{updates && updates < ok.length ? ` (${updates} updated)` : ''}
    </button>
  {/snippet}
</Modal>

<style>
  .drop {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding: 22px;
    border: 1.5px dashed var(--line-strong);
    border-radius: 12px;
    color: var(--text-2);
    text-align: center;
  }
  .drop.dragging {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .link {
    border: 0;
    background: none;
    padding: 0;
    color: var(--accent);
    font-weight: 600;
    cursor: pointer;
  }
  .paste {
    margin: 10px 0;
  }
  .paste summary {
    cursor: pointer;
    color: var(--text-2);
    font-size: 12px;
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .paste textarea {
    height: auto;
    margin: 8px 0;
    padding: 8px;
    font-size: 12px;
  }
  .item {
    border: 1px solid var(--line-strong);
    border-radius: 10px;
    padding: 9px 10px;
    margin-top: 8px;
  }
  .item.bad {
    border-color: rgba(210, 58, 58, 0.35);
  }
  .head {
    display: flex;
    align-items: center;
    gap: 7px;
  }
  .head :global(.okc) {
    color: var(--ok);
  }
  .head :global(.badc) {
    color: var(--bad);
  }
  .x {
    margin-left: auto;
  }
  .msg {
    font-size: 12px;
    margin-top: 5px;
    line-height: 1.4;
  }
  .err {
    color: var(--bad);
  }
  .warn {
    color: var(--warn);
  }
  .status {
    display: flex;
    gap: 8px;
    align-items: center;
    padding: 9px 10px;
    border-radius: 9px;
    margin-top: 8px;
    font-weight: 600;
    font-size: 12px;
  }
  .status.ok {
    background: rgba(31, 157, 106, 0.09);
    color: var(--ok);
  }
  .small {
    font-size: 11px;
  }
  .btn.small {
    height: 28px;
    font-size: 12px;
  }
</style>
