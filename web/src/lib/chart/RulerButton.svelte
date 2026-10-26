<script lang="ts">
  // The ruler's on / off button (top right, beside the altitude filter).
  import { Ruler as RulerIcon } from '@lucide/svelte'
  import { ruler } from './ruler.svelte'
</script>

<button class="rbtn glass" class:on={ruler.on} onclick={() => ruler.toggle()} title={ruler.on ? 'Ruler on: click points to measure; Space starts a new line; Backspace undoes a point; Esc clears' : 'Measure distance and course (R)'} data-tour="ruler">
  <RulerIcon size={16} /> <span class="txt">{ruler.on ? 'Measuring' : 'Measure'}</span>
</button>

<style>
  .rbtn {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 36px;
    padding: 0 12px;
    border-radius: 999px;
    cursor: pointer;
    font-weight: 600;
    font-size: 12px;
    color: var(--text-2);
  }
  .rbtn.on {
    color: white;
    background: var(--accent);
    border-color: var(--accent);
  }
  @media (max-width: 640px) {
    .rbtn {
      padding: 0 10px;
    }
    .rbtn .txt {
      display: none;
    }
  }
</style>
