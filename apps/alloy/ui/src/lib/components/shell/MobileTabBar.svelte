<script lang="ts">
  import { Icon, MessageSquare, Compass, Settings } from '$lib/icons';

  type Tab = 'chats' | 'discover' | 'settings';

  let {
    active,
    onchats,
    ondiscover,
    onsettings,
    unread = 0,
  }: {
    active: Tab;
    onchats?: () => void;
    ondiscover?: () => void;
    onsettings?: () => void;
    unread?: number;
  } = $props();

  const badge = $derived(unread > 99 ? '99+' : String(unread));
  const chatsLabel = $derived(unread > 0 ? `Chats, ${unread} unread` : 'Chats');
</script>

<nav class="tabbar" aria-label="Primary">
  <button
    type="button"
    class="tab"
    class:active={active === 'chats'}
    aria-current={active === 'chats' ? 'page' : undefined}
    aria-label={chatsLabel}
    onclick={() => onchats?.()}
  >
    <span class="glyph">
      <Icon icon={MessageSquare} size={24} />
      {#if unread > 0}
        <span class="badge" aria-hidden="true">{badge}</span>
      {/if}
    </span>
    <span class="label">Chats</span>
  </button>
  <button
    type="button"
    class="tab"
    class:active={active === 'discover'}
    aria-current={active === 'discover' ? 'page' : undefined}
    aria-label="Discover"
    onclick={() => ondiscover?.()}
  >
    <span class="glyph"><Icon icon={Compass} size={24} /></span>
    <span class="label">Discover</span>
  </button>
  <button
    type="button"
    class="tab"
    class:active={active === 'settings'}
    aria-current={active === 'settings' ? 'page' : undefined}
    aria-label="Settings"
    onclick={() => onsettings?.()}
  >
    <span class="glyph"><Icon icon={Settings} size={24} /></span>
    <span class="label">Settings</span>
  </button>
</nav>

<style>
  .tabbar {
    display: flex;
    flex: 0 0 auto;
    box-sizing: border-box;
    height: calc(56px + var(--alloy-safe-area-bottom));
    padding: 0 var(--alloy-safe-area-right) var(--alloy-safe-area-bottom) var(--alloy-safe-area-left);
    background: var(--alloy-color-surface-panel);
    border-top: 1px solid var(--alloy-color-border-subtle);
  }

  .tab {
    flex: 1 1 0;
    min-width: 44px;
    min-height: 44px;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 2px;
    padding: 4px 0;
    border: 0;
    background: transparent;
    color: var(--alloy-color-text-muted);
    font: inherit;
    cursor: pointer;
    -webkit-tap-highlight-color: transparent;
  }

  .tab.active {
    color: var(--alloy-color-accent-primary);
  }

  .tab:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: -2px;
    border-radius: var(--alloy-radius-md);
  }

  .glyph {
    position: relative;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 28px;
  }

  .label {
    font-size: var(--alloy-typography-size-xs);
    font-weight: var(--alloy-typography-weight-medium);
    line-height: 1;
  }

  .badge {
    position: absolute;
    top: -2px;
    right: -8px;
    min-width: 18px;
    height: 18px;
    padding: 0 5px;
    box-sizing: border-box;
    border-radius: var(--alloy-radius-full);
    background: var(--alloy-color-status-mention);
    color: var(--alloy-color-text-on-accent);
    font-size: var(--alloy-typography-size-xs);
    font-weight: var(--alloy-typography-weight-bold);
    line-height: 18px;
    text-align: center;
    font-variant-numeric: tabular-nums;
  }

  @media (hover: hover) {
    .tab:hover {
      color: var(--alloy-color-text-primary);
    }
    .tab.active:hover {
      color: var(--alloy-color-accent-primary);
    }
  }
</style>
