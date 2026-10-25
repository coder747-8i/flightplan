<script lang="ts">
  // API keys for Windy and the FAA NOTAM API: used by the Settings panel and the setup screen.
  import { Check, KeyRound, Eye, EyeOff, ExternalLink, LoaderCircle, X } from '@lucide/svelte'
  import { onMount } from 'svelte'
  import { config } from '../stores/config.svelte'
  import { apiFetch } from '../api'

  type Slot = { set: boolean; hint?: string }
  type Field = 'windyMap' | 'windyPoint' | 'notamId' | 'notamSecret'

  let saved = $state<Record<Field, Slot>>({ windyMap: { set: false }, windyPoint: { set: false }, notamId: { set: false }, notamSecret: { set: false } })
  let draft = $state<Record<Field, string>>({ windyMap: '', windyPoint: '', notamId: '', notamSecret: '' })
  let reveal = $state<Record<Field, boolean>>({ windyMap: false, windyPoint: false, notamId: false, notamSecret: false })
  let busy = $state(false)
  let error = $state('')
  let tests = $state<Record<string, { ok: boolean; message: string } | 'running'>>({})

  async function load() {
    try {
      saved = await (await fetch('/api/settings')).json()
    } catch (e) {
      error = `Couldn't reach the server: ${e}`
    }
  }
  onMount(load)

  async function save(fields: Field[], clear = false) {
    busy = true
    error = ''
    const body: Record<string, string> = {}
    for (const f of fields) {
      if (clear) body[f] = ''
      else if (draft[f].trim()) body[f] = draft[f].trim()
    }
    try {
      const r = await apiFetch('/api/settings', { method: 'PUT', headers: { 'content-type': 'application/json' }, body: JSON.stringify(body) })
      if (!r.ok) throw new Error(await r.text())
      saved = await r.json()
      for (const f of fields) draft[f] = ''
      await config.reload()
    } catch (e) {
      error = String(e)
    } finally {
      busy = false
    }
  }

  async function test(which: 'windyMap' | 'windyPoint' | 'notam') {
    tests[which] = 'running'
    try {
      const r = await apiFetch('/api/settings/test', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ which }) })
      tests[which] = await r.json()
    } catch (e) {
      tests[which] = { ok: false, message: String(e) }
    }
  }

  const sections: { title: string; blurb: string; link: [string, string]; test: 'windyMap' | 'windyPoint' | 'notam'; fields: { id: Field; label: string; placeholder: string }[] }[] = [
    {
      title: 'Windy map',
      blurb: 'Shows the Windy weather map (wind, clouds, rain, icing) with your route. Create a "Map Forecast API" key and allow this address in its domain list.',
      link: ['https://api.windy.com/keys', 'api.windy.com/keys'],
      test: 'windyMap',
      fields: [{ id: 'windyMap', label: 'Map Forecast API key', placeholder: 'Paste the map key' }],
    },
    {
      title: 'FAA NOTAMs',
      blurb: 'Airport, en route, FDC and TFR NOTAMs along your route. Register a free app on the FAA API portal and copy its client ID and secret.',
      link: ['https://api.faa.gov/s/', 'api.faa.gov'],
      test: 'notam',
      fields: [
        { id: 'notamId', label: 'Client ID', placeholder: 'client_id' },
        { id: 'notamSecret', label: 'Client secret', placeholder: 'client_secret' },
      ],
    },
  ]
</script>

