<script lang="ts">
  // First-run walkthrough that locks the web app: your name (for the greeting), what Flightplan
  // is, just you or several people, the one-time setup code (proves you run the server), the
  // password, the login picture, then lock it. Once locked: connect the desktop app so plans
  // saved there show up here straight away (and the other way round). After that the app's own
  // setup (chart data, keys, aircraft) carries on.
  import { ArrowLeft, ArrowRight, Lock, LoaderCircle, Eye, EyeOff, Map as MapIcon, CloudSun, Share2, ShieldCheck, Terminal, ImageUp, Check, KeyRound, User, Users, Monitor, Copy, RefreshCw } from '@lucide/svelte'
  import { tick } from 'svelte'
  import { auth, greeting, strength, type AuthStatus, type Phase } from './auth'

  let { status, phase, onsignedin, onpreview }: { status: AuthStatus; phase: Phase; onsignedin: () => void; onpreview: (url: string | null) => void } = $props()

  const STEPS = ['You', 'Welcome', 'People', 'Setup code', 'Password', 'Picture', 'Lock', 'Desktop app'] as const
  const LOCK = 6
  let step = $state(0)
  let name = $state('')
  let multi = $state(false)
  let code = $state('')
  let password = $state('')
  let again = $state('')
  let reveal = $state(false)
  let picture = $state<File | null>(null)
  let busy = $state(false)
  let error = $state('')
  let body = $state<HTMLElement>()
  // a plain file input: the app's file helpers load stores that call the API, which isn't open yet
  let fileInput = $state<HTMLInputElement>()
  /** desktop pairing, once locked */
  let pairing = $state<{ code: string; minutes: number } | null>(null)
  let copied = $state('')

  const min = $derived(status.minPassword || 10)
  const score = $derived(strength(password))
  const LABEL = ['Too weak', 'Fair', 'Good', 'Strong']
  const first = $derived(name.trim().split(/\s+/)[0] ?? '')

  /** the same rules the server applies, so problems show before the last step */
  const pwProblem = $derived.by(() => {
    if (password.length < min) return `Use at least ${min} characters.`
    if (new Set(password.toLowerCase()).size < 3) return 'Not just one character over and over.'
    const n = name.trim().toLowerCase().replace(/\s+/g, '')
    if (n.length >= 3 && password.toLowerCase().replace(/[^\p{L}\p{N}]/gu, '').replace(/\d+$/, '') === n) return 'Not your name.'
    if (again && again !== password) return "The two passwords don't match."
    return ''
  })

  async function go(n: number) {
    error = ''
    step = n
    await tick()
    body?.querySelector<HTMLInputElement>('input:not([type=checkbox]):not([type=file]):not([type=radio])')?.focus()
  }

  async function next() {
    if (busy) return
    error = ''
    if (step === 2 && multi && !name.trim()) return (error = 'With several people, everyone signs in with their name. Go back and enter yours.')
    if (step === 3) {
      busy = true
      try {
        await auth.setupCheck(code)
      } catch (e) {
        error = (e as Error).message
        return
      } finally {
        busy = false
      }
    }
    if (step === 4) {
      if (pwProblem) return (error = pwProblem)
      if (password !== again) return (error = "The two passwords don't match.")
    }
    if (step === LOCK) return lock()
    if (step === LOCK + 1) return onsignedin()
    go(step + 1)
  }

  function choosePicture() {
    fileInput?.click()
  }

  function picked() {
    const f = fileInput?.files?.[0]
    if (!f) return
    if (f.size > 8 * 1024 * 1024) return (error = 'That picture is over 8 MB.')
    picture = f
    onpreview(URL.createObjectURL(f))
  }

  function builtIn() {
    picture = null
    onpreview(null)
  }

  async function lock() {
    busy = true
    error = ''
    try {
      await auth.setup(code, name.trim(), password, multi)
      if (picture) await auth.setImage(picture).catch(() => {})
      await newPairing()
      go(LOCK + 1)
    } catch (e) {
      error = (e as Error).message
      // something the server refused: back to that step
      if (/characters|common|name|over and over/i.test(error)) step = /enter yours/i.test(error) ? 0 : 4
      if (/setup code/i.test(error)) step = 3
    } finally {
      busy = false
    }
  }

  async function newPairing() {
    pairing = await auth.pairing().catch(() => null)
  }

  async function copy(what: string, text: string) {
    try {
      await navigator.clipboard.writeText(text)
      copied = what
      setTimeout(() => copied === what && (copied = ''), 1500)
    } catch {
      /* clipboard blocked: the text is there to select */
    }
  }
</script>

