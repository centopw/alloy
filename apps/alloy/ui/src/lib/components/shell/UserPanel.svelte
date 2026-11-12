<script lang="ts">
  import { Avatar } from '../common';
  import Icon from '$lib/icons/Icon.svelte';
  import { Copy, Settings } from '$lib/icons';
  import { contextmenu } from '$lib/platform/contextmenu';
  import type { MenuItem } from '$lib/stores/contextmenu.svelte';
  import { viewport } from '$lib/stores/viewport.svelte';
  import { getUserColorValue } from '$lib/identity/userColor';
  import type { Account } from '$lib/generated';

  import { t } from '$lib/i18n';
  interface Props {
    /** The account the panel names; null hides the panel. A device may hold several accounts. */
    account: Account | null;
    /** The network the account belongs to, shown under the name. */
    networkLabel?: string;
    onsettings?: () => void;
  }
  let { account, networkLabel, onsettings }: Props = $props();

  const userId = $derived(account?.remote_user_id ?? account?.id ?? '');
  const name = $derived(account?.display_name?.trim() || userId);
  const nameColor = $derived(getUserColorValue(userId));

  function panelItems(accountId: string): MenuItem[] {
    return [
      { id: 'settings', label: t('common-settings'), icon: Settings, onselect: () => onsettings?.() },
      { separator: true },
      { id: 'copy-account-id', label: t('user-panel-copy-account-id'), icon: Copy, onselect: () => void navigator.clipboard?.writeText(accountId).catch(() => undefined) },
    ];
  }
</script>

{#if account && !viewport.isPhone}
  <div class="panel" use:contextmenu={() => panelItems(account.id)}>
    <span class="pinned">
      <Avatar userId={userId} displayName={name} size="md" />
    </span>
    <div class="who">
      <span class="name" style:color={nameColor}>{name}</span>
      {#if networkLabel}
        <span class="id">{networkLabel}</span>
      {/if}
    </div>
    <button type="button" class="gear" aria-label={t('common-settings')} data-tooltip={t('common-settings')} onclick={() => onsettings?.()}>
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
  .name {
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
