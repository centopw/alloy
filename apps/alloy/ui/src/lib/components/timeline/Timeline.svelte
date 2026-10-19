<script lang="ts">
  import { tick, untrack } from 'svelte';
  import { VList } from 'virtua/svelte';
  import type { VListHandle } from 'virtua/svelte';
  import type { TimelineItemDto } from '$lib/generated';
  import { timelineStore, findRowIndexByEventId } from '$lib/stores/timeline.svelte';
  import { membersStore } from '$lib/stores/members.svelte';
  import { prefs } from '$lib/stores/prefs.svelte';
  import { isHiddenState } from '$lib/content/stateEvents';
  import TypingIndicator from './TypingIndicator.svelte';
  import Icon from '$lib/icons/Icon.svelte';
  import { ArrowDown } from '$lib/icons';
  import MessageRow from './MessageRow.svelte';

  interface Props {
    roomId: string;
    title: string;
    isDirect: boolean;
    encrypted: boolean;
  }
  let { roomId, title, isDirect, encrypted }: Props = $props();

  let vlist: VListHandle | undefined = $state();
  let atBottom = $state(true);
  let viewportHeight = $state(0);
  let shift = $state(false);

  // Virtual items (read marker, timeline start) carry no content worth a row. Hidden state events
  // and removed messages are dropped here, before virtualization, so VList indices match rows.
  const rows = $derived(
    timelineStore.items.filter((i) => {
      const c = i.content;
      if (c.kind === 'Virtual') return false;
      if (c.kind === 'RoomState') return !isHiddenState(c.data.state_type, prefs);
      if (c.kind === 'Redacted') return prefs.showRemovedPlaceholder;
      return true;
    }),
  );

  // Only `roomId` may retrigger this: the store reads/writes its own state while opening.
  $effect(() => {
    const id = roomId;
    untrack(() => void timelineStore.open(id));
    return () => untrack(() => void timelineStore.close());
  });

  // Sender names come from the member list; load it once per room so rows show names, not IDs.
  $effect(() => {
    const id = roomId;
    untrack(() => {
      void membersStore.loadMembers(id).catch(() => {
        // Rows fall back to the Matrix ID localpart until the list loads.
      });
    });
  });

  // Keep the viewport stable when older history is prepended; stick to bottom for new messages.
  let lastFirst: string | null = null;
  let lastLen = 0;
  $effect(() => {
    const first = rows[0]?.internal_id ?? null;
    const len = rows.length;
    const prepended = lastFirst !== null && first !== lastFirst && len > lastLen && rows.some((r) => r.internal_id === lastFirst);
    const appended = len > lastLen && !prepended;
    // A new message from the signed-in user jumps to the bottom when that preference is on, even if the user had scrolled up.
    const ownSend = appended && rows[len - 1]?.is_own === true && prefs.jumpToBottomOnSend;
    if (prepended) {
      shift = true;
      void tick().then(() => requestAnimationFrame(() => (shift = false)));
    } else if (ownSend || (appended && atBottom) || (lastLen === 0 && len > 0)) {
      void tick().then(() => vlist?.scrollToIndex(len - 1, { align: 'end' }));
    }
    lastFirst = first;
    lastLen = len;
  });

  // Jump to message requested by a reply quote or other caller.
  $effect(() => {
    const target = timelineStore.jumpTarget;
    if (!target) return;
    const index = findRowIndexByEventId(rows, target.eventId);
    if (index >= 0) {
      vlist?.scrollToIndex(index, { align: 'center' });
    }
  });

  // A room opens with only its newest few events. When they do not fill the viewport there is nothing
  // to scroll, so the scroll-driven `loadOlder` never fires: keep paginating until the list fills it
  // (or the start of the room is reached), also when the viewport grows (window resize, keyboard closing).
  // A pagination error stops the loop.
  $effect(() => {
    void rows.length;
    void vlist;
    void viewportHeight;
    if (timelineStore.loading || timelineStore.paginating || timelineStore.reachedStart || timelineStore.error) return;
    let cancelled = false;
    void tick().then(() =>
      requestAnimationFrame(() => {
        if (cancelled) return;
        const fills = vlist ? vlist.getScrollSize() > vlist.getViewportSize() + 200 : false;
        if (!fills) void timelineStore.loadOlder();
      }),
    );
    return () => (cancelled = true);
  });

  // The newest event counts as read while it is on screen: when the room opens, when a message
  // arrives while the user is at the bottom, and when the window comes back to the foreground.
  // (Scrolling alone never fires for a short list, which left the unread dot on.)
  let windowActive = $state(typeof document === 'undefined' || (document.visibilityState === 'visible' && !document.hidden));
  $effect(() => {
    const update = () => (windowActive = document.visibilityState === 'visible');
    const blur = () => (windowActive = false);
    document.addEventListener('visibilitychange', update);
    window.addEventListener('focus', update);
    window.addEventListener('blur', blur);
    return () => {
      document.removeEventListener('visibilitychange', update);
      window.removeEventListener('focus', update);
      window.removeEventListener('blur', blur);
    };
  });
  $effect(() => {
    const newest = rows.findLast((r) => r.event_id)?.event_id;
    if (newest && atBottom && windowActive && !timelineStore.loading) untrack(() => void timelineStore.markRead());
  });

  function onScroll(offset: number) {
    if (!vlist) return;
    const distance = vlist.getScrollSize() - (offset + vlist.getViewportSize());
    atBottom = distance < 80;
    if (atBottom) void timelineStore.markRead();
    if (offset < 200) void timelineStore.loadOlder();
  }

  function toBottom() {
    vlist?.scrollToIndex(rows.length - 1, { align: 'end', smooth: true });
  }
