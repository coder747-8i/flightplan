<script lang="ts">
  // Full-page briefing: summary, hazards, navlog, profile, weather, winds aloft, W&B and
  // airport details. Prints cleanly (kneeboard-friendly navlog with blank ATA/fuel columns).
  import { Printer, X, ExternalLink } from '@lucide/svelte'
  import { onMount } from 'svelte'
  import { api } from '../api'
  import { app } from '../stores/app.svelte'
  import { route } from '../stores/route.svelte'
  import { plan } from '../stores/plan.svelte'
  import { aircraft, computeWB } from '../stores/aircraft.svelte'
  import { fmtTime, deg, windAt } from '../nav/navlog'
  import { fltCatColor, fmtAge } from '../wx/format'
  import { mapRef } from '../chart/mapRef'
  import HazardList from './HazardList.svelte'
  import WBChart from './WBChart.svelte'
  import WxText from '../wx/WxText.svelte'
  import { printPage } from '../io/files'

  const log = $derived(plan.log)
  const ac = $derived(aircraft.active)
  const wb = $derived(ac ? computeWB(ac, log?.tripFuel ?? 0) : null)
  const dep = $derived(route.waypoints[0])
  const dest = $derived(route.waypoints[route.waypoints.length - 1])
  const depTime = $derived(plan.departureTime)
  const eta = $derived(log ? new Date(depTime.getTime() + log.totalTime * 60000) : null)

  let mapImg = $state('')
  let wxList = $state<{ id: string; metar: any; taf: any }[]>([])
  let airports = $state<any[]>([])

  const hhmm = (d: Date) => d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
  const zz = (d: Date) => `${String(d.getUTCHours()).padStart(2, '0')}${String(d.getUTCMinutes()).padStart(2, '0')}Z`

  onMount(() => {
    // snapshot the chart framed on the route for the printout, then put the view back
    const m = mapRef.map
    const w = route.waypoints
    if (m && w.length >= 2) {
      const cam = { center: m.getCenter(), zoom: m.getZoom() }
      const lons = w.map((p) => p.lon)
      const lats = w.map((p) => p.lat)
      m.fitBounds([[Math.min(...lons), Math.min(...lats)], [Math.max(...lons), Math.max(...lats)]], { padding: 60, animate: false, maxZoom: 10 })
      m.once('idle', () => {
        m.once('render', () => {
          try {
            mapImg = m.getCanvas().toDataURL('image/jpeg', 0.85)
          } catch {
            /* tainted canvas: skip the image */
          }
          m.jumpTo(cam)
        })
        m.triggerRepaint()
      })
    }
    const ids = route.waypoints.filter((w) => w.kind === 'airport').map((w) => (w.id.length === 3 ? `K${w.id}` : w.id))
    if (ids.length) {
      Promise.all([api.metar(ids), api.taf(ids)]).then(([mm, tt]) => {
        wxList = ids.map((id) => ({ id, metar: mm.find((x) => x.icaoId === id), taf: tt.find((x) => x.icaoId === id) }))
      })
    }
    const ends = [route.waypoints[0], route.waypoints[route.waypoints.length - 1]].filter((w) => w?.kind === 'airport')
    Promise.all(ends.map((w) => api.airport(w.id).catch(() => null))).then((a) => (airports = a.filter(Boolean)))
  })

  // winds aloft at each waypoint, a few altitudes around cruise
  const windAlts = $derived.by(() => {
    const c = route.cruise
    const set = new Set([3000, 6000, 9000, 12000, Math.round(c / 1000) * 1000].filter((a) => a > 0))
    return [...set].sort((a, b) => a - b)
  })

  const keyFreqs = (a: any) => {
    const want = ['ATIS', 'D-ATIS', 'CTAF', 'UNICOM', 'CD/P', 'GND/P', 'LCL/P', 'APCH/P DEP/P', 'APCH/P', 'DEP/P']
    return a.freqs.filter((f: any) => want.includes(f.use) && parseFloat(f.freq) < 137)
  }
  const label = (u: string) => ({ 'LCL/P': 'Tower', 'GND/P': 'Ground', 'CD/P': 'Clearance', 'APCH/P DEP/P': 'App/Dep', 'APCH/P': 'Approach', 'DEP/P': 'Departure' })[u] ?? u

  function notamUrl(id: string) {
    return `https://notams.aim.faa.gov/notamSearch/nsapp.html#/results?designators=${encodeURIComponent(id)}`
  }
