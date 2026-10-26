<script lang="ts">
  import { X, Plus, Navigation, Phone, Globe, Fuel, Radio, PlaneLanding, Info, CloudSun, Building2, FileWarning } from '@lucide/svelte'
  import NotamList from './NotamList.svelte'
  import { config } from '../stores/config.svelte'
  import { parse as parseNotams, type Notam } from '../wx/notams'
  import { api } from '../api'
  import { app } from '../stores/app.svelte'
  import { route } from '../stores/route.svelte'
  import { mapRef } from '../chart/mapRef'
  import { fltCatColor, fmtAge } from '../wx/format'
  import FboList from './FboList.svelte'
  import WxText from '../wx/WxText.svelte'
  import { expand } from '../wx/decode'

  let tab = $state<'info' | 'runways' | 'freqs' | 'wx' | 'fbo' | 'notams'>('info')
  let notams = $state<{ items: Notam[]; errors: string[] } | null>(null)
  let notamsFor = ''
  let data = $state<any>(null)
  let error = $state('')
  let metar = $state<any>(null)
  let taf = $state<any>(null)

  const sel = $derived(app.selection)

  $effect(() => {
    const s = app.selection
    if (s?.kind !== 'airport') return
    data = null
    error = ''
    metar = taf = null
    notams = null
    notamsFor = ''
    api.airport(s.id).then(
      (d) => {
        data = d
        const wxId = d.icao || (d.id.length === 3 ? `K${d.id}` : d.id)
        api.metar([wxId]).then((m) => (metar = m[0] ?? null)).catch(() => {})
        api.taf([wxId]).then((t) => (taf = t[0] ?? null)).catch(() => {})
      },
      (e) => (error = String(e)),
    )
  })

  const title = $derived(data ? data.icao || data.id : '')

  // NOTAMs load when their tab opens
  $effect(() => {
    if (tab !== 'notams' || !data || !config.notams || notamsFor === title) return
    notamsFor = title
    const id = title
    api
      .notams({ locations: [data.icao || (data.id.length === 3 ? `K${data.id}` : data.id)] })
      .then((v) => notamsFor === id && (notams = parseNotams(v)))
      .catch((e) => notamsFor === id && (notams = { items: [], errors: [String(e.message ?? e)] }))
  })

  // an airport in the middle of the route can be a fuel stop
  const stopIndex = $derived(data ? route.waypoints.findIndex((w, i) => i > 0 && i < route.waypoints.length - 1 && w.kind === 'airport' && (w.id === title || w.id === data.id)) : -1)
  const groupedFreqs = $derived.by(() => {
    if (!data) return []
    const order = ['ATIS', 'D-ATIS', 'AWOS', 'ASOS', 'CTAF', 'UNICOM', 'CD/P', 'GND/P', 'LCL/P', 'LCL/S', 'APCH/P DEP/P', 'APCH/P', 'DEP/P', 'CLASS B', 'CLASS C', 'TRSA']
    const civ = data.freqs.filter((f: any) => parseFloat(f.freq) < 137 && parseFloat(f.freq) > 108)
    return civ.sort((a: any, b: any) => {
      const ia = order.findIndex((o) => a.use.startsWith(o))
      const ib = order.findIndex((o) => b.use.startsWith(o))
      return (ia < 0 ? 99 : ia) - (ib < 0 ? 99 : ib)
    })
  })

  const GROUPS: [string, string][] = [
    ['caution', 'Cautions'], ['ppr', 'Prior permission'], ['fuel', 'Fuel & services'], ['customs', 'Customs & fees'],
    ['noise', 'Noise abatement'], ['pattern', 'Traffic pattern'], ['atc', 'ATC & clearance'], ['lighting', 'Lighting'],
    ['runway', 'Runway notes'], ['other', 'Other remarks'],
  ]
  const remarkGroups = $derived(
    data
      ? GROUPS.map(([k, title]) => ({ title, items: data.remarks.filter((r: any) => (r.category || 'other') === k) })).filter((g) => g.items.length)
      : [],
  )

  function useLabel(u: string) {
    return u
      .replace('LCL/P', 'Tower').replace('LCL/S', 'Tower (sec)').replace('GND/P', 'Ground').replace('GND/S', 'Ground (sec)')
      .replace('CD/P', 'Clearance').replace('APCH/P DEP/P', 'Approach/Departure').replace('APCH/P', 'Approach').replace('DEP/P', 'Departure')
      .replace(' IC', '')
  }

  function surfaceName(s: string) {
    const m: Record<string, string> = { ASPH: 'Asphalt', CONC: 'Concrete', TURF: 'Turf', GRVL: 'Gravel', DIRT: 'Dirt', WATER: 'Water', SAND: 'Sand', GRASS: 'Grass' }
    return s.split('-').map((x) => m[x] ?? x).join(' / ')
  }

  function addToRoute(mode: 'append' | 'direct') {
    if (!data) return
    const p = { kind: 'airport' as const, id: title, name: data.name, detail: `${data.city}, ${data.state}`, lat: data.lat, lon: data.lon }
    if (route.locked) return
    if (mode === 'direct') route.replace([p])
    else route.add(p)
    app.panel = 'route'
  }

  function close() {
    app.selection = null
  }

  function flyTo() {
    if (data) mapRef.map?.flyTo({ center: [data.lon, data.lat], zoom: 12 })
  }