</script>

<section class="timeline" aria-label="Conversation with {title}, {isDirect ? 'direct message' : 'room'}, {encrypted ? 'end-to-end encrypted' : 'not encrypted'}">
  <div class="viewport" bind:clientHeight={viewportHeight}>
    {#if timelineStore.loading && rows.length === 0}
      <p class="status" aria-busy="true">Loading messages</p>
    {:else if rows.length === 0}
      <p class="status">No messages yet. Say hello.</p>
    {:else}
      <VList
        bind:this={vlist}
        data={rows}
        getKey={(i: TimelineItemDto) => i.internal_id}
        {shift}
        bufferSize={400}
        onscroll={onScroll}
        style="height: 100%; width: 100%; overscroll-behavior: contain;"
      >
        {#snippet children(item: TimelineItemDto, index: number)}
          <div class="cell">
            <MessageRow
              {item}
              prev={index > 0 ? rows[index - 1] : null}
            />
          </div>
        {/snippet}
      </VList>
    {/if}

    {#if !atBottom && rows.length > 0}
      <button class="latest" type="button" onclick={toBottom} aria-label="Jump to latest message">
        <Icon icon={ArrowDown} size={20} />
      </button>
    {/if}

    {#if timelineStore.error}
      <div class="toast" role="alert">{timelineStore.error}</div>
    {/if}
  </div>
  <TypingIndicator {roomId} />
</section>

<style>
  .timeline {
    display: flex;
    flex-direction: column;
    min-height: 0;
    flex: 1;
    background: var(--alloy-chat-background, none), var(--alloy-color-surface-canvas);
  }
  .viewport {
    position: relative;
    flex: 1;
    min-height: 0;
  }
  /* Rows sit on the shared gutter: the row highlight is inset, so the cell pads by the difference. */
  .cell {
    padding: 0 calc(var(--alloy-layout-gutter) - var(--alloy-layout-row-inset));
  }
  .cell :global(.divider) {
    margin: 0 calc(var(--alloy-layout-row-inset) * -1);
  }
  .status {
    margin: 0;
    padding: var(--alloy-spacing-scale-8) var(--alloy-layout-gutter);
    text-align: center;
    color: var(--alloy-color-text-secondary);
  }
  .latest {
    position: absolute;
    right: var(--alloy-layout-gutter);
    bottom: var(--alloy-spacing-scale-4);
    width: 40px;
    height: 40px;
    border-radius: var(--alloy-radii-full);
    border: 1px solid var(--alloy-color-border-prominent);
    background: var(--alloy-color-surface-elevated);
    color: var(--alloy-color-text-primary);
    display: grid;
    place-items: center;
    cursor: pointer;
    box-shadow: var(--alloy-elevation-md);
  }
  @media (max-width: 899px) {
    .latest {
      width: 44px;
      height: 44px;
    }
  }
  .toast {
    position: absolute;
    left: 50%;
    bottom: var(--alloy-spacing-scale-4);
    transform: translateX(-50%);
    background: var(--alloy-color-status-error);
    color: var(--alloy-color-text-on-accent);
    padding: var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-4);
    border-radius: var(--alloy-radii-md);
  }
</style>
