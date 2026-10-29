<script lang="ts">
  import { tick } from 'svelte';
  import { Modal, Button } from '../common';
  import Icon from '$lib/icons/Icon.svelte';
  import { X } from '$lib/icons';
  import { pushBack } from '$lib/platform/backstack';
  import { viewport } from '$lib/stores/viewport.svelte';
  import { verification } from '$lib/stores/verification.svelte';

  const view = $derived(verification.view);
  const open = $derived(view.step !== 'idle');
  const sheetOpen = $derived(open && viewport.isPhone);

  function close() {
    if (view.step === 'compare') verification.cancel();
    else verification.dismiss();
  }

  let sheet: HTMLDivElement | undefined = $state();

  // Phones show a bottom sheet instead of the centred Modal: Android Back and Escape dismiss it,
  // and focus moves into it on open.
  $effect(() => {
    if (!sheetOpen) return;
    return pushBack(() => close());
  });

  $effect(() => {
    if (!sheetOpen) return;
    function onKey(e: KeyboardEvent) {
      if (e.key === 'Escape') {
        e.preventDefault();
        e.stopPropagation();
        close();
      }
    }
    window.addEventListener('keydown', onKey, true);
    void tick().then(() => sheet?.focus());
    return () => window.removeEventListener('keydown', onKey, true);
  });
</script>

{#snippet body()}
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
{/snippet}

{#if sheetOpen}
  <!-- Backdrop tap dismisses; the close button and Escape do the same for keyboard users. -->
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="sheet-backdrop"
    onclick={(e) => {
      if (e.target === e.currentTarget) close();
    }}
  >
    <div class="sheet" role="dialog" aria-modal="true" aria-labelledby="verify-title" tabindex="-1" bind:this={sheet}>
      <span class="handle" aria-hidden="true"></span>
      <header>
        <h2 id="verify-title">Verify this device</h2>
        <button type="button" aria-label="Close verification" onclick={close}>
          <Icon icon={X} size={20} />
        </button>
      </header>
      <div class="content">
        {@render body()}
      </div>
    </div>
  </div>
{:else}
  <Modal {open} title="Verify this device" onclose={close} width="460px">
    {@render body()}
  </Modal>
{/if}

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

  /* Phone bottom sheet. */
  .sheet-backdrop {
    position: fixed;
    inset: 0;
    z-index: 60;
    display: flex;
    align-items: flex-end;
    background: color-mix(in srgb, var(--alloy-color-surface-canvas) 70%, transparent);
  }
  .sheet {
    display: flex;
    flex-direction: column;
    width: 100%;
    max-height: 90dvh;
    box-sizing: border-box;
    background: var(--alloy-color-surface-elevated);
    color: var(--alloy-color-text-primary);
    border-radius: var(--alloy-glass-radius-panel) var(--alloy-glass-radius-panel) 0 0;
    box-shadow: var(--alloy-elevation-lg);
    padding-bottom: var(--alloy-safe-area-bottom);
    outline: 0;
  }
  .handle {
    align-self: center;
    width: 36px;
    height: 4px;
    margin-top: var(--alloy-spacing-scale-2);
    border-radius: var(--alloy-radii-full);
    background: var(--alloy-color-border-prominent);
  }
  .sheet header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    min-height: 56px;
    padding: 0 var(--alloy-spacing-scale-2) 0 var(--alloy-layout-gutter);
    border-bottom: 1px solid var(--alloy-color-border-subtle);
  }
  .sheet h2 {
    margin: 0;
    font-size: var(--alloy-typography-size-lg);
    font-weight: var(--alloy-typography-weight-semibold);
  }
  .sheet header button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 44px;
    height: 44px;
    padding: 0;
    border: 0;
    border-radius: var(--alloy-radii-full);
    background: transparent;
    color: var(--alloy-color-text-secondary);
    cursor: pointer;
  }
  .content {
    min-height: 0;
    overflow-y: auto;
    overscroll-behavior: contain;
    -webkit-overflow-scrolling: touch;
    padding: var(--alloy-spacing-scale-4) var(--alloy-layout-gutter) var(--alloy-spacing-scale-4);
    font-size: var(--alloy-typography-size-lg);
  }
  .sheet .actions {
    flex-direction: column;
  }
  .sheet .actions :global(button) {
    width: 100%;
    min-height: 48px;
  }
</style>
