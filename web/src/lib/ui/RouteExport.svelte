<script lang="ts">
  // Send the route elsewhere without filing anything: FltPlan Go (scan a QR code of the route
  // string), FltPlan.com (each flight plan field ready to copy; saved, not filed, plans sync to
  // FltPlan Go), and files: Garmin .fpl for avionics / Garmin Pilot / ForeFlight, GPX and KML.
  import { Copy, Check, ExternalLink, FileDown, Info, Tablet, Globe, ChevronDown, ChevronRight } from '@lucide/svelte'
  import qrcode from 'qrcode-generator'
  import Modal from './Modal.svelte'
  import { route } from '../stores/route.svelte'
  import { plan } from '../stores/plan.svelte'
  import { aircraft } from '../stores/aircraft.svelte'
  import { fltplanFields, fltplanGo, toFpl, toGpx, toKml } from '../io/flightplan'
  import { copyText, openExternal, saveFile, slug } from '../io/files'

  let { onclose }: { onclose: () => void } = $props()

  const ac = $derived(aircraft.active)
  const f = $derived(fltplanFields(route.waypoints, route.cruise, ac, plan.log?.totalTime))
  const go = $derived(fltplanGo(route.waypoints))
  const name = $derived(f.destination ? `${f.departure} to ${f.destination}` : f.departure || 'route')

  /** the QR code as SVG path data (dark modules) with a 2-module quiet zone */
  const qr = $derived.by(() => {
    try {
      const q = qrcode(0, 'M')
      q.addData(go.route)
      q.make()
      const n = q.getModuleCount()
      let d = ''
      for (let r = 0; r < n; r++) for (let c = 0; c < n; c++) if (q.isDark(r, c)) d += `M${c + 2} ${r + 2}h1v1h-1z`
      return { n: n + 4, d }
    } catch {
      return null
    }
  })

  let copied = $state('')
  let saved = $state('')
  let error = $state('')
  let fieldsOpen = $state(false)

  async function copy(key: string, text: string) {
    if (await copyText(text)) {
      copied = key
      setTimeout(() => copied === key && (copied = ''), 1500)
    }
  }

  async function download(kind: 'fpl' | 'gpx' | 'kml') {
    error = ''
    try {
      const wps = $state.snapshot(route.waypoints)
      const where =
        kind === 'fpl'
          ? await saveFile(`${slug(name)}.fpl`, toFpl(wps, name), 'application/xml', 'Garmin flight plan')
          : kind === 'gpx'
            ? await saveFile(`${slug(name)}.gpx`, toGpx(wps, name), 'application/gpx+xml', 'GPX route')
            : await saveFile(`${slug(name)}.kml`, toKml(go.custom, wps, name), 'application/vnd.google-earth.kml+xml', 'KML')
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

<Modal title="Export route" subtitle={`${name} · nothing is filed with the FAA`} width={560} {onclose}>
  <section class="go">
    {#if qr}
      <svg class="qr" viewBox={`0 0 ${qr.n} ${qr.n}`} shape-rendering="crispEdges" role="img" aria-label="QR code of the route">
        <rect width={qr.n} height={qr.n} fill="#fff" />
        <path d={qr.d} fill="#14161a" />
      </svg>
    {/if}
    <div class="gobody">
      <h3><Tablet size={15} /> FltPlan Go</h3>
      <ol>
        <li>Point your iPad or iPhone camera at the code and tap Copy.</li>
        <li>In FltPlan Go, start a new flight plan and paste the route. Save it: there's no need to file.</li>
      </ol>
      <div class="full">
        <code>{go.route}</code>
        <button class="btn ghost ico" title="Copy the route" onclick={() => copy('go', go.route)}>{#if copied === 'go'}<Check size={13} />{:else}<Copy size={13} />{/if}</button>
      </div>
      {#if go.custom.length}<p class="faint small">Lat/long points are written as coordinates (3430N/11230W).</p>{/if}
    </div>
  </section>

  <section>
    <button class="fold" onclick={() => (fieldsOpen = !fieldsOpen)}>
      {#if fieldsOpen}<ChevronDown size={14} />{:else}<ChevronRight size={14} />{/if} FltPlan.com form fields
    </button>
    {#if fieldsOpen}
      <p class="faint blurb">Fill FltPlan.com's flight plan form from these values and press Save, not File: saved plans sync to FltPlan Go too.</p>
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
      <div class="btns">
        <button class="btn small ghost" onclick={() => copy('all', allText)}>{#if copied === 'all'}<Check size={13} /> Copied{:else}<Copy size={13} /> Copy all{/if}</button>
        <button class="btn small ghost" onclick={() => openExternal('https://www.fltplan.com/')}><ExternalLink size={13} /> Open FltPlan.com</button>
      </div>
    {/if}
  </section>

  <section>
    <h3>Files</h3>
    <div class="files">
      <button class="file" onclick={() => download('fpl')}>
        <FileDown size={18} />
        <b>Garmin .fpl</b>
        <span class="faint">Avionics SD card / Flight Stream, Garmin Pilot, ForeFlight, SkyVector</span>
      </button>
      <button class="file" onclick={() => download('gpx')}>
        <FileDown size={18} />
        <b>GPX</b>
        <span class="faint">Dynon, Avidyne, handheld GPS and mapping apps</span>
      </button>
      <button class="file" onclick={() => download('kml')}>
        <Globe size={18} />
        <b>KML</b>
        <span class="faint">Google Earth; also FltPlan Go custom waypoints</span>
      </button>
    </div>
    {#if saved}<p class="ok">{saved}</p>{/if}
    {#if error}<p class="err">{error}</p>{/if}
    <p class="faint note"><Info size={12} /> Check the plan on the receiving device before flight.</p>
  </section>
</Modal>

<style>
  section + section {
    margin-top: 14px;
    padding-top: 12px;
    border-top: 1px solid var(--line);
  }
  .go + section {
    border-top: 0;
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
    grid-template-columns: 1fr 1fr 1fr;
    gap: 8px;
  }
  .go {
    display: flex;
    gap: 14px;
    align-items: flex-start;
    padding: 14px;
    border-radius: 12px;
    background: var(--surface-2);
  }
  .qr {
    width: 150px;
    height: 150px;
    flex: none;
    border-radius: 8px;
  }
  .gobody {
    flex: 1;
    min-width: 0;
  }
  .gobody h3 {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  ol {
    margin: 0 0 8px;
    padding-left: 18px;
    font-size: 12px;
    color: var(--text-2);
    line-height: 1.45;
  }
  .full {
    background: var(--surface-solid);
    margin: 0;
  }
  .fold {
    display: flex;
    align-items: center;
    gap: 6px;
    border: 0;
    background: none;
    padding: 0;
    cursor: pointer;
    font-weight: 600;
    font-size: 13px;
  }
  .btns {
    display: flex;
    gap: 6px;
  }
  .small {
    font-size: 11px;
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
    .go {
      flex-direction: column;
      align-items: center;
    }
  }
</style>
