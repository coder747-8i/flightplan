<script lang="ts">
  // Settings: chart data (install / update), API keys, and re-running setup.
  import { RefreshCw, Sparkles } from '@lucide/svelte'
  import PanelShell from './PanelShell.svelte'
  import KeysForm from './KeysForm.svelte'
  import DataSetup from '../setup/DataSetup.svelte'
  import { onboarding } from '../stores/onboarding.svelte'
</script>

<PanelShell title="Settings" subtitle="Chart data and API keys" width={380}>
  <div class="section-title">Chart data</div>
  <DataSetup compact />
  {#if onboarding.dataChanged}
    <button class="btn primary reload" onclick={() => location.reload()}><RefreshCw size={14} /> Reload the chart with the new data</button>
  {/if}

  <div class="section-title">API keys</div>
  <KeysForm />

  <div class="section-title">Setup</div>
  <button class="btn" onclick={() => onboarding.reopen(0)}><Sparkles size={14} /> Run the setup screen again</button>
</PanelShell>

<style>
  .reload {
    margin-top: 10px;
  }
</style>
