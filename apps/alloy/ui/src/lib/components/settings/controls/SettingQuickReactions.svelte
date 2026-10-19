<script lang="ts">
  import Icon from '$lib/icons/Icon.svelte';
  import { Plus, X } from '$lib/icons';
  import {
    DEFAULT_QUICK_REACTIONS,
    MAX_QUICK_REACTIONS,
    addQuickReaction,
    removeQuickReaction,
  } from '$lib/content/quickReactions';
  import { emojiPicker } from '$lib/stores/emojiPicker.svelte';

  interface Props {
    value: readonly string[];
    onchange: (value: readonly string[]) => void;
  }

  let { value, onchange }: Props = $props();

  const full = $derived(value.length >= MAX_QUICK_REACTIONS);
  const isDefault = $derived(
    value.length === DEFAULT_QUICK_REACTIONS.length && value.every((e, i) => e === DEFAULT_QUICK_REACTIONS[i]),
  );

  function add(anchor: DOMRect) {
    emojiPicker.open({ anchor, onpick: (emoji) => onchange(addQuickReaction(value, emoji)) });
  }
</script>

<div class="setting-row">
  <div class="text">
    <span class="label">Quick reactions</span>
    <span class="description">
      Shown when you hover over a message, or press and hold it on a phone. Tap one to remove it, up to {MAX_QUICK_REACTIONS}.
    </span>
  </div>
  <div class="chips" role="group" aria-label="Quick reactions">
    {#each value as emoji (emoji)}
      <button
        type="button"
        class="chip"
        aria-label="Remove {emoji}"
        data-tooltip="Remove"
        disabled={value.length <= 1}
        onclick={() => onchange(removeQuickReaction(value, emoji))}
      >
        <span class="glyph">{emoji}</span>
        <span class="x" aria-hidden="true"><Icon icon={X} size={16} /></span>
      </button>
    {/each}
    {#if !full}
      <button
        type="button"
        class="chip add"
        aria-label="Add a reaction"
        data-tooltip="Add a reaction"
        onclick={(e) => add(e.currentTarget.getBoundingClientRect())}
      >
        <Icon icon={Plus} size={20} />
      </button>
    {/if}
    {#if !isDefault}
      <button type="button" class="reset" onclick={() => onchange(DEFAULT_QUICK_REACTIONS)}>Reset</button>
    {/if}
  </div>
</div>

<style>
  .setting-row {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-3);
    padding: var(--alloy-spacing-scale-3) var(--alloy-layout-row-pad-x);
    border-top: 1px solid var(--alloy-color-border-subtle);
  }

  .text {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-1);
    min-width: 0;
  }

  .label {
    font-size: var(--alloy-typography-size-base);
    font-weight: var(--alloy-typography-weight-medium);
    color: var(--alloy-color-text-primary);
  }

  .description {
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-text-secondary);
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
  }

  .chip,
  .reset {
    min-width: 44px;
    height: 44px;
    border: 0;
    border-radius: var(--alloy-radii-md);
    background: var(--alloy-color-surface-subtle);
    color: var(--alloy-color-text-secondary);
    font: inherit;
    cursor: pointer;
  }

  .chip {
    position: relative;
    display: inline-grid;
    place-items: center;
  }

  .glyph {
    font-size: 22px;
    line-height: 1;
  }

  /* The remove cross replaces the emoji while the chip is hovered or focused. */
  .x {
    position: absolute;
    inset: 0;
    display: none;
    place-items: center;
    border-radius: inherit;
    background: var(--alloy-color-surface-hover);
    color: var(--alloy-color-text-primary);
  }

  .chip:not(.add):not(:disabled):hover .x,
  .chip:not(.add):not(:disabled):focus-visible .x {
    display: grid;
  }

  .chip:disabled {
    cursor: default;
  }

  .reset {
    padding: 0 var(--alloy-spacing-scale-3);
    background: transparent;
    color: var(--alloy-color-text-link);
    font-size: var(--alloy-typography-size-sm);
  }

  .chip.add:hover,
  .reset:hover {
    background: var(--alloy-color-surface-hover);
  }

  .chip:focus-visible,
  .reset:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }
</style>
