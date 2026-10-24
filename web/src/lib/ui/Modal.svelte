<script lang="ts">
  // Centered dialog over a dimmed backdrop. Escape or the close button calls onclose.
  import { X } from '@lucide/svelte'
  import type { Snippet } from 'svelte'

  let {
    title,
    subtitle = '',
    width = 560,
    onclose,
    children,
    footer,
  }: { title: string; subtitle?: string; width?: number; onclose: () => void; children: Snippet; footer?: Snippet } = $props()
</script>

<svelte:window onkeydown={(e) => e.key === 'Escape' && onclose()} />

<div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && onclose()}>
  <div class="modal glass" role="dialog" aria-modal="true" aria-label={title} style:width={`${width}px`}>
    <header>
      <div>
        <h2>{title}</h2>
        {#if subtitle}<p class="faint">{subtitle}</p>{/if}
      </div>
      <button class="btn ghost icon" onclick={onclose} aria-label="Close"><X size={16} /></button>
    </header>
    <div class="body scroll">{@render children()}</div>
    {#if footer}<footer>{@render footer()}</footer>{/if}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 60;
    background: rgba(10, 14, 20, 0.38);
    display: grid;
    place-items: center;
    padding: 16px;
    animation: fade 0.15s ease-out;
  }
  @keyframes fade {
    from { opacity: 0; }
  }
  .modal {
    max-width: 100%;
    max-height: min(820px, calc(100vh - 32px));
    display: flex;
    flex-direction: column;
    background: var(--surface-solid);
  }
  header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 12px;
    padding: 18px 18px 10px 20px;
  }
  h2 {
    margin: 0;
    font-size: 17px;
    font-weight: 650;
    letter-spacing: -0.01em;
  }
  p {
    margin: 3px 0 0;
    font-size: 12px;
  }
  .icon {
    width: 30px;
    padding: 0;
    justify-content: center;
  }
  .body {
    flex: 1;
    padding: 4px 20px 18px;
    min-height: 0;
  }
  footer {
    display: flex;
    justify-content: flex-end;
    align-items: center;
    gap: 8px;
    padding: 12px 20px;
    border-top: 1px solid var(--line);
  }
</style>
