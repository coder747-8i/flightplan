<script lang="ts">
  let { checked = $bindable(false), label, hint = '', swatch = '' }: { checked: boolean; label: string; hint?: string; swatch?: string } = $props()
</script>

<label class="row">
  {#if swatch}<span class="swatch" style:background={swatch}></span>{/if}
  <span class="text">
    <span>{label}</span>
    {#if hint}<small class="faint">{hint}</small>{/if}
  </span>
  <input type="checkbox" bind:checked />
  <span class="switch" aria-hidden="true"></span>
</label>

<style>
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 7px 2px;
    cursor: pointer;
    user-select: none;
  }
  .swatch {
    width: 14px;
    height: 14px;
    border-radius: 4px;
    flex: none;
  }
  .text {
    flex: 1;
    display: flex;
    flex-direction: column;
    line-height: 1.25;
  }
  small {
    font-size: 11px;
  }
  input {
    position: absolute;
    opacity: 0;
    pointer-events: none;
  }
  .switch {
    width: 32px;
    height: 19px;
    border-radius: 999px;
    background: var(--line-strong);
    position: relative;
    transition: background 0.15s;
    flex: none;
  }
  .switch::after {
    content: '';
    position: absolute;
    top: 2px;
    left: 2px;
    width: 15px;
    height: 15px;
    border-radius: 50%;
    background: white;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.25);
    transition: transform 0.15s;
  }
  input:checked + .switch {
    background: var(--accent);
  }
  input:checked + .switch::after {
    transform: translateX(13px);
  }
  input:focus-visible + .switch {
    box-shadow: 0 0 0 3px var(--accent-soft);
  }
</style>
