<script module lang="ts">
  /** The row whose actions are showing; opening another row (or tapping) closes it. */
  let openId = $state<string | null>(null);
</script>

<script lang="ts">
  import type { Snippet } from 'svelte';
  import Icon from '$lib/icons/Icon.svelte';
  import { Archive, ArchiveRestore, Bell, BellOff } from '$lib/icons';
  import { swipe } from '$lib/platform/swipe';

  interface Props {
    id: string;
    /** Swipe actions only exist on touch layouts. */
    enabled: boolean;
    archived: boolean;
    muted: boolean;
    onarchive: () => void;
    onmute: () => void;
    children: Snippet;
  }
  let { id, enabled, archived, muted, onarchive, onmute, children }: Props = $props();

  /** Width of one action; two actions are revealed. */
  const ACTION_PX = 72;
  const REVEAL_PX = ACTION_PX * 2;

  let dragX = $state<number | null>(null);
  let width = $state(0);
  const open = $derived(openId === id);
  const shift = $derived(dragX !== null ? dragX : open ? REVEAL_PX : 0);

  function run(action: () => void) {
    openId = null;
    action();
  }

  const swipeOptions = $derived({
    direction: 'left' as const,
    threshold: REVEAL_PX,
    commitAt: ACTION_PX,
    enabled,
    edgeGuard: 24,
    onmove: (offset: number) => {
      dragX = offset;
    },
    onend: () => {
      dragX = null;
    },
    // A long swipe archives at once; a short one opens the two actions.
    oncommit: (travel: number) => {
      if (width > 0 && travel > width * 0.6) run(onarchive);
      else openId = id;
    },
  });

  // A tap on an open row only closes it.
  function onFrontClick(e: MouseEvent) {
    if (!open) return;
    e.preventDefault();
    e.stopPropagation();
    openId = null;
  }
</script>

{#if enabled}
  <div class="swipe" bind:clientWidth={width}>
    <div class="actions" class:shown={shift > 0} aria-hidden={!open && dragX === null}>
      <button type="button" class="action mute" tabindex={open ? 0 : -1} onclick={() => run(onmute)}>
        <Icon icon={muted ? Bell : BellOff} size={20} />
        <span>{muted ? 'Unmute' : 'Mute'}</span>
      </button>
      <button type="button" class="action archive" tabindex={open ? 0 : -1} onclick={() => run(onarchive)}>
        <Icon icon={archived ? ArchiveRestore : Archive} size={20} />
        <span>{archived ? 'Unarchive' : 'Archive'}</span>
      </button>
    </div>
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div
      class="front"
      class:dragging={dragX !== null}
      class:moved={shift > 0}
      style:transform={shift > 0 ? `translateX(${-shift}px)` : undefined}
      onclickcapture={onFrontClick}
      use:swipe={swipeOptions}
    >
      {@render children()}
    </div>
  </div>
{:else}
  {@render children()}
{/if}

<style>
  .swipe {
    position: relative;
    overflow: hidden;
    border-radius: var(--alloy-radii-lg);
  }
  .front {
    position: relative;
    background: transparent;
    transition: transform var(--alloy-motion-duration-normal) ease-out;
  }
  .front.moved {
    background: var(--alloy-color-surface-panel);
  }
  .front.dragging {
    transition: none;
  }
  .actions {
    visibility: hidden;
    position: absolute;
    inset: 0 0 0 auto;
    display: flex;
  }
  .actions.shown {
    visibility: visible;
  }
  .action {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 2px;
    width: 72px;
    border: 0;
    font: inherit;
    font-size: var(--alloy-typography-size-xs);
    cursor: pointer;
  }
  .mute {
    background: var(--alloy-color-surface-active);
    color: var(--alloy-color-text-primary);
  }
  .archive {
    background: var(--alloy-color-accent-primary);
    color: var(--alloy-color-text-on-accent);
  }
  @media (prefers-reduced-motion: reduce) {
    .front {
      transition: none;
    }
  }
</style>
