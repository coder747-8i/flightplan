<script lang="ts">
  import { OctagonAlert, TriangleAlert, Info } from '@lucide/svelte'
  import type { Hazard } from '../nav/hazards'

  let { hazards, compact = false }: { hazards: Hazard[]; compact?: boolean } = $props()
  let expanded = $state(false)
  const shown = $derived(compact && !expanded ? hazards.slice(0, 4) : hazards)
</script>

{#if !hazards.length}
  <p class="clear">No hazards found for this plan. Still get an official briefing before you fly.</p>
{:else}
  <ul class="hz">
    {#each shown as h}
      <li class={h.level}>
        <span class="ico">
          {#if h.level === 'warning'}<OctagonAlert size={15} />{:else if h.level === 'caution'}<TriangleAlert size={15} />{:else}<Info size={15} />{/if}
        </span>
        <div>
          <div class="t">{h.title}{#if h.at != null}<span class="at num"> · {h.at.toFixed(0)} NM</span>{/if}</div>
          {#if h.detail && !compact}<div class="d">{h.detail}</div>{/if}
          <div class="cat">{h.category}</div>
        </div>
      </li>
    {/each}
  </ul>
  {#if compact && hazards.length > 4}
    <button class="more" onclick={() => (expanded = !expanded)}>{expanded ? 'Show fewer' : `Show all ${hazards.length}`}</button>
  {/if}
{/if}

<style>
  .hz {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  li {
    display: flex;
    gap: 9px;
    padding: 8px 10px;
    border-radius: 9px;
    background: var(--surface-2);
    border-left: 3px solid var(--text-3);
  }
  li.warning {
    border-left-color: var(--bad);
    background: rgba(210, 58, 58, 0.07);
  }
  li.caution {
    border-left-color: var(--warn);
    background: rgba(212, 138, 11, 0.07);
  }
  li.info {
    border-left-color: var(--blue);
  }
  .ico {
    flex: none;
    margin-top: 1px;
  }
  li.warning .ico {
    color: var(--bad);
  }
  li.caution .ico {
    color: var(--warn);
  }
  li.info .ico {
    color: var(--blue);
  }
  .t {
    font-weight: 600;
    font-size: 12.5px;
    line-height: 1.35;
  }
  .at {
    font-weight: 500;
    color: var(--text-3);
  }
  .d {
    font-size: 12px;
    color: var(--text-2);
    margin-top: 2px;
    line-height: 1.4;
  }
  .cat {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-3);
    margin-top: 3px;
  }
  .clear {
    font-size: 12px;
    color: var(--ok);
    background: rgba(31, 157, 106, 0.08);
    padding: 10px;
    border-radius: 9px;
    margin: 0;
  }
  .more {
    margin-top: 6px;
    border: 0;
    background: none;
    color: var(--accent);
    font-weight: 600;
    cursor: pointer;
    padding: 4px 0;
  }
</style>
