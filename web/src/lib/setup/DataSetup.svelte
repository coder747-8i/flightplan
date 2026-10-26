<script lang="ts">
  // Install or update the chart & airport data pack: download it, import a .zip, or copy it
  // from a folder. Progress comes from the server, which does the work. A server that builds its
  // own data (the Proxmox LXC) shows that build's progress instead, with the other ways folded away.
  import { CloudDownload, FileArchive, FolderOpen, CircleCheck, TriangleAlert, LoaderCircle, X, RefreshCw, Server, ChevronDown } from '@lucide/svelte'
  import { onDestroy } from 'svelte'
  import { api, type SetupJob } from '../api'
  import { onboarding } from '../stores/onboarding.svelte'
  import { config } from '../stores/config.svelte'
  import { pickFiles } from '../io/files'

  let { compact = false }: { compact?: boolean } = $props()

  const ACTIVE: SetupJob['state'][] = ['downloading', 'uploading', 'copying', 'extracting', 'checking']
  const status = $derived(onboarding.status)
  const job = $derived(status?.job)
  const busy = $derived(!!job && ACTIVE.includes(job.state))
  const info = $derived(status?.info)
  const expired = $derived(!!info?.expires && new Date(`${info.expires}T09:01:00Z`).getTime() < Date.now())

  let url = $state('')
  let editUrl = $state(false)
  let folderPath = $state('')
  let error = $state('')
  let finished = $state(false)
  let timer: ReturnType<typeof setInterval> | undefined

  const builder = $derived(status?.builder ?? null)
  const BUILDING = ['queued', 'checking', 'building']
  const serverBusy = $derived(!!builder && BUILDING.includes(builder.state))
  let otherWays = $state(false)
  let now = $state(Date.now())
  // follow the server's own build; when it lands, the chart reloads onto it
  $effect(() => {
    if (!serverBusy) return
    const wasInstalled = !!status?.installed
    const cycle = info?.cycle
    const t = setInterval(async () => {
      now = Date.now()
      const s = await onboarding.refresh()
      if (s && s.installed && (!wasInstalled || s.info?.cycle !== cycle)) {
        onboarding.dataChanged = true
        config.reload()
      }
    }, 4000)
    return () => clearInterval(t)
  })
  const elapsed = $derived(builder?.started ? Math.max(0, Math.round((now / 1000 - builder.started) / 60)) : 0)

  $effect(() => {
    if (!url && status) url = onboarding.prefs.dataUrl || status.defaultUrl
  })

  function stop() {
    clearInterval(timer)
    timer = undefined
  }

  function poll() {
    stop()
    timer = setInterval(async () => {
      const s = await onboarding.refresh()
      if (!s || ACTIVE.includes(s.job.state)) return
      stop()
      if (s.job.state === 'done') {
        finished = true
        onboarding.dataChanged = true
        config.reload()
      }
    }, 600)
  }
  // pick up an install that was already running (e.g. the panel was reopened)
  $effect(() => {
    if (busy && !timer) poll()
  })
  onDestroy(stop)

  async function run(action: () => Promise<{ cancelled?: boolean } | unknown>) {
    error = ''
    finished = false
    try {
      poll()
      const r = (await action()) as { cancelled?: boolean } | undefined
      if (r?.cancelled) stop()
    } catch (e) {
      error = (e as Error).message
    } finally {
      onboarding.refresh()
    }
  }

  function download() {
    const custom = url.trim() && url.trim() !== status?.defaultUrl ? url.trim() : undefined
    onboarding.savePrefs({ dataUrl: custom })
    run(() => api.setupDownload(custom))
  }

  async function importZip() {
    const [file] = await pickFiles('.zip,application/zip')
    if (file) run(() => api.setupUpload(file))
  }

  function folder() {
    run(() => api.setupFolder(status?.canPickFolder ? undefined : folderPath.trim()))
  }

  const mb = (n: number) => (n / 1e6).toFixed(n > 1e8 ? 0 : 1)
  const phase: Record<string, string> = {
    downloading: 'Downloading',
    uploading: 'Reading file',
    copying: 'Copying',
    extracting: 'Unpacking',
    checking: 'Checking',
  }
</script>

