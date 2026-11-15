<script lang="ts">
  import type { RoomSummary } from '$lib/generated/protocol';
  import { Avatar, Badge } from '../common';
  import Icon from '$lib/icons/Icon.svelte';
  import { BellOff } from '$lib/icons';
  import { formatListTime } from '$lib/content/format';
  import { resolveAvatar } from '$lib/stores/avatars.svelte';
  import { isRoomMuted, roomsStore, roomTitle } from '$lib/stores/rooms.svelte';
  import { topicsOf } from './forumTopics';

  import { t } from '$lib/i18n';
  interface Props {
    /** The forum room whose topics this pane lists. */
    forum: RoomSummary;
    /** Opens a topic: the shell selects it and, on a phone, moves to the conversation pane. */
    onselect?: (roomId: string) => void;
  }
  let { forum, onselect }: Props = $props();

  const forumId = $derived(forum.id);
  const title = $derived(roomTitle(forum));
  // Topics are rooms of their own: the whole store is asked, not the filtered list, so a filter
  // never hides part of a forum.
  const topics = $derived(topicsOf(roomsStore.rooms, forumId));
  const currentId = $derived(roomsStore.activeRoomId);

  /** Resolved topic pictures by room id; null until the lookup runs. */
  let avatars = $state<Record<string, string | null>>({});
  let listEl: HTMLUListElement | null = $state(null);

  // Topic pictures are stored media; resolve them for the rows that show one.
  $effect(() => {
    for (const topic of topics) {
      if (!topic.avatar || Object.hasOwn(avatars, topic.id)) continue;
      avatars = { ...avatars, [topic.id]: null };
      void resolveAvatar(topic.avatar, 36).then((url) => (avatars = { ...avatars, [topic.id]: url }));
    }
  });

  /** Arrow keys move between rows and Home/End jump to the ends; Enter opens the row natively. */
  function onListKeydown(e: KeyboardEvent) {
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    const rows = Array.from(listEl?.querySelectorAll<HTMLButtonElement>('button.topic') ?? []);
    if (rows.length === 0) return;
    const at = rows.indexOf(document.activeElement as HTMLButtonElement);
    let next: number;
    if (e.key === 'ArrowDown') next = Math.min(at + 1, rows.length - 1);
    else if (e.key === 'ArrowUp') next = Math.max(at - 1, 0);
    else if (e.key === 'Home') next = 0;
    else if (e.key === 'End') next = rows.length - 1;
    else return;
    e.preventDefault();
    rows[Math.max(next, 0)]?.focus();
  }
</script>

