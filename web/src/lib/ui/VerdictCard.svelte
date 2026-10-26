<script lang="ts">
  // The go / no-go card: a green, yellow or red call with the reasons behind it, and the checks
  // that passed. Used by the route panel and the briefing.
  import { CircleCheck, TriangleAlert, Ban, CalendarCheck, ChevronRight, ChevronDown } from '@lucide/svelte'
  import { CALL_WORD, nextGreen, type Call, type Factor, type Verdict } from '../nav/verdict'
  import { leadText } from '../wx/forecast'
  import { plan } from '../stores/plan.svelte'
  import { route, setDeparture } from '../stores/route.svelte'

  let { v, print = false }: { v: Verdict; print?: boolean } = $props()

  let open = $state<boolean | null>(null)
  let checksOpen = $state<boolean | null>(null)
  const checks = $derived(checksOpen ?? print)
  const expanded = $derived(open ?? (print || v.call === 'nogo'))

  const dep = $derived(plan.departureTime)
  const eta = $derived(plan.log ? new Date(dep.getTime() + plan.log.totalTime * 60000) : null)
  const problems = $derived(v.factors.filter((f) => f.call !== 'go'))
  const passed = $derived(v.factors.filter((f) => f.call === 'go'))
  const single = $derived(problems.length === 1 && problems[0].title === v.headline)
  const shown = $derived(expanded ? problems : problems.slice(0, 3))
  const green = $derived(v.call !== 'go' && !print ? nextGreen(plan.windows, dep.getTime()) : null)

  const fmtDay = (d: Date) => d.toLocaleString([], { weekday: 'short', day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit', hour12: false })
  const hhmm = (d: Date) => d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', hour12: false })
  const wkhm = (ms: number) => new Date(ms).toLocaleString([], { weekday: 'short', hour: '2-digit', minute: '2-digit', hour12: false })

  function hover(f: Factor | null) {
    plan.hover = f?.at != null ? { d: f.at, from: 'profile' } : null
  }
  const atText = (f: Factor) => (f.at != null && f.at > 0.5 && !f.title.includes(' NM') ? ` · ${Math.round(f.at)} NM` : '')
</script>

{#snippet icon(c: Call, size = 15)}
  {#if c === 'go'}<CircleCheck {size} />{:else if c === 'caution'}<TriangleAlert {size} />{:else}<Ban {size} />{/if}
{/snippet}

<div class="card {v.call}" data-tour="route-verdict">
  <div class="top">
    <span class="badge {v.call}">{@render icon(v.call, 14)} {CALL_WORD[v.call]}</span>
    <span class="when">{fmtDay(dep)} · {leadText(dep)}</span>
  </div>
  <div class="headline">{v.headline}</div>
  {#if eta}<div class="sub">Departs {hhmm(dep)} · arrives {hhmm(eta)}</div>{/if}

  {#if single}
    {#if problems[0].detail}<p class="detail" role="note" onmouseenter={() => hover(problems[0])} onmouseleave={() => hover(null)}>{problems[0].detail}</p>{/if}
  {:else if problems.length}
    <ul>
      {#each shown as f}
        <li class={f.call} onmouseenter={() => hover(f)} onmouseleave={() => hover(null)}>
          <span class="ico">{@render icon(f.call)}</span>
          <div>
            <div class="t">{f.title}{atText(f)}</div>
            {#if expanded && f.detail}<div class="d">{f.detail}</div>{/if}
          </div>
        </li>
      {/each}
    </ul>
    {#if !print && (problems.length > 3 || problems.some((f) => f.detail))}
      <button class="link" onclick={() => (open = !expanded)}>{expanded ? 'Less detail' : problems.length > 3 ? `All ${problems.length} reasons and details` : 'Details'}</button>
    {/if}
  {/if}

  {#if green && !route.locked}
    <button class="link green" title="Move the departure to the start of it" onclick={() => setDeparture(new Date(green[0]))}>
      <CalendarCheck size={13} /> Next green window: {wkhm(green[0])} – {wkhm(green[1])}
    </button>
  {/if}

  {#if passed.length}
    {#if !print}
      <button class="checks" onclick={() => (checksOpen = !checks)}>
        {#if checks}<ChevronDown size={13} />{:else}<ChevronRight size={13} />{/if}
        {passed.length} check{passed.length === 1 ? '' : 's'} passed
      </button>
    {:else}
      <div class="sub">Checks passed</div>
    {/if}
    {#if checks}
      <ul>
        {#each passed as f}
          <li class="go">
            <span class="ico">{@render icon('go')}</span>
            <div>
              <div class="t">{f.title}</div>
              {#if f.detail}<div class="d">{f.detail}</div>{/if}
            </div>
          </li>
        {/each}
      </ul>
    {/if}
  {/if}
  <div class="foot">Advisory only. You are the pilot in command.</div>
</div>

<style>
  .card {
    --c: var(--ok);
    border: 1px solid color-mix(in srgb, var(--c) 45%, transparent);
    background: color-mix(in srgb, var(--c) 9%, transparent);
    border-radius: 12px;
    padding: 12px;
  }
  .card.caution {
    --c: var(--warn);
  }
  .card.nogo {
    --c: var(--bad);
  }
  :global([data-theme='night']) .card {
    background: color-mix(in srgb, var(--c) 16%, transparent);
  }
  .top {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
  }
  .badge {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 3px 10px;
    border-radius: 999px;
    color: white;
    font-weight: 700;
    font-size: 12.5px;
    background: var(--c);
  }
  .when {
    font-size: 11.5px;
    color: var(--text-2);
    text-align: right;
  }
  .headline {
    font-weight: 650;
    font-size: 14px;
    margin-top: 6px;
    line-height: 1.35;
  }
  .sub {
    font-size: 11.5px;
    color: var(--text-3);
    margin-top: 2px;
  }
  .detail {
    font-size: 11.5px;
    color: var(--text-2);
    margin: 4px 0 0;
    line-height: 1.4;
  }
  ul {
    list-style: none;
    margin: 8px 0 2px;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 5px;
  }
  li {
    display: flex;
    gap: 7px;
    align-items: flex-start;
  }
  .ico {
    flex: none;
    margin-top: 1px;
  }
  li.go .ico {
    color: var(--ok);
  }
  li.caution .ico {
    color: var(--warn);
  }
  li.nogo .ico {
    color: var(--bad);
  }
  .t {
    font-weight: 600;
    font-size: 12.5px;
    line-height: 1.35;
  }
  .d {
    font-size: 11.5px;
    color: var(--text-2);
    line-height: 1.4;
    margin-top: 1px;
  }
  .link,
  .checks {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    border: 0;
    background: none;
    padding: 4px 0;
    cursor: pointer;
    font-size: 12px;
    font-weight: 600;
    color: var(--accent);
  }
  .link.green {
    display: flex;
    color: var(--ok);
  }
  .checks {
    display: flex;
    color: var(--text-2);
    font-weight: 500;
    font-size: 11.5px;
  }
  .foot {
    font-size: 10.5px;
    color: var(--text-3);
    margin-top: 4px;
  }
</style>