<form
  class="walk"
  oninput={() => (error = '')}
  onsubmit={(e) => {
    e.preventDefault()
    next()
  }}
>
  <ol class="dots" aria-label="Setup steps">
    {#each STEPS as s, i}
      <li class:on={i === step} class:done={i < step} aria-current={i === step ? 'step' : undefined} title={s}></li>
    {/each}
  </ol>

  <div class="body" bind:this={body}>
    {#if step === 0}
      <h1>What's your name?</h1>
      <p class="lead">It goes in the greeting when you sign in. You can change it any time in Settings → Sign-in, or leave it empty.</p>
      <label>
        <span>Your name</span>
        <input class="field" bind:value={name} autocomplete="given-name" autocapitalize="words" maxlength="40" />
      </label>
      <div class="preview">
        <span class="faint">Your sign-in screen will say</span>
        <b>{greeting(phase)}{name.trim() ? ',' : ''}</b>
        {#if name.trim()}<b class="nm">{name.trim()}</b>{/if}
      </div>
    {:else if step === 1}
      <h1>Welcome{first ? `, ${first}` : ''}</h1>
      <p class="lead">Flightplan is a US flight planner that runs on your own computer. A few things it does:</p>
      <ul class="points">
        <li><MapIcon size={16} /><span>A vector sectional with airspace, airports, terrain, a vertical profile and a navlog with official winds aloft.</span></li>
        <li><CloudSun size={16} /><span>Weather up to five days out and a go / no-go call against your personal minimums.</span></li>
        <li><Share2 size={16} /><span>A printable briefing, export to FltPlan Go and Garmin, and plans that sync with the desktop app.</span></li>
      </ul>
      <p class="lead"><Lock size={14} /> First, lock it with a password, so only you can open it, even if it's on the internet.</p>
    {:else if step === 2}
      <h1>Who uses it?</h1>
      <div class="modes" role="radiogroup" aria-label="Who uses Flightplan">
        <button type="button" role="radio" aria-checked={!multi} class="mode" class:on={!multi} onclick={() => (multi = false)}>
          <User size={18} />
          <span><b>Just me</b><small>Recommended. You're already picked on the sign-in screen{first ? ` ("${greeting(phase)}, ${first}")` : ''}: only the password is asked for.</small></span>
          {#if !multi}<Check size={16} class="tick" />{/if}
        </button>
        <button type="button" role="radio" aria-checked={multi} class="mode" class:on={multi} onclick={() => (multi = true)}>
          <Users size={18} />
          <span><b>Several people</b><small>Everyone signs in with their name and password and has their own plans, aircraft and minimums. You add them in Settings.</small></span>
          {#if multi}<Check size={16} class="tick" />{/if}
        </button>
      </div>
      <p class="faint small">You can switch later in Settings → Sign-in.</p>
    {:else if step === 3}
      <h1>Setup code</h1>
      <p class="lead">To prove you run this server, enter the one-time code it printed when it started.</p>
      <div class="where">
        <Terminal size={15} />
        <span>Look in the terminal or service log where <code>flightplan-server</code> runs, or in <code>data/setup-code.txt</code> next to its data.</span>
      </div>
      <label>
        <span>Setup code</span>
        <input class="field code" bind:value={code} placeholder="XXXX-XXXX-XXXX" autocomplete="one-time-code" autocapitalize="characters" spellcheck="false" maxlength="20" required />
      </label>
    {:else if step === 4}
      <h1>Choose a password</h1>
      <p class="lead">At least {min} characters. A few unrelated words are easy to remember and hard to guess.</p>
      <label>
        <span>Password</span>
        <span class="pw">
          <input class="field" bind:value={password} type={reveal ? 'text' : 'password'} autocomplete="new-password" required />
          <button type="button" class="eye" onclick={() => (reveal = !reveal)} aria-label={reveal ? 'Hide password' : 'Show password'}>
            {#if reveal}<EyeOff size={16} />{:else}<Eye size={16} />{/if}
          </button>
        </span>
      </label>
      <div class="meter" aria-live="polite">
        {#each [0, 1, 2] as i}<i class:on={password && score > i} data-s={score}></i>{/each}
        <span>{password ? LABEL[score] : ''}</span>
      </div>
      <label>
        <span>Same password again</span>
        <input class="field" bind:value={again} type={reveal ? 'text' : 'password'} autocomplete="new-password" required />
      </label>
    {:else if step === 5}
      <h1>Sign-in picture</h1>
      <p class="lead">Shown beside the sign-in form. Keep the built-in one, which follows the time of day, or use a photo of your own.</p>
      <div class="pick">
        <button type="button" class="choice" class:on={!picture} onclick={builtIn}>
          {#if !picture}<Check size={14} />{/if} Built-in
        </button>
        <button type="button" class="choice" class:on={!!picture} onclick={choosePicture}>
          {#if picture}<Check size={14} />{:else}<ImageUp size={14} />{/if}
          {picture ? picture.name : 'Choose a photo…'}
        </button>
      </div>
      <input class="hidden" type="file" accept="image/jpeg,image/png,image/webp,image/gif,image/avif" bind:this={fileInput} onchange={picked} />
      <p class="faint small">JPEG, PNG or WebP, up to 8 MB. You can change it later in Settings.</p>
    {:else if step === 6}
      <h1>Ready to lock</h1>
      <ul class="points">
        <li><ShieldCheck size={16} /><span>Every browser needs the password. This one stays signed in for 30 days of use (90 at most).</span></li>
        <li><KeyRound size={16} /><span>Your name, the password, {multi ? 'the people, ' : ''}the picture and signing out are under Settings → Sign-in.</span></li>
        <li><Terminal size={16} /><span>Forgot it? Run <code>flightplan-server set-password</code> on the server.</span></li>
        <li><Lock size={16} /><span>On the internet, serve it over HTTPS (a reverse proxy such as Caddy) with <code>FP_TRUST_PROXY=1</code>. See the README.</span></li>
      </ul>
    {:else}
      <h1>Connect the desktop app</h1>
      <p class="lead">Locked. If you also use the Flightplan desktop app, connect it now: plans you save there show up here straight away, and the other way round.</p>
      <ol class="how">
        <li>In the desktop app, open <b>Settings → Sync with a Flightplan server</b>.</li>
        <li>Enter this server's address and the pairing code, then press <b>Connect</b>.</li>
      </ol>
      <div class="kv">
        <span class="faint">Server address</span>
        <code class="big">{location.origin}</code>
        <button type="button" class="btn ghost ico" title="Copy the address" onclick={() => copy('url', location.origin)}>{#if copied === 'url'}<Check size={14} />{:else}<Copy size={14} />{/if}</button>
      </div>
      <div class="kv">
        <span class="faint">Pairing code</span>
        {#if pairing}
          <code class="big code">{pairing.code}</code>
          <button type="button" class="btn ghost ico" title="Copy the code" onclick={() => copy('code', pairing!.code)}>{#if copied === 'code'}<Check size={14} />{:else}<Copy size={14} />{/if}</button>
        {:else}
          <span class="faint">couldn't make one</span>
        {/if}
      </div>
      <p class="faint small">
        <Monitor size={12} /> The code works once, for {pairing?.minutes ?? 10} minutes.
        <button type="button" class="link" onclick={newPairing}><RefreshCw size={12} /> New code</button>
        You can connect later from Settings → Sign-in too.
      </p>
    {/if}
  </div>

  {#if error}<p class="err" role="alert">{error}</p>{/if}

  <div class="nav">
    {#if step > 0 && step <= LOCK}<button type="button" class="btn back" onclick={() => go(step - 1)} disabled={busy}><ArrowLeft size={15} /> Back</button>{/if}
    <button class="btn primary next" type="submit" disabled={busy || (step === 3 && code.replace(/[^a-z0-9]/gi, '').length < 12) || (step === 4 && (!password || !again))}>
      {#if busy}<LoaderCircle size={16} class="spin" />{/if}
      {#if step === 1}Let's lock it{:else if step === LOCK}<Lock size={15} /> Lock it{:else if step === LOCK + 1}Open Flightplan <ArrowRight size={15} />{:else}Next <ArrowRight size={15} />{/if}
    </button>
  </div>
</form>

<style>
  .walk {
    margin: auto 0;
    display: flex;
    flex-direction: column;
    gap: 16px;
    max-width: 380px;
    width: 100%;
    padding: 20px 0;
  }
  .dots {
    display: flex;
    gap: 6px;
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .dots li {
    width: 22px;
    height: 4px;
    border-radius: 2px;
    background: var(--line-strong);
  }
  .dots li.done {
    background: color-mix(in srgb, var(--accent) 45%, transparent);
  }
  .dots li.on {
    background: var(--accent);
    width: 34px;
  }
  .body {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  h1 {
    margin: 0;
    font-size: 28px;
    font-weight: 650;
    letter-spacing: -0.02em;
    line-height: 1.15;
  }
  .lead {
    margin: 0;
    color: var(--text-2);
    line-height: 1.5;
    display: flex;
    gap: 6px;
    align-items: baseline;
  }
  .points {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .points li {
    display: flex;
    gap: 10px;
    align-items: flex-start;
    line-height: 1.45;
  }
  .points :global(svg) {
    flex: none;
    margin-top: 2px;
    color: var(--accent);
  }
  .modes {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .mode {
    display: flex;
    gap: 12px;
    align-items: flex-start;
    text-align: left;
    padding: 12px 14px;
    border-radius: 12px;
    border: 1px solid var(--line-strong);
    background: var(--surface-solid);
    cursor: pointer;
    position: relative;
  }
  .mode.on {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .mode > :global(svg:first-child) {
    flex: none;
    margin-top: 1px;
    color: var(--accent);
  }
  .mode span {
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding-right: 18px;
  }
  .mode small {
    color: var(--text-2);
    font-size: 12px;
    line-height: 1.45;
  }
  .mode :global(.tick) {
    position: absolute;
    right: 12px;
    top: 12px;
    color: var(--accent);
  }
  .where {
    display: flex;
    gap: 8px;
    padding: 10px 12px;
    border-radius: 10px;
    background: var(--surface-2);
    font-size: 12.5px;
    line-height: 1.45;
    color: var(--text-2);
  }
  .where :global(svg) {
    flex: none;
    margin-top: 2px;
  }
  code {
    font-family: var(--mono);
    font-size: 11.5px;
    background: var(--surface-2);
    padding: 1px 5px;
    border-radius: 4px;
  }
  .where code {
    background: var(--surface-solid);
  }
  .how {
    margin: 0;
    padding-left: 18px;
    color: var(--text-2);
    line-height: 1.5;
  }
  .kv {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    border-radius: 10px;
    background: var(--surface-2);
  }
  .kv .faint {
    font-size: 11.5px;
    width: 92px;
    flex: none;
  }
  .kv .big {
    flex: 1;
    font-size: 13.5px;
    background: none;
    padding: 0;
    word-break: break-all;
  }
  .kv .big.code {
    letter-spacing: 0.1em;
    font-size: 15px;
    font-weight: 650;
  }
  .ico {
    width: 30px;
    height: 30px;
    padding: 0;
    justify-content: center;
    flex: none;
  }
  .link {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    border: 0;
    background: none;
    padding: 0;
    color: var(--accent);
    font-weight: 600;
    cursor: pointer;
    font-size: 11.5px;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: 12.5px;
    font-weight: 550;
    color: var(--text-2);
  }
  .field {
    height: 42px;
    font-size: 15px;
    border-radius: 999px;
    padding: 0 18px;
  }
  .field.code {
    font-family: var(--mono);
    letter-spacing: 0.12em;
    text-transform: uppercase;
    font-size: 16px;
  }
  .pw {
    position: relative;
    display: flex;
  }
  .pw .field {
    flex: 1;
    padding-right: 44px;
  }
  .eye {
    position: absolute;
    right: 8px;
    top: 50%;
    transform: translateY(-50%);
    border: 0;
    background: none;
    color: var(--text-3);
    cursor: pointer;
    display: flex;
    padding: 6px;
  }
  .meter {
    display: flex;
    align-items: center;
    gap: 5px;
    margin-top: -6px;
    font-size: 11.5px;
    color: var(--text-3);
  }
  .meter i {
    width: 44px;
    height: 5px;
    border-radius: 3px;
    background: var(--line-strong);
  }
  .meter i.on[data-s='1'] {
    background: var(--warn);
  }
  .meter i.on[data-s='2'],
  .meter i.on[data-s='3'] {
    background: var(--ok);
  }
  .meter span {
    margin-left: 4px;
  }
  .preview {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 12px 14px;
    border-radius: 12px;
    background: var(--surface-2);
    font-size: 20px;
  }
  .preview .faint {
    font-size: 11px;
  }
  .preview .nm {
    color: var(--accent);
  }
  .pick {
    display: flex;
    gap: 8px;
  }
  .choice {
    flex: 1;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    height: 42px;
    padding: 0 12px;
    border-radius: 12px;
    border: 1px solid var(--line-strong);
    background: var(--surface-solid);
    cursor: pointer;
    font-weight: 550;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .choice.on {
    border-color: var(--accent);
    color: var(--accent);
    background: var(--accent-soft);
  }
  .hidden {
    display: none;
  }
  .small {
    font-size: 11.5px;
    margin: -6px 0 0;
    line-height: 1.5;
  }
  .err {
    margin: 0;
    color: var(--bad);
    font-size: 12.5px;
  }
  .nav {
    display: flex;
    gap: 8px;
  }
  .nav .btn {
    height: 42px;
    border-radius: 999px;
    justify-content: center;
    font-size: 14px;
  }
  .next {
    flex: 1;
  }
  .nav :global(.spin) {
    animation: spin 1s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
