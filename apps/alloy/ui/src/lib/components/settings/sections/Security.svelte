<script lang="ts">
  import { onMount } from 'svelte';
  import { Button } from '$lib/components/common';
  import { verification } from '$lib/stores/verification.svelte';

  interface Props {
    onclose: () => void;
  }
  let { onclose }: Props = $props();

  let password = $state('');
  let recoveryInput = $state('');
  let confirmedKey = $state(false);

  onMount(() => {
    void verification.refreshRecovery();
  });

  async function enable() {
    const ok = await verification.enableRecovery(password || null);
    if (ok) {
      password = '';
      confirmedKey = false;
    }
  }
  async function recover() {
    if (await verification.recoverWithKey(recoveryInput)) recoveryInput = '';
  }
</script>

<section class="section">
  <div class="block">
    <h3 class="subtitle">Verify this device</h3>
    <p class="muted">Confirm it is you by comparing emojis with another signed-in device.</p>
    <div>
      <Button
        onclick={() => {
          onclose();
          void verification.requestVerification();
        }}>Start verification</Button
      >
    </div>
  </div>

  <div class="block">
    <h3 class="subtitle">Recovery key</h3>
    {#if verification.recoveryKey}
      <p class="muted">
        Save this key somewhere safe. It is the only way to restore your encrypted messages, and it
        will not be shown again.
      </p>
      <code class="key">{verification.recoveryKey}</code>
      <label class="check">
        <input type="checkbox" bind:checked={confirmedKey} />
        I have saved my recovery key
      </label>
      <div>
        <Button disabled={!confirmedKey} onclick={() => verification.clearKey()}>Done</Button>
      </div>
    {:else if verification.recovery === 'Enabled'}
      <p class="muted">Recovery is set up. This device has your keys.</p>
    {:else if verification.recovery === 'Lost'}
      <p class="muted">Enter your recovery key to unlock your encrypted history on this device.</p>
      <form
        class="form"
        onsubmit={(e) => {
          e.preventDefault();
          void recover();
        }}
      >
        <input
          type="password"
          bind:value={recoveryInput}
          placeholder="Recovery key"
          aria-label="Recovery key"
          autocomplete="off"
        />
        <Button type="submit" disabled={!recoveryInput.trim() || verification.busy}>Unlock</Button>
      </form>
    {:else}
      <p class="muted">Set up a recovery key so you never lose your encrypted messages.</p>
      <form
        class="form"
        onsubmit={(e) => {
          e.preventDefault();
          void enable();
        }}
      >
        <input
          type="password"
          bind:value={password}
          placeholder="Account password"
          aria-label="Account password"
          autocomplete="current-password"
        />
        <Button type="submit" disabled={verification.busy}>Set up recovery</Button>
      </form>
    {/if}
    {#if verification.error}<p class="error" role="alert">{verification.error}</p>{/if}
  </div>
</section>

<style>
  .section {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-layout-section-gap);
  }
  .block {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-2);
  }
  .subtitle {
    margin: 0;
    font-size: var(--alloy-typography-size-lg);
    font-weight: var(--alloy-typography-weight-semibold);
    color: var(--alloy-color-text-primary);
  }
  .muted {
    margin: 0;
    color: var(--alloy-color-text-secondary);
    line-height: var(--alloy-typography-line-height-normal);
  }
  .form {
    display: flex;
    gap: var(--alloy-spacing-scale-2);
  }
  input[type='password'] {
    flex: 1;
    min-width: 0;
    font: inherit;
    color: var(--alloy-color-text-primary);
    background: var(--alloy-color-surface-subtle);
    border: 1px solid var(--alloy-color-border-subtle);
    border-radius: var(--alloy-radii-md);
    padding: var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-3);
  }
  input[type='password']:focus-visible,
  input[type='checkbox']:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }
  .key {
    display: block;
    padding: var(--alloy-spacing-scale-3);
    background: var(--alloy-color-surface-subtle);
    border-radius: var(--alloy-radii-md);
    font-family: var(--alloy-typography-font-family-mono);
    overflow-wrap: anywhere;
    user-select: all;
    color: var(--alloy-color-text-primary);
  }
  .check {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    color: var(--alloy-color-text-primary);
  }
  .error {
    margin: 0;
    color: var(--alloy-color-status-error);
  }
</style>
