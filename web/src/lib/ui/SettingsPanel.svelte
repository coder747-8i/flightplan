<script lang="ts">
  // Settings: personal minimums for the go / no-go call, chart data (install / update), API keys,
  // sign-in (web app) and re-running setup.
  import { RefreshCw, Sparkles } from '@lucide/svelte'
  import PanelShell from './PanelShell.svelte'
  import KeysForm from './KeysForm.svelte'
  import MinimumsForm from './MinimumsForm.svelte'
  import SignInSettings from '../auth/SignInSettings.svelte'
  import { config } from '../stores/config.svelte'
  import DataSetup from '../setup/DataSetup.svelte'
  import { onboarding } from '../stores/onboarding.svelte'
</script>

<PanelShell title="Settings" subtitle="Minimums, chart data and API keys" width={380}>
  <div class="section-title">Personal minimums</div>
  <MinimumsForm />

  <div class="section-title">Chart data</div>
  <DataSetup compact />
  {#if onboarding.dataChanged}
    <button class="btn primary reload" onclick={() => location.reload()}><RefreshCw size={14} /> Reload the chart with the new data</button>
  {/if}

  <div class="section-title">API keys (FAA NOTAMs, Windy)</div>
  <KeysForm />

  {#if !config.desktop}
    <div class="section-title">Sign-in</div>
    <SignInSettings />
  {/if}

  <div class="section-title">Setup</div>
  <button class="btn" onclick={() => onboarding.reopen(0)}><Sparkles size={14} /> Run the setup screen again</button>
</PanelShell>

<style>
  .reload {
    margin-top: 10px;
  }
</style>
