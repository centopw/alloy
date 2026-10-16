<script lang="ts">
  import { onMount, tick } from "svelte";
  import { VList } from "virtua/svelte";
  import type { VListHandle } from "virtua/svelte";
  import type { TimelineEvent } from "../mock/types.js";
  import { generateSyntheticTimeline } from "../mock/generator.js";

  let vlist: VListHandle | undefined = $state();
  let events: TimelineEvent[] = $state([]);
  let shift = $state(false);
  let isAtBottom = $state(true);
  let showScrollBottomBtn = $state(false);
  let unreadCount = $state(0);
  let firstUnreadIndex = $state(-1);

  // Composer state
  let composerText = $state("");
  let isSending = $state(false);

  // Initialize 10,000 events
  onMount(() => {
    // 10,000 events with unread messages starting near the end (index 9850)
    const initial = generateSyntheticTimeline(10000, 9850);
    events = initial;

    // Calculate unread
    firstUnreadIndex = initial.findIndex(e => e.isUnread);
    unreadCount = firstUnreadIndex >= 0 ? initial.length - firstUnreadIndex : 0;

    // Initial pin to bottom after DOM mounts
    tick().then(() => {
      setTimeout(() => {
        if (vlist) {
          vlist.scrollToIndex(events.length - 1, { align: "end" });
          isAtBottom = true;
        }
      }, 50);
    });
  });

  function handleScroll(offset: number) {
    if (!vlist) return;
    const scrollSize = vlist.getScrollSize();
    const viewportSize = vlist.getViewportSize();
    
    // Bottom threshold is 80px
    const distanceFromBottom = scrollSize - (offset + viewportSize);
    isAtBottom = distanceFromBottom < 80;
    showScrollBottomBtn = distanceFromBottom > 200;
  }

  export function scrollToBottom(smooth = true) {
    if (!vlist || events.length === 0) return;
    vlist.scrollToIndex(events.length - 1, { align: "end", smooth });
    isAtBottom = true;
    showScrollBottomBtn = false;
  }

  export function scrollToUnread() {
    if (!vlist || firstUnreadIndex < 0) return;
    vlist.scrollToIndex(firstUnreadIndex, { align: "start", smooth: true });
  }

  export function prependHistory(batchSize = 50) {
    if (events.length === 0) return;
    shift = true;
    const firstTs = events[0].timestamp;
    const prepended: TimelineEvent[] = [];

    for (let i = batchSize; i > 0; i--) {
      const ts = firstTs - i * 120_000;
      prepended.push({
        id: `history-prepend-${Date.now()}-${i}`,
        sender: i % 2 === 0 ? "@alice:matrix.org" : "@bob:matrix.org",
        timestamp: ts,
        kind: "text",
        body: `[Archived message prepended] #${i} History anchor test at timestamp ${new Date(ts).toLocaleTimeString()}`,
      });
    }

    // Insert at front
    events = [...prepended, ...events];
    if (firstUnreadIndex >= 0) {
      firstUnreadIndex += batchSize;
    }

    // Reset shift on next tick once layout recalculates
    tick().then(() => {
      requestAnimationFrame(() => {
        shift = false;
      });
    });
  }

  export function appendMessage(customText?: string) {
    const text = customText?.trim() || composerText.trim() || `Synthetic real-time message #${events.length + 1}`;
    const newMsg: TimelineEvent = {
      id: `live-event-${Date.now()}`,
      sender: "@you:matrix.org",
      senderAvatar: "Y",
      timestamp: Date.now(),
      kind: "text",
      body: text,
      isUnread: false,
    };

    const wasAtBottom = isAtBottom;
    events = [...events, newMsg];
    composerText = "";

    tick().then(() => {
      if (wasAtBottom && vlist) {
        vlist.scrollToIndex(events.length - 1, { align: "end", smooth: true });
      }
    });
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      appendMessage();
    }
  }

  function formatTime(timestamp: number): string {
    const d = new Date(timestamp);
    return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  }
</script>

