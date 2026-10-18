<script lang="ts">
  import { tick } from 'svelte';
  import Modal from './Modal.svelte';
  import Button from './Button.svelte';
  import { errorMessage } from '$lib/stores/errors';

  interface Props {
    open: boolean;
    title: string;
    message: string;
    confirmLabel: string;
    /** Destructive action: the confirm button uses the danger style and focus starts on Cancel. */
    danger?: boolean;
    /** Runs the action. The dialog closes when it resolves; a rejection is shown inline. */
    onconfirm: () => void | Promise<void>;
    onclose: () => void;
  }
  let { open, title, message, confirmLabel, danger = false, onconfirm, onclose }: Props = $props();

  let busy = $state(false);
  let error = $state<string | null>(null);
  let actions: HTMLDivElement | undefined = $state();

  $effect(() => {
    if (!open) return;
    error = null;
    busy = false;
    if (danger) void tick().then(() => actions?.querySelector<HTMLButtonElement>('button')?.focus());
  });

  async function confirm() {
    busy = true;
    error = null;
    try {
      await onconfirm();
      onclose();
    } catch (err) {
      error = errorMessage(err);
    } finally {
      busy = false;
    }
  }
</script>

<Modal {open} {title} {onclose} width="420px">
  <form
    onsubmit={(e) => {
      e.preventDefault();
      void confirm();
    }}
  >
    <p class="message">{message}</p>
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    <div class="actions" bind:this={actions}>
      <Button variant="ghost" onclick={onclose}>Cancel</Button>
      <Button type="submit" variant={danger ? 'danger' : 'primary'} disabled={busy}>
        {busy ? 'Working...' : confirmLabel}
      </Button>
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
  .message {
    margin: 0;
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-text-secondary);
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
</style>
