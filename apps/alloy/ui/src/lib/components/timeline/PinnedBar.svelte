<script lang="ts">
  import { Modal } from '../common';
  import Icon from '$lib/icons/Icon.svelte';
  import { ChevronRight, Pin, PinOff } from '$lib/icons';
  import { pinsStore } from '$lib/stores/pins.svelte';
  import { timelineStore } from '$lib/stores/timeline.svelte';
  import { membersStore } from '$lib/stores/members.svelte';
  import { formatTime } from '$lib/content/format';
  import { prefs } from '$lib/stores/prefs.svelte';
  import { displayNameFor } from '$lib/platform/names';
  import { nextPinIndex, pinPreview } from '$lib/content/pins';
  import { quote } from './ReplyQuote.svelte';

  interface Props {
    roomId: string;
  }
  let { roomId }: Props = $props();

  const pins = $derived(pinsStore.itemsOf(roomId));
  const canPin = $derived(membersStore.detailsOf(roomId)?.can_pin ?? false);

  // The bar shows the newest pin first; each press jumps to it, then moves to the next older one.
  let index = $state(0);
  let listOpen = $state(false);
  $effect(() => {
    index = Math.max(0, pins.length - 1);
  });

  // Unpinning the last message leaves nothing to list.
  $effect(() => {
    if (pins.length === 0) listOpen = false;
  });

  $effect(() => {
    void pinsStore.load(roomId);
  });

  const current = $derived(pins[Math.min(index, pins.length - 1)] ?? null);

  async function jumpTo(eventId: string | null) {
    if (!eventId) return;
    await timelineStore.jumpToEvent(eventId);
  }

  async function openCurrent() {
    if (!current) return;
    await jumpTo(current.event_id);
    index = nextPinIndex(index, pins.length);
  }

  function senderName(userId: string, display: string | null | undefined) {
    const m = membersStore.membersOf(roomId).find((x) => x.user_id === userId);
    return displayNameFor(userId, display, m?.display_name ?? null);
  }
</script>

{#if current}
  <div class="bar" role="region" aria-label="Pinned messages">
    <button type="button" class="main" onclick={openCurrent} aria-label="Jump to pinned message {Math.min(index, pins.length - 1) + 1} of {pins.length}">
      <span class="stripe" aria-hidden="true"></span>
      <span class="text">
        <span class="label">{pins.length === 1 ? 'Pinned message' : `Pinned message ${Math.min(index, pins.length - 1) + 1} of ${pins.length}`}</span>
        <span class="preview">{pinPreview(quote(current))}</span>
      </span>
    </button>
    <button type="button" class="more" aria-label="All pinned messages" data-tooltip="All pinned messages" onclick={() => (listOpen = true)}>
      <Icon icon={Pin} size={20} />
    </button>
  </div>
{/if}

<Modal open={listOpen} title="Pinned messages" onclose={() => (listOpen = false)} width="460px">
  <ul class="list">
    {#each [...pins].reverse() as pin (pin.event_id)}
      <li class="entry">
        <button
          type="button"
          class="entry-main"
          onclick={() => {
            listOpen = false;
            void jumpTo(pin.event_id);
          }}
        >
          <span class="who">{senderName(pin.sender, pin.sender_display_name)}<span class="when">{formatTime(pin.timestamp_millis, prefs.timeFormat)}</span></span>
          <span class="preview">{pinPreview(quote(pin), 140)}</span>
          <Icon icon={ChevronRight} size={16} />
        </button>
        {#if canPin && pin.event_id}
          <button
            type="button"
            class="unpin"
            aria-label="Unpin message"
            data-tooltip="Unpin"
            onclick={() => pin.event_id && void pinsStore.setPinned(roomId, pin.event_id, false)}
          >
            <Icon icon={PinOff} size={16} />
          </button>
        {/if}
      </li>
    {:else}
      <li class="empty">No pinned messages.</li>
    {/each}
  </ul>
</Modal>

<style>
  .bar {
    display: flex;
    align-items: stretch;
    border-bottom: 1px solid var(--alloy-glass-rim);
    background: transparent;
  }
  .main {
    display: flex;
    flex: 1;
    align-items: stretch;
    gap: var(--alloy-spacing-scale-3);
    min-width: 0;
    min-height: 44px;
    padding: var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-4);
    border: 0;
    background: transparent;
    color: inherit;
    font: inherit;
    text-align: start;
    cursor: pointer;
  }
  @media (hover: hover) {
    .main:hover,
    .more:hover {
      background: var(--alloy-glass-control);
    }
  }
  .stripe {
    flex: none;
    width: 2px;
    border-radius: var(--alloy-radii-full);
    background: var(--alloy-color-accent-primary);
  }
  .text {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .label {
    color: var(--alloy-color-accent-primary);
    font-size: var(--alloy-typography-size-xs);
    font-weight: var(--alloy-typography-weight-semibold);
  }
  .preview {
    overflow: hidden;
    color: var(--alloy-color-text-secondary);
    font-size: var(--alloy-typography-size-sm);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .more {
    display: grid;
    flex: none;
    place-items: center;
    width: 44px;
    border: 0;
    background: transparent;
    color: var(--alloy-color-text-secondary);
    cursor: pointer;
  }
  .list {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .entry {
    display: flex;
    align-items: center;
    border-bottom: 1px solid var(--alloy-color-border-subtle);
  }
  .entry-main {
    display: grid;
    flex: 1;
    grid-template-columns: 1fr auto;
    gap: 2px var(--alloy-spacing-scale-2);
    min-width: 0;
    min-height: 44px;
    padding: var(--alloy-spacing-scale-2) 0;
    border: 0;
    background: transparent;
    color: inherit;
    font: inherit;
    text-align: start;
    cursor: pointer;
  }
  .entry-main .preview {
    grid-column: 1;
    white-space: normal;
  }
  .who {
    font-weight: var(--alloy-typography-weight-semibold);
  }
  .when {
    margin-left: var(--alloy-spacing-scale-2);
    color: var(--alloy-color-text-muted);
    font-size: var(--alloy-typography-size-xs);
    font-weight: var(--alloy-typography-weight-regular);
  }
  .entry-main :global(svg) {
    grid-column: 2;
    grid-row: 1 / span 2;
    align-self: center;
    color: var(--alloy-color-text-muted);
  }
  .unpin {
    display: grid;
    flex: none;
    place-items: center;
    width: 44px;
    height: 44px;
    border: 0;
    background: transparent;
    color: var(--alloy-color-text-secondary);
    cursor: pointer;
  }
  .empty {
    padding: var(--alloy-spacing-scale-4);
    color: var(--alloy-color-text-muted);
  }
</style>
