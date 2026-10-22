<script lang="ts">
  // Plain-English rendering of a coded report, with a one-tap switch to the raw text.
  import { decodeMetar, decodePirep, decodeTaf, describeSigmet, describeGairmet, expand } from './decode'
  import { app } from '../stores/app.svelte'

  let { kind, raw = '', data = null, compact = false }: { kind: 'metar' | 'taf' | 'pirep' | 'sigmet' | 'gairmet' | 'remark'; raw?: string; data?: any; compact?: boolean } = $props()

  const decoded = $derived.by((): { title?: string; lines: string[] }[] => {
    try {
      switch (kind) {
        case 'metar':
          return [{ lines: decodeMetar(raw) }]
        case 'taf':
          return decodeTaf(raw)
        case 'pirep':
          return [{ lines: decodePirep(raw) }]
        case 'sigmet': {
          const d = describeSigmet(data ?? { rawAirSigmet: raw })
          return [{ lines: [...d.summary, d.text] }]
        }
        case 'gairmet':
          return [{ lines: [describeGairmet(data)] }]
        case 'remark':
          return [{ lines: [expand(raw)] }]
      }
    } catch {
      return []
    }
    return []
  })
  const rawText = $derived(raw || data?.rawAirSigmet || '')
  const showRaw = $derived(app.rawWx || !decoded.length)
</script>

<div class="wxt" class:compact>
  {#if showRaw}
    <pre>{kind === 'taf' ? rawText.replace(/ (FM|BECMG|TEMPO|PROB)/g, '\n  $1') : rawText}</pre>
  {:else}
    {#each decoded as block}
      {#if block.title}<div class="bt">{block.title}</div>{/if}
      {#if compact}
        <p>{block.lines.join('. ').replace(/\.\./g, '.')}</p>
      {:else}
        <ul>{#each block.lines as l}<li>{l}</li>{/each}</ul>
      {/if}
    {/each}
  {/if}
  {#if rawText && kind !== 'remark'}
    <button class="sw" onclick={(e) => { e.stopPropagation(); app.rawWx = !app.rawWx }}>{app.rawWx ? 'Plain English' : 'Raw'}</button>
  {/if}
</div>

<style>
  .wxt {
    position: relative;
    font-size: 12px;
    line-height: 1.45;
  }
  pre {
    margin: 0;
    white-space: pre-wrap;
    font-family: var(--mono);
    font-size: 11.5px;
    background: var(--surface-2);
    padding: 8px 10px;
    border-radius: 8px;
  }
  ul {
    margin: 0;
    padding-left: 16px;
  }
  li {
    margin: 1px 0;
  }
  p {
    margin: 0;
  }
  .bt {
    font-weight: 650;
    margin: 6px 0 2px;
    font-size: 11.5px;
    color: var(--text-2);
  }
  .sw {
    margin-top: 4px;
    border: 0;
    background: none;
    color: var(--accent);
    font-size: 11px;
    font-weight: 600;
    cursor: pointer;
    padding: 0;
  }
  .compact {
    font-size: 11.5px;
  }
</style>
