<script lang="ts">
  // The pilot's personal minimums: what turns the go / no-go call yellow or red.
  import Toggle from './Toggle.svelte'
  import { minimums } from '../stores/minimums.svelte'
  import { DEFAULT_MINIMUMS, type Minimums } from '../nav/verdict'

  type NumKey = { [K in keyof Minimums]: Minimums[K] extends number ? K : never }[keyof Minimums]
  const rows: { key: NumKey; label: string; hint: string; unit: string; min: number; max: number; step: number }[] = [
    { key: 'ceilingFt', label: 'Ceiling', hint: 'Caution below this ceiling at departure, fuel stops or destination (ft AGL)', unit: 'ft', min: 500, max: 10000, step: 100 },
    { key: 'visSm', label: 'Visibility', hint: 'Caution below this visibility (statute miles)', unit: 'SM', min: 1, max: 10, step: 0.5 },
    { key: 'windKt', label: 'Wind', hint: 'No-go above this steady surface wind (kt)', unit: 'kt', min: 5, max: 50, step: 1 },
    { key: 'gustKt', label: 'Gusts', hint: 'No-go above this gust (kt)', unit: 'kt', min: 5, max: 60, step: 1 },
    { key: 'gustSpreadKt', label: 'Gust spread', hint: 'Caution when gusts exceed the steady wind by more than this (kt)', unit: 'kt', min: 3, max: 30, step: 1 },
    { key: 'xwindPct', label: 'Crosswind', hint: "Caution above this share of the aircraft's demonstrated crosswind; above 100% is a no-go", unit: '%', min: 30, max: 100, step: 1 },
    { key: 'tstmCaution', label: 'Thunder: caution', hint: 'Caution at or above this probability of thunderstorms', unit: '%', min: 5, max: 80, step: 1 },
    { key: 'tstmNogo', label: 'Thunder: no-go', hint: 'No-go at or above this probability of thunderstorms', unit: '%', min: 5, max: 100, step: 1 },
    { key: 'densityAltFt', label: 'Density altitude', hint: 'Caution above this density altitude at departure (ft)', unit: 'ft', min: 2000, max: 12000, step: 100 },
  ]
  const changed = $derived(Object.entries(DEFAULT_MINIMUMS).some(([k, v]) => minimums[k as keyof Minimums] !== v))

  function clamp(key: NumKey, r: (typeof rows)[number]) {
    const v = Number(minimums[key])
    minimums[key] = Number.isFinite(v) ? Math.min(r.max, Math.max(r.min, v)) : DEFAULT_MINIMUMS[key]
  }
</script>

<p class="faint lead">Used for the go / no-go call. Below a minimum is a caution; past a hard limit is a no-go.</p>
{#each rows as r}
  <label class="row" title={r.hint}>
    <span>{r.label}</span>
    <input class="field num" type="number" min={r.min} max={r.max} step={r.step} bind:value={minimums[r.key]} onchange={() => clamp(r.key, r)} />
    <span class="faint unit">{r.unit}</span>
  </label>
{/each}
<Toggle bind:checked={minimums.nightOk} label="Night flight OK" hint="Off: a night arrival is a no-go instead of a caution" />
<Toggle bind:checked={minimums.ifr} label="Instrument rated and current" hint="On: IFR or LIFR at the ends is a caution instead of a no-go" />
{#if changed}<button class="link" onclick={() => Object.assign(minimums, DEFAULT_MINIMUMS)}>Reset to defaults</button>{/if}

<style>
  .lead {
    font-size: 11.5px;
    margin: 0 0 6px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 3px 2px;
    font-size: 12.5px;
  }
  .row span:first-child {
    flex: 1;
  }
  .row input {
    width: 80px;
    height: 28px;
    text-align: right;
  }
  .unit {
    width: 22px;
    font-size: 11px;
  }
  .link {
    border: 0;
    background: none;
    color: var(--accent);
    font-weight: 600;
    font-size: 12px;
    padding: 4px 0;
    cursor: pointer;
  }
</style>
