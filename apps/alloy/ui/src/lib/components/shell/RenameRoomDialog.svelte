<script lang="ts">
  import Modal from '../common/Modal.svelte';
  import Button from '../common/Button.svelte';
  import { roomInfo } from '$lib/stores/roomInfo.svelte';
  import { errorMessage } from '$lib/stores/errors';

  interface Props {
    roomId: string;
    open: boolean;
    onclose: () => void;
  }
  let { roomId, open, onclose }: Props = $props();

  let draft = $state('');
  let busy = $state(false);
  let thrown = $state<string | null>(null);
  // Plain (non-reactive) bookkeeping: whether the user typed, and whether the dialog was open last run.
  let dirty = false;
  let wasOpen = false;

  const currentName = $derived((roomInfo.details(roomId)?.name ?? '').trim());
  const next = $derived(draft.trim());
  const canSave = $derived(!busy && next.length > 0 && next !== currentName);
  const shownError = $derived(roomInfo.error(roomId) ?? thrown);

  // Prefill from the loaded room name each time the dialog opens, and keep following the name
  // while details are still loading, until the user starts typing.
  $effect(() => {
    const name = roomInfo.details(roomId)?.name ?? '';
    if (!open) {
      wasOpen = false;
      return;
    }
    if (!wasOpen) {
      wasOpen = true;
      dirty = false;
      busy = false;
      thrown = null;
    }
    if (!dirty) draft = name;
  });

  async function save(e: SubmitEvent) {
    e.preventDefault();
    if (!canSave) return;
    busy = true;
    thrown = null;
    try {
      await roomInfo.rename(roomId, next);
      if (!roomInfo.error(roomId)) onclose();
    } catch (err) {
      thrown = errorMessage(err);
    } finally {
      busy = false;
    }
  }
</script>

<Modal {open} title="Rename room" {onclose} width="420px">
  <form onsubmit={save}>
    <label class="field">
      <span>Room name</span>
      <input
        type="text"
        bind:value={draft}
        oninput={() => (dirty = true)}
        maxlength="255"
        autocomplete="off"
        disabled={busy}
      />
    </label>
    {#if shownError}<p class="error" role="alert">{shownError}</p>{/if}
    <div class="actions">
      <Button variant="ghost" onclick={onclose}>Cancel</Button>
      <Button type="submit" disabled={!canSave}>{busy ? 'Saving...' : 'Save'}</Button>
    </div>
  </form>
</Modal>

<style>
  form {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-layout-section-gap);
    margin-top: var(--alloy-layout-section-gap);
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-1);
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-text-secondary);
  }
  input {
    box-sizing: border-box;
    width: 100%;
    padding: var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-3);
    border: 1px solid var(--alloy-color-border-subtle);
    border-radius: var(--alloy-radii-md);
    background: var(--alloy-color-surface-elevated);
    color: var(--alloy-color-text-primary);
    font: inherit;
  }
  input:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 1px;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--alloy-spacing-scale-2);
  }
  .error {
    margin: 0;
    color: var(--alloy-color-status-error);
  }
  @media (max-width: 899px) {
    input {
      min-height: 44px;
      font-size: var(--alloy-typography-size-lg);
    }
    .actions {
      gap: var(--alloy-spacing-scale-2);
    }
    .actions :global(button) {
      flex: 1;
      min-height: 44px;
    }
  }
</style>
