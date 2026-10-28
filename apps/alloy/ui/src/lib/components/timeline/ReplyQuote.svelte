<script lang="ts">
  import type { TimelineItemDto } from '$lib/generated/index.js';
  import { timelineStore } from '$lib/stores/timeline.svelte.js';
  import { membersStore } from '$lib/stores/members.svelte.js';
  import { displayNameFor } from '$lib/platform/names.js';
  import { getUserColorValue } from '$lib/identity/userColor.js';

  interface Props {
    replyTo: TimelineItemDto | null | undefined;
    /** Whether the quote is rendered inside the user's own bubble in bubbles layout. */
    inOwnBubble?: boolean;
    /** Bubbles layout: the quote is its own bubble stacked above the reply, not a panel inside it. */
    stacked?: boolean;
    own?: boolean;
    roomId?: string | null;
  }

  let {
    replyTo,
    inOwnBubble = false,
    stacked = false,
    own = false,
    roomId = timelineStore.roomId
  }: Props = $props();

  const senderId = $derived(replyTo?.sender ?? '');
  const memberName = $derived(
    senderId
      ? membersStore.membersOf(roomId).find((m) => m.user_id === senderId)?.display_name ?? null
      : null
  );
  const senderName = $derived(
    senderId ? displayNameFor(senderId, replyTo?.sender_display_name, memberName) : ''
  );
  const userColor = $derived(
    senderId ? getUserColorValue(senderId) : 'var(--alloy-color-border-prominent)'
  );

  const isUnavailable = $derived(
    !replyTo ||
      replyTo.content.kind === 'Redacted' ||
      replyTo.content.kind === 'UnableToDecrypt' ||
      quote(replyTo).trim().length === 0
  );

  const excerptText = $derived(
    isUnavailable ? 'Original message unavailable' : quote(replyTo)
  );

  const isInteractive = $derived(
    Boolean(replyTo?.event_id && timelineStore.hasEvent(replyTo.event_id))
  );

  function jump() {
    if (replyTo?.event_id) {
      timelineStore.jumpTo(replyTo.event_id);
    }
  }
</script>

<script lang="ts" module>
  import type { TimelineItemDto as ModuleTimelineItemDto } from '$lib/generated/index.js';

  /**
   * Plain-text excerpt of a timeline item for replies and clipboard copying.
   */
  export function quote(it: ModuleTimelineItemDto | null | undefined): string {
    if (!it) return '';
    const c = it.content;
    if (c.kind === 'TextMessage') return c.data.body;
    if (c.kind === 'Media') return c.data.body || 'Attachment';
    if (c.kind === 'Location') return c.data.description || 'Location';
    return '';
  }
</script>

{#snippet quoteBubble()}
  {#if isInteractive}
    <button
      type="button"
      class="reply-quote interactive"
      class:own-bubble={inOwnBubble || (stacked && own)}
      class:stacked
      style="--quote-border-color: {userColor};"
      onclick={jump}
      aria-label="Jump to message from {senderName || 'user'}"
    >
      {#if senderName}
        <span class="sender" style="color: {userColor};">{senderName}</span>
      {/if}
      <span class="excerpt" class:unavailable={isUnavailable}>{excerptText}</span>
    </button>
  {:else}
    <div
      class="reply-quote"
      class:own-bubble={inOwnBubble || (stacked && own)}
      class:stacked
      style="--quote-border-color: {userColor};"
    >
      {#if senderName}
        <span class="sender" style="color: {userColor};">{senderName}</span>
      {/if}
      <span class="excerpt" class:unavailable={isUnavailable}>{excerptText}</span>
    </div>
  {/if}
{/snippet}

{#if stacked}
  <div class="quote-stack">
    {@render quoteBubble()}
  </div>
{:else}
  {@render quoteBubble()}
{/if}

<style>
  .reply-quote {
    display: flex;
    max-width: 36rem;
    flex-direction: column;
    justify-content: center;
    gap: 2px;
    width: 100%;
    min-width: 0;
    max-width: 100%;
    box-sizing: border-box;
    text-align: start;
    padding: var(--alloy-spacing-scale-1) var(--alloy-spacing-scale-2);
    margin: 2px 0 var(--alloy-spacing-scale-1) 0;
    border: 0;
    border-left: 3px solid var(--quote-border-color, var(--alloy-color-border-prominent));
    border-radius: var(--alloy-radii-sm);
    background: color-mix(in srgb, var(--alloy-color-surface-hover) 35%, transparent);
    font: inherit;
    color: inherit;
    text-decoration: none;
  }

  /* Stacked reply: the quote is a card joined to the top of the message bubble. Both share one
     column width and meet at a pair of matched tight corners, so the two read as a single message
     rather than as two unrelated bubbles. The sender colour stays as the leading bar. */
  .quote-stack {
    display: flex;
    flex-direction: column;
    min-width: 0;
    width: 100%;
  }
  .reply-quote.stacked {
    width: 100%;
    max-width: 100%;
    margin: 0;
    padding: var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-3);
    border: 0;
    border-left: 3px solid var(--quote-border-color, var(--alloy-color-border-prominent));
    border-radius: var(--alloy-bubble-radius) var(--alloy-bubble-radius) 0 0;
    background: color-mix(
      in srgb,
      var(--quote-border-color, var(--alloy-color-border-prominent)) 7%,
      var(--alloy-bubble-other-surface)
    );
  }
  /* On the saturated own bubble the quote is a lighter plate of the same blue, not a grey card. */
  .reply-quote.stacked.own-bubble {
    background: color-mix(
      in srgb,
      var(--alloy-color-bubble-own-text) 12%,
      var(--alloy-color-bubble-own-background)
    );
    border-left-color: color-mix(
      in srgb,
      var(--alloy-color-bubble-own-text) 55%,
      var(--quote-border-color)
    );
  }
  .reply-quote.stacked.own-bubble .sender {
    color: var(--alloy-color-bubble-own-text);
  }

  .reply-quote.own-bubble {
    background: color-mix(in srgb, var(--alloy-color-surface-canvas) 40%, transparent);
  }

  .reply-quote.interactive {
    cursor: pointer;
  }

  @media (hover: hover) {
    .reply-quote.interactive:hover {
      background: color-mix(in srgb, var(--alloy-color-surface-hover) 65%, transparent);
    }
    .reply-quote.interactive.own-bubble:hover {
      background: color-mix(in srgb, var(--alloy-color-surface-canvas) 60%, transparent);
    }
  }

  .reply-quote.interactive:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 1px;
  }

  .sender {
    display: block;
    max-width: 100%;
    font-size: var(--alloy-typography-size-sm);
    font-weight: var(--alloy-typography-weight-semibold);
    line-height: var(--alloy-typography-line-height-tight);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .excerpt {
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    overflow: hidden;
    text-overflow: ellipsis;
    font-size: var(--alloy-typography-size-sm);
    line-height: var(--alloy-typography-line-height-normal);
    color: var(--alloy-color-text-secondary);
    word-break: break-word;
    overflow-wrap: anywhere;
  }

  .reply-quote.own-bubble .excerpt {
    color: color-mix(in srgb, var(--alloy-color-bubble-own-text) 82%, transparent);
  }

  .excerpt.unavailable {
    font-style: italic;
  }

  @media (max-width: 899px) {
    .reply-quote.interactive {
      min-height: 44px;
      padding: var(--alloy-spacing-scale-2);
    }
  }
</style>
