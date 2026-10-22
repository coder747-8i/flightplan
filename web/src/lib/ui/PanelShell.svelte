<script lang="ts">
  import { X } from '@lucide/svelte'
  import type { Snippet } from 'svelte'
  import { app } from '../stores/app.svelte'

  let { title, subtitle = '', width = 340, children, actions }: { title: string; subtitle?: string; width?: number; children: Snippet; actions?: Snippet } = $props()
</script>

<aside class="panel glass" style:width={`${width}px`}>
  <header>
    <div>
      <h2>{title}</h2>
      {#if subtitle}<p class="faint">{subtitle}</p>{/if}
    </div>
    <div class="actions">
      {@render actions?.()}
      <button class="btn ghost icon" onclick={() => (app.panel = 'none')} aria-label="Close"><X size={16} /></button>
    </div>
  </header>
  <div class="body scroll">
    {@render children()}
  </div>
</aside>

<style>
  .panel {
    position: absolute;
    top: 12px;
    left: 88px;
    bottom: 12px;
    max-width: calc(100vw - 100px);
    display: flex;
    flex-direction: column;
    z-index: 15;
    animation: slide 0.18s ease-out;
  }
  @keyframes slide {
    from { opacity: 0; transform: translateX(-8px); }
  }
  header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    padding: 16px 16px 8px 18px;
  }
  h2 {
    margin: 0;
    font-size: 16px;
    font-weight: 650;
    letter-spacing: -0.01em;
  }
  p {
    margin: 2px 0 0;
    font-size: 12px;
  }
  .actions {
    display: flex;
    gap: 4px;
  }
  .icon {
    width: 30px;
    padding: 0;
    justify-content: center;
  }
  .body {
    flex: 1;
    padding: 0 18px 18px;
  }
  @media (max-width: 640px) {
    .panel {
      left: 8px;
      right: 8px;
      top: auto;
      bottom: 72px;
      width: auto !important;
      max-width: none;
      height: 62vh;
    }
  }
</style>
