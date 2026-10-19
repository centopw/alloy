<script lang="ts">
  import { Button, Modal } from '../common';
  import Icon from '$lib/icons/Icon.svelte';
  import { MapPin } from '$lib/icons';
  import { client } from '$lib/ipc';
  import { formatAccuracy, formatCoordinates } from '$lib/content/geo';
  import { currentFix, type Fix } from '$lib/platform/geolocation';
  import { errorMessage, expect } from '$lib/stores/errors';

  interface Props {
    open: boolean;
    roomId: string;
    onclose: () => void;
  }
  let { open, roomId, onclose }: Props = $props();

  let fix = $state<Fix | null>(null);
  let error = $state<string | null>(null);
  let loading = $state(false);
  let sending = $state(false);

  async function locate() {
    loading = true;
    error = null;
    fix = null;
    try {
      fix = await currentFix();
    } catch (e) {
      error = errorMessage(e);
    } finally {
      loading = false;
    }
  }

  // Asking for the position only happens once the dialog is open, never in the background.
  $effect(() => {
    if (open) void locate();
  });

  async function send() {
    if (!fix || sending) return;
    sending = true;
    error = null;
    try {
      expect(await client.sendLocation(roomId, fix.latitude, fix.longitude, { accuracy_m: fix.accuracy }), 'LocationSent');
      onclose();
    } catch (e) {
      error = errorMessage(e);
    } finally {
      sending = false;
    }
  }
</script>

<Modal {open} title="Send your location" {onclose} width="400px">
  <div class="body">
    <span class="pin" aria-hidden="true"><Icon icon={MapPin} size={24} /></span>
    {#if loading}
      <p role="status">Finding your position</p>
    {:else if fix}
      <p class="coords">{formatCoordinates(fix)}</p>
      <p class="muted">Accuracy {formatAccuracy(fix.accuracy)}. Everyone in this room will see where you are.</p>
    {/if}
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    <div class="actions">
      <Button variant="ghost" onclick={onclose}>Cancel</Button>
      {#if error && !fix}
        <Button variant="ghost" onclick={() => void locate()}>Try again</Button>
      {/if}
      <Button disabled={!fix || sending} onclick={() => void send()}>{sending ? 'Sending' : 'Send location'}</Button>
    </div>
  </div>
</Modal>

<style>
  .body {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-3);
  }
  .body p {
    margin: 0;
  }
  .pin {
    display: grid;
    place-items: center;
    align-self: flex-start;
    width: 48px;
    height: 48px;
    border-radius: var(--alloy-radii-full);
    background: var(--alloy-color-accent-subtle);
    color: var(--alloy-color-accent-primary);
  }
  .coords {
    font-size: var(--alloy-typography-size-lg);
    font-variant-numeric: tabular-nums;
  }
  .muted {
    color: var(--alloy-color-text-secondary);
    font-size: var(--alloy-typography-size-sm);
  }
  .error {
    color: var(--alloy-color-status-error);
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: var(--alloy-spacing-scale-2);
  }
</style>
