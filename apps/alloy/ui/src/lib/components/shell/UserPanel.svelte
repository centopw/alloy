<script lang="ts">
  import { Avatar, UserName } from '../common';
  import Icon from '$lib/icons/Icon.svelte';
  import { Settings } from '$lib/icons';
  import { session } from '$lib/stores/session.svelte';

  interface Props {
    onsettings?: () => void;
  }
  let { onsettings }: Props = $props();

  const account = $derived(session.account);
  const name = $derived(account?.display_name?.trim() || account?.user_id || '');
</script>

{#if account}
  <div class="panel">
    <Avatar userId={account.user_id} displayName={name} src={account.avatar_url} size="md" />
    <div class="who">
      <UserName userId={account.user_id} displayName={name} class="name" />
      <span class="id" title={account.user_id}>{account.user_id}</span>
    </div>
    <button type="button" class="gear" aria-label="User settings" title="User settings" onclick={() => onsettings?.()}>
      <Icon icon={Settings} size={20} />
    </button>
  </div>
{/if}

<style>
  .panel {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    flex-shrink: 0;
    margin: var(--alloy-spacing-scale-2);
    padding: var(--alloy-spacing-scale-2);
    border-radius: var(--alloy-radii-md);
    background: var(--alloy-color-surface-canvas);
  }
  .who {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
  }
  .who :global(.name) {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--alloy-typography-size-sm);
    font-weight: var(--alloy-typography-weight-semibold);
  }
  .id {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--alloy-typography-size-xs);
    color: var(--alloy-color-text-muted);
  }
  .gear {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    padding: 0;
    border: 0;
    border-radius: var(--alloy-radii-full);
    background: transparent;
    color: var(--alloy-color-text-secondary);
    cursor: pointer;
  }
  .gear:hover {
    background: var(--alloy-color-surface-hover);
    color: var(--alloy-color-text-primary);
  }
  .gear:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }
</style>
