<script lang="ts">
  import { Modal, Button } from '../common';
  import { verification } from '$lib/stores/verification.svelte';

  const view = $derived(verification.view);
  const open = $derived(view.step !== 'idle');
</script>

<Modal {open} title="Verify this device" onclose={() => (view.step === 'compare' ? verification.cancel() : verification.dismiss())} width="460px">
  {#if view.step === 'incoming'}
    <p>Another device of yours wants to verify. Only continue if you started it.</p>
    <div class="actions">
      <Button variant="ghost" onclick={() => verification.cancel()}>Decline</Button>
      <Button onclick={() => verification.accept()}>Continue</Button>
    </div>
  {:else if view.step === 'waiting'}
    <p aria-busy="true">Waiting for your other device. Open Alloy there and accept the request.</p>
    <div class="actions"><Button variant="ghost" onclick={() => verification.cancel()}>Cancel</Button></div>
  {:else if view.step === 'compare'}
    <p>Check that the same emojis appear, in the same order, on your other device.</p>
    <ul class="emojis" aria-label="Verification emojis">
      {#each view.emojis as e}
        <li><span class="glyph" aria-hidden="true">{e.emoji}</span><span>{e.description}</span></li>
      {/each}
    </ul>
    <div class="actions">
      <Button variant="danger" onclick={() => verification.cancel()}>They don't match</Button>
      <Button disabled={verification.busy} onclick={() => verification.confirm()}>They match</Button>
    </div>
  {:else if view.step === 'done'}
    <p>This device is verified. Your encrypted history is now available here.</p>
    <div class="actions"><Button onclick={() => verification.dismiss()}>Done</Button></div>
  {:else if view.step === 'cancelled'}
    <p>Verification stopped: {view.reason}</p>
    <div class="actions"><Button onclick={() => verification.dismiss()}>Close</Button></div>
  {/if}
  {#if verification.error}<p class="error" role="alert">{verification.error}</p>{/if}
</Modal>

<style>
  p {
    margin-top: 0;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--alloy-spacing-scale-2);
    margin-top: var(--alloy-spacing-scale-4);
  }
  .emojis {
    list-style: none;
    padding: 0;
    margin: var(--alloy-spacing-scale-4) 0 0;
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: var(--alloy-spacing-scale-3);
    text-align: center;
  }
  .glyph {
    display: block;
    font-size: 2rem;
  }
  .error {
    color: var(--alloy-color-status-error);
  }
</style>
