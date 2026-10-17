<script lang="ts">
  import type { TimelineItemDto } from '$lib/generated';
  import { Avatar, UserName } from '../common';
  import Icon from '$lib/icons/Icon.svelte';
  import { Reply, Pencil, Trash2, Smile, Check, CheckCheck } from '$lib/icons';
  import { timelineStore } from '$lib/stores/timeline.svelte';
  import { themeStore } from '$lib/theme/theme.svelte';
  import { sanitizeHtml } from '$lib/content/html';
  import { formatDay, formatTime } from '$lib/content/format';
  import { QUICK_REACTIONS } from '$lib/content/emoji';
  import { interceptLinks } from '$lib/platform/links';
  import { portal } from '$lib/platform/portal';
  import MediaView from './MediaView.svelte';

  interface Props {
    item: TimelineItemDto;
    prev: TimelineItemDto | null;
    next: TimelineItemDto | null;
    isDirect: boolean;
    myUserId: string | null;
  }
  let { item, prev, next, isDirect, myUserId }: Props = $props();

  const GROUP_MS = 5 * 60_000;
  const isMessage = (i: TimelineItemDto | null) =>
    !!i && ['TextMessage', 'Media', 'Redacted', 'UnableToDecrypt'].includes(i.content.kind);

  const gutterStyle = $derived(themeStore.messageStyle === 'gutter');
  const kind = $derived(item.content.kind);
  const grouped = $derived(
    isMessage(item) &&
      isMessage(prev) &&
      prev!.sender === item.sender &&
      item.timestamp_millis - prev!.timestamp_millis < GROUP_MS
  );
  const endsGroup = $derived(
    !(
      isMessage(next) &&
      next!.sender === item.sender &&
      next!.timestamp_millis - item.timestamp_millis < GROUP_MS
    )
  );
  const showName = $derived((!item.is_own && !isDirect && !grouped) || (gutterStyle && !grouped));
  const failed = $derived(item.send_state.status === 'Failed');
  const sending = $derived(item.send_state.status === 'Sending');
  const seen = $derived((item.read_receipts ?? []).some((u) => u !== myUserId));
  const canEdit = $derived(item.is_own && kind === 'TextMessage' && !!item.event_id);
  const canAct = $derived(!!item.event_id && (kind === 'TextMessage' || kind === 'Media'));

  let picker = $state(false);
  let pickerPos = $state({ x: 0, y: 0 });

  function togglePicker(e: MouseEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    pickerPos = { x: Math.max(8, Math.min(r.left, window.innerWidth - 280)), y: r.bottom + 6 };
    picker = !picker;
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
</script>

{#if kind === 'DayDivider' && item.content.kind === 'DayDivider'}
  <div class="divider"><span>{formatDay(item.content.data.timestamp_millis)}</span></div>
{:else if kind === 'RoomState'}
  <div class="system">{item.content.kind === 'RoomState' && item.content.data.state_type === 'membership' ? 'Membership changed' : 'Room updated'}</div>
{:else if kind === 'Virtual'}
  <div class="spacer"></div>
{:else}
  <div
    class="row"
    class:own={item.is_own}
    class:grouped
    class:ends-group={endsGroup}
    data-event-id={item.event_id ?? undefined}
  >
    <div class="gutter">
      {#if (!item.is_own && !isDirect && endsGroup) || (gutterStyle && !grouped)}
        <Avatar
          userId={item.sender}
          displayName={item.sender_display_name ?? item.sender}
          size="sm"
        />
      {/if}
    </div>

    <div class="stack">
      <div class="bubble" class:failed>
        {#if showName}
          <div class="name">
            <UserName userId={item.sender} displayName={item.sender_display_name} />
          </div>
        {/if}

        {#if item.reply_to}
          <div class="quote" style="border-color: {item.is_own ? 'currentColor' : 'var(--alloy-color-accent-primary)'};">
            <UserName
              userId={item.reply_to.sender}
              displayName={item.reply_to.sender_display_name}
            />
            <span class="quote-body">{quote(item.reply_to)}</span>
          </div>
        {/if}

        {#if item.content.kind === 'TextMessage'}
          {#if html}
            <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
            <div class="body html" onclick={interceptLinks}>{@html html}</div>
          {:else}
            <div class="body">{item.content.data.body}</div>
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
          />
          {#if item.content.data.media_kind === 'image' && item.content.data.body}
            <div class="body caption">{item.content.data.body}</div>
          {/if}
        {:else if item.content.kind === 'Redacted'}
          <div class="body muted">Message deleted</div>
        {:else if item.content.kind === 'UnableToDecrypt'}
          <div class="body muted">Waiting for this message. Verify this device to read it.</div>
        {:else}
          <div class="body muted">This message type is not supported yet.</div>
        {/if}

        <div class="meta">
          {#if item.edited}<span>edited</span>{/if}
          <span>{formatTime(item.timestamp_millis)}</span>
          {#if item.is_own}
            {#if sending}
              <span>Sending</span>
            {:else if failed}
              <span class="err">Not sent</span>
            {:else if seen}
              <Icon icon={CheckCheck} size={16} label="Seen" />
            {:else}
              <Icon icon={Check} size={16} label="Sent" />
            {/if}
          {/if}
        </div>
      </div>

      {#if failed}
        <div class="failed-actions">
          <button type="button" onclick={() => timelineStore.retry(item)}>Retry</button>
          <button type="button" onclick={() => timelineStore.cancel(item)}>Discard</button>
        </div>
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

      {#if canAct}
        <div class="toolbar" role="toolbar" aria-label="Message actions">
          <button type="button" aria-label="Reply" title="Reply" onclick={() => timelineStore.startReply(item)}>
            <Icon icon={Reply} size={16} />
          </button>
          <button type="button" aria-label="React" title="React" onclick={togglePicker}>
            <Icon icon={Smile} size={16} />
          </button>
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
          <button type="button" class="picker-backdrop" aria-label="Close reactions" tabindex="-1" use:portal onclick={() => (picker = false)}></button>
          <div class="picker" role="menu" use:portal style="left: {pickerPos.x}px; top: {pickerPos.y}px;">
            {#each QUICK_REACTIONS as e}
              <button type="button" role="menuitem" onclick={() => react(e)}>{e}</button>
            {/each}
          </div>
        {/if}
      {/if}
    </div>
  </div>
{/if}

<style>
  .row {
    position: relative;
    display: flex;
    align-items: flex-end;
    gap: var(--alloy-spacing-scale-2);
    padding: 2px var(--alloy-spacing-scale-4);
    padding-top: 6px;
  }
  .row.grouped {
    padding-top: 1px;
  }
  .row.own {
    flex-direction: row-reverse;
  }
  .gutter {
    width: 28px;
    flex: none;
    align-self: flex-end;
  }
  .row.own .gutter {
    display: none;
  }
  .stack {
    position: relative;
    display: flex;
    flex-direction: column;
    max-width: min(70%, 620px);
    align-items: flex-start;
    min-width: 0;
  }
  .row.own .stack {
    align-items: flex-end;
  }
  .bubble {
    background: var(--alloy-color-bubble-other-background);
    color: var(--alloy-color-bubble-other-text);
    border-radius: var(--alloy-radii-lg);
    padding: var(--alloy-spacing-message-padding) calc(var(--alloy-spacing-message-padding) + 4px);
    min-width: 0;
    max-width: 100%;
    overflow-wrap: anywhere;
  }
  .row.own .bubble {
    background: var(--alloy-color-bubble-own-background);
    color: var(--alloy-color-bubble-own-text);
  }
  .row.grouped .bubble {
    border-top-left-radius: var(--alloy-radii-sm);
  }
  .row.own.grouped .bubble {
    border-top-left-radius: var(--alloy-radii-lg);
    border-top-right-radius: var(--alloy-radii-sm);
  }
  .row:not(.ends-group) .bubble {
    border-bottom-left-radius: var(--alloy-radii-sm);
  }
  .row.own:not(.ends-group) .bubble {
    border-bottom-left-radius: var(--alloy-radii-lg);
    border-bottom-right-radius: var(--alloy-radii-sm);
  }
  .bubble.failed {
    outline: 1px solid var(--alloy-color-status-error);
  }
  .name {
    font-size: var(--alloy-typography-size-sm);
    margin-bottom: 2px;
  }
  .body {
    white-space: pre-wrap;
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
    background: rgba(0, 0, 0, 0.25);
    padding: 8px;
    border-radius: var(--alloy-radii-sm);
    overflow-x: auto;
  }
  .body.html :global(code) {
    font-family: var(--alloy-typography-font-family-mono);
    font-size: 0.92em;
  }
  .body.html :global(a) {
    color: inherit;
    text-decoration: underline;
  }
  .body.html :global(blockquote) {
    margin: 4px 0;
    padding-left: 8px;
    border-left: 3px solid currentColor;
    opacity: 0.85;
  }
  .caption {
    margin-top: 4px;
  }
  .muted {
    font-style: italic;
    opacity: 0.7;
  }
  .quote {
    display: flex;
    flex-direction: column;
    border-left: 3px solid;
    padding-left: 8px;
    margin-bottom: 4px;
    font-size: var(--alloy-typography-size-sm);
  }
  .quote-body {
    opacity: 0.85;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .meta {
    display: flex;
    justify-content: flex-end;
    align-items: center;
    gap: 4px;
    font-size: var(--alloy-typography-size-xs);
    opacity: 0.75;
    margin-top: 2px;
  }
  .err {
    color: var(--alloy-color-status-error);
    opacity: 1;
  }
  .failed-actions {
    display: flex;
    gap: var(--alloy-spacing-scale-3);
    font-size: var(--alloy-typography-size-sm);
  }
  .failed-actions button {
    background: none;
    border: 0;
    color: var(--alloy-color-status-error);
    cursor: pointer;
    padding: 2px 0;
    font: inherit;
  }
  .reactions {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin-top: 4px;
  }
  .chip {
    display: inline-flex;
    gap: 4px;
    align-items: center;
    padding: 2px 8px;
    border-radius: var(--alloy-radii-full);
    border: 1px solid var(--alloy-color-border-prominent);
    background: var(--alloy-color-surface-subtle);
    color: var(--alloy-color-text-primary);
    font: inherit;
    font-size: var(--alloy-typography-size-sm);
    cursor: pointer;
  }
  .chip.mine {
    border-color: var(--alloy-color-accent-primary);
    background: var(--alloy-color-accent-subtle);
  }
  .toolbar {
    position: absolute;
    top: 50%;
    transform: translateY(-50%);
    left: calc(100% + 6px);
    display: none;
    gap: 2px;
    padding: 2px;
    background: var(--alloy-color-surface-elevated);
    border: 1px solid var(--alloy-color-border-subtle);
    border-radius: var(--alloy-radii-md);
    box-shadow: var(--alloy-elevation-sm);
    z-index: 2;
  }
  .row.own .toolbar {
    left: auto;
    right: calc(100% + 6px);
  }
  .row:hover .toolbar,
  .row:focus-within .toolbar {
    display: flex;
  }
  .toolbar button {
    border: 0;
    background: transparent;
    color: var(--alloy-color-text-secondary);
    border-radius: var(--alloy-radii-sm);
    padding: 4px;
    cursor: pointer;
    display: inline-flex;
  }
  .toolbar button:hover {
    background: var(--alloy-color-surface-hover);
    color: var(--alloy-color-text-primary);
  }
  .picker-backdrop {
    border: 0;
    padding: 0;
    background: transparent;
    position: fixed;
    inset: 0;
    z-index: 70;
  }
  .picker {
    position: fixed;
    z-index: 71;
    display: flex;
    gap: 2px;
    padding: 4px;
    background: var(--alloy-color-surface-elevated);
    border: 1px solid var(--alloy-color-border-subtle);
    border-radius: var(--alloy-radii-md);
    box-shadow: var(--alloy-elevation-md);
  }
  .picker button {
    font-size: 1.1rem;
  }
  /* Gutter ("lines") style: flat, left-aligned rows like a channel log. */
  :global([data-message-style='gutter']) .row.own {
    flex-direction: row;
  }
  :global([data-message-style='gutter']) .row.own .gutter {
    display: block;
  }
  :global([data-message-style='gutter']) .stack {
    max-width: 100%;
    flex: 1;
  }
  :global([data-message-style='gutter']) .bubble,
  :global([data-message-style='gutter']) .row.own .bubble {
    background: transparent;
    color: var(--alloy-color-text-primary);
    padding: 0;
    border-radius: 0;
  }
  :global([data-message-style='gutter']) .meta {
    justify-content: flex-start;
  }
  .divider {
    display: flex;
    justify-content: center;
    padding: var(--alloy-spacing-scale-3) 0;
  }
  .divider span,
  .system {
    background: var(--alloy-color-surface-subtle);
    color: var(--alloy-color-text-secondary);
    border-radius: var(--alloy-radii-full);
    padding: 2px 12px;
    font-size: var(--alloy-typography-size-sm);
  }
  .system {
    width: fit-content;
    margin: 6px auto;
  }
  .spacer {
    height: 1px;
  }
</style>
