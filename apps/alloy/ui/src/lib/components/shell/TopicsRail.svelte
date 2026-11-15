<script lang="ts">
  import type { RoomSummary } from '$lib/generated/protocol';
  import { Avatar, Badge } from '../common';
  import Icon from '$lib/icons/Icon.svelte';
  import { BellOff } from '$lib/icons';
  import { isRoomMuted, roomsStore, roomTitle } from '$lib/stores/rooms.svelte';
  import { viewport } from '$lib/stores/viewport.svelte';
  import { topicsOf } from './forumTopics';

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
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<nav
  class="rail"
  class:phone
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
            title={name}
            onclick={() => onpick(topic.id)}
          >
            <span class="badge">
              <Avatar userId={topic.id} displayName={name} size={32} />
              <span class="mark">
                {#if unread > 0}
                  {#if muted}
                    <span class="dot"></span>
                  {:else}
                    <Badge count={unread} />
                  {/if}
                {:else if topic.marked_unread}
                  <span class="dot accent"></span>
                {/if}
              </span>
            </span>
            <span class="label">{name}</span>
            {#if muted}
              <span class="quiet"><Icon icon={BellOff} size={16} /></span>
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
    width: 84px; /* control metric: an avatar plus two lines of label, narrower than a room row */
    /* `--chat-rail-lead` clears the macOS traffic lights when the chat starts at the window edge. */
    padding: var(--chat-rail-lead, var(--alloy-spacing-scale-2)) 0 var(--alloy-spacing-scale-2);
    overflow-x: hidden;
    overflow-y: auto;
    overscroll-behavior: contain;
    border-right: 1px solid var(--alloy-chat-rim);
  }
  .items {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-1);
    margin: 0;
    padding: 0 var(--alloy-spacing-scale-1);
    list-style: none;
  }
  .slot {
    display: flex;
  }
  .topic {
    display: flex;
    flex: 1;
    flex-direction: column;
    align-items: center;
    gap: var(--alloy-spacing-scale-1);
    box-sizing: border-box;
    width: 100%;
    padding: var(--alloy-spacing-scale-2) 0;
    border: 1px solid transparent;
    border-radius: var(--alloy-radii-lg);
    background: transparent;
    color: var(--alloy-color-text-secondary);
    font: inherit;
    text-align: center;
    cursor: pointer;
    transition: background var(--alloy-motion-duration-fast) ease;
  }
  .topic:hover {
    background: var(--alloy-glass-control);
  }
  .topic:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: -2px;
  }
  /* The open topic keeps the accent the chat list uses for the open chat. */
  .topic.current {
    background: color-mix(in srgb, var(--alloy-color-accent-primary) 10%, transparent);
  }
  .topic.current .badge {
    box-shadow: 0 0 0 2px var(--alloy-color-accent-primary);
  }
  .badge {
    position: relative;
    display: inline-flex;
    border-radius: var(--alloy-radii-full);
  }
  .mark {
    position: absolute;
    top: -4px;
    right: -6px;
    display: inline-flex;
    align-items: center;
  }
  /* The count rides the badge, so the label keeps the full width of the column. */
  .mark :global(.alloy-badge) {
    min-width: 16px !important;
    height: 16px !important;
    padding: 0 var(--alloy-spacing-scale-1) !important;
    font-size: 10px !important;
  }
  .dot {
    width: var(--alloy-spacing-scale-2);
    height: var(--alloy-spacing-scale-2);
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
    max-width: 100%;
    font-size: var(--alloy-typography-size-xs);
    line-height: 1.25;
    overflow-wrap: anywhere;
    color: var(--alloy-color-text-secondary);
  }
  .topic.unread .label {
    font-weight: var(--alloy-typography-weight-semibold);
    color: var(--alloy-color-text-primary);
  }
  .topic.current .label {
    color: var(--alloy-color-text-primary);
  }
  .topic.muted .label {
    color: var(--alloy-color-text-muted);
  }
  .quiet {
    display: inline-flex;
    color: var(--alloy-color-text-muted);
  }
  .empty {
    margin: 0;
    padding: var(--alloy-spacing-scale-2);
    font-size: var(--alloy-typography-size-xs);
    line-height: 1.3;
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
    border-right: 0;
    border-bottom: 1px solid var(--alloy-chat-rim);
    scrollbar-width: none;
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
    max-width: 200px; /* control metric: a chip long enough for a two-word topic */
    min-height: var(--alloy-touch-target, 44px);
    padding: 0 var(--alloy-spacing-scale-3) 0 var(--alloy-spacing-scale-1);
    border-color: var(--alloy-glass-rim);
    border-radius: var(--alloy-radii-full);
  }
  .rail.phone .label {
    -webkit-line-clamp: 1;
    line-clamp: 1;
    text-align: left;
  }
</style>
