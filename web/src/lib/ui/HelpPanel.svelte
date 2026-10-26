<script lang="ts">
  // Help: the tour, the setup screen, shortcuts and what's installed.
  import { Sparkles, Rocket, Keyboard, Info, CircleCheck } from '@lucide/svelte'
  import PanelShell from './PanelShell.svelte'
  import { onboarding } from '../stores/onboarding.svelte'
  import { config } from '../stores/config.svelte'
  import { app } from '../stores/app.svelte'
</script>

<PanelShell title="Help" subtitle="Tour, setup and shortcuts">
  <div class="grid">
    <button class="big" onclick={() => { app.panel = 'none'; onboarding.touring = true }}>
      <Sparkles size={18} />
      <b>Tour the screen</b>
      <span class="faint">A minute, one control at a time</span>
    </button>
    <button class="big" onclick={() => { app.panel = 'none'; onboarding.reopen(0) }}>
      <Rocket size={18} />
      <b>Setup screen</b>
      <span class="faint">Walkthrough, chart data, keys, aircraft</span>
    </button>
  </div>

  <div class="section-title"><CircleCheck size={12} /> Go / no-go</div>
  <p class="muted small">
    With a route loaded, the Route panel gives a green (go), yellow (caution) or red (no-go) call for your departure time. It checks the departure, fuel-stop and
    arrival weather (METAR within the hour, then the TAF, then NBM guidance out to about five days) against your personal minimums, crosswind on the best runway
    against the aircraft's limit, the WPC forecast chart and SPC convective outlook areas the route crosses on the day you fly, NOTAMs, strong winds aloft over
    mountains, terrain clearance, TFRs and prohibited / restricted areas, fuel reserve, weight & balance and night. Set your minimums under Settings. It's advice:
    you are the pilot in command.
  </p>

  <div class="section-title"><Keyboard size={12} /> Shortcuts</div>
  <dl>
    <dt><kbd>/</kbd></dt><dd>Search airports, navaids and fixes</dd>
    <dt><kbd>Enter</kbd></dt><dd>Load the route typed in the Route panel</dd>
    <dt><kbd>Shift+Enter</kbd></dt><dd>Add a search result to the route</dd>
    <dt><kbd>R</kbd></dt><dd>Ruler: click points to measure; Space starts a new line (up to 3); Backspace undoes a point; Esc clears</dd>
    <dt><kbd>Ctrl+Z</kbd></dt><dd>Undo a change to the route or aircraft (Ctrl+Shift+Z or Ctrl+Y to redo)</dd>
    <dt><kbd>L</kbd></dt><dd>View only: lock the plan so clicks can't change it</dd>
    <dt><kbd>← →</kbd></dt><dd>On the timeline: move the departure an hour (Shift: a day)</dd>
    <dt><kbd>Esc</kbd></dt><dd>Close dialogs and the tour</dd>
  </dl>

  <div class="section-title"><Info size={12} /> About</div>
  <dl>
    <dt>Version</dt><dd>{config.version || 'unknown'}{config.desktop ? ' (desktop app)' : ''}</dd>
    <dt>Chart data</dt>
    <dd>
      {#if config.data}
        {config.data.cycle ? `FAA cycle ${config.data.cycle}` : 'built locally'}{#if config.dataExpired} · <b class="bad">out of date</b>{:else if config.data.expires} · until {config.data.expires}{/if}
      {:else}
        not installed
      {/if}
    </dd>
  </dl>
  <p class="faint small">
    For planning only. Flightplan is not a substitute for an official briefing (1800wxbrief.com), current charts, NOTAMs or the aircraft's POH.
    Data: FAA NASR, FAA AIS, FAA Digital Obstacle File, aviationweather.gov, OpenStreetMap / OpenFreeMap, Mapzen terrain, Overture Maps, NOAA (NWS, WPC, SPC), Iowa Environmental Mesonet.
  </p>
</PanelShell>

<style>
  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }
  .big {
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
  .big:hover {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .big :global(svg) {
    color: var(--accent);
  }
  .big span {
    font-size: 11px;
  }
  .section-title {
    display: flex;
    align-items: center;
    gap: 5px;
  }
  dl {
    display: grid;
    grid-template-columns: 80px 1fr;
    gap: 6px 10px;
    margin: 0;
    font-size: 12.5px;
  }
  dt {
    color: var(--text-3);
  }
  dd {
    margin: 0;
  }
  kbd {
    font-family: var(--mono);
    font-size: 11px;
    padding: 1px 6px;
    border-radius: 5px;
    border: 1px solid var(--line-strong);
    background: var(--surface-2);
  }
  .bad {
    color: var(--bad);
  }
  .small {
    font-size: 11px;
    line-height: 1.5;
    margin-top: 16px;
  }
</style>
