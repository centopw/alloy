<script lang="ts">
  import { tick, untrack } from 'svelte';
  import { VList } from 'virtua/svelte';
  import { offsetForBottomGap } from './scrollAnchor';
  import type { VListHandle } from 'virtua/svelte';
  import type { TimelineItemDto } from '$lib/generated';
  import { timelineStore, findRowIndexByEventId } from '$lib/stores/timeline.svelte';
  import { membersStore } from '$lib/stores/members.svelte';
  import { prefs } from '$lib/stores/prefs.svelte';
  import { isHiddenState } from '$lib/content/stateEvents';
  import TypingIndicator from './TypingIndicator.svelte';
  import Icon from '$lib/icons/Icon.svelte';
  import { ArrowDown, AtSign } from '$lib/icons';
  import MessageRow from './MessageRow.svelte';
  import { groupAlbums } from '$lib/content/albums';
  import { contextmenu } from '$lib/platform/contextmenu';
  import { badgeLabel, countArrivals } from './arrivals';
  import type { MenuItem } from '$lib/stores/contextmenu.svelte';
  import { refreshStore } from '$lib/stores/refresh.svelte';

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
  /** Messages from others that arrived while scrolled up, and which of them mention the user. */
  let unseen = $state(0);
  let mentionIds = $state<string[]>([]);

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
  /** Runs of captionless pictures fold into one grid row. */
  const albums = $derived(groupAlbums(rows));

  // Only `roomId` may retrigger this: the store reads/writes its own state while opening.
  $effect(() => {
    const id = roomId;
    untrack(() => {
      unseen = 0;
      mentionIds = [];
      void timelineStore.open(id);
    });
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
    if (appended && !ownSend && !atBottom && lastLen > 0) {
      const arrived = countArrivals(rows.slice(lastLen));
      untrack(() => {
        unseen += arrived.count;
        mentionIds = [...mentionIds, ...arrived.mentions];
      });
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

  // Distance between the visible bottom edge and the end of the content, kept from the last scroll.
  let bottomGap = 0;
  let lastViewportHeight = 0;

  // The on-screen keyboard resizes the viewport from below. Hold the bottom gap so the view the user
  // was in stays put: pinned to the newest message when at the bottom, unchanged when scrolled up.
  $effect(() => {
    const height = viewportHeight;
    const previous = lastViewportHeight;
    lastViewportHeight = height;
    if (!vlist || previous === 0 || height === 0 || height === previous) return;
    const list = vlist;
    void tick().then(() => list.scrollTo(offsetForBottomGap(list.getScrollSize(), height, bottomGap)));
  });

  function onScroll(offset: number) {
    if (!vlist) return;
    const distance = vlist.getScrollSize() - (offset + vlist.getViewportSize());
    bottomGap = Math.max(0, distance);
    atBottom = distance < 80;
    if (atBottom && (unseen > 0 || mentionIds.length > 0)) {
      unseen = 0;
      mentionIds = [];
    }
    if (atBottom) void timelineStore.markRead();
    if (offset < 200) void timelineStore.loadOlder();
  }

  function toBottom() {
    vlist?.scrollToIndex(rows.length - 1, { align: 'end', smooth: true });
  }

  /** Scroll to the oldest unseen message that mentions the user. */
  function toMention() {
    const [next, ...rest] = mentionIds;
    if (!next) return;
    mentionIds = rest;
    timelineStore.jumpTo(next);
  }

  /** Set on each press so the background menu yields to a message row that owns the press. */
  let pressedOnRow = false;

  function onViewportPointerDown(e: PointerEvent) {
    pressedOnRow = e.target instanceof Element && e.target.closest('.row') !== null;
  }

  function scrollToTop() {
    vlist?.scrollToIndex(0, { align: 'start', smooth: true });
    void timelineStore.loadOlder();
  }

  /** Background menu items. Empty when a message row owns the press, so the row menu wins. */
  function backgroundItems(): MenuItem[] {
    if (pressedOnRow) return [];
    const empty = rows.length === 0;
    return [
      { id: 'jump-latest', label: 'Jump to latest', disabled: empty, onselect: toBottom },
      { id: 'scroll-top', label: 'Scroll to top (load older)', disabled: empty, onselect: scrollToTop },
      { id: 'mark-read', label: 'Mark as read', onselect: () => void timelineStore.markRead() },
      { separator: true },
      { id: 'refresh', label: 'Refresh', onselect: () => void refreshStore.run() },
    ];
  }
</script>

<section class="timeline" aria-label="Conversation with {title}, {isDirect ? 'direct message' : 'room'}, {encrypted ? 'end-to-end encrypted' : 'not encrypted'}">
  <div class="viewport" role="presentation" bind:clientHeight={viewportHeight} onpointerdown={onViewportPointerDown} use:contextmenu={backgroundItems}>
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
              album={albums.leads.get(item.internal_id) ?? null}
              albumHidden={albums.hidden.has(item.internal_id)}
            />
          </div>
        {/snippet}
      </VList>
    {/if}

    {#if !atBottom && rows.length > 0}
      <div class="fabs">
        {#if mentionIds.length > 0}
          <button class="fab" type="button" onclick={toMention} aria-label="Jump to next mention" data-tooltip="Next mention">
            <Icon icon={AtSign} size={20} />
            <span class="badge">{badgeLabel(mentionIds.length)}</span>
          </button>
        {/if}
        <button class="fab" type="button" onclick={toBottom} aria-label={unseen > 0 ? `Jump to latest message, ${unseen} new` : 'Jump to latest message'}>
          <Icon icon={ArrowDown} size={20} />
          {#if unseen > 0}<span class="badge">{badgeLabel(unseen)}</span>{/if}
        </button>
      </div>
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
  .fabs {
    position: absolute;
    right: var(--alloy-layout-gutter);
    bottom: var(--alloy-spacing-scale-4);
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
  }
  .fab {
    position: relative;
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
  .badge {
    position: absolute;
    top: -6px;
    right: -6px;
    min-width: 20px;
    height: 20px;
    padding: 0 var(--alloy-spacing-scale-1);
    box-sizing: border-box;
    border-radius: var(--alloy-radii-full);
    background: var(--alloy-color-accent-primary);
    color: var(--alloy-color-text-on-accent);
    font-size: var(--alloy-typography-size-xs);
    line-height: 20px;
    text-align: center;
    font-variant-numeric: tabular-nums;
  }
  @media (max-width: 899px) {
    .fab {
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