</script>

<div class="brief">
  <div class="toolbar noprint">
    <button class="btn" onclick={() => (app.panel = 'none')}><X size={15} /> Back to chart</button>
    <div class="spacer"></div>
    <a class="btn" href="https://www.1800wxbrief.com/" target="_blank" rel="noopener noreferrer"><ExternalLink size={14} /> Official briefing (1800wxbrief)</a>
    <button class="btn primary" onclick={printPage}><Printer size={15} /> Print</button>
  </div>

  {#if !log || !ac}
    <div class="empty">
      <h2>No route yet</h2>
      <p class="muted">Build a route with at least two waypoints to get a navlog and briefing.</p>
    </div>
  {:else}
    <article>
      <header class="hero">
        <div>
          <div class="faint small">FLIGHT BRIEFING · generated {new Date().toLocaleString()}</div>
          <h1>{route.text}</h1>
          <div class="muted">{ac.tail} · {ac.name} · cruise {route.cruise.toLocaleString()} ft · TAS {ac.perf.tas} kt</div>
        </div>
        <div class="kpis">
          <div><span>Departs</span><b>{hhmm(depTime)}</b><small>{zz(depTime)}</small></div>
          <div><span>Arrives</span><b>{eta ? hhmm(eta) : '—'}</b><small>{eta ? zz(eta) : ''}</small></div>
          <div><span>Distance</span><b class="num">{log.totalDist.toFixed(0)}</b><small>NM</small></div>
          <div><span>Time</span><b class="num">{fmtTime(log.totalTime)}</b><small>ETE</small></div>
          <div><span>Fuel</span><b class="num">{(log.tripFuel + log.taxiFuel).toFixed(1)}</b><small>gal incl. taxi</small></div>
          <div class:bad={log.fuelAtLanding < log.reserveFuel}><span>At landing</span><b class="num">{log.fuelAtLanding.toFixed(1)}</b><small>gal ({log.reserveFuel.toFixed(1)} reserve)</small></div>
        </div>
      </header>

      <section>
        <h2>Hazards</h2>
        <HazardList hazards={plan.hazards} />
      </section>

      {#if mapImg}
        <section class="mapshot">
          <img src={mapImg} alt="Route on the chart" />
        </section>
      {/if}

      <section class="break-before">
        <h2>Navigation log</h2>
        <div class="tablewrap">
          <table class="navlog num">
            <thead>
              <tr>
                <th class="l">From → To</th><th>Alt</th><th>TC</th><th>Wind</th><th>WCA</th><th>TH</th><th>Var</th><th>MH</th>
                <th>TAS</th><th>GS</th><th>Dist</th><th>ETE</th><th>ETA</th><th>Fuel</th><th>Rem</th><th class="blank">ATA</th><th class="blank">Act fuel</th>
              </tr>
            </thead>
            <tbody>
              {#each log.rows as r}
                <tr class={r.phase}>
                  <td class="l"><b>{r.from}</b> → <b>{r.to}</b>{#if r.phase !== 'cruise'}<span class="ph">{r.phase}</span>{/if}</td>
                  <td>{r.alt.toLocaleString()}</td>
                  <td>{deg(r.tc)}</td>
                  <td>{r.wind ? `${deg(r.wind.dir)}/${r.wind.spd}` : '—'}</td>
                  <td>{r.wca >= 0 ? '+' : ''}{r.wca.toFixed(0)}</td>
                  <td>{deg(r.th)}</td>
                  <td>{Math.abs(r.variation).toFixed(0)}{r.variation >= 0 ? 'E' : 'W'}</td>
                  <td><b>{deg(r.mh)}</b></td>
                  <td>{r.tas}</td>
                  <td><b>{Math.round(r.gs)}</b></td>
                  <td>{r.dist.toFixed(1)}</td>
                  <td>{fmtTime(r.ete)}</td>
                  <td>{hhmm(r.eta)}</td>
                  <td>{r.fuel.toFixed(1)}</td>
                  <td>{r.fuelRem.toFixed(1)}</td>
                  <td class="blank"></td>
                  <td class="blank"></td>
                </tr>
              {/each}
            </tbody>
            <tfoot>
              <tr>
                <td class="l">Totals</td><td colspan="9"></td><td>{log.totalDist.toFixed(1)}</td><td>{fmtTime(log.totalTime)}</td><td>{eta ? hhmm(eta) : ''}</td><td>{log.tripFuel.toFixed(1)}</td><td>{log.fuelAtLanding.toFixed(1)}</td><td class="blank"></td><td class="blank"></td>
              </tr>
            </tfoot>
          </table>
        </div>
        <p class="faint small">Courses true unless marked. Variation WMM-2025 (east positive, MH = TH − E var). Winds: {plan.windSource === 'windy' ? 'Windy GFS' : 'Open-Meteo GFS'}, interpolated to altitude at each leg's ETA. Taxi {log.taxiFuel} gal not included in the leg fuel.</p>
      </section>

      <section>
        <h2>Winds and temperatures aloft</h2>
        <div class="tablewrap">
          <table class="winds num">
            <thead><tr><th class="l">Waypoint</th>{#each windAlts as a}<th>{a.toLocaleString()}</th>{/each}</tr></thead>
            <tbody>
              {#each route.waypoints as w, i}
                <tr>
                  <td class="l"><b>{w.id}</b></td>
                  {#each windAlts as a}
                    {@const wd = windAt(plan.winds[i], a, depTime)}
                    <td>{wd ? `${deg(wd.dir)}/${wd.spd}` : '—'}{#if wd?.temp != null}<small class="faint"> {wd.temp > 0 ? '+' : ''}{Math.round(wd.temp)}°</small>{/if}</td>
                  {/each}
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      </section>

      <section>
        <h2>Weather</h2>
        {#each wxList as r}
          <div class="stn">
            <div class="head">
              <b>{r.id}</b>
              {#if r.metar?.fltCat}<span class="cat" style:background={fltCatColor(r.metar.fltCat)}>{r.metar.fltCat}</span>{/if}
              {#if r.metar}<span class="faint small">{fmtAge(r.metar.obsTime)}</span>{/if}
              <a class="small" href={notamUrl(r.id)} target="_blank" rel="noopener noreferrer">NOTAMs ↗</a>
            </div>
            {#if r.metar}<WxText kind="metar" raw={r.metar.rawOb} />{:else}<p class="faint">No METAR</p>{/if}
            {#if r.taf}<div class="tafw"><WxText kind="taf" raw={r.taf.rawTAF} /></div>{/if}
          </div>
        {/each}
      </section>

      <section class="two">
        <div>
          <h2>Weight & balance</h2>
          {#if wb}
            <table class="wbt num">
              <thead><tr><th class="l">Item</th><th>Weight</th><th>Arm</th><th>Moment</th></tr></thead>
              <tbody>
                {#each wb.rows as r}<tr><td class="l">{r.name}</td><td>{Math.round(r.weight)}</td><td>{r.arm.toFixed(1)}</td><td>{Math.round(r.moment).toLocaleString()}</td></tr>{/each}
              </tbody>
              <tfoot>
                <tr><td class="l">Takeoff</td><td>{Math.round(wb.takeoff.weight)}</td><td>{wb.takeoff.cg.toFixed(2)}</td><td></td></tr>
                <tr><td class="l">Landing</td><td>{Math.round(wb.landing.weight)}</td><td>{wb.landing.cg.toFixed(2)}</td><td></td></tr>
              </tfoot>
            </table>
            {#each wb.problems as p}<p class="bad small">{p}</p>{/each}
          {/if}
        </div>
        <div>
          {#if wb && ac}<WBChart {ac} {wb} height={240} />{/if}
        </div>
      </section>

      <section class="two">
        {#each airports as a, i}
          <div>
            <h2>{i === 0 ? 'Departure' : 'Destination'}: {a.icao || a.id}</h2>
            <div class="muted">{a.name} · elev {Math.round(a.elev)} ft{a.tpa ? ` · TPA ${(+a.tpa).toLocaleString()}` : ''}</div>
            <table class="kv num">
              <tbody>
                {#each keyFreqs(a) as f}<tr><td class="l">{label(f.use)}</td><td>{f.freq}</td></tr>{/each}
                {#each a.runways as r}<tr><td class="l">RWY {r.id}</td><td>{r.len.toLocaleString()} × {r.width} {r.surface}</td></tr>{/each}
              </tbody>
            </table>
          </div>
        {/each}
      </section>

      <p class="disclaimer">For planning only. Not a substitute for an official weather briefing, current charts, NOTAMs or the aircraft's POH. The pilot in command is responsible for the flight.</p>
    </article>
  {/if}
</div>

<style>
  .brief {
    position: absolute;
    inset: 0;
    z-index: 30;
    background: var(--bg);
    overflow-y: auto;
  }
  .toolbar {
    position: sticky;
    top: 0;
    display: flex;
    gap: 8px;
    padding: 12px 20px;
    background: var(--surface);
    backdrop-filter: var(--blur);
    border-bottom: 1px solid var(--line);
    z-index: 2;
  }
  .toolbar a {
    text-decoration: none;
  }
  .spacer {
    flex: 1;
  }
  .empty {
    max-width: 480px;
    margin: 80px auto;
    text-align: center;
  }
  article {
    max-width: 1080px;
    margin: 0 auto;
    padding: 24px 24px 60px;
  }
  .hero {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding-bottom: 18px;
    border-bottom: 1px solid var(--line);
  }
  h1 {
    margin: 6px 0 4px;
    font-size: 24px;
    letter-spacing: -0.01em;
    font-family: var(--mono);
  }
  h2 {
    font-size: 14px;
    margin: 0 0 10px;
    letter-spacing: -0.005em;
  }
  section {
    padding: 18px 0;
    border-bottom: 1px solid var(--line);
  }
  .kpis {
    display: grid;
    grid-template-columns: repeat(6, 1fr);
    gap: 8px;
  }
  .kpis div {
    background: var(--surface-solid);
    border: 1px solid var(--line);
    border-radius: 10px;
    padding: 10px 12px;
    display: flex;
    flex-direction: column;
  }
  .kpis span {
    font-size: 10.5px;
    color: var(--text-3);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  .kpis b {
    font-size: 19px;
    margin: 2px 0;
  }
  .kpis small {
    font-size: 11px;
    color: var(--text-2);
  }
  .kpis .bad b {
    color: var(--bad);
  }
  .mapshot img {
    width: 100%;
    border-radius: 12px;
    border: 1px solid var(--line);
    display: block;
  }
  .tablewrap {
    overflow-x: auto;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 12px;
  }
  th {
    font-size: 10.5px;
    font-weight: 600;
    color: var(--text-3);
    text-align: right;
    padding: 6px 6px;
    border-bottom: 1px solid var(--line-strong);
    white-space: nowrap;
  }
  td {
    text-align: right;
    padding: 6px 6px;
    border-bottom: 1px solid var(--line);
    white-space: nowrap;
  }
  .l {
    text-align: left;
  }
  .navlog tr.climb td,
  .navlog tr.descent td {
    color: var(--text-2);
  }
  .ph {
    margin-left: 6px;
    font-size: 10px;
    text-transform: uppercase;
    color: var(--text-3);
  }
  .blank {
    min-width: 56px;
    border-left: 1px dashed var(--line-strong);
  }
  tfoot td {
    font-weight: 700;
    border-bottom: 0;
  }
  .stn {
    padding: 8px 0;
  }
  .head {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .cat {
    color: white;
    font-size: 10px;
    font-weight: 700;
    padding: 1px 6px;
    border-radius: 5px;
  }
  .tafw {
    margin-top: 6px;
  }
  .two {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 24px;
  }
  .kv td {
    padding: 4px 6px;
  }
  .small {
    font-size: 11.5px;
  }
  .bad {
    color: var(--bad);
  }
  .disclaimer {
    font-size: 11px;
    color: var(--text-3);
    margin-top: 18px;
  }
  a {
    color: var(--blue);
  }
  @media (max-width: 760px) {
    .kpis {
      grid-template-columns: repeat(3, 1fr);
    }
    .two {
      grid-template-columns: 1fr;
    }
  }
  @media print {
    .brief {
      position: static;
      overflow: visible;
      background: white;
    }
    .noprint {
      display: none !important;
    }
    article {
      padding: 0;
      max-width: none;
    }
    .break-before {
      break-before: page;
    }
    section {
      break-inside: avoid;
    }
    .navlog {
      font-size: 10.5px;
    }
  }
</style>
