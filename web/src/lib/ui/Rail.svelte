<script lang="ts">
  import { Layers, Route, CloudSun, Plane, FileText, CircleHelp, Moon, Sun, Settings, Lock, LockOpen } from '@lucide/svelte'
  import { app, toast, type Panel } from '../stores/app.svelte'
  import { route } from '../stores/route.svelte'

  const items: { id: Panel; label: string; icon: typeof Layers }[] = [
    { id: 'route', label: 'Route', icon: Route },
    { id: 'layers', label: 'Layers', icon: Layers },
    { id: 'weather', label: 'Weather', icon: CloudSun },
    { id: 'aircraft', label: 'Aircraft', icon: Plane },
    { id: 'briefing', label: 'Briefing', icon: FileText },
  ]

  function toggle(id: Panel) {
    app.panel = app.panel === id ? 'none' : id
  }
</script>

<nav class="rail glass" data-tour="rail">
  <div class="brand" title="Flightplan">
    <svg viewBox="0 0 24 24" width="22" height="22" aria-hidden="true">
      <path d="M3 17 L10 13 L14 15 L21 7" fill="none" stroke="var(--accent)" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" />
      <circle cx="3" cy="17" r="2.2" fill="var(--accent)" />
      <circle cx="21" cy="7" r="2.2" fill="var(--accent)" />
    </svg>
  </div>
  {#each items as it}
    <button class:active={app.panel === it.id} onclick={() => toggle(it.id)} title={it.label} data-tour={`rail-${it.id}`}>
      <it.icon size={19} strokeWidth={1.8} />
      <span>{it.label}</span>
    </button>
  {/each}
  <div class="spacer"></div>
  <button
    class:active={route.locked}
    onclick={() => {
      route.locked = !route.locked
      toast(route.locked ? 'View only: the plan is locked (L to edit)' : 'Editing on')
    }}
    title={route.locked ? "View only: the plan can't be changed. Click (or L) to edit." : 'Editing. Click (or L) to lock the plan for viewing.'}
    data-tour="rail-lock"
  >
    {#if route.locked}<Lock size={18} strokeWidth={1.8} />{:else}<LockOpen size={18} strokeWidth={1.8} />{/if}
    <span>{route.locked ? 'Locked' : 'Edit'}</span>
  </button>
  <button class:active={app.panel === 'settings'} onclick={() => toggle('settings')} title="Settings & API keys" data-tour="rail-settings">
    <Settings size={18} strokeWidth={1.8} />
    <span>Settings</span>
  </button>
  <button onclick={() => (app.theme = app.theme === 'day' ? 'night' : 'day')} title="Day / night chart" data-tour="rail-theme">
    {#if app.theme === 'day'}<Moon size={18} strokeWidth={1.8} />{:else}<Sun size={18} strokeWidth={1.8} />{/if}
    <span>{app.theme === 'day' ? 'Night' : 'Day'}</span>
  </button>
  <button class:active={app.panel === 'help'} onclick={() => toggle('help')} title="Help & tutorial" data-tour="rail-help">
    <CircleHelp size={18} strokeWidth={1.8} />
    <span>Help</span>
  </button>
</nav>

<style>
  .rail {
    position: absolute;
    top: 12px;
    left: 12px;
    bottom: 12px;
    width: 64px;
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 10px 0;
    gap: 2px;
    z-index: 20;
  }
  .brand {
    height: 40px;
    display: grid;
    place-items: center;
    margin-bottom: 8px;
  }
  button {
    width: 54px;
    padding: 8px 0 6px;
    border: 0;
    background: transparent;
    border-radius: 10px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 3px;
    color: var(--text-2);
    cursor: pointer;
    transition: background 0.15s, color 0.15s;
  }
  button span {
    font-size: 10px;
    font-weight: 500;
  }
  button:hover {
    background: var(--surface-2);
    color: var(--text);
  }
  button.active {
    background: var(--accent-soft);
    color: var(--accent);
  }
  .spacer {
    flex: 1;
  }
  @media (max-width: 640px) {
    .rail {
      top: auto;
      bottom: 0;
      left: 0;
      right: 0;
      width: auto;
      height: 64px;
      flex-direction: row;
      justify-content: space-around;
      padding: 0 4px;
      border-radius: 0;
    }
    .brand,
    .spacer {
      display: none;
    }
    button {
      width: auto;
      flex: 1;
    }
  }
</style>