<div class="timeline-container">
  <!-- Top bar with actions / info -->
  <div class="timeline-header">
    <div class="header-info">
      <span class="room-title">#general:alloy.chat</span>
      <span class="room-stats">{events.length.toLocaleString()} events loaded</span>
    </div>
    <div class="header-controls">
      <button class="btn btn-secondary" onclick={() => prependHistory(50)} title="Test scroll anchoring by prepending 50 older messages">
        +50 Older (Anchor Test)
      </button>
      {#if unreadCount > 0}
        <button class="btn btn-unread" onclick={scrollToUnread} title="Jump to first unread message">
          Jump to Unread ({unreadCount})
        </button>
      {/if}
      <button class="btn btn-secondary" onclick={() => scrollToBottom(true)} title="Jump to bottom">
        ↓ Bottom
      </button>
    </div>
  </div>

  <!-- Virtualized list viewport -->
  <div class="timeline-viewport">
    {#if events.length > 0}
      <VList
        bind:this={vlist}
        data={events}
        getKey={(e) => e.id}
        {shift}
        bufferSize={300}
        onscroll={handleScroll}
        style="height: 100%; width: 100%;"
      >
        {#snippet children(event, index)}
          {#if event.kind === "day-divider"}
            <div class="day-divider" data-event-id={event.id}>
              <div class="divider-line"></div>
              <span class="divider-text">{event.body}</span>
              <div class="divider-line"></div>
            </div>
          {:else}
            <div
              class="event-row"
              class:unread-border={event.isUnread}
              data-event-id={event.id}
              data-index={index}
            >
              {#if index === firstUnreadIndex}
                <div class="unread-banner">
                  <span>NEW MESSAGES BELOW</span>
                </div>
              {/if}

              <div class="avatar">
                {event.senderAvatar || event.sender.slice(1, 2).toUpperCase()}
              </div>

              <div class="event-content">
                <div class="event-meta">
                  <span class="sender-name">{event.sender}</span>
                  <span class="timestamp">{formatTime(event.timestamp)}</span>
                </div>

                {#if event.kind === "reply" && event.replyTo}
                  <div class="reply-quote">
                    <span class="reply-sender">{event.replyTo.sender}</span>
                    <span class="reply-body">{event.replyTo.snippet}</span>
                  </div>
                {/if}

                {#if event.kind === "code"}
                  <div class="code-wrapper">
                    <div class="code-header">{event.codeLang || "code"}</div>
                    <pre class="code-block"><code>{event.body}</code></pre>
                  </div>
                {:else if event.kind === "image"}
                  <div class="image-wrapper">
                    <p class="image-caption">{event.body}</p>
                    {#if event.imageUrl}
                      <img
                        src={event.imageUrl}
                        alt={event.body}
                        loading="lazy"
                        class="timeline-image"
                        style="aspect-ratio: {event.imageAspectRatio || 16/9};"
                      />
                    {/if}
                  </div>
                {:else}
                  <div class="text-body">
                    {event.body}
                  </div>
                {/if}
              </div>
            </div>
          {/if}
        {/snippet}
      </VList>
    {/if}

    {#if showScrollBottomBtn}
      <button class="floating-bottom-btn" onclick={() => scrollToBottom(true)}>
        <span>↓</span>
      </button>
    {/if}
  </div>

  <!-- Message Composer -->
  <div class="composer-container">
    <div class="composer-box">
      <input
        type="text"
        placeholder="Send a message in #general (press Enter)..."
        bind:value={composerText}
        onkeydown={handleKeydown}
        class="composer-input"
      />
      <button
        class="composer-send-btn"
        disabled={!composerText.trim()}
        onclick={() => appendMessage()}
      >
        Send
      </button>
    </div>
  </div>
</div>

<style>
  .timeline-container {
    display: flex;
    flex-direction: column;
    height: 100%;
    width: 100%;
    background: var(--alloy-color-surface-canvas);
    overflow: hidden;
    position: relative;
  }

  .timeline-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-4);
    background: var(--alloy-color-surface-subtle);
    border-bottom: 1px solid var(--alloy-color-border-subtle);
    min-height: 48px;
    flex-shrink: 0;
  }

  .header-info {
    display: flex;
    align-items: baseline;
    gap: var(--alloy-spacing-scale-2);
  }

  .room-title {
    font-weight: 700;
    font-size: 14px;
    color: var(--alloy-color-text-primary);
  }

  .room-stats {
    font-size: 11px;
    color: var(--alloy-color-text-muted);
  }

  .header-controls {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
  }

  .btn {
    border: none;
    border-radius: var(--alloy-radii-sm);
    padding: var(--alloy-spacing-scale-1) var(--alloy-spacing-scale-2);
    font-size: 11px;
    font-weight: 500;
    cursor: pointer;
    transition: opacity 0.15s ease, background 0.15s ease;
  }

  .btn-secondary {
    background: var(--alloy-color-surface-elevated);
    color: var(--alloy-color-text-secondary);
    border: 1px solid var(--alloy-color-border-subtle);
  }

  .btn-secondary:hover {
    background: var(--alloy-color-surface-hover);
    color: var(--alloy-color-text-primary);
  }

  .btn-unread {
    background: var(--alloy-color-status-mention);
    color: var(--alloy-color-text-on-accent);
  }

  .btn-unread:hover {
    opacity: 0.9;
  }

  .timeline-viewport {
    flex: 1;
    min-height: 0;
    position: relative;
    overflow: hidden;
  }

  .day-divider {
    display: flex;
    align-items: center;
    margin: var(--alloy-spacing-scale-3) var(--alloy-spacing-scale-4);
    gap: var(--alloy-spacing-scale-2);
  }

  .divider-line {
    flex: 1;
    height: 1px;
    background: var(--alloy-color-border-subtle);
  }

  .divider-text {
    font-size: 11px;
    font-weight: 600;
    color: var(--alloy-color-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .event-row {
    display: flex;
    gap: var(--alloy-spacing-scale-3);
    padding: var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-4);
    transition: background-color 0.1s ease;
    position: relative;
  }

  .event-row:hover {
    background: var(--alloy-color-surface-subtle);
  }

  .event-row.unread-border {
    border-left: 3px solid var(--alloy-color-accent-primary);
  }

  .unread-banner {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    height: 2px;
    background: var(--alloy-color-status-mention);
    display: flex;
    justify-content: flex-end;
  }

  .unread-banner span {
    font-size: 9px;
    font-weight: 700;
    color: var(--alloy-color-status-mention);
    background: var(--alloy-color-surface-canvas);
    padding: 0 4px;
    transform: translateY(-8px);
  }

  .avatar {
    width: 36px;
    height: 36px;
    border-radius: var(--alloy-radii-full);
    background: var(--alloy-color-accent-primary);
    color: var(--alloy-color-text-on-accent);
    display: flex;
    align-items: center;
    justify-content: center;
    font-weight: 700;
    font-size: 14px;
    flex-shrink: 0;
  }

  .event-content {
    flex: 1;
    min-width: 0;
  }

  .event-meta {
    display: flex;
    align-items: baseline;
    gap: var(--alloy-spacing-scale-2);
    margin-bottom: var(--alloy-spacing-scale-1);
  }

  .sender-name {
    font-weight: 600;
    font-size: 13px;
    color: var(--alloy-color-text-primary);
  }

  .timestamp {
    font-size: 10px;
    color: var(--alloy-color-text-muted);
  }

  .text-body {
    font-size: 13px;
    line-height: 1.5;
    color: var(--alloy-color-text-primary);
    word-break: break-word;
    white-space: pre-wrap;
  }

  .reply-quote {
    background: var(--alloy-color-surface-elevated);
    border-left: 3px solid var(--alloy-color-accent-primary);
    border-radius: 0 var(--alloy-radii-sm) var(--alloy-radii-sm) 0;
    padding: var(--alloy-spacing-scale-1) var(--alloy-spacing-scale-2);
    margin-bottom: var(--alloy-spacing-scale-1);
    font-size: 11px;
    display: flex;
    flex-direction: column;
  }

  .reply-sender {
    font-weight: 600;
    color: var(--alloy-color-accent-primary);
  }

  .reply-body {
    color: var(--alloy-color-text-secondary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .code-wrapper {
    background: var(--alloy-color-surface-subtle);
    border: 1px solid var(--alloy-color-border-subtle);
    border-radius: var(--alloy-radii-md);
    margin-top: var(--alloy-spacing-scale-1);
    overflow: hidden;
  }

  .code-header {
    background: var(--alloy-color-surface-elevated);
    padding: 2px var(--alloy-spacing-scale-2);
    font-size: 10px;
    color: var(--alloy-color-text-muted);
    font-family: var(--alloy-typography-font-family-mono);
    text-transform: uppercase;
  }

  .code-block {
    margin: 0;
    padding: var(--alloy-spacing-scale-2);
    font-family: var(--alloy-typography-font-family-mono);
    font-size: 12px;
    color: var(--alloy-color-text-primary);
    overflow-x: auto;
    line-height: 1.4;
  }

  .image-wrapper {
    margin-top: var(--alloy-spacing-scale-1);
  }

  .image-caption {
    margin: 0 0 var(--alloy-spacing-scale-1) 0;
    font-size: 12px;
    color: var(--alloy-color-text-secondary);
  }

  .timeline-image {
    max-width: 480px;
    width: 100%;
    height: auto;
    border-radius: var(--alloy-radii-md);
    border: 1px solid var(--alloy-color-border-subtle);
    display: block;
  }

  .floating-bottom-btn {
    position: absolute;
    bottom: var(--alloy-spacing-scale-3);
    right: var(--alloy-spacing-scale-4);
    width: 36px;
    height: 36px;
    border-radius: var(--alloy-radii-full);
    background: var(--alloy-color-surface-elevated);
    border: 1px solid var(--alloy-color-border-prominent);
    color: var(--alloy-color-text-primary);
    font-size: 16px;
    display: flex;
    align-items: center;
    justify-content: center;
    box-shadow: var(--alloy-elevation-md);
    cursor: pointer;
    z-index: 10;
    transition: transform 0.15s ease;
  }

  .floating-bottom-btn:hover {
    transform: scale(1.05);
    background: var(--alloy-color-surface-hover);
  }

  .composer-container {
    padding: var(--alloy-spacing-scale-3) var(--alloy-spacing-scale-4);
    background: var(--alloy-color-surface-subtle);
    border-top: 1px solid var(--alloy-color-border-subtle);
    flex-shrink: 0;
  }

  .composer-box {
    display: flex;
    gap: var(--alloy-spacing-scale-2);
    background: var(--alloy-color-surface-panel);
    border: 1px solid var(--alloy-color-border-prominent);
    border-radius: var(--alloy-radii-md);
    padding: var(--alloy-spacing-scale-1) var(--alloy-spacing-scale-2);
  }

  .composer-box:focus-within {
    border-color: var(--alloy-color-accent-primary);
  }

  .composer-input {
    flex: 1;
    border: none;
    background: transparent;
    color: var(--alloy-color-text-primary);
    font-size: 13px;
    padding: var(--alloy-spacing-scale-2);
    outline: none;
  }

  .composer-input::placeholder {
    color: var(--alloy-color-text-muted);
  }

  .composer-send-btn {
    background: var(--alloy-color-accent-primary);
    color: var(--alloy-color-text-on-accent);
    border: none;
    border-radius: var(--alloy-radii-sm);
    padding: 0 var(--alloy-spacing-scale-3);
    font-weight: 600;
    font-size: 12px;
    cursor: pointer;
    transition: opacity 0.15s ease;
  }

  .composer-send-btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .composer-send-btn:not(:disabled):hover {
    background: var(--alloy-color-accent-hover);
  }
</style>
