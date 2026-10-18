<script lang="ts">
  import type { TimelineItemDto } from '$lib/generated';
  import { Avatar, UserName } from '../common';
  import Icon from '$lib/icons/Icon.svelte';
  import { Reply, Pencil, Trash2, Smile, Copy, Link, MessageSquare } from '$lib/icons';
  import { timelineStore } from '$lib/stores/timeline.svelte';
  import { threadStore } from '$lib/stores/thread.svelte';
  import { sanitizeHtml } from '$lib/content/html';
  import { formatDay, formatTime } from '$lib/content/format';
  import { QUICK_REACTIONS } from '$lib/content/emoji';
  import { interceptLinks } from '$lib/platform/links';
  import { portal } from '$lib/platform/portal';
  import { contextmenu } from '$lib/platform/contextmenu';
  import { profileCard } from '$lib/stores/profileCard.svelte';
  import type { MenuItem } from '$lib/stores/contextmenu.svelte';
  import MediaView from './MediaView.svelte';
  import PollView from './PollView.svelte';

  interface Props {
    item: TimelineItemDto;
    prev: TimelineItemDto | null;
    /** Rendered inside the thread panel: room-level reply and thread actions are hidden. */
    inThread?: boolean;
  }
  let { item, prev, inThread = false }: Props = $props();

  // Same sender within this window continues the previous message as a compact row.
  const GROUP_MS = 7 * 60_000;
  const isMessage = (i: TimelineItemDto | null) =>
    !!i && ['TextMessage', 'Media', 'Poll', 'Redacted', 'UnableToDecrypt'].includes(i.content.kind);

  const kind = $derived(item.content.kind);
  const grouped = $derived(
    isMessage(item) &&
      isMessage(prev) &&
      prev!.sender === item.sender &&
      item.timestamp_millis - prev!.timestamp_millis < GROUP_MS
  );
  const time = $derived(formatTime(item.timestamp_millis));
  const senderName = $derived(item.sender_display_name ?? item.sender);
  const failed = $derived(item.send_state.status === 'Failed');
  const sending = $derived(item.send_state.status === 'Sending');
  const canEdit = $derived(item.is_own && kind === 'TextMessage' && !!item.event_id);
  const canAct = $derived(!!item.event_id && (kind === 'TextMessage' || kind === 'Media'));
  const canCopy = $derived(kind === 'TextMessage');

  let picker = $state(false);
  let pickerPos = $state({ x: 0, y: 0 });
  let rowEl = $state<HTMLElement | undefined>();

  /** Opens the reaction picker beside `anchor`, or closes it when already open. */
  function openPicker(anchor: HTMLElement) {
    const r = anchor.getBoundingClientRect();
    pickerPos = { x: Math.max(8, Math.min(r.left, window.innerWidth - 280)), y: r.bottom + 6 };
    picker = !picker;
  }

  function openThread() {
    const roomId = timelineStore.roomId;
    if (roomId && item.event_id) void threadStore.open(roomId, item.event_id);
  }

  /** Opens the sender's profile card at the pointer, or centred on the avatar when activated from the keyboard. */
  function openSenderProfile(e: MouseEvent) {
    let at = { x: e.clientX, y: e.clientY };
    if (e.detail === 0) {
      const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
      at = { x: r.left + r.width / 2, y: r.top + r.height / 2 };
    }
    profileCard.open(item.sender, timelineStore.roomId, at);
  }

  const html = $derived(
    item.content.kind === 'TextMessage' && item.content.data.formatted_body
      ? sanitizeHtml(item.content.data.formatted_body)
      : null
  );

  function react(key: string) {
    picker = false;
    void timelineStore.toggleReaction(item, key);
  }


  function quote(it: TimelineItemDto): string {
    const c = it.content;
    if (c.kind === 'TextMessage') return c.data.body;
    if (c.kind === 'Media') return c.data.body || 'Attachment';
    return '';
  }

  /** Context-menu entries for this message. Groups that do not apply are omitted, not disabled. */
  function menuItems(): MenuItem[] {
    const roomId = timelineStore.roomId;
    const replies: MenuItem[] = [];
    if (canAct && !inThread) {
      replies.push({ id: 'reply', label: 'Reply', icon: Reply, onselect: () => timelineStore.startReply(item) });
      replies.push({ id: 'thread', label: 'Reply in thread', icon: MessageSquare, onselect: openThread });
    }
    if (canAct) replies.push({ id: 'react', label: 'React', icon: Smile, onselect: () => rowEl && openPicker(rowEl) });

    const copies: MenuItem[] = [];
    if (canCopy) copies.push({ id: 'copy-text', label: 'Copy text', icon: Copy, onselect: () => void navigator.clipboard.writeText(quote(item)) });
    if (roomId && item.event_id) {
      const link = `https://matrix.to/#/${roomId}/${item.event_id}`;
      copies.push({ id: 'copy-link', label: 'Copy link', icon: Link, onselect: () => void navigator.clipboard.writeText(link) });
    }
    copies.push({ id: 'copy-sender', label: 'Copy sender ID', icon: Copy, onselect: () => void navigator.clipboard.writeText(item.sender) });

    const edits: MenuItem[] = [];
    if (canEdit) edits.push({ id: 'edit', label: 'Edit', icon: Pencil, onselect: () => timelineStore.startEdit(item) });
    if (item.is_own) edits.push({ id: 'delete', label: 'Delete', icon: Trash2, danger: true, onselect: () => timelineStore.redact(item) });

    const separator: MenuItem = { separator: true };
    const groups = [replies, copies, edits].filter((g) => g.length > 0);
    return groups.flatMap((g, i) => (i === 0 ? g : [separator, ...g]));
  }
