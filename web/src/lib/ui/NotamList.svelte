<script lang="ts">
  // NOTAM lists: the route's in the Route panel and briefing, one airport's on its card. Most
  // important first (closures, fuel, navaids, GPS, airspace), each with when it's in effect and
  // plain English; the raw text on request. Without FAA credentials: how to get them.
  import { Ban, Fuel, RadioTower, Hexagon, Diamond, StickyNote, Mail, Settings } from '@lucide/svelte'
  import { app } from '../stores/app.svelte'
  import { window as notamWindow, REQUEST_MAILTO, type Notam } from '../wx/notams'
  import { openExternal } from '../io/files'

  let { items, configured, errors = [], loading = false, limit = 12, print = false }: { items: Notam[]; configured: boolean; errors?: string[]; loading?: boolean; limit?: number; print?: boolean } = $props()
  let all = $state(false)
  const shown = $derived(all || print ? items : items.slice(0, limit))
</script>

{#if !configured}
  <p class="hint">NOTAMs come from the FAA NOTAM Management Service. Access is free but by request: email notams@faa.gov, then paste the client ID and secret they send in Settings.</p>
  {#if !print}
    <div class="links">
      <button class="link" onclick={() => openExternal(REQUEST_MAILTO)}><Mail size={13} /> Email the FAA</button>
      <button class="link" onclick={() => (app.panel = 'settings')}><Settings size={13} /> Add it in Settings</button>
    </div>
  {/if}
{:else}
  {#if errors.length && !items.length}<p class="err">Couldn't get NOTAMs: {errors[0]}</p>{/if}
  {#if loading && !items.length}<p class="faint small">Loading NOTAMs…</p>{:else if !items.length && !errors.length}<p class="faint small">No NOTAMs in effect.</p>{/if}
  <ul>
    {#each shown as n (n.id)}
      {@const k = n.effect.kind}
      <li>
        <span class="ico {k === 'aerodromeClosed' || k === 'runwayClosed' ? 'bad' : k === 'fuelUnavailable' || k === 'navaidOut' || k === 'gps' ? 'warn' : k === 'airspace' ? 'blue' : ''}">
          {#if k === 'aerodromeClosed' || k === 'runwayClosed'}<Ban size={14} />{:else if k === 'fuelUnavailable'}<Fuel size={14} />{:else if k === 'navaidOut' || k === 'gps'}<RadioTower size={14} />{:else if k === 'airspace'}<Hexagon size={14} />{:else if k === 'obstacle'}<Diamond size={14} />{:else}<StickyNote size={14} />{/if}
        </span>
        <div>
          <div class="h"><b>{n.icao || n.location}</b><span class="faint">{n.category} · {n.number}</span></div>
          <div class:mono={app.rawWx} class="txt">{app.rawWx ? n.raw : n.plain}</div>
          {#if notamWindow(n)}<div class="faint small">{notamWindow(n)}</div>{/if}
        </div>
      </li>
    {/each}
  </ul>
  {#if items.length && !print}
    <div class="links">
      {#if items.length > limit}<button class="link" onclick={() => (all = !all)}>{all ? 'Show fewer' : `Show all ${items.length}`}</button>{/if}
      <button class="link" onclick={() => (app.rawWx = !app.rawWx)}>{app.rawWx ? 'Plain English' : 'Raw text'}</button>
    </div>
  {/if}
{/if}

<style>
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  li {
    display: flex;
    gap: 8px;
    padding: 6px 9px;
    border-radius: 8px;
    background: var(--surface-2);
    break-inside: avoid;
  }
  .ico {
    flex: none;
    margin-top: 1px;
    color: var(--text-3);
  }
  .ico.bad {
    color: var(--bad);
  }
  .ico.warn {
    color: var(--warn);
  }
  .ico.blue {
    color: var(--blue);
  }
  .h {
    display: flex;
    gap: 6px;
    align-items: baseline;
    flex-wrap: wrap;
    font-size: 12px;
  }
  .h .faint {
    font-size: 10.5px;
  }
  .txt {
    font-size: 11.5px;
    line-height: 1.4;
    word-break: break-word;
  }
  .txt.mono {
    font-size: 11px;
  }
  .small {
    font-size: 10.5px;
  }
  .hint {
    font-size: 11.5px;
    color: var(--text-2);
    line-height: 1.45;
    margin: 0 0 4px;
  }
  .err {
    font-size: 11.5px;
    color: var(--bad);
    margin: 0 0 4px;
  }
  .links {
    display: flex;
    gap: 14px;
    margin-top: 4px;
  }
  .link {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    border: 0;
    background: none;
    padding: 3px 0;
    cursor: pointer;
    color: var(--accent);
    font-size: 12px;
    font-weight: 600;
  }
</style>
