<script lang="ts">
  import { Avatar, UserName } from '../common';
  import Icon from '$lib/icons/Icon.svelte';
  import { Copy, Settings } from '$lib/icons';
  import { contextmenu } from '$lib/platform/contextmenu';
  import type { MenuItem } from '$lib/stores/contextmenu.svelte';
  import { session } from '$lib/stores/session.svelte';
  import { viewport } from '$lib/stores/viewport.svelte';

  interface Props {
    onsettings?: () => void;
  }
  let { onsettings }: Props = $props();

  const account = $derived(session.account);
  const name = $derived(account?.display_name?.trim() || account?.user_id || '');

  function panelItems(userId: string): MenuItem[] {
    return [
      { id: 'settings', label: 'Settings', icon: Settings, onselect: () => onsettings?.() },
      { separator: true },
      { id: 'copy-user-id', label: 'Copy user ID', icon: Copy, onselect: () => void navigator.clipboard?.writeText(userId).catch(() => undefined) },
    ];
  }
</script>

{#if account && !viewport.isPhone}
  <div class="panel" use:contextmenu={() => panelItems(account.user_id)}>
    <span class="pinned">
      <Avatar userId={account.user_id} displayName={name} src={account.avatar_url} size="md" />
    </span>
    <div class="who">
      <UserName userId={account.user_id} displayName={name} class="name" />
      <span class="id" title={account.user_id}>{account.user_id}</span>
    </div>
    <button type="button" class="gear" aria-label="Settings" onclick={() => onsettings?.()}>
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
    margin: var(--alloy-layout-row-inset);
    padding: var(--alloy-spacing-scale-2) var(--alloy-layout-row-pad-x);
    border: 1px solid var(--alloy-glass-rim);
    border-radius: var(--alloy-glass-radius-control);
    background: var(--alloy-glass-control);
    box-shadow: var(--alloy-glass-highlight);
  }
  /* Named `.pinned`, not `.fixed`: Tailwind's utilities layer emits a global `.fixed{position:fixed}`
     and a bare `.fixed` class would turn the avatar container into a viewport-fixed box. */
  .pinned {
    flex: none;
    display: inline-flex;
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
    background: var(--alloy-glass-control-hover);
    color: var(--alloy-color-text-primary);
  }
  .gear:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }
</style>
