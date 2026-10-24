<script lang="ts">
  // Send the route elsewhere: FltPlan.com (fields to copy into its flight plan form), Garmin
  // .fpl for avionics / Garmin Pilot / ForeFlight, or GPX.
  import { Copy, Check, ExternalLink, FileDown, Info } from '@lucide/svelte'
  import Modal from './Modal.svelte'
  import { route } from '../stores/route.svelte'
  import { plan } from '../stores/plan.svelte'
  import { aircraft } from '../stores/aircraft.svelte'
  import { fltplanFields, toFpl, toGpx } from '../io/flightplan'
  import { copyText, openExternal, saveFile, slug } from '../io/files'

  let { onclose }: { onclose: () => void } = $props()

  const ac = $derived(aircraft.active)
  const f = $derived(fltplanFields(route.waypoints, route.cruise, ac, plan.log?.totalTime))
  const name = $derived(f.destination ? `${f.departure} to ${f.destination}` : f.departure || 'route')

  let copied = $state('')
  let saved = $state('')
  let error = $state('')

  async function copy(key: string, text: string) {
    if (await copyText(text)) {
      copied = key
      setTimeout(() => copied === key && (copied = ''), 1500)
    }
  }

  async function download(kind: 'fpl' | 'gpx') {
    error = ''
    try {
      const wps = $state.snapshot(route.waypoints)
      const where =
        kind === 'fpl'
          ? await saveFile(`${slug(name)}.fpl`, toFpl(wps, name), 'application/xml', 'Garmin flight plan')
          : await saveFile(`${slug(name)}.gpx`, toGpx(wps, name), 'application/gpx+xml', 'GPX route')
      saved = where ? (where === 'downloaded' ? `${kind.toUpperCase()} downloaded` : `Saved to ${where}`) : ''
    } catch (e) {
      error = (e as Error).message
    }
  }

  const rows = $derived([
    { key: 'departure', label: 'Departure', value: f.departure },
    { key: 'destination', label: 'Destination', value: f.destination },
    { key: 'route', label: 'Route', value: f.route },
    { key: 'altitude', label: 'Altitude', value: f.altitude },
    { key: 'aircraftId', label: 'Aircraft ID', value: f.aircraftId },
    { key: 'aircraftType', label: 'Type', value: f.aircraftType },
    { key: 'tas', label: 'True airspeed', value: f.tas },
    { key: 'ete', label: 'Time en route', value: f.ete },
    { key: 'fuel', label: 'Fuel on board', value: f.fuel },
  ])

  const allText = $derived(rows.filter((r) => r.value).map((r) => `${r.label}: ${r.value}`).join('\n'))
</script>

<Modal title="Export route" subtitle={name} width={560} {onclose}>
  <section>
    <div class="head">
      <h3>FltPlan.com</h3>
      <button class="btn small primary" onclick={() => openExternal('https://www.fltplan.com/')}><ExternalLink size={13} /> Open FltPlan.com</button>
    </div>
    <p class="faint blurb">
      FltPlan.com files from what you type into its flight plan form. Copy each value across, or copy the whole route
      into its route box. Plans you save there sync to FltPlan Go.
    </p>
    <div class="full">
      <code>{f.full}</code>
      <button class="btn small" onclick={() => copy('full', f.full)}>{#if copied === 'full'}<Check size={13} /> Copied{:else}<Copy size={13} /> Copy route{/if}</button>
    </div>
    <dl>
      {#each rows as r}
        {#if r.value}
          <dt>{r.label}</dt>
          <dd>
            <span class="mono">{r.value}</span>
            <button class="btn ghost ico" title={`Copy ${r.label.toLowerCase()}`} onclick={() => copy(r.key, r.value)}>
              {#if copied === r.key}<Check size={13} />{:else}<Copy size={13} />{/if}
            </button>
          </dd>
        {/if}
      {/each}
    </dl>
    <button class="btn small ghost" onclick={() => copy('all', allText)}>{#if copied === 'all'}<Check size={13} /> Copied{:else}<Copy size={13} /> Copy all as text{/if}</button>
  </section>

  <section>
    <h3>Files</h3>
    <div class="files">
      <button class="file" onclick={() => download('fpl')}>
        <FileDown size={18} />
        <b>Garmin .fpl</b>
        <span class="faint">Garmin avionics (SD card / Flight Stream), Garmin Pilot, ForeFlight, SkyVector</span>
      </button>
      <button class="file" onclick={() => download('gpx')}>
        <FileDown size={18} />
        <b>GPX</b>
        <span class="faint">Dynon, Avidyne, handheld GPS units and most mapping apps</span>
      </button>
    </div>
    {#if saved}<p class="ok">{saved}</p>{/if}
    {#if error}<p class="err">{error}</p>{/if}
    <p class="faint note"><Info size={12} /> User waypoints are written as lat/long ({f.full.match(/\d{4}[NS]\d{5}[EW]/)?.[0] ?? '3330N11203W'}). Check the plan on the receiving device before flight.</p>
  </section>
</Modal>

<style>
  section + section {
    margin-top: 18px;
    padding-top: 14px;
    border-top: 1px solid var(--line);
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  h3 {
    margin: 0 0 6px;
    font-size: 14px;
  }
  .blurb {
    margin: 4px 0 10px;
    line-height: 1.45;
  }
  .full {
    display: flex;
    gap: 8px;
    align-items: center;
    padding: 8px 10px;
    border-radius: 9px;
    background: var(--surface-2);
    margin-bottom: 10px;
  }
  .full code {
    flex: 1;
    font-family: var(--mono);
    font-size: 12.5px;
    word-break: break-word;
  }
  dl {
    display: grid;
    grid-template-columns: 120px 1fr;
    gap: 2px 10px;
    margin: 0 0 8px;
    align-items: center;
  }
  dt {
    color: var(--text-3);
    font-size: 12px;
  }
  dd {
    margin: 0;
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .ico {
    width: 28px;
    height: 26px;
    padding: 0;
    justify-content: center;
  }
  .files {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }
  .file {
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
  .file:hover {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .file span {
    font-size: 11px;
    line-height: 1.35;
  }
  .btn.small {
    height: 28px;
    font-size: 12px;
  }
  .ok {
    color: var(--ok);
    font-size: 12px;
    font-weight: 600;
  }
  .err {
    color: var(--bad);
    font-size: 12px;
  }
  .note {
    display: flex;
    gap: 6px;
    align-items: flex-start;
    font-size: 11px;
    margin-top: 10px;
  }
  @media (max-width: 640px) {
    .files {
      grid-template-columns: 1fr;
    }
  }
</style>
