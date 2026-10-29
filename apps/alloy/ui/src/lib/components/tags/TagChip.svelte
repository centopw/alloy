<script lang="ts">
  import type { TagColour } from '$lib/generated/index.js';
  import { TAG_COLOUR_VARS } from '$lib/stores/tags.js';

  interface Props {
    label: string;
    /** Omitted for chips without a tag (the "All" chip). */
    colour?: TagColour;
    count?: number;
    pressed: boolean;
    /** Hover text for a chip whose label alone is ambiguous (the grouping toggle). */
    title?: string;
    onclick: () => void;
  }
  let { label, colour, count, pressed, title, onclick }: Props = $props();
</script>

<button type="button" class="chip" class:pressed aria-pressed={pressed} {title} {onclick}>
  {#if colour}<span class="dot" style:background-color={TAG_COLOUR_VARS[colour]}></span>{/if}
  <span class="label">{label}</span>
  {#if count !== undefined}<span class="count">{count}</span>{/if}
</button>

<style>
  .chip {
    display: inline-flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-1);
    flex: 0 0 auto;
    min-height: 28px;
    padding: 0 var(--alloy-spacing-scale-3);
    border: 1px solid var(--alloy-color-surface-hover);
    border-radius: var(--alloy-radii-full);
    background: transparent;
    color: var(--alloy-color-text-secondary);
    font: inherit;
    font-size: var(--alloy-typography-size-sm);
    cursor: pointer;
  }
  .chip:hover {
    background: var(--alloy-color-surface-hover);
  }
  .chip.pressed {
    border-color: var(--alloy-color-accent-primary);
    background: var(--alloy-color-accent-primary);
    color: var(--alloy-color-text-on-accent);
  }
  .chip:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: var(--alloy-radii-full);
    flex: 0 0 auto;
  }
  .count {
    opacity: 0.8;
  }
  @media (max-width: 899px) {
    .chip {
      min-height: 44px;
    }
  }
</style>
