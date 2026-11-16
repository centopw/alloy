<script lang="ts">
  import type { TimelineItemDto } from '$lib/generated';
  import { timelineStore } from '$lib/stores/timeline.svelte';
  import { membersStore } from '$lib/stores/members.svelte';
  import { displayNameFor } from '$lib/platform/names';
  import { getUserColorValue } from '$lib/identity/userColor';
  import { messagePreview } from './a11y';
  import Icon from '$lib/icons/Icon.svelte';
  import { Pin, X } from '$lib/icons';
  import { t } from '$lib/i18n';

  interface Props {
    roomId: string;
    /** Short label of a non-text message (photo, file, voice note); shared with the timeline's previews. */
    kindLabel: (item: TimelineItemDto) => string;
  }
  let { roomId, kindLabel }: Props = $props();

  /** Newest first, straight from the store's merged view. */
  const pinned = $derived(timelineStore.pinned);
  /** Which pinned message the bar shows: 0 is the newest, each click moves one older and wraps. */
  let cursor = $state(0);
  // A new room, or a pin added or removed, starts over at the newest pin instead of pointing past the end.
  $effect(() => {
    void roomId;
    void pinned.length;
    cursor = 0;
  });
  const current = $derived(pinned[Math.min(cursor, Math.max(0, pinned.length - 1))] ?? null);

  const senderName = $derived.by(() => {
    if (!current) return '';
    const member = membersStore.membersOf(roomId).find((m) => m.user_id === current.sender);
    return displayNameFor(current.sender, current.sender_display_name, member?.display_name ?? null);
  });
  const preview = $derived(current ? messagePreview(current, kindLabel) : '');
  const userColor = $derived(current ? getUserColorValue(current.sender) : 'var(--alloy-color-border-prominent)');
  /** "1 of N" counts from the newest pin; `cursor` is zero-based. */
  const position = $derived(pinned.length === 0 ? 0 : Math.min(cursor, pinned.length - 1) + 1);

  /** Shows the pinned message the bar points at and moves on to the next older one for the next click. */
  function show() {
    if (!current?.event_id) return;
    const eventId = current.event_id;
    cursor = pinned.length > 1 ? (cursor + 1) % pinned.length : 0;
    void timelineStore.jumpToEvent(eventId);
  }

  function unpin() {
    if (!current) return;
    void timelineStore.setPinned(current, false);
  }
</script>

{#if current}
  <div class="pinned-bar" style="--pin-color: {userColor};" role="group" aria-label={t('pinned-bar-pinned-messages')}>
    <span class="pin-glyph" aria-hidden="true"><Icon icon={Pin} size={16} /></span>
    <button
      type="button"
      class="jump"
      onclick={show}
      aria-label={t('pinned-bar-show-pinned-message', { position, count: pinned.length, name: senderName })}
    >
      <span class="sender" style="color: {userColor};">{senderName}:</span>
      <span class="preview">{preview}</span>
    </button>
    {#if pinned.length > 1}
      <span class="counter" aria-hidden="true">{t('pinned-bar-position', { position, count: pinned.length })}</span>
    {/if}
    <button type="button" class="unpin" onclick={unpin} aria-label={t('pinned-bar-unpin')} data-tooltip={t('pinned-bar-unpin')}>
      <Icon icon={X} size={16} />
    </button>
  </div>
{/if}

<style>
  /* A real bar between the header and the timeline: one 36px row on its own hairline, inset on the
     conversation rail so its pin, its text and its close button line up with the header and the
     bubbles. The header's icon cluster never shares it. */
  .pinned-bar {
    display: flex;
    flex-shrink: 0;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    height: 36px;
    padding: 0 var(--conversation-rail, var(--alloy-layout-gutter));
    border-bottom: 1px solid var(--alloy-chat-rim, var(--alloy-color-border-subtle));
    background: color-mix(in srgb, var(--alloy-color-surface-hover) 35%, transparent);
    min-width: 0;
  }
  .pin-glyph {
    display: inline-flex;
    flex: none;
    color: var(--pin-color);
  }
  /* The one line: `author: preview`, truncated with an ellipsis before the counter. */
  .jump {
    display: flex;
    flex: 1 1 0;
    align-items: center;
    gap: var(--alloy-spacing-scale-1);
    min-width: 0;
    height: 100%;
    padding: 0 var(--alloy-spacing-scale-1);
    border: 0;
    border-radius: var(--alloy-radii-sm);
    background: transparent;
    color: inherit;
    font: inherit;
    text-align: start;
    cursor: pointer;
  }
  .sender,
  .preview {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    line-height: var(--alloy-typography-line-height-tight);
  }
  .sender {
    flex: none;
    max-width: 45%;
    font-size: var(--alloy-typography-size-sm);
    font-weight: var(--alloy-typography-weight-semibold);
  }
  .preview {
    min-width: 0;
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-text-secondary);
  }
  .counter {
    flex: none;
    font-size: var(--alloy-typography-size-xs);
    color: var(--alloy-color-text-muted);
    font-variant-numeric: tabular-nums;
  }
  .unpin {
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
    cursor: pointer;
  }
  @media (hover: hover) {
    .jump:hover,
    .unpin:hover {
      background: var(--alloy-color-surface-hover);
    }
    .unpin:hover {
      color: var(--alloy-color-text-primary);
    }
  }
  .jump:focus-visible,
  .unpin:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: -2px;
  }
  /* Phones: one step taller (40px) and the close button keeps its 28px mark inside the bar. */
  @media (max-width: 899px) {
    .pinned-bar {
      height: 44px;
    }
    /* The bar is a phone surface too: its close control keeps the 44px touch target. */
    .unpin {
      width: 44px;
      height: 44px;
    }
  }
</style>