<div class="keys">
  <p class="note">
    <KeyRound size={14} />
    <span>
      All optional. Keys are saved in <code>settings.json</code> in Flightplan's data folder (readable only by your user) and never shown again after saving.
      {#if !config.desktop}Anyone who can open this app on your network can replace them, so keep it on your home network.{/if}
    </span>
  </p>
  {#if error}<p class="err">{error}</p>{/if}

  {#each sections as sec}
    {@const t = tests[sec.test]}
    {@const anySet = sec.fields.some((f) => saved[f.id]?.set)}
    <section>
      <div class="head">
        <h3>{sec.title}</h3>
        <span class="badge" class:on={sec.fields.every((f) => saved[f.id]?.set)}>
          {#if sec.fields.every((f) => saved[f.id]?.set)}<Check size={12} /> Connected{:else if anySet}Incomplete{:else}Not set{/if}
        </span>
      </div>
      <p class="faint blurb">{sec.blurb} <a href={sec.link[0]} target="_blank" rel="noopener noreferrer">{sec.link[1]} <ExternalLink size={11} /></a></p>
      {#each sec.fields as f}
        <label>
          <span class="faint">{f.label}{#if saved[f.id]?.set}<span class="hint"> · saved {saved[f.id].hint}</span>{/if}</span>
          <span class="inp">
            <input
              class="field mono"
              type={reveal[f.id] ? 'text' : 'password'}
              autocomplete="off"
              spellcheck="false"
              placeholder={saved[f.id]?.set ? 'Enter a new value to replace' : f.placeholder}
              bind:value={draft[f.id]}
              onkeydown={(e) => e.key === 'Enter' && save(sec.fields.map((x) => x.id))}
            />
            <button class="btn ghost ico" onclick={() => (reveal[f.id] = !reveal[f.id])} aria-label={reveal[f.id] ? 'Hide' : 'Show'}>
              {#if reveal[f.id]}<EyeOff size={14} />{:else}<Eye size={14} />{/if}
            </button>
          </span>
        </label>
      {/each}
      <div class="row">
        <button class="btn primary" disabled={busy || !sec.fields.some((f) => draft[f.id].trim())} onclick={() => save(sec.fields.map((x) => x.id))}>Save</button>
        <button class="btn" disabled={!anySet || t === 'running'} onclick={() => test(sec.test)}>
          {#if t === 'running'}<LoaderCircle size={14} class="spin" />{/if} Test
        </button>
        {#if anySet}<button class="btn ghost" disabled={busy} onclick={() => save(sec.fields.map((x) => x.id), true)}><X size={14} /> Remove</button>{/if}
      </div>
      {#if t && t !== 'running'}<p class="result" class:ok={t.ok}>{t.message}</p>{/if}
    </section>
  {/each}
</div>

<style>
  .note {
    display: flex;
    gap: 8px;
    font-size: 11.5px;
    color: var(--text-2);
    background: var(--surface-2);
    padding: 10px;
    border-radius: 9px;
    line-height: 1.45;
  }
  .note :global(svg) {
    flex: none;
    margin-top: 2px;
  }
  code {
    font-family: var(--mono);
    font-size: 11px;
  }
  section {
    padding: 14px 0;
    border-bottom: 1px solid var(--line);
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  h3 {
    margin: 0;
    font-size: 13.5px;
  }
  .badge {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 10.5px;
    font-weight: 650;
    padding: 2px 8px;
    border-radius: 999px;
    background: var(--surface-2);
    color: var(--text-3);
  }
  .badge.on {
    background: rgba(31, 157, 106, 0.12);
    color: var(--ok);
  }
  .blurb {
    font-size: 11.5px;
    line-height: 1.45;
    margin: 6px 0 10px;
  }
  .blurb a {
    color: var(--blue);
    white-space: nowrap;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 11px;
    margin-bottom: 8px;
  }
  .hint {
    font-family: var(--mono);
  }
  .inp {
    display: flex;
    gap: 4px;
  }
  .ico {
    width: 34px;
    padding: 0;
    justify-content: center;
  }
  .row {
    display: flex;
    gap: 6px;
  }
  .btn:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .result {
    font-size: 12px;
    margin: 8px 0 0;
    color: var(--bad);
  }
  .result.ok {
    color: var(--ok);
  }
  .err {
    color: var(--bad);
    font-size: 12px;
  }
</style>
