<script lang="ts">
  import { Phone, Globe, Fuel, MapPin } from '@lucide/svelte'

  let { airport }: { airport: any } = $props()
  let fbos = $state<any[] | null>(null)
  let showOthers = $state(false)
  const main = $derived((fbos ?? []).filter((f) => f.kind !== 'service'))
  const others = $derived((fbos ?? []).filter((f) => f.kind === 'service'))
  const notes = $derived([...new Set((fbos ?? []).flatMap((f) => f.notes ?? []))] as string[])
  const remarkFuel = $derived((airport?.remarks ?? []).filter((r: any) => r.category === 'fuel').map((r: any) => r.plain))
  const fuelNotes = $derived(notes.length ? notes : remarkFuel)
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
{:else if fbos}
  {#if fuelNotes.length}
    <div class="section-title">Fuel & service notes (FAA)</div>
    <ul class="notes">{#each fuelNotes as n}<li>{n}</li>{/each}</ul>
  {/if}
  {#if !main.length}
    <p class="muted">No FBO found for this field{others.length ? ', but there are aviation businesses on the field below' : ''}. Fuel and services on the Info tab come from the FAA airport record.</p>
  {:else}
    <div class="section-title">FBOs</div>
  {/if}
  {#each main as f}
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
      {#if f.website}<a class="row" href={f.website.startsWith('http') ? f.website : `https://${f.website}`} target="_blank" rel="noopener noreferrer"><Globe size={13} /> {f.website.replace(/^https?:\/\/(www\.)?/, '').replace(/\/$/, '')}</a>{/if}
      {#if f.address}<div class="row muted"><MapPin size={13} /> {f.address}</div>{/if}
      {#if f.hours}<div class="row muted">{f.hours}</div>{/if}
      {#if f.amenities?.length}<div class="chips">{#each f.amenities as a}<span class="chip">{a}</span>{/each}</div>{/if}
      <div class="src faint">Source: {f.sources.join(', ')}</div>
    </div>
  {/each}
  {#if others.length}
    <button class="more" onclick={() => (showOthers = !showOthers)}>{showOthers ? 'Hide' : 'Show'} {others.length} other aviation businesses (maintenance, charter, schools)</button>
    {#if showOthers}
      {#each others as f}
        <div class="other">
          <b>{f.name}</b>
          {#if f.phone}<a href={`tel:${f.phone}`}>{f.phone}</a>{/if}
          {#if f.website}<a href={f.website.startsWith('http') ? f.website : `https://${f.website}`} target="_blank" rel="noopener noreferrer">website</a>{/if}
        </div>
      {/each}
    {/if}
  {/if}
  <p class="faint small">FBO listings come from Overture Maps (open places data) and FAA remarks. Call ahead to confirm services and prices.</p>
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
  .notes {
    margin: 0;
    padding-left: 16px;
    font-size: 12px;
    line-height: 1.5;
    color: var(--text-2);
  }
  .more {
    border: 0;
    background: none;
    color: var(--accent);
    font-weight: 600;
    cursor: pointer;
    padding: 10px 0 4px;
    text-align: left;
  }
  .other {
    display: flex;
    gap: 10px;
    padding: 5px 0;
    font-size: 12px;
    border-bottom: 1px solid var(--line);
    align-items: baseline;
    flex-wrap: wrap;
  }
  .other a {
    color: var(--blue);
  }
  .small {
    font-size: 11px;
  }
</style>