<div class="data" class:compact>
  {#if status?.installed}
    <div class="state" class:warn={expired}>
      {#if expired}<TriangleAlert size={16} />{:else}<CircleCheck size={16} />{/if}
      <div>
        <b>{expired ? 'Chart data is out of date' : 'Chart data installed'}</b>
        <div class="faint">
          {#if info?.cycle}{`FAA cycle ${info.cycle}`}{#if info.expires}{` · ${expired ? 'expired' : 'good until'} ${info.expires}`}{/if}{:else}Built locally (no cycle date){/if}
        </div>
      </div>
    </div>
  {:else if status}
    <div class="state none">
      <CloudDownload size={16} />
      <div>
        <b>{builder && builder.state !== 'error' ? 'Chart data on its way' : 'Chart data needed'}</b>
        <div class="faint">Airports, navaids, airspace, obstacles and chart tiles{builder ? ', built on this server from the FAA every 28 days.' : ': one download of a few hundred MB, updated every 28 days.'}</div>
      </div>
    </div>
  {/if}

  {#if builder && !busy}
    {#if serverBusy}
      <div class="progress">
        <div class="row">
          <span><LoaderCircle size={14} class="spin" /> {builder.state === 'building' ? `Building${builder.cycle ? ` cycle ${builder.cycle}` : ''}: ${builder.step ?? '…'}` : builder.state === 'checking' ? 'Checking the FAA for the current cycle…' : 'Waiting to start…'}</span>
          {#if builder.state === 'building'}<span class="faint num">{elapsed} min</span>{/if}
        </div>
        <div class="bar"><div style:width="35%" class="indeterminate"></div></div>
        <p class="faint small-note">
          <Server size={13} /> This server builds the chart data itself from the FAA, usually in 10 to 30 minutes. {status?.installed ? 'The current data stays in use until the new cycle is ready.' : 'You can carry on with setup meanwhile; the chart fills in when it finishes.'}
        </p>
      </div>
    {:else if builder.state === 'error'}
      <p class="err">{builder.message ?? 'The chart data build failed.'} It tries again every 6 hours, or run <code>flightplan-data-update</code> in the container.</p>
    {:else if status?.installed}
      <p class="faint small-note"><Server size={13} /> This server builds new chart data itself when each FAA cycle starts, and connected desktop apps get it from here.</p>
    {/if}
    {#if !otherWays}
      <button class="link" onclick={() => (otherWays = true)}><ChevronDown size={13} /> Install data another way</button>
    {/if}
  {/if}

  {#if builder && !busy && !otherWays}
    <!-- the server's own build covers it -->
  {:else if busy && job}
    <div class="progress">
      <div class="row">
        <span><LoaderCircle size={14} class="spin" /> {phase[job.state] ?? job.state}…</span>
        <span class="faint num">{#if job.total}{mb(job.done)} / {mb(job.total)} MB{:else if job.done}{mb(job.done)} MB{/if}</span>
        {#if ['downloading', 'uploading', 'copying', 'extracting'].includes(job.state)}
          <button class="btn ghost small" onclick={() => api.setupCancel()}><X size={13} /> Cancel</button>
        {/if}
      </div>
      {#key job.state}
        <div class="bar"><div style:width={job.total ? `${Math.min(100, (job.done / job.total) * 100)}%` : '35%'} class:indeterminate={!job.total}></div></div>
      {/key}
    </div>
  {:else}
    {#if finished}<p class="ok"><CircleCheck size={14} /> Installed. The chart will load the new data.</p>{/if}
    {#if job?.state === 'error' && !finished}<p class="err">{job.message}</p>{/if}
    {#if error}<p class="err">{error}</p>{/if}

    <div class="options">
      <div class="opt">
        <div class="t"><CloudDownload size={17} /> <b>{status?.installed ? 'Download the latest' : 'Download'}</b></div>
        <p class="faint">From the Flightplan data release.</p>
        {#if editUrl}
          <input class="field mono small-input" bind:value={url} spellcheck="false" />
        {:else}
          <button class="link" onclick={() => (editUrl = true)}>Change address</button>
        {/if}
        <button class="btn primary" onclick={download}>{#if status?.installed}<RefreshCw size={14} />{:else}<CloudDownload size={14} />{/if} Download</button>
      </div>
      <div class="opt">
        <div class="t"><FileArchive size={17} /> <b>Import a file</b></div>
        <p class="faint">A <code>flightplan-data.zip</code> you downloaded or were given.</p>
        <button class="btn" onclick={importZip}><FileArchive size={14} /> Choose .zip</button>
      </div>
      <div class="opt">
        <div class="t"><FolderOpen size={17} /> <b>Use a folder</b></div>
        <p class="faint">An unzipped pack, or a Flightplan checkout with <code>data/build</code> and <code>tiles</code>.</p>
        {#if !status?.canPickFolder}<input class="field mono small-input" bind:value={folderPath} placeholder="/path/to/flightplan" spellcheck="false" />{/if}
        <button class="btn" onclick={folder} disabled={!status?.canPickFolder && !folderPath.trim()}><FolderOpen size={14} /> {status?.canPickFolder ? 'Choose folder' : 'Copy from folder'}</button>
      </div>
    </div>
    {#if !compact && status}<p class="faint where">Stored in <code>{status.dataDir}</code></p>{/if}
  {/if}
</div>

<style>
  .small-note {
    display: flex;
    gap: 6px;
    align-items: flex-start;
    margin: 8px 0 4px;
    font-size: 12.5px;
    line-height: 1.45;
  }
  .small-note :global(svg) {
    flex: none;
    margin-top: 2px;
  }
  .state {
    display: flex;
    gap: 10px;
    align-items: flex-start;
    padding: 11px 12px;
    border-radius: 10px;
    background: rgba(31, 157, 106, 0.09);
    color: var(--ok);
    margin-bottom: 12px;
    line-height: 1.4;
  }
  .state :global(svg) {
    flex: none;
    margin-top: 2px;
  }
  .state b {
    color: var(--text);
  }
  .state.warn {
    background: rgba(212, 138, 11, 0.1);
    color: var(--warn);
  }
  .state.none {
    background: var(--accent-soft);
    color: var(--accent);
  }
  .options {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 8px;
  }
  .compact .options {
    grid-template-columns: 1fr;
  }
  .opt {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 12px;
    border-radius: 11px;
    border: 1px solid var(--line-strong);
    background: var(--surface-solid);
  }
  .opt .btn {
    margin-top: auto;
    align-self: flex-start;
  }
  .t {
    display: flex;
    gap: 7px;
    align-items: center;
  }
  .opt p {
    margin: 0;
    font-size: 11.5px;
    line-height: 1.4;
  }
  .small-input {
    height: 30px;
    font-size: 11px;
  }
  .link {
    border: 0;
    background: none;
    padding: 0;
    color: var(--accent);
    font-size: 11.5px;
    cursor: pointer;
    align-self: flex-start;
  }
  code {
    font-family: var(--mono);
    font-size: 10.5px;
    overflow-wrap: anywhere;
  }
  .progress {
    padding: 12px;
    border-radius: 11px;
    border: 1px solid var(--line-strong);
  }
  .progress .row {
    display: flex;
    gap: 10px;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 8px;
  }
  .progress .row span:first-child {
    display: flex;
    align-items: center;
    gap: 6px;
    font-weight: 600;
  }
  .bar {
    height: 6px;
    border-radius: 99px;
    background: var(--surface-2);
    overflow: hidden;
  }
  .bar div {
    height: 100%;
    background: var(--accent);
    border-radius: 99px;
    transition: width 0.3s;
  }
  .bar .indeterminate {
    animation: slide 1.2s ease-in-out infinite;
  }
  @keyframes slide {
    from { transform: translateX(-100%); }
    to { transform: translateX(300%); }
  }
  .btn.small {
    height: 26px;
    font-size: 12px;
  }
  .ok,
  .err {
    font-size: 12px;
    margin: 0 0 10px;
    line-height: 1.45;
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .ok {
    color: var(--ok);
    font-weight: 600;
  }
  .err {
    color: var(--bad);
  }
  .where {
    font-size: 11px;
    margin: 10px 0 0;
  }
  :global(.spin) {
    animation: spin 1s linear infinite;
  }
  @keyframes spin {
    to { transform: rotate(360deg); }
  }
  @media (max-width: 720px) {
    .options {
      grid-template-columns: 1fr;
    }
  }
</style>