</script>

{#if sel}
  <aside class="card glass">
    {#if sel.kind === 'airport'}
      <header>
        <div class="title">
          {#if data}
            <div class="ident">
              <span class="code" style:color={data.towered ? 'var(--blue)' : 'var(--magenta)'}>{title}</span>
              {#if metar?.fltCat}<span class="cat" style:background={fltCatColor(metar.fltCat)}>{metar.fltCat}</span>{/if}
            </div>
            <div class="name">{data.name}</div>
            <div class="faint">{data.city}, {data.state} · {data.use === 'PR' ? 'Private' : 'Public'}{data.towered ? ' · Towered' : ''}</div>
          {:else if error}
            <div class="name">Not found</div>
            <div class="faint">{error}</div>
          {:else}
            <div class="skeleton" style="width: 90px"></div>
            <div class="skeleton" style="width: 200px"></div>
          {/if}
        </div>
        <button class="btn ghost icon" onclick={close} aria-label="Close"><X size={16} /></button>
      </header>

      {#if data}
        <div class="actions">
          <button class="btn primary" disabled={route.locked} onclick={() => addToRoute('append')}><Plus size={15} /> Add to route</button>
          <button class="btn" disabled={route.locked} onclick={() => addToRoute('direct')} title="Start a new route here"><Navigation size={14} /> Start here</button>
          <button class="btn ghost icon" onclick={flyTo} title="Zoom to airport"><PlaneLanding size={15} /></button>
        </div>
        {#if stopIndex >= 0}
          {@const on = !!route.waypoints[stopIndex].fuelStop}
          <button class="stop" class:on disabled={route.locked} onclick={() => route.toggleFuelStop(stopIndex)} title={on ? 'Not a fuel stop' : 'Land here, top off, go on'}>
            <Fuel size={14} /> {on ? 'Fuel stop on your route' : 'Make this a fuel stop'}
          </button>
        {/if}

        <div class="tabs">
          <button class:on={tab === 'info'} onclick={() => (tab = 'info')}><Info size={14} />Info</button>
          <button class:on={tab === 'wx'} onclick={() => (tab = 'wx')}><CloudSun size={14} />Weather</button>
          <button class:on={tab === 'runways'} onclick={() => (tab = 'runways')}><PlaneLanding size={14} />Runways</button>
          <button class:on={tab === 'freqs'} onclick={() => (tab = 'freqs')}><Radio size={14} />Freqs</button>
          <button class:on={tab === 'fbo'} onclick={() => (tab = 'fbo')}><Building2 size={14} />FBO</button>
          <button class:on={tab === 'notams'} onclick={() => (tab = 'notams')}><FileWarning size={14} />NOTAMs</button>
        </div>

        <div class="content scroll">
          {#if tab === 'info'}
            <div class="facts">
              <div><span class="faint">Elevation</span><b class="num">{Math.round(data.elev ?? 0).toLocaleString()} ft</b></div>
              <div><span class="faint">Pattern</span><b class="num">{data.tpa ? `${(+data.tpa).toLocaleString()} ft` : '—'}</b></div>
              <div><span class="faint">Mag var</span><b>{data.magVar || '—'}</b></div>
              <div><span class="faint">CTAF</span><b class="num">{data.freqs.find((f: any) => f.use === 'CTAF')?.freq ?? '—'}</b></div>
              <div><span class="faint">Sectional</span><b>{data.sectional || '—'}</b></div>
              <div><span class="faint">ARTCC</span><b>{data.artcc || '—'}</b></div>
            </div>
            {#if data.fuel.length}
              <div class="section-title"><Fuel size={12} /> Fuel</div>
              <div class="chips">{#each data.fuel as f}<span class="chip">{f.name}</span>{/each}</div>
            {/if}
            <div class="section-title">Services</div>
            <ul class="list">
              {#if data.repair.airframe}<li>Airframe repair: {data.repair.airframe.toLowerCase()}</li>{/if}
              {#if data.repair.powerplant}<li>Powerplant repair: {data.repair.powerplant.toLowerCase()}</li>{/if}
              {#if data.oxygen.bottled || data.oxygen.bulk}<li>Oxygen: {[data.oxygen.bottled, data.oxygen.bulk].filter(Boolean).join(', ').toLowerCase()}</li>{/if}
              {#if data.storage.hangar || data.storage.tiedown}<li>Transient storage: {[data.storage.hangar && 'hangars', data.storage.tiedown && 'tie-downs'].filter(Boolean).join(', ')}</li>{/if}
              {#each data.services as s}<li>{s}</li>{/each}
              {#if data.landingFee}<li>Landing fee</li>{/if}
              {#if data.customs}<li>Customs (airport of entry)</li>{/if}
            </ul>
            {#if data.attendance.length}
              <div class="section-title">Attended</div>
              {#each data.attendance as a}<div class="muted">{[a.month, a.day, a.hour].filter(Boolean).join(' · ')}</div>{/each}
            {/if}
            {#if data.contacts.length}
              <div class="section-title">Contacts</div>
              {#each data.contacts as c}
                <div class="contact">
                  <div><b>{c.name}</b> <span class="faint">{c.title.toLowerCase()}</span></div>
                  {#if c.phone}<a href={`tel:${c.phone}`}><Phone size={12} /> {c.phone}</a>{/if}
                </div>
              {/each}
            {/if}
            {#each remarkGroups as g}
              <div class="section-title">{g.title}</div>
              <ul class="list remarks">
                {#each g.items as r}<li title={r.text}>{app.rawWx ? r.text : expand(r.text)}</li>{/each}
              </ul>
            {/each}
          {:else if tab === 'wx'}
            {#if metar}
              <div class="section-title">METAR <span class="faint">· {fmtAge(metar.obsTime)}</span></div>
              <WxText kind="metar" raw={metar.rawOb} />
            {:else}
              <p class="muted">No METAR for this field. Check the Weather panel for nearby stations.</p>
            {/if}
            {#if taf}
              <div class="section-title">TAF</div>
              <WxText kind="taf" raw={taf.rawTAF} />
            {/if}
            {#if data.awos.length}
              <div class="section-title">AWOS / ASOS</div>
              {#each data.awos as w}<div class="muted">{w.type} {w.freq} {w.phone && `· ${w.phone}`}</div>{/each}
            {/if}
          {:else if tab === 'runways'}
            {#each data.runways as r}
              <div class="rwy">
                <div class="rwy-head">
                  <b>{r.id}</b>
                  <span class="num">{r.len.toLocaleString()} × {r.width} ft</span>
                </div>
                <div class="faint">{surfaceName(r.surface)}{r.cond ? ` · ${r.cond.toLowerCase()}` : ''}{r.lighting && r.lighting !== 'NONE' ? ` · lights ${r.lighting.toLowerCase()}` : ''}</div>
                <div class="ends">
                  {#each r.ends as e}
                    <div>
                      <b>{e.id}</b>
                      {#if e.rightTraffic}<span class="chip warn">RP</span>{/if}
                      {#if e.ils}<span class="chip">{e.ils}</span>{/if}
                      {#if e.vgsi}<span class="chip">{e.vgsi}</span>{/if}
                      {#if e.displaced}<span class="faint"> · displaced {e.displaced} ft</span>{/if}
                      {#if e.tdz}<span class="faint"> · TDZE {Math.round(e.tdz)}</span>{/if}
                    </div>
                  {/each}
                </div>
              </div>
            {/each}
          {:else if tab === 'freqs'}
            <table class="freqs">
              <tbody>
                {#each groupedFreqs as f}
                  <tr>
                    <td>{useLabel(f.use)}</td>
                    <td class="num"><b>{f.freq}</b></td>
                    <td class="faint">{f.sector || ''}</td>
                  </tr>
                {/each}
              </tbody>
            </table>
          {:else if tab === 'notams'}
            <NotamList items={notams?.items ?? []} configured={config.notams} errors={notams?.errors ?? []} loading={config.notams && !notams} limit={30} />
          {:else if tab === 'fbo'}
            <FboList airport={data} />
          {/if}
        </div>
      {/if}
    {:else}
      <header>
        <div class="title">
          <div class="name">At this point</div>
          <div class="faint num">{sel.lngLat[1].toFixed(4)}, {sel.lngLat[0].toFixed(4)}</div>
        </div>
        <button class="btn ghost icon" onclick={close} aria-label="Close"><X size={16} /></button>
      </header>
      <div class="content scroll">
        {#each sel.items as it}
          <button class="item" disabled={it.type !== 'airport'} onclick={() => it.id && (app.selection = { kind: 'airport', id: it.id })}>
            <span class="bar" style:background={it.color}></span>
            <span>
              <b>{it.title}</b>
              {#if it.subtitle}<small class="faint">{it.subtitle}</small>{/if}
              {#if it.decode}<WxText kind={it.decode.kind} raw={it.decode.raw} data={it.decode.data} compact />{/if}
            </span>
          </button>
        {/each}
      </div>
    {/if}
  </aside>
{/if}

<style>
  .card {
    position: absolute;
    top: 60px;
    right: 12px;
    width: 360px;
    max-height: calc(100% - 72px);
    display: flex;
    flex-direction: column;
    z-index: 14;
    animation: pop 0.16s ease-out;
  }
  @keyframes pop {
    from { opacity: 0; transform: translateY(-4px); }
  }
  header {
    display: flex;
    justify-content: space-between;
    padding: 14px 12px 6px 16px;
  }
  .ident {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .code {
    font-size: 20px;
    font-weight: 700;
    letter-spacing: 0.02em;
  }
  .cat {
    color: white;
    font-size: 10.5px;
    font-weight: 700;
    padding: 2px 7px;
    border-radius: 6px;
  }
  .name {
    font-weight: 600;
    font-size: 14px;
    margin: 2px 0;
  }
  .icon {
    width: 30px;
    padding: 0;
    justify-content: center;
  }
  .actions {
    display: flex;
    gap: 6px;
    padding: 6px 16px 10px;
  }
  .stop {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0 16px 8px;
    padding: 5px 10px;
    border-radius: 8px;
    border: 1px dashed var(--accent);
    background: none;
    color: var(--accent);
    font-weight: 600;
    font-size: 12px;
    cursor: pointer;
  }
  .stop.on {
    border-style: solid;
    background: var(--accent-soft);
  }
  .tabs {
    display: flex;
    gap: 2px;
    padding: 0 10px;
    border-bottom: 1px solid var(--line);
  }
  .tabs button {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 4px;
    border: 0;
    background: none;
    padding: 8px 2px;
    font-size: 11.5px;
    font-weight: 550;
    color: var(--text-2);
    border-bottom: 2px solid transparent;
    cursor: pointer;
  }
  .tabs button.on {
    color: var(--accent);
    border-bottom-color: var(--accent);
  }
  .content {
    padding: 4px 16px 16px;
    flex: 1;
  }
  .facts {
    display: grid;
    grid-template-columns: 1fr 1fr 1fr;
    gap: 10px;
    margin-top: 12px;
  }
  .facts div {
    display: flex;
    flex-direction: column;
    gap: 1px;
    font-size: 12px;
  }
  .facts b {
    font-size: 13px;
  }
  .section-title {
    display: flex;
    align-items: center;
    gap: 5px;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
  }
  .chip.warn {
    background: rgba(212, 138, 11, 0.15);
    color: var(--warn);
  }
  .list {
    margin: 0;
    padding-left: 16px;
    line-height: 1.6;
  }
  .remarks {
    font-size: 12px;
    color: var(--text-2);
    line-height: 1.45;
  }
  .remarks li {
    margin-bottom: 4px;
  }
  .contact {
    display: flex;
    justify-content: space-between;
    padding: 4px 0;
  }
  .contact a {
    color: var(--blue);
    text-decoration: none;
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .rwy {
    padding: 12px 0;
    border-bottom: 1px solid var(--line);
  }
  .rwy-head {
    display: flex;
    justify-content: space-between;
    font-size: 14px;
  }
  .ends {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-top: 6px;
  }
  .freqs {
    width: 100%;
    border-collapse: collapse;
    margin-top: 8px;
  }
  .freqs td {
    padding: 6px 4px;
    border-bottom: 1px solid var(--line);
  }
  .item {
    width: 100%;
    display: flex;
    gap: 10px;
    align-items: stretch;
    text-align: left;
    border: 0;
    background: none;
    padding: 8px 4px;
    border-radius: 8px;
    cursor: pointer;
  }
  .item:disabled {
    cursor: default;
    color: inherit;
  }
  .item:not(:disabled):hover {
    background: var(--surface-2);
  }
  .item .bar {
    width: 3px;
    border-radius: 2px;
    flex: none;
  }
  .item span:last-child {
    display: flex;
    flex-direction: column;
  }
  .skeleton {
    height: 14px;
    margin: 6px 0;
    border-radius: 4px;
    background: var(--surface-2);
  }
  @media (max-width: 640px) {
    .card {
      left: 8px;
      right: 8px;
      width: auto;
      top: auto;
      bottom: 72px;
      max-height: 60vh;
    }
  }
</style>
