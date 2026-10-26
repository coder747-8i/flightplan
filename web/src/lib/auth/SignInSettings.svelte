<script lang="ts">
  // Settings → Sign-in: your name (the greeting), your password, connected desktop apps, and
  // signing out. The owner also picks just-me or several people, manages the people, and sets
  // the sign-in picture.
  import { LogOut, ImageUp, Trash2, Check, User, Users, UserPlus, Monitor, Copy, RefreshCw } from '@lucide/svelte'
  import { onMount } from 'svelte'
  import { auth, type AuthStatus, type DeviceInfo, type Me, type Person } from './auth'
  import { pickFiles } from '../io/files'

  let status = $state<AuthStatus | null>(null)
  let me = $state<Me | null>(null)
  let people = $state<Person[]>([])
  let devices = $state<DeviceInfo[]>([])
  let name = $state('')
  let current = $state('')
  let next = $state('')
  let again = $state('')
  let newName = $state('')
  let newPassword = $state('')
  let pairing = $state<{ code: string; minutes: number } | null>(null)
  let msg = $state<{ ok: boolean; text: string } | null>(null)
  let busy = $state(false)
  let bust = $state(Date.now())

  const min = $derived(status?.minPassword ?? 10)
  const ago = (s: number) => {
    const m = Math.round((Date.now() / 1000 - s) / 60)
    return m < 2 ? 'just now' : m < 120 ? `${m} min ago` : m < 2880 ? `${Math.round(m / 60)} h ago` : `${Math.round(m / 1440)} days ago`
  }

  async function load() {
    status = await auth.status().catch(() => null)
    me = await auth.me().catch(() => null)
    name = me?.name ?? ''
    people = me?.owner ? await auth.users().catch(() => []) : []
    devices = await auth.devices().catch(() => [])
  }
  onMount(load)

  async function run(f: () => Promise<unknown>, ok: string) {
    busy = true
    msg = null
    try {
      await f()
      msg = { ok: true, text: ok }
      await load()
      bust = Date.now()
    } catch (e) {
      msg = { ok: false, text: (e as Error).message }
    } finally {
      busy = false
    }
  }

  async function chooseImage() {
    const [file] = await pickFiles('image/jpeg,image/png,image/webp,image/gif,image/avif')
    if (file) run(() => auth.setImage(file), 'Sign-in picture saved')
  }

  function changePassword(e: Event) {
    e.preventDefault()
    if (next.length < min) return (msg = { ok: false, text: `Use at least ${min} characters.` })
    if (next !== again) return (msg = { ok: false, text: "The new passwords don't match." })
    run(async () => {
      await auth.changePassword(current, next)
      current = next = again = ''
    }, 'Password changed. Your other browsers and desktop apps have to sign in again.')
  }

  function addPerson(e: Event) {
    e.preventDefault()
    run(async () => {
      await auth.addUser(newName.trim(), newPassword)
      newName = newPassword = ''
    }, 'Added. Give them their name and password; they can change the password in Settings.')
  }

  async function signOut() {
    await auth.logout().catch(() => {})
    location.reload()
  }

  async function signOutEverywhere() {
    if (!confirm('Sign out every browser and desktop app, this one included?')) return
    await auth.logoutAll().catch(() => {})
    location.reload()
  }

  async function copy(text: string) {
    await navigator.clipboard.writeText(text).catch(() => {})
  }
</script>

