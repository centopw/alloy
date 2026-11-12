<script lang="ts">
  import Icon from '$lib/icons/Icon.svelte';
  import { Copy, Forward, Trash2, X } from '$lib/icons';
  import { pushBack } from '$lib/platform/backstack';
  import { forwardStore } from '$lib/stores/forward.svelte';
  import { timelineStore } from '$lib/stores/timeline.svelte';
  import { quote } from './ReplyQuote.svelte';

  import { t } from '$lib/i18n';
  const items = $derived(timelineStore.selectedItems);
  const count = $derived(timelineStore.selectedIds.length);
  /** Deleting is only offered when the account may delete for everyone and the rows are sent. */
  const canDelete = $derived(
    timelineStore.capabilities?.delete_for_everyone === true &&
      items.length > 0 &&
      items.every((i) => !!i.event_id && i.send_state.status === 'Sent')
  );
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

<header class="bar" aria-label={t('selection-bar-selected-messages')}>
  <button type="button" class="tool" aria-label={t('selection-bar-cancel-selection')} data-tooltip={t('common-cancel')} onclick={() => timelineStore.endSelect()}>
    <Icon icon={X} size={20} />
  </button>
  <h2 class="count" aria-live="polite">{count} {t('selection-bar-selected')}</h2>
  <button type="button" class="tool" aria-label={t('selection-bar-copy-selected-messages')} data-tooltip={t('common-copy')} disabled={!copyable} onclick={copy}>
    <Icon icon={Copy} size={20} />
  </button>
  <button type="button" class="tool" aria-label={t('selection-bar-forward-selected-messages')} data-tooltip={t('selection-bar-forward')} onclick={forward}>
    <Icon icon={Forward} size={20} />
  </button>
  <button type="button" class="tool danger" aria-label={t('selection-bar-delete-selected-messages')} data-tooltip={t('common-delete')} disabled={!canDelete} onclick={remove}>
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
    border-bottom: 1px solid var(--alloy-glass-rim);
    background: transparent;
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
      background: var(--alloy-glass-control);
    }
  }
  @media (max-width: 899px) {
    .tool {
      width: 44px;
      height: 44px;
    }
  }
</style>
