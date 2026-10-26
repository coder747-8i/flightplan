<script lang="ts">
  // Settings → Sign-in: the greeting name, the login picture, the password, and signing out.
  import { LogOut, ImageUp, Trash2, Check } from '@lucide/svelte'
  import { onMount } from 'svelte'
  import { auth, type AuthStatus } from './auth'
  import { pickFiles } from '../io/files'

  let status = $state<AuthStatus | null>(null)
  let name = $state('')
  let current = $state('')
  let next = $state('')
  let again = $state('')
  let msg = $state<{ ok: boolean; text: string } | null>(null)
  let busy = $state(false)
  let bust = $state(Date.now())

  async function load() {
    status = await auth.status().catch(() => null)
    name = status?.name ?? ''
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
    if (file) run(() => auth.setImage(file), 'Login picture saved')
  }

  function changePassword(e: Event) {
    e.preventDefault()
    if (next.length < (status?.minPassword ?? 10)) return (msg = { ok: false, text: `Use at least ${status?.minPassword ?? 10} characters.` })
    if (next !== again) return (msg = { ok: false, text: "The new passwords don't match." })
    run(async () => {
      await auth.changePassword(current, next)
      current = next = again = ''
    }, 'Password changed. Other devices have to sign in again.')
  }

  async function signOut() {
    await auth.logout().catch(() => {})
    location.reload()
  }
</script>

{#if status?.enforced}
  <p class="faint lead">Flightplan asks for a password in every browser. A sign-in lasts 30 days of use (90 at most).</p>

  <form class="row" onsubmit={(e) => { e.preventDefault(); run(() => auth.setName(name), 'Name saved') }}>
    <label><span class="faint">Name in the greeting</span><input class="field" bind:value={name} maxlength="40" /></label>
    <button class="btn" disabled={busy || name === status.name}><Check size={14} /> Save</button>
  </form>

  <div class="pic">
    <div class="thumb">
      {#if status.hasImage}<img src={`/api/auth/image?v=${bust}`} alt="Current sign-in background" />{:else}<span class="faint">Built-in picture</span>{/if}
    </div>
    <div class="picbtns">
      <button class="btn" disabled={busy} onclick={chooseImage}><ImageUp size={14} /> {status.hasImage ? 'Change picture' : 'Use my own picture'}</button>
      {#if status.hasImage}<button class="btn ghost" disabled={busy} onclick={() => run(auth.removeImage, 'Back to the built-in picture')}><Trash2 size={14} /> Use the built-in one</button>{/if}
      <span class="faint small">Shown next to the sign-in form. JPEG, PNG or WebP, up to 8 MB.</span>
    </div>
  </div>

  <form class="pw" onsubmit={changePassword}>
    <span class="faint">Change password</span>
    <input class="field" type="password" placeholder="Current password" autocomplete="current-password" bind:value={current} />
    <input class="field" type="password" placeholder={`New password (${status.minPassword ?? 10}+ characters)`} autocomplete="new-password" bind:value={next} />
    <input class="field" type="password" placeholder="New password again" autocomplete="new-password" bind:value={again} />
    <button class="btn" disabled={busy || !current || !next}>Change password</button>
  </form>

  {#if msg}<p class="msg" class:ok={msg.ok}>{msg.text}</p>{/if}

  <div class="outs">
    <button class="btn" onclick={signOut}><LogOut size={14} /> Sign out</button>
    <button class="btn ghost" title="Every browser and device has to sign in again" onclick={async () => { if (confirm('Sign out every device, this one included?')) { await auth.logoutAll().catch(() => {}); location.reload() } }}>Sign out everywhere</button>
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
  .pic {
    display: flex;
    gap: 10px;
    margin-top: 10px;
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
  }
  .pw {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-top: 12px;
    font-size: 11.5px;
  }
  .pw .btn {
    align-self: flex-start;
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
    margin-top: 12px;
  }
</style>
