<script lang="ts">
  import { Icon, MessageSquare, Settings } from '$lib/icons';

  // Two destinations: public-room browsing lives in the room list's menu, so the bar is the two
  // places a phone actually switches between.
  type Tab = 'chats' | 'settings';

  let {
    active,
    onchats,
    onsettings,
    unread = 0,
  }: {
    active: Tab;
    onchats?: () => void;
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
    /* Two tabs hug their content and sit centred: stretched across the screen they read as a dot. */
    flex: 0 0 auto;
    width: fit-content;
    margin-inline: auto;
    box-sizing: border-box;
    height: 56px;
    margin-top: var(--alloy-spacing-scale-2);
    margin-bottom: calc(var(--alloy-spacing-scale-2) + var(--alloy-safe-area-bottom));
    padding: 0 var(--alloy-spacing-scale-1);
    border: 1px solid var(--alloy-glass-rim);
    border-radius: var(--alloy-glass-radius-shell);
    background: var(--alloy-glass-sheen), var(--alloy-glass-fill-strong);
    -webkit-backdrop-filter: var(--alloy-glass-filter);
    backdrop-filter: var(--alloy-glass-filter);
    box-shadow: var(--alloy-glass-shadow-soft), var(--alloy-glass-highlight);
  }

  .tab {
    /* 96px is the narrowest width where "Settings" clears the 40px glyph pill at size-xs. */
    flex: 0 0 auto;
    min-width: 96px;
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 2px;
    padding: var(--alloy-spacing-scale-1) var(--alloy-spacing-scale-3);
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

  /* Pressed state: the platform flash is off, so a tap needs its own acknowledgement. Transient
     only — the accent colour is what says which page is current. */
  .tab:active .glyph {
    background: color-mix(in srgb, var(--alloy-color-accent-primary) 10%, transparent);
    transition-duration: 90ms;
  }
  .tab:active .label {
    color: var(--alloy-color-text-primary);
  }

  .tab:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: -2px;
    border-radius: var(--alloy-radii-md);
  }

  .glyph {
    position: relative;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 40px;
    height: 28px;
    border-radius: var(--alloy-radii-full);
    transition: background var(--alloy-motion-duration-fast) ease;
  }

  .tab.active .glyph {
    background: color-mix(in srgb, var(--alloy-color-accent-primary) 14%, transparent);
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
    padding: 0 5px; /* token-audit: badge inset, set by the 18px badge height */
    box-sizing: border-box;
    border-radius: var(--alloy-radii-full);
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
