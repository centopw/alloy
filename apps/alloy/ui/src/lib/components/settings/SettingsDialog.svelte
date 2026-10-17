<script lang="ts">
  import { Modal, Button, Toggle, SegmentedControl } from '../common';
  import { themeStore } from '$lib/theme/theme.svelte';
  import type { Density, MessageStyle } from '$lib/theme/theme.svelte';
  import { session } from '$lib/stores/session.svelte';
  import { verification } from '$lib/stores/verification.svelte';

  interface Props {
    open: boolean;
    onclose: () => void;
  }
  let { open, onclose }: Props = $props();

  type Tab = 'appearance' | 'security' | 'account';
  let tab = $state<Tab>('appearance');
  let password = $state('');
  let recoveryInput = $state('');
  let confirmedKey = $state(false);

  const dark = $derived(themeStore.activeTheme !== 'alloy-light');
  const amoled = $derived(themeStore.activeTheme === 'alloy-amoled');

  $effect(() => {
    if (open && tab === 'security') void verification.refreshRecovery();
  });

  function setDark(on: boolean) {
    themeStore.setTheme(on ? (amoled ? 'alloy-amoled' : 'alloy-dark') : 'alloy-light');
  }
  function setAmoled(on: boolean) {
    themeStore.setTheme(on ? 'alloy-amoled' : 'alloy-dark');
  }

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
  async function signOut() {
    onclose();
    await session.logout();
  }
</script>

<Modal {open} title="Settings" {onclose} width="560px">
  <div class="tabs" role="tablist">
    {#each [['appearance', 'Appearance'], ['security', 'Security'], ['account', 'Account']] as [id, label]}
      <button type="button" role="tab" aria-selected={tab === id} class:on={tab === id} onclick={() => (tab = id as Tab)}>{label}</button>
    {/each}
  </div>

  {#if tab === 'appearance'}
    <div class="row"><span>Follow system appearance</span><Toggle label="Follow system appearance" checked={themeStore.followSystem} onchange={(v) => themeStore.setFollowSystem(v)} /></div>
    <div class="row"><span>Dark mode</span><Toggle label="Dark mode" checked={dark} disabled={themeStore.followSystem} onchange={setDark} /></div>
    <div class="row"><span>Pure black background</span><Toggle label="Pure black background" checked={amoled} disabled={!dark || themeStore.followSystem} onchange={setAmoled} /></div>
    <div class="row col">
      <span>Density</span>
      <SegmentedControl
        label="Density"
        value={themeStore.density}
        options={[{ value: 'compact', label: 'Compact' }, { value: 'normal', label: 'Normal' }, { value: 'cozy', label: 'Cozy' }]}
        onchange={(v) => themeStore.setDensity(v as Density)}
      />
    </div>
    <div class="row col">
      <span>Message style</span>
      <SegmentedControl
        label="Message style"
        value={themeStore.messageStyle}
        options={[{ value: 'bubble', label: 'Bubbles' }, { value: 'gutter', label: 'Lines' }]}
        onchange={(v) => themeStore.setMessageStyle(v as MessageStyle)}
      />
    </div>
  {:else if tab === 'security'}
    <section>
      <h3>Verify this device</h3>
      <p>Confirm it is you by comparing emojis with another signed-in device.</p>
      <Button onclick={() => { onclose(); void verification.requestVerification(); }}>Start verification</Button>
    </section>
    <section>
      <h3>Recovery key</h3>
      {#if verification.recoveryKey}
        <p>Save this key somewhere safe. It is the only way to restore your encrypted messages, and it will not be shown again.</p>
        <code class="key">{verification.recoveryKey}</code>
        <label class="check"><input type="checkbox" bind:checked={confirmedKey} /> I have saved my recovery key</label>
        <Button disabled={!confirmedKey} onclick={() => verification.clearKey()}>Done</Button>
      {:else if verification.recovery === 'Enabled'}
        <p>Recovery is set up. This device has your keys.</p>
      {:else if verification.recovery === 'Lost'}
        <p>Enter your recovery key to unlock your encrypted history on this device.</p>
        <form onsubmit={(e) => { e.preventDefault(); void recover(); }}>
          <input type="password" bind:value={recoveryInput} placeholder="Recovery key" aria-label="Recovery key" autocomplete="off" />
          <Button type="submit" disabled={!recoveryInput.trim() || verification.busy}>Unlock</Button>
        </form>
      {:else}
        <p>Set up a recovery key so you never lose your encrypted messages.</p>
        <form onsubmit={(e) => { e.preventDefault(); void enable(); }}>
          <input type="password" bind:value={password} placeholder="Account password" aria-label="Account password" autocomplete="current-password" />
          <Button type="submit" disabled={verification.busy}>Set up recovery</Button>
        </form>
      {/if}
      {#if verification.error}<p class="error" role="alert">{verification.error}</p>{/if}
    </section>
  {:else}
    <section>
      <h3>{session.account?.display_name ?? session.account?.user_id}</h3>
      <p>{session.account?.user_id}<br />{session.account?.homeserver_url}</p>
      <Button variant="danger" onclick={signOut}>Sign out</Button>
    </section>
  {/if}
</Modal>

<style>
  .tabs {
    display: flex;
    gap: var(--alloy-spacing-scale-2);
    margin-bottom: var(--alloy-spacing-scale-4);
    border-bottom: 1px solid var(--alloy-color-border-subtle);
  }
  .tabs button {
    border: 0;
    background: transparent;
    color: var(--alloy-color-text-secondary);
    font: inherit;
    padding: var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-3);
    cursor: pointer;
    border-bottom: 2px solid transparent;
  }
  .tabs button.on {
    color: var(--alloy-color-text-primary);
    border-bottom-color: var(--alloy-color-accent-primary);
  }
  .row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: var(--alloy-spacing-scale-3) 0;
    border-bottom: 1px solid var(--alloy-color-border-subtle);
  }
  .row.col {
    flex-direction: column;
    align-items: flex-start;
    gap: var(--alloy-spacing-scale-2);
  }
  section {
    margin-bottom: var(--alloy-spacing-scale-6);
  }
  h3 {
    margin: 0 0 var(--alloy-spacing-scale-2);
  }
  p {
    color: var(--alloy-color-text-secondary);
    margin-top: 0;
  }
  form {
    display: flex;
    gap: var(--alloy-spacing-scale-2);
  }
  input[type='password'] {
    flex: 1;
    font: inherit;
    color: var(--alloy-color-text-primary);
    background: var(--alloy-color-surface-subtle);
    border: 1px solid var(--alloy-color-border-subtle);
    border-radius: var(--alloy-radii-md);
    padding: 8px 12px;
  }
  .key {
    display: block;
    padding: var(--alloy-spacing-scale-3);
    margin-bottom: var(--alloy-spacing-scale-3);
    background: var(--alloy-color-surface-subtle);
    border-radius: var(--alloy-radii-md);
    font-family: var(--alloy-typography-font-family-mono);
    word-break: break-word;
    user-select: all;
  }
  .check {
    display: flex;
    gap: var(--alloy-spacing-scale-2);
    margin-bottom: var(--alloy-spacing-scale-3);
  }
  .error {
    color: var(--alloy-color-status-error);
  }
</style>
