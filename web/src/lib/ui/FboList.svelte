<script lang="ts">
  import { Phone, Globe, Fuel, MapPin } from '@lucide/svelte'

  let { airport }: { airport: any } = $props()
  let fbos = $state<any[] | null>(null)
  let error = $state('')

  $effect(() => {
    const id = airport?.icao || airport?.id
    if (!id) return
    fbos = null
    error = ''
    fetch(`/api/fbo/${encodeURIComponent(id)}`)
      .then((r) => (r.ok ? r.json() : r.status === 404 ? [] : Promise.reject(r.statusText)))
      .then((d) => (fbos = d))
      .catch((e) => (error = String(e)))
  })
</script>

{#if fbos === null && !error}
  <p class="faint">Looking up FBOs…</p>
{:else if error}
  <p class="muted">Couldn't load FBO data ({error}).</p>
{:else if fbos && fbos.length === 0}
  <p class="muted">No FBO listed for this field. Fuel and services above come from the FAA airport record.</p>
{:else if fbos}
  {#each fbos as f}
    <div class="fbo">
      <div class="head">
        <b>{f.name}</b>
        {#if f.brand}<span class="chip">{f.brand}</span>{/if}
      </div>
      {#if f.fuel?.length}
        <div class="row"><Fuel size={13} /> {f.fuel.join(' · ')}</div>
      {/if}
      {#if f.prices?.length}
        <div class="prices">
          {#each f.prices as p}
            <span><b class="num">${p.price.toFixed(2)}</b> {p.fuel} <small class="faint">{p.service ?? ''} · {p.updated}</small></span>
          {/each}
        </div>
      {/if}
      {#if f.phone}<a class="row" href={`tel:${f.phone}`}><Phone size={13} /> {f.phone}</a>{/if}
      {#if f.unicom}<div class="row muted">Radio {f.unicom}</div>{/if}
      {#if f.website}<a class="row" href={f.website} target="_blank" rel="noopener noreferrer"><Globe size={13} /> {f.website.replace(/^https?:\/\/(www\.)?/, '').replace(/\/$/, '')}</a>{/if}
      {#if f.address}<div class="row muted"><MapPin size={13} /> {f.address}</div>{/if}
      {#if f.hours}<div class="row muted">{f.hours}</div>{/if}
      {#if f.amenities?.length}<div class="chips">{#each f.amenities as a}<span class="chip">{a}</span>{/each}</div>{/if}
      <div class="src faint">Source: {f.sources.join(', ')}</div>
    </div>
  {/each}
{/if}

<style>
  .fbo {
    padding: 12px 0;
    border-bottom: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    gap: 5px;
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-size: 14px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 6px;
    color: inherit;
    text-decoration: none;
  }
  a.row {
    color: var(--blue);
  }
  .prices {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .src {
    font-size: 10.5px;
  }
</style>
