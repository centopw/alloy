<script lang="ts">
  import Icon from '$lib/icons/Icon.svelte';
  import { Copy, Forward, Trash2, X } from '$lib/icons';
  import { pushBack } from '$lib/platform/backstack';
  import { forwardStore } from '$lib/stores/forward.svelte';
  import { membersStore } from '$lib/stores/members.svelte';
  import { timelineStore } from '$lib/stores/timeline.svelte';
  import { quote } from './ReplyQuote.svelte';

  const items = $derived(timelineStore.selectedItems);
  const count = $derived(timelineStore.selectedIds.length);
  const canRedactOthers = $derived(membersStore.detailsOf(timelineStore.roomId)?.can_redact_others ?? false);
  const canDelete = $derived(items.length > 0 && items.every((i) => i.is_own || canRedactOthers));
  const copyable = $derived(items.some((i) => quote(i) !== ''));

  // Android Back and Escape leave selection mode.
  $effect(() => pushBack(() => timelineStore.endSelect()));

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape') timelineStore.endSelect();
  }

  function copy() {
    const text = items.map((i) => quote(i)).filter((t) => t !== '').join('\n\n');
    void navigator.clipboard.writeText(text);
    timelineStore.endSelect();
  }

  function forward() {
    const roomId = timelineStore.roomId;
    if (!roomId) return;
    const ids = items.flatMap((i) => (i.event_id ? [i.event_id] : []));
    timelineStore.endSelect();
    forwardStore.begin(roomId, ids);
  }

  function remove() {
    timelineStore.pendingDelete = items;
    timelineStore.endSelect();
  }
</script>

<svelte:window onkeydown={onKey} />

<header class="bar" aria-label="Selected messages">
  <button type="button" class="tool" aria-label="Cancel selection" data-tooltip="Cancel" onclick={() => timelineStore.endSelect()}>
    <Icon icon={X} size={20} />
  </button>
  <h2 class="count" aria-live="polite">{count} selected</h2>
  <button type="button" class="tool" aria-label="Copy selected messages" data-tooltip="Copy" disabled={!copyable} onclick={copy}>
    <Icon icon={Copy} size={20} />
  </button>
  <button type="button" class="tool" aria-label="Forward selected messages" data-tooltip="Forward" onclick={forward}>
    <Icon icon={Forward} size={20} />
  </button>
  <button type="button" class="tool danger" aria-label="Delete selected messages" data-tooltip="Delete" disabled={!canDelete} onclick={remove}>
    <Icon icon={Trash2} size={20} />
  </button>
</header>

<style>
  .bar {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-1);
    min-height: 56px;
    padding: 0 var(--alloy-spacing-scale-3);
    border-bottom: 1px solid var(--alloy-color-border-subtle);
    background: var(--alloy-color-surface-panel);
  }
  .count {
    flex: 1;
    margin: 0;
    font-size: var(--alloy-typography-size-base);
    font-weight: var(--alloy-typography-weight-semibold);
  }
  .tool {
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    border: 0;
    border-radius: var(--alloy-radii-md);
    background: transparent;
    color: var(--alloy-color-text-secondary);
    cursor: pointer;
  }
  .tool:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .tool.danger:not(:disabled) {
    color: var(--alloy-color-status-error);
  }
  @media (hover: hover) {
    .tool:not(:disabled):hover {
      background: var(--alloy-color-surface-hover);
    }
  }
  @media (max-width: 899px) {
    .tool {
      width: 44px;
      height: 44px;
    }
  }
</style>
