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
  import { anchorsOf, detectRowChange, type RowAnchors } from './rowChange';
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
      // One Timeline serves every room: forget the previous room's rows so the next rows count as an opening.
      anchors = null;
      sticky = false;
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
  // This runs before the list renders ($effect.pre): VList reads `shift` in the same pass that sees the
  // longer data, so the flag must already be set then. Set after render (a plain $effect) it comes too late
  // and the view jumps by the height of the loaded history wherever the webview has no scroll anchoring.
  let anchors: RowAnchors | null = null;
  // True from opening a room until the user first scrolls it: history that fills the window after the room
  // opened keeps the view on the newest message. A scroll-to-bottom aimed at an index goes stale when rows are
  // inserted above it, so it is re-aimed at the current last row on every change while this is set.
  let sticky = false;
  /** Any wheel, touch or pointer press on the list: the user is in charge of the scroll position from here. */
  function onUserScroll() {
    sticky = false;
  }
  $effect.pre(() => {
    const change = detectRowChange(anchors, rows);
    const opening = anchors === null && rows.length > 0;
    anchors = anchorsOf(rows);
    const len = rows.length;
    untrack(() => {
      // A new message from the signed-in user jumps to the bottom when that preference is on, even if the user had scrolled up.
      const appended = change.arrived.length > 0;
      const ownSend = appended && change.arrived[change.arrived.length - 1]?.is_own === true && prefs.jumpToBottomOnSend;
      if (change.prepended) {
        shift = true;
        void tick().then(() => requestAnimationFrame(() => (shift = false)));
      }
      // A reopened room carries a scroll offset to restore; that is where the reader was, so the
      // opening jump to the newest message must not fight it.
      const restoring = opening && timelineStore.pendingScroll !== null;
      if (opening && !restoring) sticky = true;
      if (restoring) {
        // the pending-scroll effect below applies the cached offset once the rows are mounted
      } else if (opening || ownSend || ((appended || change.replaced) && atBottom) || (sticky && (change.prepended || appended))) {
        void tick().then(() => vlist?.scrollToIndex(len - 1, { align: 'end' }));
      }
      // Only rows after the old last row are new messages; history loaded above, or rows inserted in the
      // middle, must not raise the "jump to latest" count.
      if (appended && !ownSend && !atBottom && !sticky) {
        const arrived = countArrivals(change.arrived);
        unseen += arrived.count;
        mentionIds = [...mentionIds, ...arrived.mentions];
      }
    });
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

  // The newest event counts as read when the room opens, when a message arrives while the user is
  // at the bottom, when the window returns to the foreground, and when the user's own message
  // arrives (they have read up to it even if the list is scrolled up).
  // "Visible" means the document is not hidden: a merely unfocused window is still showing the
  // conversation, and treating a desktop click elsewhere as "not reading" stopped receipts entirely.
  // (Scrolling alone never fires for a short list, which left the unread dot on.)
  let windowVisible = $state(typeof document === 'undefined' || document.visibilityState === 'visible');
  $effect(() => {
    const update = () => (windowVisible = document.visibilityState === 'visible');
    document.addEventListener('visibilitychange', update);
    return () => document.removeEventListener('visibilitychange', update);
  });
  $effect(() => {
    const newest = rows.findLast((r) => r.event_id);
    void windowVisible;
    if (!newest || timelineStore.loading) return;
    // Your own message is read by definition; anything else needs the viewport at the bottom.
    if (!atBottom && newest.is_own !== true) return;
    untrack(() => void timelineStore.markRead());
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

  // Reopening a cached room puts the reader back where they were; the store holds the offset.
  $effect(() => {
    const target = timelineStore.pendingScroll;
    if (target === null) return;
    untrack(() => {
      void tick().then(() =>
        requestAnimationFrame(() => {
          vlist?.scrollTo(target);
          timelineStore.pendingScroll = null;
        })
      );
    });
  });

  function onScroll(offset: number) {
    if (!vlist) return;
    timelineStore.rememberScroll(offset);
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
    onUserScroll();
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
  <div class="viewport" role="presentation" bind:clientHeight={viewportHeight} onpointerdown={onViewportPointerDown} onwheel={onUserScroll} ontouchstart={onUserScroll} use:contextmenu={backgroundItems}>
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

    {#if timelineStore.error || timelineStore.receiptError}
      <div class="toast" role="alert">{timelineStore.error ?? timelineStore.receiptError}</div>
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
    /* The chat pane is the glass card; the timeline stays clear so messages sit on it. */
    background: var(--alloy-chat-background, none), transparent;
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
    border: 1px solid var(--alloy-glass-rim);
    background: var(--alloy-glass-sheen), var(--alloy-glass-fill-strong);
    -webkit-backdrop-filter: var(--alloy-glass-filter);
    backdrop-filter: var(--alloy-glass-filter);
    color: var(--alloy-color-text-primary);
    display: grid;
    place-items: center;
    cursor: pointer;
    box-shadow: var(--alloy-glass-shadow-soft), var(--alloy-glass-highlight);
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