<section class="pane" aria-labelledby="topics-pane-heading">
  <header class="pane-head">
    <h2 class="heading" id="topics-pane-heading">{t('topics-pane-heading', { name: title })}</h2>
    <p class="count">{t('forum-topic-count', { count: topics.length })}</p>
  </header>

  {#if topics.length === 0}
    <p class="empty" role="status">{t('topics-pane-empty')}</p>
  {:else}
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <ul
      class="list"
      bind:this={listEl}
      aria-label={t('topics-pane-section-label')}
      onkeydown={onListKeydown}
    >
      {#each topics as topic (topic.id)}
        {@const unread = roomsStore.unreadOf(topic)}
        {@const muted = isRoomMuted(topic)}
        {@const current = topic.id === currentId}
        {@const latest = topic.latest_event}
        <li>
          <button
            type="button"
            class="topic"
            class:current
            class:muted
            class:unread={unread > 0 && !muted}
            aria-current={current ? 'true' : undefined}
            onclick={() => onselect?.(topic.id)}
          >
            <Avatar
              userId={topic.id}
              displayName={roomTitle(topic)}
              src={avatars[topic.id] ?? null}
              size={36}
              shape="rounded"
            />
            <span class="text">
              <span class="title">{roomTitle(topic)}</span>
              <span class="preview">{latest?.body ?? t('topics-pane-no-messages')}</span>
            </span>
            <span class="end">
              {#if latest}
                <time class="time" datetime={new Date(latest.timestamp_millis).toISOString()}>
                  {formatListTime(latest.timestamp_millis)}
                </time>
              {/if}
              <span class="marks">
                {#if muted}
                  <Icon icon={BellOff} size={16} label={t('sidebar-notifications-muted')} />
                {/if}
                {#if unread > 0}
                  {#if muted}
                    <span class="dot" aria-hidden="true"></span>
                  {:else}
                    <Badge count={unread} />
                  {/if}
                {:else if topic.marked_unread}
                  <span class="dot accent" role="img" aria-label={t('sidebar-marked-as-unread')}></span>
                {/if}
              </span>
            </span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  /* The topics list is the forums' primary navigation surface: one click from a forum to all of
     its conversations, with the same row grammar as the chat list (avatar, preview, time, marks). */
  .pane {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    overflow-x: hidden;
    padding: var(--alloy-spacing-scale-2) 0 var(--alloy-spacing-scale-6);
  }
  .pane-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: var(--alloy-spacing-scale-2);
    padding: 0 var(--alloy-layout-gutter);
    margin-bottom: var(--alloy-spacing-scale-2);
  }
  .heading {
    margin: 0;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--alloy-typography-size-lg);
    font-weight: var(--alloy-typography-weight-semibold);
    letter-spacing: -0.01em;
    color: var(--alloy-color-text-primary);
  }
  .count {
    flex: none;
    margin: 0;
    font-size: var(--alloy-typography-size-xs);
    font-variant-numeric: tabular-nums;
    color: var(--alloy-color-text-muted);
  }
  .empty {
    margin: var(--alloy-spacing-scale-6) var(--alloy-layout-gutter);
    font-size: var(--alloy-typography-size-sm);
    text-align: center;
    color: var(--alloy-color-text-muted);
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    list-style: none;
    margin: 0;
    padding: 0 var(--alloy-spacing-scale-2);
  }
  .topic {
    position: relative;
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-3);
    box-sizing: border-box;
    width: 100%;
    min-height: 68px; /* the forum's main navigation row, taller than a chat row */
    padding: var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-3);
    border: 1px solid transparent;
    border-radius: var(--alloy-radii-lg);
    background: transparent;
    color: var(--alloy-color-text-secondary);
    font: inherit;
    text-align: left;
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
  /* The open topic keeps the accent rail the chat list uses for the open chat. */
  .topic.current {
    background: color-mix(in srgb, var(--alloy-color-accent-primary) 9%, transparent);
  }
  .topic.current::before {
    content: '';
    position: absolute;
    top: 50%;
    left: 3px;
    width: 3px;
    height: 24px;
    border-radius: var(--alloy-radii-full);
    background: var(--alloy-color-accent-primary);
    translate: 0 -50%;
  }
  .text {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-1);
    min-width: 0;
  }
  .title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--alloy-font-size-row);
    font-weight: var(--alloy-typography-weight-semibold);
    letter-spacing: -0.005em;
    color: var(--alloy-color-text-primary);
  }
  .topic:not(.unread) .title {
    font-weight: var(--alloy-typography-weight-medium);
  }
  .preview {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--alloy-font-size-preview);
    line-height: 1.35;
    color: var(--alloy-color-text-secondary);
  }
  .topic.muted .title,
  .topic.muted .preview {
    color: var(--alloy-color-text-muted);
  }
  .end {
    display: flex;
    flex: none;
    flex-direction: column;
    align-items: flex-end;
    gap: var(--alloy-spacing-scale-1);
  }
  .time {
    font-size: var(--alloy-font-size-meta);
    font-variant-numeric: tabular-nums;
    color: var(--alloy-color-text-muted);
    white-space: nowrap;
  }
  .topic.unread .time {
    color: var(--alloy-color-accent-primary);
    font-weight: var(--alloy-typography-weight-semibold);
  }
  .marks {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-1);
    min-height: 20px;
    color: var(--alloy-color-text-muted);
  }
  .dot {
    width: var(--alloy-spacing-scale-2);
    height: var(--alloy-spacing-scale-2);
    margin: 0 var(--alloy-spacing-scale-1);
    border-radius: var(--alloy-radii-full);
    background: var(--alloy-color-text-muted);
  }
  .dot.accent {
    background: var(--alloy-color-accent-primary);
  }
  @media (max-width: 899px) {
    .topic:active {
      background: var(--alloy-color-surface-hover);
    }
    .marks :global(.alloy-badge) {
      min-width: 20px !important;
      height: 20px !important;
    }
  }
</style>
