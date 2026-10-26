<script lang="ts">
  import { Search, Plane, RadioTower, Triangle } from '@lucide/svelte'
  import { api, type Place } from '../api'
  import { app } from '../stores/app.svelte'
  import { route } from '../stores/route.svelte'
  import { mapRef } from '../chart/mapRef'

  let q = $state('')
  let results = $state<Place[]>([])
  let active = $state(0)
  let open = $state(false)
  let input: HTMLInputElement
  let timer: ReturnType<typeof setTimeout>

  $effect(() => {
    const term = q
    clearTimeout(timer)
    if (term.trim().length < 2) {
      results = []
      return
    }
    timer = setTimeout(async () => {
      try {
        results = await api.search(term)
        active = 0
      } catch {
        results = []
      }
    }, 140)
  })

  function pick(p: Place, addToRoute = false) {
    open = false
    q = ''
    if (addToRoute && !route.locked) {
      route.add(p)
      app.panel = 'route'
    }
    mapRef.map?.flyTo({ center: [p.lon, p.lat], zoom: Math.max(mapRef.map.getZoom(), p.kind === 'airport' ? 11 : 10) })
    if (p.kind === 'airport') app.selection = { kind: 'airport', id: p.id }
  }

  function key(e: KeyboardEvent) {
    if (e.key === 'ArrowDown') active = Math.min(active + 1, results.length - 1)
    else if (e.key === 'ArrowUp') active = Math.max(active - 1, 0)
    else if (e.key === 'Enter' && results[active]) pick(results[active], e.shiftKey)
    else if (e.key === 'Escape') {
      open = false
      input.blur()
    } else return
    e.preventDefault()
  }

  function globalKey(e: KeyboardEvent) {
    if (e.key === '/' && document.activeElement?.tagName !== 'INPUT' && document.activeElement?.tagName !== 'TEXTAREA') {
      e.preventDefault()
      input.focus()
    }
  }
</script>

<svelte:window onkeydown={globalKey} />

<div class="search" data-tour="search">
  <div class="box glass">
    <Search size={16} strokeWidth={2} />
    <input
      bind:this={input}
      bind:value={q}
      onfocus={() => (open = true)}
      onblur={() => setTimeout(() => (open = false), 150)}
      onkeydown={key}
      placeholder="Search airports, navaids, waypoints"
      spellcheck="false"
      autocomplete="off"
    />
    <kbd>/</kbd>
  </div>
  {#if open && results.length}
    <ul class="results glass">
      {#each results as r, i}
        <li>
          <button class:active={i === active} onmousedown={() => pick(r)} onmouseenter={() => (active = i)}>
            <span class="ico">
              {#if r.kind === 'airport'}<Plane size={14} />{:else if r.kind === 'navaid'}<RadioTower size={14} />{:else}<Triangle size={12} />{/if}
            </span>
            <b>{r.id}</b>
            <span class="nm">{r.name}</span>
            <span class="faint dt">{r.detail}</span>
          </button>
        </li>
      {/each}
      <li class="hint faint">Enter to view · Shift+Enter to add to route</li>
    </ul>
  {/if}
</div>

<style>
  .search {
    position: absolute;
    top: 12px;
    left: 50%;
    transform: translateX(-50%);
    width: min(440px, calc(100vw - 340px));
    z-index: 16;
  }
  .box {
    height: 40px;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 12px;
    border-radius: 999px;
    color: var(--text-3);
  }
  input {
    flex: 1;
    border: 0;
    background: transparent;
    outline: none;
    font-size: 13.5px;
  }
  kbd {
    font-family: var(--mono);
    font-size: 11px;
    border: 1px solid var(--line-strong);
    border-radius: 5px;
    padding: 0 5px;
  }
  .results {
    list-style: none;
    margin: 6px 0 0;
    padding: 6px;
  }
  .results button {
    width: 100%;
    display: grid;
    grid-template-columns: 20px 52px 1fr auto;
    gap: 8px;
    align-items: center;
    text-align: left;
    border: 0;
    background: none;
    padding: 7px 8px;
    border-radius: 8px;
    cursor: pointer;
  }
  .results button.active {
    background: var(--accent-soft);
  }
  .ico {
    color: var(--text-3);
    display: grid;
    place-items: center;
  }
  .nm {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .dt {
    font-size: 11.5px;
    white-space: nowrap;
  }
  .hint {
    font-size: 11px;
    padding: 6px 8px 2px;
  }
  @media (max-width: 640px) {
    .search {
      left: 8px;
      right: 8px;
      width: auto;
      transform: none;
      top: 8px;
    }
  }
</style>
