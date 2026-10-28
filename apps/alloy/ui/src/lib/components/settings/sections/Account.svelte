<script lang="ts">
  import { onMount } from 'svelte';
  import { Avatar, Button, TextField } from '$lib/components/common';
  import { SettingButtonRow, SettingGroup } from '$lib/components/settings/controls';
  import { client } from '$lib/ipc';
  import { pickFile } from '$lib/platform/files';
  import { errorMessage, expect } from '$lib/stores/errors';
  import { session } from '$lib/stores/session.svelte';
  import { Copy, ImagePlus, Trash2 } from '$lib/icons';

  interface Props {
    onclose: () => void;
  }
  let { onclose }: Props = $props();

  let draftName = $state('');
  let savingName = $state(false);
  let savingAvatar = $state(false);
  let signingOut = $state(false);
  let copied = $state(false);
  let error = $state<string | null>(null);

  const trimmedName = $derived(draftName.trim());
  const nameChanged = $derived(
    trimmedName.length > 0 && trimmedName !== (session.account?.display_name ?? '').trim()
  );
  const busy = $derived(savingName || savingAvatar);

  onMount(() => {
    void loadProfile();
  });

  async function loadProfile() {
    try {
      const profile = expect(await client.invoke({ action: 'GetOwnProfile' }), 'OwnProfile')
        .payload.profile;
      draftName = profile.display_name ?? '';
    } catch (e) {
      error = errorMessage(e);
    }
  }

  async function saveName(e: SubmitEvent) {
    e.preventDefault();
    if (!nameChanged || busy) return;
    savingName = true;
    error = null;
    try {
      expect(
        await client.invoke({ action: 'SetDisplayName', params: { name: trimmedName } }),
        'Unit'
      );
      if (session.account) session.account = { ...session.account, display_name: trimmedName };
    } catch (err) {
      error = errorMessage(err);
    } finally {
      savingName = false;
    }
  }

  async function setAvatar(path: string | null) {
    savingAvatar = true;
    error = null;
    try {
      expect(
        await client.invoke({ action: 'SetOwnAvatar', params: { file_path: path } }),
        'Unit'
      );
      const profile = expect(await client.invoke({ action: 'GetOwnProfile' }), 'OwnProfile')
        .payload.profile;
      if (session.account)
        session.account = { ...session.account, avatar_url: profile.avatar_url };
    } catch (err) {
      error = errorMessage(err);
    } finally {
      savingAvatar = false;
    }
  }

  async function changeAvatar() {
    const path = await pickFile();
    if (path) await setAvatar(path);
  }

  async function copyUserId() {
    if (!session.account) return;
    try {
      await navigator.clipboard.writeText(session.account.user_id);
      copied = true;
    } catch (err) {
      error = errorMessage(err);
    }
  }

  async function signOut() {
    signingOut = true;
    // Close first so the dialog never renders the signed-out state.
    onclose();
    await session.logout();
    signingOut = false;
  }
</script>

<section class="section">
  {#if session.account}
    <div class="profile">
      <Avatar
        userId={session.account.user_id}
        displayName={session.account.display_name ?? session.account.user_id}
        src={session.account.avatar_url}
        size="xl"
      />
      <div class="identity">
        <p class="name">{session.account.display_name ?? session.account.user_id}</p>
        <p class="meta">{session.account.user_id}</p>
      </div>
    </div>

    <div class="actions">
      <Button
        variant="ghost"
        size="sm"
        icon={ImagePlus}
        disabled={busy}
        onclick={() => void changeAvatar()}>Change photo</Button
      >
      {#if session.account.avatar_url}
        <Button
          variant="ghost"
          size="sm"
          icon={Trash2}
          disabled={busy}
          onclick={() => void setAvatar(null)}>Remove photo</Button
        >
      {/if}
    </div>

    <SettingGroup title="Profile">
      <form class="name-form" onsubmit={saveName}>
        <label class="field">
          <span class="field-label">Display name</span>
          <TextField
            type="text"
            bind:value={draftName}
            autocomplete="nickname"
            disabled={savingName}
          />
        </label>
        <Button type="submit" disabled={!nameChanged || busy}>
          {savingName ? 'Saving...' : 'Save'}
        </Button>
      </form>
    </SettingGroup>

    <SettingGroup title="Identity">
      <div class="row">
        <span class="row-label">Username</span>
        <span class="row-value">
          {session.account.user_id}
          <button
            type="button"
            class="icon-button"
            aria-label="Copy username"
            onclick={() => void copyUserId()}
          >
            <Copy size={16} />
          </button>
        </span>
      </div>
      {#if copied}
        <p class="row note" role="status">Username copied.</p>
      {/if}
      <div class="row">
        <span class="row-label">Homeserver</span>
        <span class="row-value">{session.account.homeserver_url}</span>
      </div>
    </SettingGroup>

    <SettingGroup title="Session">
      <SettingButtonRow
        label="Sign out"
        description="Removes this account from Alloy on this device."
        buttonLabel={signingOut ? 'Signing out...' : 'Sign out'}
        danger
        busy={signingOut}
        onclick={() => void signOut()}
      />
    </SettingGroup>
  {:else}
    <p class="muted">No account is signed in.</p>
  {/if}

  {#if error}<p class="error" role="alert">{error}</p>{/if}
</section>

<style>
  .section {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-layout-section-gap);
  }
  .profile {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-4);
    padding: 0 var(--alloy-layout-row-pad-x);
  }
  .identity {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-1);
  }
  .name {
    margin: 0;
    font-size: var(--alloy-typography-size-lg);
    font-weight: var(--alloy-typography-weight-semibold);
    color: var(--alloy-color-text-primary);
    overflow-wrap: anywhere;
  }
  .meta {
    margin: 0;
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-text-secondary);
    overflow-wrap: anywhere;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--alloy-spacing-scale-2);
    padding: 0 var(--alloy-layout-row-pad-x);
  }
  .name-form {
    display: flex;
    align-items: flex-end;
    gap: var(--alloy-spacing-scale-2);
    padding: var(--alloy-spacing-scale-3) var(--alloy-layout-row-pad-x);
  }
  .field {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-1);
  }
  .field-label {
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-text-secondary);
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--alloy-spacing-scale-3);
    padding: var(--alloy-spacing-scale-3) var(--alloy-layout-row-pad-x);
    font-size: var(--alloy-typography-size-base);
    color: var(--alloy-color-text-primary);
  }
  .row + .row {
    border-top: 1px solid var(--alloy-color-border-subtle);
  }
  .row-label {
    color: var(--alloy-color-text-secondary);
  }
  .row-value {
    min-width: 0;
    display: inline-flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    overflow-wrap: anywhere;
    text-align: right;
  }
  .note {
    color: var(--alloy-color-text-secondary);
    font-size: var(--alloy-typography-size-sm);
    justify-content: flex-start;
  }
  .icon-button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: var(--alloy-layout-control-height-sm);
    height: var(--alloy-layout-control-height-sm);
    padding: 0;
    border: none;
    border-radius: var(--alloy-radii-full);
    background: transparent;
    color: var(--alloy-color-text-secondary);
    cursor: pointer;
  }
  .icon-button:hover {
    background: var(--alloy-color-surface-hover);
    color: var(--alloy-color-text-primary);
  }
  .icon-button:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }
  .muted {
    margin: 0;
    color: var(--alloy-color-text-muted);
  }
  .error {
    margin: 0;
    padding: 0 var(--alloy-layout-row-pad-x);
    color: var(--alloy-color-status-error);
    overflow-wrap: anywhere;
  }
</style>