{#if status?.enforced && me}
  <p class="faint lead">
    {status.multi ? 'Several people sign in with their name and password.' : 'Just you: the sign-in screen asks only for the password.'} A sign-in lasts 30 days of use (90 at most).
  </p>

  <form class="row" onsubmit={(e) => { e.preventDefault(); run(() => auth.setName(name), 'Name saved') }}>
    <label><span class="faint">Your name {status.multi ? '(you sign in with it)' : '(in the greeting)'}</span><input class="field" bind:value={name} maxlength="40" /></label>
    <button class="btn" disabled={busy || name === me.name}><Check size={14} /> Save</button>
  </form>

  <form class="col" onsubmit={changePassword}>
    <span class="faint">Change your password</span>
    <input class="field" type="password" placeholder="Current password" autocomplete="current-password" bind:value={current} />
    <input class="field" type="password" placeholder={`New password (${min}+ characters)`} autocomplete="new-password" bind:value={next} />
    <input class="field" type="password" placeholder="New password again" autocomplete="new-password" bind:value={again} />
    <button class="btn" disabled={busy || !current || !next}>Change password</button>
  </form>

  {#if me.owner}
    <div class="sub">Who uses Flightplan</div>
    <div class="modes">
      <button class="mode" class:on={!status.multi} disabled={busy} onclick={() => status!.multi && run(() => auth.setMode(false), 'Just you now: only the password is asked for')}>
        <User size={15} /> Just me
      </button>
      <button class="mode" class:on={status.multi} disabled={busy} onclick={() => !status!.multi && run(() => auth.setMode(true), 'Several people: everyone signs in with their name')}>
        <Users size={15} /> Several people
      </button>
    </div>
    {#if status.multi}
      <ul class="list">
        {#each people as p (p.id)}
          <li>
            <span><b>{p.name || '(no name)'}</b>{#if p.owner}<span class="faint">&nbsp;· owner</span>{/if}{#if p.id === me.id}<span class="faint">&nbsp;· you</span>{/if}</span>
            {#if !p.owner}<button class="btn ghost ico" title={`Remove ${p.name}`} onclick={() => confirm(`Remove ${p.name}? They're signed out; their files stay on the server.`) && run(() => auth.removeUser(p.id), 'Removed')}><Trash2 size={13} /></button>{/if}
          </li>
        {/each}
      </ul>
      <form class="col" onsubmit={addPerson}>
        <span class="faint">Add someone</span>
        <input class="field" placeholder="Their name" bind:value={newName} maxlength="40" />
        <input class="field" type="password" placeholder={`A first password (${min}+ characters)`} autocomplete="new-password" bind:value={newPassword} />
        <button class="btn" disabled={busy || !newName.trim() || !newPassword}><UserPlus size={14} /> Add</button>
      </form>
    {/if}

    <div class="sub">Sign-in picture</div>
    <div class="pic">
      <div class="thumb">
        {#if status.hasImage}<img src={`/api/auth/image?v=${bust}`} alt="Current sign-in background" />{:else}<span class="faint">Built-in picture</span>{/if}
      </div>
      <div class="picbtns">
        <button class="btn" disabled={busy} onclick={chooseImage}><ImageUp size={14} /> {status.hasImage ? 'Change picture' : 'Use my own picture'}</button>
        {#if status.hasImage}<button class="btn ghost" disabled={busy} onclick={() => run(auth.removeImage, 'Back to the built-in picture')}><Trash2 size={14} /> Use the built-in one</button>{/if}
        <span class="faint small">JPEG, PNG or WebP, up to 8 MB.</span>
      </div>
    </div>
  {/if}

  <div class="sub">Desktop app</div>
  <p class="faint small">Connect the Flightplan desktop app to sync plans both ways, instantly. In the desktop app: Settings → Sync with a Flightplan server.</p>
  {#if pairing}
    <div class="kv"><span class="faint">Address</span><code>{location.origin}</code><button class="btn ghost ico" title="Copy" onclick={() => copy(location.origin)}><Copy size={13} /></button></div>
    <div class="kv"><span class="faint">Code</span><code class="code">{pairing.code}</code><button class="btn ghost ico" title="Copy" onclick={() => copy(pairing!.code)}><Copy size={13} /></button></div>
    <p class="faint small">Works once, for {pairing.minutes} minutes.</p>
  {/if}
  <button class="btn" disabled={busy} onclick={async () => (pairing = await auth.pairing().catch(() => null))}>
    {#if pairing}<RefreshCw size={14} /> New code{:else}<Monitor size={14} /> Connect a desktop app{/if}
  </button>
  {#if devices.length}
    <ul class="list">
      {#each devices as d (d.id)}
        <li>
          <span><b>{d.name}</b><span class="faint">&nbsp;· synced {ago(d.lastSeen)}</span></span>
          <button class="btn ghost ico" title="Disconnect" onclick={() => run(() => auth.removeDevice(d.id), `${d.name} disconnected`)}><Trash2 size={13} /></button>
        </li>
      {/each}
    </ul>
  {/if}

  {#if msg}<p class="msg" class:ok={msg.ok}>{msg.text}</p>{/if}

  <div class="outs">
    <button class="btn" onclick={signOut}><LogOut size={14} /> Sign out</button>
    <button class="btn ghost" title="Every browser and desktop app has to sign in again" onclick={signOutEverywhere}>Sign out everywhere</button>
  </div>
{/if}

<style>
  .lead {
    font-size: 11.5px;
    margin: 0 0 8px;
  }
  .row {
    display: flex;
    gap: 6px;
    align-items: flex-end;
  }
  .row label {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 11.5px;
  }
  .col {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-top: 12px;
    font-size: 11.5px;
  }
  .col .btn {
    align-self: flex-start;
  }
  .sub {
    font-weight: 600;
    font-size: 12px;
    margin: 16px 0 6px;
  }
  .modes {
    display: flex;
    gap: 6px;
  }
  .mode {
    flex: 1;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    height: 34px;
    border-radius: 9px;
    border: 1px solid var(--line-strong);
    background: var(--surface-solid);
    cursor: pointer;
    font-weight: 550;
  }
  .mode.on {
    border-color: var(--accent);
    color: var(--accent);
    background: var(--accent-soft);
  }
  .list {
    list-style: none;
    margin: 8px 0 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .list li {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 5px 4px 5px 10px;
    border-radius: 8px;
    background: var(--surface-2);
    font-size: 12px;
  }
  .ico {
    width: 28px;
    height: 26px;
    padding: 0;
    justify-content: center;
  }
  .pic {
    display: flex;
    gap: 10px;
  }
  .thumb {
    width: 96px;
    height: 64px;
    flex: none;
    border-radius: 8px;
    overflow: hidden;
    background: var(--surface-2);
    display: grid;
    place-items: center;
    font-size: 10.5px;
    text-align: center;
  }
  .thumb img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .picbtns {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 4px;
  }
  .small {
    font-size: 10.5px;
    margin: 0 0 6px;
    line-height: 1.45;
  }
  .kv {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 4px 4px 10px;
    border-radius: 8px;
    background: var(--surface-2);
    margin-bottom: 4px;
    font-size: 12px;
  }
  .kv .faint {
    width: 52px;
    flex: none;
    font-size: 11px;
  }
  .kv code {
    flex: 1;
    font-family: var(--mono);
    word-break: break-all;
  }
  .kv .code {
    font-weight: 650;
    letter-spacing: 0.08em;
  }
  .msg {
    font-size: 12px;
    color: var(--bad);
    margin: 8px 0 0;
  }
  .msg.ok {
    color: var(--ok);
  }
  .outs {
    display: flex;
    gap: 6px;
    margin-top: 14px;
  }
</style>
