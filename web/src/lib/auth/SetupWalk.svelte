<script lang="ts">
  // First-run walkthrough that locks the web app: what Flightplan is, the one-time setup code
  // (proves you run the server), your name for the greeting, the password, the login picture,
  // then a summary. After it, the app's own setup (chart data, keys, aircraft) carries on.
  import { ArrowLeft, ArrowRight, Lock, LoaderCircle, Eye, EyeOff, Map as MapIcon, CloudSun, Share2, ShieldCheck, Terminal, ImageUp, Check, KeyRound } from '@lucide/svelte'
  import { tick } from 'svelte'
  import { auth, greeting, strength, type AuthStatus, type Phase } from './auth'

  let { status, phase, onsignedin, onpreview }: { status: AuthStatus; phase: Phase; onsignedin: () => void; onpreview: (url: string | null) => void } = $props()

  const STEPS = ['Welcome', 'Setup code', 'You', 'Password', 'Picture', 'Ready'] as const
  let step = $state(0)
  let code = $state('')
  let name = $state('')
  let password = $state('')
  let again = $state('')
  let reveal = $state(false)
  let picture = $state<File | null>(null)
  let busy = $state(false)
  let error = $state('')
  let body = $state<HTMLElement>()
  // a plain file input: the app's file helpers load stores that call the API, which isn't open yet
  let fileInput = $state<HTMLInputElement>()

  const min = $derived(status.minPassword || 10)
  const score = $derived(strength(password))
  const LABEL = ['Too weak', 'Fair', 'Good', 'Strong']

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
    body?.querySelector<HTMLInputElement>('input:not([type=checkbox])')?.focus()
  }

  async function next() {
    if (busy) return
    error = ''
    if (step === 1) {
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
    if (step === 3) {
      if (pwProblem) return (error = pwProblem)
      if (password !== again) return (error = "The two passwords don't match.")
    }
    if (step < STEPS.length - 1) go(step + 1)
    else finish()
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

  async function finish() {
    busy = true
    error = ''
    try {
      await auth.setup(code, name.trim(), password)
      if (picture) await auth.setImage(picture).catch(() => {})
      onsignedin()
    } catch (e) {
      error = (e as Error).message
      // a password the server refused: back to that step
      if (/characters|common|name|over and over/i.test(error)) step = 3
      if (/setup code/i.test(error)) step = 1
    } finally {
      busy = false
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
      <h1>Welcome to Flightplan</h1>
      <p class="lead">A US flight planner that runs on your own computer. A few things it does:</p>
      <ul class="points">
        <li><MapIcon size={16} /><span>A vector sectional with airspace, airports, terrain, a vertical profile and a navlog with official winds aloft.</span></li>
        <li><CloudSun size={16} /><span>Weather up to five days out and a go / no-go call against your personal minimums.</span></li>
        <li><Share2 size={16} /><span>A printable briefing, and export to FltPlan Go, Garmin and more.</span></li>
      </ul>
      <p class="lead"><Lock size={14} /> First, lock it with a password, so only you can open it, even if it's on the internet.</p>
    {:else if step === 1}
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
    {:else if step === 2}
      <h1>Who's flying?</h1>
      <p class="lead">Your name goes in the greeting on the sign-in screen. Leave it empty if you'd rather not show one.</p>
      <label>
        <span>Your name</span>
        <input class="field" bind:value={name} autocomplete="given-name" maxlength="40" />
      </label>
      <div class="preview">
        <span class="faint">Preview</span>
        <b>{greeting(phase)}{name.trim() ? ',' : ''}</b>
        {#if name.trim()}<b class="nm">{name.trim()}</b>{/if}
      </div>
    {:else if step === 3}
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
    {:else if step === 4}
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
    {:else}
      <h1>Ready to lock</h1>
      <ul class="points">
        <li><ShieldCheck size={16} /><span>Every browser needs the password. This one stays signed in for 30 days of use (90 at most).</span></li>
        <li><KeyRound size={16} /><span>Change the password, sign out, or sign out every device under Settings → Sign-in.</span></li>
        <li><Terminal size={16} /><span>Forgot it? Run <code>flightplan-server set-password</code> on the server.</span></li>
        <li><Lock size={16} /><span>On the internet, serve it over HTTPS (a reverse proxy such as Caddy) with <code>FP_TRUST_PROXY=1</code>. See the README.</span></li>
      </ul>
      <p class="lead">Next, the app walks you through chart data, keys and your aircraft.</p>
    {/if}
  </div>

  {#if error}<p class="err" role="alert">{error}</p>{/if}

  <div class="nav">
    {#if step > 0}<button type="button" class="btn back" onclick={() => go(step - 1)} disabled={busy}><ArrowLeft size={15} /> Back</button>{/if}
    <button class="btn primary next" type="submit" disabled={busy || (step === 1 && code.replace(/[^a-z0-9]/gi, '').length < 12) || (step === 3 && (!password || !again))}>
      {#if busy}<LoaderCircle size={16} class="spin" />{/if}
      {#if step === 0}Let's lock it{:else if step === STEPS.length - 1}<Lock size={15} /> Lock it and open Flightplan{:else}Next <ArrowRight size={15} />{/if}
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
