<script lang="ts">
  import Icon from '$lib/icons/Icon.svelte';
  import { SmilePlus, ChevronDown, Plus } from '$lib/icons';
  interface Props {
    /** Emoji to offer, in order (the user's quick-reaction list). */
    emojis: readonly string[];
    /** Emoji the current user already reacted with; shown pressed. */
    active?: readonly string[];
    onpick: (emoji: string) => void;
    /** Open the full emoji picker. */
    onmore: () => void;
    /** `touch` gives 44px targets for the phone sheet. */
    size?: 'compact' | 'touch';
  }

  let { emojis, active = [], onpick, onmore, size = 'compact' }: Props = $props();
</script>

<div class="bar" class:touch={size === 'touch'} role="group" aria-label="Quick reactions">
  {#each emojis as emoji (emoji)}
    <button
      type="button"
      class="emoji"
      class:on={active.includes(emoji)}
      aria-pressed={active.includes(emoji)}
      aria-label="React with {emoji}"
      onclick={() => onpick(emoji)}
    >
      {emoji}
    </button>
  {/each}
  <button type="button" class="more" aria-label="More reactions" data-tooltip="More reactions" onclick={onmore}>
    <Icon icon={size === 'touch' ? Plus : ChevronDown} size={size === 'touch' ? 24 : 16} />
  </button>
</div>
<style>
  .bar {
    display: flex;
    align-items: center;
    gap: 2px;
  }

  button {
    display: inline-grid;
    flex: none;
    place-items: center;
    width: 28px;
    height: 28px;
    padding: 0;
    border: 0;
    border-radius: var(--alloy-radii-md);
    background: transparent;
    color: var(--alloy-color-text-secondary);
    font: inherit;
    cursor: pointer;
  }

  .emoji {
    font-size: 16px;
    line-height: 1;
  }

  .emoji.on {
    background: var(--alloy-color-accent-subtle);
  }

  button:hover {
    background: var(--alloy-color-surface-hover);
    color: var(--alloy-color-text-primary);
  }

  button:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: -2px;
  }

  /* Phone floating pill: 44px targets, larger glyphs. */
  .bar.touch {
    justify-content: flex-start;
    gap: 0;
  }

  .touch button {
    width: 44px;
    height: 44px;
    border-radius: var(--alloy-radii-full);
  }

  .touch .emoji {
    font-size: 26px;
  }
</style>
