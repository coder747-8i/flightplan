<script lang="ts">
  import PanelShell from './PanelShell.svelte'
  import Toggle from './Toggle.svelte'
  import { app } from '../stores/app.svelte'
  import { DAY, NIGHT } from '../chart/palette'
  import type { LayerGroup } from '../chart/style'

  const p = $derived(app.theme === 'night' ? NIGHT : DAY)

  type Item = { key: LayerGroup; label: string; hint?: string; swatch?: (p: typeof DAY) => string }
  const groups: { title: string; items: Item[] }[] = [
    {
      title: 'Airspace',
      items: [
        { key: 'classB', label: 'Class B', swatch: (p) => p.classB },
        { key: 'classC', label: 'Class C', swatch: (p) => p.classC },
        { key: 'classD', label: 'Class D', swatch: (p) => p.classD },
        { key: 'classE', label: 'Class E', hint: 'Surface areas and 700 ft AGL floors', swatch: (p) => p.classE },
        { key: 'classEhigh', label: 'Class E, 1200 AGL & en route', hint: 'Usually hidden on sectionals' },
        { key: 'modeC', label: 'Mode C veil', hint: '30 NM rings around Class B' },
        { key: 'sua', label: 'Special use', hint: 'Restricted, prohibited, MOA, warning, alert', swatch: (p) => p.restricted },
        { key: 'airspaceLabels', label: 'Altitude labels' },
      ],
    },
    {
      title: 'Airports',
      items: [
        { key: 'airportsPublic', label: 'Public & military', swatch: (p) => p.towered },
        { key: 'airportsPrivate', label: 'Private', swatch: (p) => p.nontowered },
        { key: 'heliports', label: 'Heliports' },
        { key: 'runways', label: 'Runway diagrams', hint: 'Zoomed in' },
      ],
    },
    {
      title: 'Navigation',
      items: [
        { key: 'navaids', label: 'VOR, NDB & compass roses' },
        { key: 'vfrWaypoints', label: 'VFR waypoints' },
        { key: 'ifrFixes', label: 'IFR fixes' },
        { key: 'airwaysLow', label: 'Low airways (V, T)' },
        { key: 'airwaysHigh', label: 'High airways (J, Q)' },
      ],
    },
    {
      title: 'Map',
      items: [
        { key: 'terrain', label: 'Terrain relief' },
        { key: 'obstacles', label: 'Obstacles', hint: '200 ft AGL and taller' },
        { key: 'basemap', label: 'Roads, cities & borders' },
      ],
    },
  ]

  const presets: { label: string; on: LayerGroup[] }[] = [
    { label: 'VFR', on: ['terrain', 'basemap', 'classB', 'classC', 'classD', 'classE', 'sua', 'airspaceLabels', 'airportsPublic', 'airportsPrivate', 'runways', 'navaids', 'vfrWaypoints', 'obstacles'] },
    { label: 'IFR low', on: ['terrain', 'classB', 'classC', 'classD', 'sua', 'airportsPublic', 'runways', 'navaids', 'ifrFixes', 'airwaysLow'] },
    { label: 'Minimal', on: ['terrain', 'classB', 'classC', 'classD', 'airportsPublic'] },
  ]

  function applyPreset(on: LayerGroup[]) {
    for (const k of Object.keys(app.layers) as LayerGroup[]) app.layers[k] = on.includes(k)
  }
</script>

<PanelShell title="Layers" subtitle="Turn chart features on and off">
  <div class="presets">
    {#each presets as pr}
      <button class="btn" onclick={() => applyPreset(pr.on)}>{pr.label}</button>
    {/each}
  </div>
  {#each groups as g}
    <div class="section-title">{g.title}</div>
    {#each g.items as it}
      <Toggle bind:checked={app.layers[it.key]} label={it.label} hint={it.hint} swatch={it.swatch?.(p)} />
    {/each}
  {/each}
</PanelShell>

<style>
  .presets {
    display: flex;
    gap: 6px;
    margin-top: 6px;
  }
  .presets .btn {
    flex: 1;
    justify-content: center;
  }
</style>
