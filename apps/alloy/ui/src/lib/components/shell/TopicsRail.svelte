<script lang="ts">
  import type { RoomSummary } from '$lib/generated/protocol';
  import { Bell, BellOff, CheckCheck, Copy, Mail } from '$lib/icons';
  import { contextmenu } from '$lib/platform/contextmenu';
  import type { MenuItem } from '$lib/stores/contextmenu.svelte';
  import { muteRoom, unmuteRoom } from '$lib/stores/muteTimer.svelte';
  import { Avatar, Badge } from '../common';
  import { isRoomMuted, roomsStore, roomTitle } from '$lib/stores/rooms.svelte';
  import { viewport } from '$lib/stores/viewport.svelte';
  import { topicsOf } from './forumTopics';
  import { copyItems, notifyItems } from './roomMenus';

  import { t } from '$lib/i18n';
  interface Props {
    /** The forum room whose topics this rail lists. */
    forum: RoomSummary;
    /** Opens a topic: the shell selects it without leaving the conversation. */
    onpick: (roomId: string) => void;
  }
  let { forum, onpick }: Props = $props();

  // Topics are conversations of their own: the whole store is asked, so a sidebar filter never
  // hides part of a forum, and the order is the newest activity first (Telegram's topic rail).
  const topics = $derived(topicsOf(roomsStore.rooms, forum.id));
  const currentId = $derived(roomsStore.activeRoomId);
  const forumTitle = $derived(roomTitle(forum));
  const phone = $derived(viewport.isPhone);

  let railEl = $state<HTMLElement | null>(null);
  /** The first placement is instant: a rail that animates before the reader touches it reads as lag. */
  let placed = false;

  /**
   * Selection can land on a topic the rail is not showing (a deep link, a notification, or the
   * newest activity moving the open topic), so the rail brings it into view. It animates only when
   * the reader has not asked for reduced motion.
   */
  $effect(() => {
    const id = currentId;
    const rail = railEl;
    if (!id || !rail) return;
    const topic = rail.querySelector<HTMLElement>(`[data-topic-id="${CSS.escape(id)}"]`);
    if (!topic) return;
    const reduced =
      document.documentElement.dataset.reducedMotion === 'true' ||
      window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    topic.scrollIntoView({
      block: 'nearest',
      inline: 'nearest',
      behavior: placed && !reduced ? 'smooth' : 'auto'
    });
    placed = true;
  });

  /**
   * The accessible name carries the state the small marks cannot: the full topic name, its unread
   * count, and whether it is muted or was marked unread by hand.
   */
  function labelOf(topic: RoomSummary, unread: number): string {
    const parts = [roomTitle(topic)];
    // The unread count reuses the room list's word for it, so both surfaces read the same.
    if (unread > 0) parts.push(`${unread} ${t('sidebar-unread')}`);
    if (isRoomMuted(topic)) parts.push(t('sidebar-notifications-muted'));
    else if (topic.marked_unread) parts.push(t('sidebar-marked-as-unread'));
    return parts.join(', ');
  }

  /**
   * Arrow keys walk the rail along its axis: up/down in the column, left/right in the phone strip.
   * Enter and Space open a topic, because every row is a button.
   */
  function onRailKeydown(e: KeyboardEvent) {
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    const back = phone ? 'ArrowLeft' : 'ArrowUp';
    const forward = phone ? 'ArrowRight' : 'ArrowDown';
    if (e.key !== back && e.key !== forward) return;
    const rows = Array.from(
      (e.currentTarget as HTMLElement).querySelectorAll<HTMLButtonElement>('button.topic')
    );
    const at = rows.indexOf(document.activeElement as HTMLButtonElement);
    if (at === -1) return;
    const to = e.key === forward ? Math.min(at + 1, rows.length - 1) : Math.max(at - 1, 0);
    if (to === at) return;
    e.preventDefault();
    rows[to]?.focus();
  }

  /** Context and long-press actions for a forum topic: read state, mute, notifications and ID copy. */
  function topicItems(topic: RoomSummary): MenuItem[] {
    const unread = roomsStore.unreadOf(topic);
    const muted = isRoomMuted(topic);
    return [
      {
        id: 'mark-unread',
        label: unread > 0 || topic.marked_unread ? t('common-mark-as-read') : t('sidebar-mark-as-unread'),
        icon: unread > 0 || topic.marked_unread ? CheckCheck : Mail,
        onselect: () => {
          if (unread > 0) {
            roomsStore.markRead(topic.id);
            if (topic.marked_unread) void roomsStore.setMarkedUnread(topic.id, false);
          } else {
            void roomsStore.setMarkedUnread(topic.id, !topic.marked_unread);
          }
        },
      },
      {
        id: 'mute',
        label: muted ? t('room-swipe-unmute') : t('room-swipe-mute'),
        icon: muted ? Bell : BellOff,
        onselect: () => void (muted ? unmuteRoom(topic.id) : muteRoom(topic.id, null)),
      },
      { separator: true },
      {
        id: 'notifications',
        label: t('sidebar-notifications'),
        icon: Bell,
        submenu: {
          items: () => notifyItems(topic.id),
        },
      },
      {
        id: 'copy',
        label: t('common-copy'),
        icon: Copy,
        submenu: {
          items: copyItems(topic),
        },
      },
    ];
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<nav
  class="rail"
  class:phone
  bind:this={railEl}
  aria-label={t('topics-rail-label', { name: forumTitle })}
  onkeydown={onRailKeydown}
>
  {#if topics.length === 0}
    <p class="empty" role="status">{t('topics-rail-empty')}</p>
  {:else}
    <ul class="items">
      {#each topics as topic (topic.id)}
        {@const name = roomTitle(topic)}
        {@const unread = roomsStore.unreadOf(topic)}
        {@const muted = isRoomMuted(topic)}
        {@const current = topic.id === currentId}
        <li class="slot">
          <button
            type="button"
            class="topic"
            class:current
            class:muted
            class:unread={unread > 0 && !muted}
            aria-current={current ? 'true' : undefined}
            aria-label={labelOf(topic, unread)}
            aria-keyshortcuts="Shift+F10"
            title={name}
            data-topic-id={topic.id}
            use:contextmenu={{
              items: () => topicItems(topic),
              options: () => ({ ariaLabel: t('sidebar-room-actions', { room: name }) }),
            }}
            onclick={() => onpick(topic.id)}
          >
            <Avatar userId={topic.id} displayName={name} size={phone ? 24 : 32} />
            <span class="label">{name}</span>
            {#if unread > 0 || topic.marked_unread}
              <span class="mark" aria-hidden="true">
                {#if unread > 0 && !muted}
                  <Badge count={unread} />
                {:else if unread > 0}
                  <span class="dot"></span>
                {:else}
                  <span class="dot accent"></span>
                {/if}
              </span>
            {/if}
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</nav>

<style>
  /* The rail is the forum's own navigation: a narrow column beside the messages on desktop, a
     chip strip above them on a phone. It scrolls on its own and never moves with the timeline. */
  .rail {
    display: flex;
    flex: none;
    flex-direction: column;
    min-height: 0;
    width: 88px; /* 8pt grid: 8px gutters around a 72px item, an avatar plus two lines of label */
    /* `--chat-rail-lead` clears the macOS traffic lights when the chat starts at the window edge. */
    padding: var(--chat-rail-lead, var(--alloy-spacing-scale-2)) 0 var(--alloy-spacing-scale-2);
    overflow-x: hidden;
    overflow-y: auto;
    overscroll-behavior: contain;
    -webkit-overflow-scrolling: touch;
    touch-action: pan-y;
    border-right: 1px solid var(--alloy-chat-rim);
    scroll-padding-block: var(--alloy-spacing-scale-2);
  }
  .items {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-2);
    margin: 0;
    padding: 0 var(--alloy-spacing-scale-2);
    list-style: none;
  }
  .slot {
    display: flex;
  }
  .topic {
    /* The opaque surface behind an item, so the unread mark can ring itself in it and read as a
       gap between the avatar and the count rather than as a halo. */
    --rail-item-surface: var(--alloy-color-surface-canvas);
    position: relative;
    display: flex;
    flex: 1;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--alloy-spacing-scale-1);
    box-sizing: border-box;
    width: 100%;
    /* 8pt grid: avatar 32 + 4 + two 11px lines 28 + 8 padding. Two-line labels grow it by a hair. */
    min-height: 72px;
    padding: var(--alloy-spacing-scale-1);
    border: 0;
    border-radius: var(--alloy-bubble-radius);
    background: transparent;
    color: var(--alloy-color-text-secondary);
    font: inherit;
    text-align: center;
    cursor: pointer;
    scroll-margin-block: var(--alloy-spacing-scale-2);
    transition: background var(--alloy-motion-duration-fast) var(--alloy-motion-easing-ease);
  }
  @media (hover: hover) {
    .topic:hover {
      background: var(--alloy-glass-control);
    }
  }
  .topic:not(.current):active {
    background: var(--alloy-glass-control-hover);
  }
  /* The open topic is a filled surface with a start bar, so it is unmistakable at a glance. */
  .topic.current {
    background: var(--alloy-color-accent-subtle);
    --rail-item-surface: var(--alloy-color-accent-subtle);
  }
  .topic.current::before {
    content: '';
    position: absolute;
    /* Inset past the 20px corner curve so the bar never breaks the item's edge. */
    inset-block: var(--alloy-spacing-scale-5);
    inset-inline-start: 0;
    width: calc(var(--alloy-spacing-scale-1) / 2);
    border-radius: var(--alloy-radii-full);
    background: var(--alloy-color-accent-primary);
  }
  .topic:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }
  /* The unread mark rides the avatar's top corner, clear of the circle and of the next item, and
     is ringed in the item's own surface so the avatar never shows through. */
  .mark {
    position: absolute;
    inset-block-start: var(--alloy-spacing-scale-1);
    inset-inline-end: var(--alloy-spacing-scale-4);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    box-shadow: 0 0 0 2px var(--rail-item-surface);
    border-radius: var(--alloy-radii-full);
  }
  /* The count stays on the mark, so the label keeps the full width of the column. */
  .mark :global(.alloy-badge) {
    min-width: 16px !important;
    height: 16px !important;
    padding: 0 var(--alloy-spacing-scale-1) !important;
    font-size: 10px !important;
  }
  .dot {
    width: 6px;
    height: 6px;
    border-radius: var(--alloy-radii-full);
    background: var(--alloy-color-text-muted);
  }
  .dot.accent {
    background: var(--alloy-color-accent-primary);
  }
  .label {
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    overflow: hidden;
    width: 100%;
    /* 11px: two lines of a topic name fit a 64px column without clipping at the rail edge. */
    font-size: var(--alloy-typography-size-xs);
    line-height: var(--alloy-typography-line-height-tight);
    overflow-wrap: break-word;
    text-wrap: pretty;
    color: var(--alloy-color-text-secondary);
  }
  .topic.unread .label {
    font-weight: var(--alloy-typography-weight-semibold);
    color: var(--alloy-color-text-primary);
  }
  .topic.muted .label {
    color: var(--alloy-color-text-muted);
  }
  /* The open topic reads at full strength even when its notifications are muted. */
  .topic.current .label {
    color: var(--alloy-color-text-primary);
  }
  .empty {
    margin: 0;
    padding: var(--alloy-spacing-scale-2);
    font-size: var(--alloy-typography-size-xs);
    line-height: var(--alloy-typography-line-height-tight);
    text-align: center;
    color: var(--alloy-color-text-muted);
  }

  /* Phone: the same topics as a horizontal strip of chips above the header. */
  .rail.phone {
    flex-direction: row;
    width: auto;
    max-width: 100%;
    padding: var(--alloy-spacing-scale-2) 0 0;
    overflow-x: auto;
    overflow-y: hidden;
    overscroll-behavior: contain;
    -webkit-overflow-scrolling: touch;
    touch-action: pan-x;
    border-right: 0;
    border-bottom: 1px solid var(--alloy-chat-rim);
    scrollbar-width: none;
    scroll-padding-inline: var(--alloy-spacing-scale-4);
    scroll-padding-block: 0;
  }
  .rail.phone .items {
    flex-direction: row;
    gap: var(--alloy-spacing-scale-2);
    padding: 0 var(--alloy-spacing-scale-4) var(--alloy-spacing-scale-2);
  }
  .rail.phone .topic {
    flex-direction: row;
    flex: none;
    gap: var(--alloy-spacing-scale-2);
    width: auto;
    max-width: 220px; /* control metric: a chip long enough for a two-word topic plus its count */
    height: 36px;
    min-height: 36px;
    padding: 0 var(--alloy-spacing-scale-2) 0 var(--alloy-spacing-scale-1);
    border: 1px solid var(--alloy-glass-rim);
    border-radius: var(--alloy-radii-full);
    text-align: start;
    scroll-margin-block: 0;
    scroll-margin-inline: var(--alloy-spacing-scale-2);
  }
  /* The chip keeps its 36px shape while the hit area still clears the 44px touch minimum. */
  .rail.phone .topic::after {
    content: '';
    position: absolute;
    inset: calc(-1 * var(--alloy-spacing-scale-1)) 0;
  }
  /* A chip is too short for a start bar: the accent edge outlines the capsule instead. */
  .rail.phone .topic.current::before {
    content: none;
  }
  .rail.phone .topic.current {
    border-color: var(--alloy-color-accent-primary);
  }
  /* A chip carries its count after the label, in the flow, so nothing sits on the avatar. */
  .rail.phone .mark {
    position: static;
    inset: auto;
    flex: none;
    box-shadow: none;
  }
  .rail.phone .label {
    display: block;
    -webkit-line-clamp: unset;
    line-clamp: unset;
    flex: none;
    width: auto;
    max-width: 150px; /* control metric: keeps a long topic from swallowing the strip */
    font-size: var(--alloy-typography-size-sm);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  @media (pointer: coarse) {
    .topic {
      min-width: var(--alloy-touch-target, 44px);
    }
    .rail.phone .topic {
      height: auto;
      min-height: var(--alloy-touch-target, 44px);
      min-width: var(--alloy-touch-target, 44px);
    }
    .rail.phone .topic::after {
      content: none;
    }
  }
</style>