</script>

{#if item.content.kind === 'DayDivider'}
  <div class="divider"><span>{formatDay(item.content.data.timestamp_millis)}</span></div>
{:else if item.content.kind === 'RoomState'}
  <div class="system">
    {item.content.data.state_type === 'membership' ? 'Membership changed' : 'Room updated'}
  </div>
{:else if item.content.kind === 'Virtual'}
  <div class="spacer"></div>
{:else}
  <div class="row" class:grouped class:own={item.is_own} data-event-id={item.event_id ?? undefined} bind:this={rowEl} use:contextmenu={menuItems}>
    <div class="gutter">
      {#if grouped}
        <span class="gutter-time">{time}</span>
      {:else}
        <button type="button" class="avatar-btn" aria-label="Show profile of {senderName}" onclick={openSenderProfile}>
          <Avatar
            userId={item.sender}
            displayName={senderName}
            src={item.sender_avatar_url ?? null}
            size={40}
          />
        </button>
      {/if}
    </div>

    <div class="stack">
      {#if item.reply_to}
        <div class="reply">
          <UserName userId={item.reply_to.sender} displayName={item.reply_to.sender_display_name} roomId={timelineStore.roomId} interactive />
          <span class="reply-body">{quote(item.reply_to)}</span>
        </div>
      {/if}

      {#if !grouped}
        <div class="head">
          <UserName userId={item.sender} displayName={item.sender_display_name} roomId={timelineStore.roomId} interactive />
          <span class="ts">{time}</span>
        </div>
      {/if}

      {#if item.content.kind === 'TextMessage'}
        {#if html}
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
          <div class="body html" onclick={interceptLinks} onauxclick={interceptLinks}>{@html html}{#if item.edited}<span class="edited">(edited)</span>{/if}</div>
        {:else}
          <div class="body">{item.content.data.body}{#if item.edited}<span class="edited">(edited)</span>{/if}</div>
        {/if}
      {:else if item.content.kind === 'Media'}
        <MediaView
          uri={item.content.data.uri}
          mimeType={item.content.data.mime_type}
          body={item.content.data.body ?? ''}
          mediaKind={item.content.data.media_kind ?? 'file'}
          sizeBytes={item.content.data.size_bytes}
          width={item.content.data.width}
          height={item.content.data.height}
          thumbnailUri={item.content.data.thumbnail_uri}
        />
        {#if item.content.data.media_kind === 'image' && item.content.data.body}
          <div class="body caption">{item.content.data.body}{#if item.edited}<span class="edited">(edited)</span>{/if}</div>
        {/if}
      {:else if item.content.kind === 'Redacted'}
        <div class="body muted">Message deleted</div>
      {:else if item.content.kind === 'UnableToDecrypt'}
        <div class="body muted">Waiting for this message. Verify this device to read it.</div>
      {:else if item.content.kind === 'Poll'}
        <PollView {item} poll={item.content.data} />
      {:else}
        <div class="body muted">This message type is not supported yet.</div>
      {/if}

      {#if item.is_own && sending}
        <div class="state">Sending</div>
      {:else if failed}
        <div class="failed-actions">
          <span class="err">Failed to send</span>
          <button type="button" onclick={() => timelineStore.retry(item)}>Retry</button>
          <button type="button" onclick={() => timelineStore.cancel(item)}>Discard</button>
        </div>
      {/if}

      {#if item.thread_summary && !inThread}
        <button type="button" class="thread-chip" onclick={openThread} aria-label="Open thread, {item.thread_summary.reply_count} {item.thread_summary.reply_count === 1 ? 'reply' : 'replies'}">
          <Icon icon={MessageSquare} size={16} />
          <span>{item.thread_summary.reply_count} {item.thread_summary.reply_count === 1 ? 'reply' : 'replies'}</span>
        </button>
      {/if}
      {#if (item.reactions ?? []).length > 0}
        <div class="reactions">
          {#each item.reactions ?? [] as r (r.key)}
            <button
              type="button"
              class="chip"
              class:mine={r.by_me}
              onclick={() => react(r.key)}
              aria-pressed={r.by_me}
              aria-label="{r.key} {r.count}"
            >
              <span>{r.key}</span><span class="count">{r.count}</span>
            </button>
          {/each}
        </div>
      {/if}
    </div>

    {#if canAct}
      <div class="toolbar" role="toolbar" aria-label="Message actions">
        <button type="button" aria-label="React" title="React" onclick={(e) => openPicker(e.currentTarget as HTMLElement)}>
          <Icon icon={Smile} size={16} />
        </button>
        {#if !inThread}
          <button type="button" aria-label="Reply" title="Reply" onclick={() => timelineStore.startReply(item)}>
            <Icon icon={Reply} size={16} />
          </button>
        {/if}
        {#if item.event_id && !inThread}
          <button type="button" aria-label="Reply in thread" title="Reply in thread" onclick={openThread}>
            <Icon icon={MessageSquare} size={16} />
          </button>
        {/if}
        {#if canEdit}
          <button type="button" aria-label="Edit" title="Edit" onclick={() => timelineStore.startEdit(item)}>
            <Icon icon={Pencil} size={16} />
          </button>
        {/if}
        {#if item.is_own}
          <button type="button" aria-label="Delete" title="Delete" onclick={() => timelineStore.redact(item)}>
            <Icon icon={Trash2} size={16} />
          </button>
        {/if}
      </div>

      {#if picker}
        <button type="button" class="backdrop" aria-label="Close reactions" tabindex="-1" use:portal onclick={() => (picker = false)}></button>
        <div class="popover picker" role="menu" use:portal style="left: {pickerPos.x}px; top: {pickerPos.y}px;">
          {#each QUICK_REACTIONS as e}
            <button type="button" role="menuitem" onclick={() => react(e)}>{e}</button>
          {/each}
        </div>
      {/if}

    {/if}
  </div>
{/if}

<style>
  .row {
    position: relative;
    display: flex;
    align-items: flex-start;
    margin: 0 var(--alloy-layout-row-inset);
    padding: var(--alloy-spacing-scale-1) var(--alloy-layout-row-pad-x);
    border-radius: var(--alloy-radii-md);
    min-width: 0;
  }
  .row:not(.grouped) {
    padding-top: var(--alloy-spacing-scale-2);
  }
  .row:hover,
  .row:focus-within {
    background: var(--alloy-color-surface-hover);
  }
  .gutter {
    flex: none;
    width: 56px;
    display: flex;
    justify-content: center;
  }
  .gutter-time {
    padding-top: 4px;
    font-size: var(--alloy-typography-size-xs);
    color: var(--alloy-color-text-muted);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
    opacity: 0;
  }
  .row:hover .gutter-time,
  .row:focus-within .gutter-time {
    opacity: 1;
  }
  .stack {
    position: relative;
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
  }
  .head {
    display: flex;
    align-items: baseline;
    gap: var(--alloy-spacing-scale-2);
    min-width: 0;
  }
  .ts {
    font-size: var(--alloy-typography-size-xs);
    color: var(--alloy-color-text-muted);
    white-space: nowrap;
  }
  .reply {
    position: relative;
    display: flex;
    align-items: baseline;
    gap: var(--alloy-spacing-scale-2);
    min-width: 0;
    margin-bottom: 2px;
    padding-left: var(--alloy-spacing-scale-4);
    font-size: var(--alloy-typography-size-sm);
  }
  /* Curved connector: drops from the top of the quote and bends into its text. */
  .reply::before {
    content: '';
    position: absolute;
    left: 0;
    top: 0;
    width: var(--alloy-spacing-scale-3);
    height: 50%;
    border-left: 2px solid var(--alloy-color-border-prominent);
    border-bottom: 2px solid var(--alloy-color-border-prominent);
    border-bottom-left-radius: var(--alloy-radii-sm);
  }
  .reply-body {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--alloy-color-text-secondary);
  }
  .body {
    min-width: 0;
    color: var(--alloy-color-text-primary);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    line-height: 1.45;
  }
  .body.html {
    white-space: normal;
  }
  .body.html :global(p) {
    margin: 0;
  }
  .body.html :global(p + p) {
    margin-top: 0.5em;
  }
  .body.html :global(pre) {
    margin: var(--alloy-spacing-scale-1) 0;
    padding: var(--alloy-spacing-scale-3);
    background: var(--alloy-color-surface-subtle);
    border-radius: var(--alloy-radii-lg);
    overflow-x: auto;
  }
  .body.html :global(pre) :global(code) {
    padding: 0;
    background: none;
    border-radius: 0;
  }
  .body.html :global(code) {
    font-family: var(--alloy-typography-font-family-mono);
    font-size: 0.92em;
    padding: 0 var(--alloy-spacing-scale-1);
    background: var(--alloy-color-surface-subtle);
    border-radius: var(--alloy-radii-sm);
  }
  .body.html :global(a) {
    color: var(--alloy-color-accent-primary);
    text-decoration: underline;
  }
  .body.html :global(blockquote) {
    margin: var(--alloy-spacing-scale-1) 0;
    padding-left: var(--alloy-spacing-scale-3);
    border-left: 3px solid var(--alloy-color-border-prominent);
    color: var(--alloy-color-text-secondary);
  }
  .caption {
    margin-top: var(--alloy-spacing-scale-1);
  }
  .muted {
    font-style: italic;
    color: var(--alloy-color-text-muted);
  }
  .edited {
    margin-left: var(--alloy-spacing-scale-1);
    font-size: var(--alloy-typography-size-xs);
    color: var(--alloy-color-text-muted);
    white-space: nowrap;
  }
  .state {
    font-size: var(--alloy-typography-size-xs);
    color: var(--alloy-color-text-muted);
  }
  .failed-actions {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-3);
    font-size: var(--alloy-typography-size-sm);
  }
  .err {
    color: var(--alloy-color-status-error);
  }
  .failed-actions button {
    background: none;
    border: 0;
    color: var(--alloy-color-status-error);
    cursor: pointer;
    padding: 2px 0;
    font: inherit;
    text-decoration: underline;
  }
  .thread-chip {
    display: inline-flex;
    gap: var(--alloy-spacing-scale-1);
    align-items: center;
    margin-top: var(--alloy-spacing-scale-1);
    padding: 2px var(--alloy-spacing-scale-2);
    border: 0;
    border-radius: var(--alloy-radii-md);
    background: transparent;
    color: var(--alloy-color-accent-primary);
    font: inherit;
    font-size: var(--alloy-typography-size-sm);
    cursor: pointer;
  }
  .thread-chip:hover {
    background: var(--alloy-color-surface-hover);
  }
  .thread-chip:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 1px;
  }
  .reactions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--alloy-spacing-scale-1);
    margin-top: var(--alloy-spacing-scale-1);
  }
  .chip {
    display: inline-flex;
    gap: var(--alloy-spacing-scale-1);
    align-items: center;
    padding: 2px var(--alloy-spacing-scale-2);
    border-radius: var(--alloy-radii-full);
    border: 1px solid var(--alloy-color-border-prominent);
    background: var(--alloy-color-surface-subtle);
    color: var(--alloy-color-text-primary);
    font: inherit;
    font-size: var(--alloy-typography-size-sm);
    cursor: pointer;
  }
  .chip:hover {
    background: var(--alloy-color-surface-hover);
  }
  .chip.mine {
    border-color: var(--alloy-color-accent-primary);
    background: var(--alloy-color-accent-subtle);
  }
  .count {
    color: var(--alloy-color-text-secondary);
    font-variant-numeric: tabular-nums;
  }
  .toolbar {
    position: absolute;
    top: calc(-1 * var(--alloy-spacing-scale-3));
    right: var(--alloy-spacing-scale-4);
    display: none;
    align-items: center;
    gap: 2px;
    padding: 2px;
    background: var(--alloy-color-surface-elevated);
    border: 1px solid var(--alloy-color-border-subtle);
    border-radius: var(--alloy-radii-lg);
    box-shadow: var(--alloy-elevation-sm);
    z-index: 2;
  }
  .row:hover .toolbar,
  .row:focus-within .toolbar {
    display: flex;
  }
  .toolbar button {
    display: inline-grid;
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
  .toolbar button:hover {
    background: var(--alloy-color-surface-hover);
    color: var(--alloy-color-text-primary);
  }
  .toolbar button:focus-visible,
  .failed-actions button:focus-visible,
  .chip:focus-visible,
  .popover button:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 1px;
  }
  .backdrop {
    border: 0;
    padding: 0;
    background: transparent;
    position: fixed;
    inset: 0;
    z-index: 70;
  }
  .popover {
    position: fixed;
    z-index: 71;
    background: var(--alloy-color-surface-elevated);
    border: 1px solid var(--alloy-color-border-subtle);
    border-radius: var(--alloy-radii-lg);
    box-shadow: var(--alloy-elevation-md);
  }
  .picker {
    display: flex;
    gap: 2px;
    padding: var(--alloy-spacing-scale-1);
  }
  .picker button {
    padding: var(--alloy-spacing-scale-1);
    border: 0;
    border-radius: var(--alloy-radii-md);
    background: transparent;
    font-size: 1.1rem;
    line-height: 1;
    cursor: pointer;
  }
  .picker button:hover {
    background: var(--alloy-color-surface-hover);
  }
  .avatar-btn {
    display: inline-grid;
    padding: 0;
    border: 0;
    border-radius: var(--alloy-radii-full);
    background: none;
    cursor: pointer;
  }
  .avatar-btn:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }
  .divider {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-3);
    padding: var(--alloy-spacing-scale-3) var(--alloy-spacing-scale-4);
  }
  .divider::before,
  .divider::after {
    content: '';
    flex: 1;
    height: 1px;
    background: var(--alloy-color-border-prominent);
  }
  .divider span {
    font-size: var(--alloy-typography-size-sm);
    font-weight: var(--alloy-typography-weight-semibold);
    color: var(--alloy-color-text-secondary);
    white-space: nowrap;
  }
  .system {
    padding: var(--alloy-spacing-scale-1) var(--alloy-spacing-scale-4);
    text-align: center;
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-text-muted);
  }
  .spacer {
    height: 1px;
  }
</style>
