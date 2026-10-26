<script lang="ts">
  // The sign-in screen: a greeting for the time of day with the pilot's name and the password
  // on the left, the login picture (your own, or the built-in one) on the right. On a fresh
  // install it is a short walkthrough instead (SetupWalk) that sets the first password.
  import { Lock, LoaderCircle, Eye, EyeOff, ChevronDown } from '@lucide/svelte'
  import { onMount } from 'svelte'
  import LoginArt from './LoginArt.svelte'
  import SetupWalk from './SetupWalk.svelte'
  import { auth, greeting, phaseOf, type AuthStatus } from './auth'

  let { status, onsignedin }: { status: AuthStatus; onsignedin: () => void } = $props()

  let phase = $state(phaseOf())
  let password = $state('')
  let reveal = $state(false)
  let busy = $state(false)
  let error = $state('')
  let forgot = $state(false)
  let imageFailed = $state(false)
  /** a picture chosen during setup, shown before it's uploaded */
  let preview = $state<string | null>(null)
  let input = $state<HTMLInputElement>()

  onMount(() => {
    input?.focus()
    const id = setInterval(() => (phase = phaseOf()), 60_000)
    return () => clearInterval(id)
  })

  async function submit(e: Event) {
    e.preventDefault()
    if (busy) return
    error = ''
    busy = true
    try {
      await auth.login(password)
      onsignedin()
    } catch (err) {
      error = (err as Error).message
      password = ''
      input?.focus()
    } finally {
      busy = false
    }
  }
</script>

<main class="login" data-phase={phase}>
  <section class="side">
    <div class="brand">
      <svg viewBox="0 0 24 24" width="26" height="26" aria-hidden="true">
        <path d="M3 17 L10 13 L14 15 L21 7" fill="none" stroke="var(--accent)" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" />
        <circle cx="3" cy="17" r="2.2" fill="var(--accent)" />
        <circle cx="21" cy="7" r="2.2" fill="var(--accent)" />
      </svg>
      <span>Flightplan</span>
    </div>

    {#if !status.configured}
      <SetupWalk {status} {phase} {onsignedin} onpreview={(url) => (preview = url)} />
    {:else}
      <form onsubmit={submit} oninput={() => (error = '')}>
        <h1>{greeting(phase)}{status.name ? ',' : ''}</h1>
        {#if status.name}<div class="name">{status.name}</div>{/if}
        <label>
          <span>Password</span>
          <span class="pw">
            <input class="field" bind:this={input} bind:value={password} type={reveal ? 'text' : 'password'} autocomplete="current-password" aria-invalid={!!error} required />
            <button type="button" class="eye" onclick={() => (reveal = !reveal)} aria-label={reveal ? 'Hide password' : 'Show password'}>
              {#if reveal}<EyeOff size={16} />{:else}<Eye size={16} />{/if}
            </button>
          </span>
        </label>
        {#if error}<p class="err" role="alert">{error}</p>{/if}
        <button class="btn primary go" type="submit" disabled={busy || !password}>
          {#if busy}<LoaderCircle size={16} class="spin" />{:else}<Lock size={15} />{/if}
          Sign in
        </button>
        <button type="button" class="forgot" onclick={() => (forgot = !forgot)} aria-expanded={forgot}>
          Forgot your password? <ChevronDown size={13} style={`transform: rotate(${forgot ? 180 : 0}deg)`} />
        </button>
        {#if forgot}
          <p class="hint">
            On the computer that runs Flightplan, run <code>flightplan-server set-password</code> (from the <code>server</code> folder). It sets a new
            password and signs every device out.
          </p>
        {/if}
      </form>
    {/if}

    <p class="foot">For planning only. Not a substitute for an official briefing.</p>
  </section>

  <section class="art">
    {#if preview}
      <img src={preview} alt="" />
    {:else if status.hasImage && !imageFailed}
      <img src="/api/auth/image" alt="" onerror={() => (imageFailed = true)} />
    {:else}
      <LoginArt {phase} />
    {/if}
  </section>
</main>

<style>
  .login {
    position: fixed;
    inset: 0;
    display: grid;
    grid-template-columns: minmax(360px, 36%) 1fr;
    background: var(--bg);
  }
  .side {
    display: flex;
    flex-direction: column;
    padding: 36px 44px;
    background: var(--surface-solid);
    border-right: 1px solid var(--line);
    overflow-y: auto;
    z-index: 1;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    font-weight: 650;
    font-size: 15px;
  }
  form {
    margin: auto 0;
    display: flex;
    flex-direction: column;
    gap: 14px;
    max-width: 360px;
    width: 100%;
  }
  h1 {
    margin: 0;
    font-size: 32px;
    font-weight: 650;
    letter-spacing: -0.02em;
    line-height: 1.1;
  }
  .name {
    font-size: 32px;
    font-weight: 750;
    letter-spacing: -0.02em;
    line-height: 1.1;
    margin: -10px 0 10px;
    color: var(--accent);
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
  .go {
    height: 42px;
    border-radius: 999px;
    justify-content: center;
    font-size: 14px;
  }
  .go :global(.spin) {
    animation: spin 1s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  .err {
    margin: -4px 0 0;
    color: var(--bad);
    font-size: 12.5px;
  }
  .forgot {
    align-self: flex-start;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    border: 0;
    background: none;
    padding: 0;
    cursor: pointer;
    color: var(--text-3);
    font-size: 12px;
  }
  .hint {
    margin: -6px 0 0;
    font-size: 12px;
    line-height: 1.5;
    color: var(--text-2);
  }
  code {
    font-family: var(--mono);
    font-size: 11.5px;
    background: var(--surface-2);
    padding: 1px 5px;
    border-radius: 4px;
  }
  .foot {
    margin: 0;
    font-size: 11px;
    color: var(--text-3);
  }
  .art {
    position: relative;
    overflow: hidden;
    background: #1b2433;
  }
  .art img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
  @media (max-width: 760px) {
    .login {
      grid-template-columns: 1fr;
      grid-template-rows: 28vh 1fr;
    }
    .art {
      grid-row: 1;
    }
    .side {
      grid-row: 2;
      border-right: 0;
      border-top: 1px solid var(--line);
      padding: 24px;
    }
    h1,
    .name {
      font-size: 26px;
    }
  }
</style>
