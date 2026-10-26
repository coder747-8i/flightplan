<script lang="ts">
  // API keys for Windy and the FAA NOTAM API: used by the Settings panel and the setup screen.
  import { Check, KeyRound, Eye, EyeOff, ExternalLink, LoaderCircle, X, Mail } from '@lucide/svelte'
  import { REQUEST_MAILTO } from '../wx/notams'
  import { openExternal } from '../io/files'
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

  /** A pasted key, cleaned: no whitespace or line breaks anywhere, no surrounding quotes, and no
   *  "client_id:" style label copied along with it. */
  function cleanKey(s: string) {
    let v = s.replace(/\s+/g, '')
    for (const label of ['client_id:', 'clientid:', 'client_secret:', 'clientsecret:', 'client_id=', 'client_secret=', 'key:', 'id:', 'secret:']) {
      if (v.toLowerCase().startsWith(label)) v = v.slice(label.length)
    }
    return v.replace(/^["',;]+|["',;]+$/g, '')
  }

  async function save(fields: Field[], clear = false): Promise<boolean> {
    error = ''
    // both NOTAM halves are needed: don't save one and leave the other missing
    if (!clear && fields.length > 1 && !fields.every((f) => cleanKey(draft[f]) || saved[f]?.set)) {
      error = 'Enter both the client ID and the client secret from the FAA.'
      return false
    }
    busy = true
    const body: Record<string, string> = {}
    for (const f of fields) {
      if (clear) body[f] = ''
      else if (cleanKey(draft[f])) body[f] = cleanKey(draft[f])
    }
    try {
      const r = await apiFetch('/api/settings', { method: 'PUT', headers: { 'content-type': 'application/json' }, body: JSON.stringify(body) })
      if (!r.ok) throw new Error(await r.text())
      saved = await r.json()
      for (const f of fields) draft[f] = ''
      await config.reload()
      return true
    } catch (e) {
      error = String(e)
      return false
    } finally {
      busy = false
    }
  }

  /** Save, then check it straight away against the service (the Windy map key can only be
   *  checked by Windy in the browser). */
  async function saveAndTest(sec: (typeof sections)[number]) {
    delete tests[sec.test]
    if ((await save(sec.fields.map((x) => x.id))) && sec.test !== 'windyMap') test(sec.test)
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
      blurb: 'Shows the Windy weather map (wind, clouds, rain, icing) with your route. Create a "Map Forecast API" key and allow this address in its domain list. Winds aloft for the navlog come from the NWS forecast and need no key.',
      link: ['https://api.windy.com/keys', 'api.windy.com/keys'],
      test: 'windyMap',
      fields: [{ id: 'windyMap', label: 'Map Forecast API key', placeholder: 'Paste the map key' }],
    },
    {
      title: 'FAA NOTAMs',
      blurb: "NOTAMs for your route, the airport cards, the briefing and the go / no-go call. The FAA's NOTAM Management Service (NMS) isn't self-serve: request API access by email to notams@faa.gov, and paste the client ID and client secret they send. Older api.faa.gov NOTAM keys work too.",
      link: ['https://nms.aim.faa.gov/', 'nms.aim.faa.gov'],
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
      {#if !config.desktop}Only someone signed in to Flightplan can replace them.{/if}
    </span>
  </p>
  {#if error}<p class="err">{error}</p>{/if}

  {#each sections as sec}
    {@const t = tests[sec.test]}
    {@const anySet = sec.fields.some((f) => saved[f.id]?.set)}
    {@const allSet = sec.fields.every((f) => saved[f.id]?.set)}
    {@const passed = t && t !== 'running' ? t.ok : null}
    <section>
      <div class="head">
        <h3>{sec.title}</h3>
        <!-- "Connected" only once a test has passed -->
        <span class="badge" class:on={allSet && passed === true} class:off={allSet && passed === false}>
          {#if allSet && passed === true}<Check size={12} /> Connected{:else if allSet && passed === false}<X size={12} /> Not accepted{:else if allSet}Saved{:else if anySet}Incomplete{:else}Not set{/if}
        </span>
      </div>
      <p class="faint blurb">{sec.blurb} <a href={sec.link[0]} target="_blank" rel="noopener noreferrer">{sec.link[1]} <ExternalLink size={11} /></a></p>
      {#if sec.test === 'notam'}
        <button class="mail" onclick={() => openExternal(REQUEST_MAILTO)}><Mail size={13} /> Email the FAA for access</button>
      {/if}
      {#each sec.fields as f}
        <label>
          <span class="faint">{f.label}{#if saved[f.id]?.set}<span class="hint"> · saved {saved[f.id].hint}</span>{/if}</span>
          <span class="inp">
            <input
              class="field mono"
              type={reveal[f.id] || f.id === 'notamId' ? 'text' : 'password'}
              autocomplete="off"
              spellcheck="false"
              placeholder={saved[f.id]?.set ? 'Enter a new value to replace' : f.placeholder}
              bind:value={draft[f.id]}
              onkeydown={(e) => e.key === 'Enter' && saveAndTest(sec)}
            />
            <button class="btn ghost ico" onclick={() => (reveal[f.id] = !reveal[f.id])} aria-label={reveal[f.id] ? 'Hide' : 'Show'}>
              {#if reveal[f.id]}<EyeOff size={14} />{:else}<Eye size={14} />{/if}
            </button>
          </span>
          {#if cleanKey(draft[f.id]).length}<span class="faint count">{cleanKey(draft[f.id]).length} characters entered (spaces and line breaks are removed)</span>{/if}
        </label>
      {/each}
      <div class="row">
        <button class="btn primary" disabled={busy || !sec.fields.some((f) => draft[f.id].trim())} onclick={() => saveAndTest(sec)}>{sec.test === 'notam' ? 'Save and test' : 'Save'}</button>
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
  .mail {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    border: 0;
    background: none;
    color: var(--accent);
    font-weight: 600;
    font-size: 12px;
    padding: 0 0 8px;
    cursor: pointer;
  }
  .count {
    font-size: 10.5px;
  }
  .badge.off {
    color: var(--bad);
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
