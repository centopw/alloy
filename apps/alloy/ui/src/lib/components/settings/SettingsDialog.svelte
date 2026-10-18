<script lang="ts">
  import { untrack } from 'svelte';
  import { Icon, X, ChevronLeft } from '$lib/icons';
  import { session } from '$lib/stores/session.svelte';
  import { pushBack } from '$lib/platform/backstack';
import Account from './sections/Account.svelte';
import Devices from './sections/Devices.svelte';
import Appearance from './sections/Appearance.svelte';
import Notifications from './sections/Notifications.svelte';
import Preferences from './sections/Preferences.svelte';
import Keyboard from './sections/Keyboard.svelte';
import SidebarPrefs from './sections/SidebarPrefs.svelte';
import SecurityPrivacy from './sections/SecurityPrivacy.svelte';
import Encryption from './sections/Encryption.svelte';
import HelpAbout from './sections/HelpAbout.svelte';

  interface Props {
    open: boolean;
    onclose: () => void;
  }
  let { open, onclose }: Props = $props();

  type Section =
    | 'account'
    | 'devices'
    | 'appearance'
    | 'notifications'
    | 'preferences'
    | 'keyboard'
    | 'sidebar'
    | 'security-privacy'
    | 'encryption'
    | 'help-about';

  const GROUPS: { label: string; items: { id: Section; label: string }[] }[] = [
    {
      label: 'User settings',
      items: [
        { id: 'account', label: 'Account' },
        { id: 'devices', label: 'Devices' },
        { id: 'security-privacy', label: 'Security & Privacy' },
        { id: 'encryption', label: 'Encryption' },
      ],
    },
    {
      label: 'App settings',
      items: [
        { id: 'appearance', label: 'Appearance' },
        { id: 'notifications', label: 'Notifications' },
        { id: 'preferences', label: 'Preferences' },
        { id: 'keyboard', label: 'Keyboard' },
        { id: 'sidebar', label: 'Sidebar' },
      ],
    },
    {
      label: 'Info',
      items: [{ id: 'help-about', label: 'Help & About' }],
    },
  ];

  let section = $state<Section>('account');
  // Shared header band: the content column title matches the nav title for the selected section.
  const sectionLabel = $derived(GROUPS.flatMap((group) => group.items).find((item) => item.id === section)?.label ?? '');
  // Phones show either the section list or one section; wide layouts show both side by side.
  let mobileView = $state<'nav' | 'content'>('nav');

  function openSection(id: Section) {
    section = id;
    mobileView = 'content';
  }

  // Android Back closes the dialog; inside a section it first returns to the section list.
  $effect(() => {
    if (!open) return;
    mobileView = 'nav';
    return pushBack(() => onclose());
  });
  $effect(() => {
    if (!open || mobileView !== 'content') return;
    return pushBack(() => (mobileView = 'nav'));
  });
  let panel: HTMLElement | undefined = $state();

  const FOCUSABLE =
    'button:not([disabled]), a[href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

  // Move focus into the dialog on open and return it to the opener on close.
  $effect(() => {
    if (!open) return;
    const returnTo = document.activeElement as HTMLElement | null;
    untrack(() => panel)?.focus();
    return () => returnTo?.focus?.();
  });

  // Escape closes; Tab stays inside the dialog while it is open.
  $effect(() => {
    if (!open) return;
    function onKey(e: KeyboardEvent) {
      if (e.key === 'Escape') {
        e.preventDefault();
        e.stopPropagation();
        onclose();
        return;
      }
      if (e.key === 'Tab') trapFocus(e);
    }
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  });

  function trapFocus(e: KeyboardEvent) {
    if (!panel) return;
    const items = Array.from(panel.querySelectorAll<HTMLElement>(FOCUSABLE));
    if (items.length === 0) {
      e.preventDefault();
      return;
    }
    const first = items[0];
    const last = items[items.length - 1];
    const active = document.activeElement;
    const outside = !panel.contains(active);
    if (e.shiftKey) {
      if (outside || active === first) {
        e.preventDefault();
        last.focus();
      }
    } else if (outside || active === last) {
      e.preventDefault();
      first.focus();
    }
  }

  async function signOut() {
    onclose();
    await session.logout();
  }
</script>

{#if open}
  <!-- Backdrop click dismisses; keyboard users close with Escape or the close button. -->
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="backdrop"
    onclick={(e) => {
      if (e.target === e.currentTarget) onclose();
    }}
  >
    <div
      class="panel"
      data-view={mobileView}
      role="dialog"
      aria-modal="true"
      aria-labelledby="settings-title"
      tabindex="-1"
      bind:this={panel}
    >
      <nav class="nav" aria-labelledby="settings-title">
        <h2 id="settings-title" class="nav-title">Settings</h2>

        {#each GROUPS as group (group.label)}
          <div class="group">
            <h3 class="group-label">{group.label}</h3>
            {#each group.items as item (item.id)}
              <button
                type="button"
                class="nav-item"
                aria-current={section === item.id ? 'page' : undefined}
                onclick={() => openSection(item.id)}
              >
                {item.label}
              </button>
            {/each}
          </div>
        {/each}

        <div class="nav-footer">
          <button type="button" class="nav-item danger" onclick={signOut}>Sign out</button>
        </div>
      </nav>

      <div class="content" role="region" aria-labelledby="settings-section-title">
        <header class="header">
          <button type="button" class="back" aria-label="Back to settings" onclick={() => (mobileView = 'nav')}>
            <Icon icon={ChevronLeft} size={20} />
          </button>
          <h2 id="settings-section-title" class="header-title">{sectionLabel}</h2>
          <button type="button" class="close" aria-label="Close settings" onclick={onclose}>
            <Icon icon={X} size={20} />
          </button>
        </header>

        <div class="scroll">
          <div class="column">
            {#if section === 'account'}
              <Account {onclose} />
            {:else if section === 'devices'}
              <Devices {onclose} />
            {:else if section === 'appearance'}
              <Appearance />
            {:else if section === 'notifications'}
              <Notifications />
            {:else if section === 'preferences'}
              <Preferences />
            {:else if section === 'keyboard'}
              <Keyboard />
            {:else if section === 'sidebar'}
              <SidebarPrefs />
            {:else if section === 'security-privacy'}
              <SecurityPrivacy />
            {:else if section === 'encryption'}
              <Encryption {onclose} />
            {:else}
              <HelpAbout />
            {/if}
          </div>
        </div>
      </div>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 60;
    display: flex;
    padding: var(--alloy-spacing-scale-4);
    background: color-mix(in srgb, var(--alloy-color-surface-canvas) 70%, transparent);
  }
  .panel {
    display: flex;
    width: 100%;
    height: 100%;
    overflow: hidden;
    background: var(--alloy-color-surface-canvas);
    border-radius: var(--alloy-radii-lg);
    box-shadow: var(--alloy-elevation-lg);
    outline: none;
  }
  .nav {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-4);
    width: 240px;
    flex-shrink: 0;
    padding: 0 var(--alloy-layout-row-inset) var(--alloy-layout-row-inset);
    overflow-y: auto;
    background: var(--alloy-color-surface-panel);
  }
  .nav-title {
    margin: 0;
    display: flex;
    align-items: center;
    min-height: var(--alloy-layout-header-height);
    padding: 0 calc(var(--alloy-layout-row-inset) + var(--alloy-layout-row-pad-x));
    font-size: var(--alloy-typography-size-lg);
    font-weight: var(--alloy-typography-weight-semibold);
    color: var(--alloy-color-text-primary);
  }
  .group {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-1);
  }
  .group-label {
    margin: 0;
    padding: var(--alloy-spacing-scale-2) calc(var(--alloy-layout-row-inset) + var(--alloy-layout-row-pad-x)) var(--alloy-spacing-scale-1);
    font-size: var(--alloy-typography-size-xs);
    font-weight: var(--alloy-typography-weight-semibold);
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--alloy-color-text-muted);
  }
  .nav-item {
    display: block;
    width: 100%;
    margin: 0;
    padding: var(--alloy-spacing-scale-2) var(--alloy-layout-row-pad-x);
    border: 0;
    border-radius: var(--alloy-radii-md);
    background: transparent;
    font: inherit;
    font-size: var(--alloy-typography-size-base);
    text-align: left;
    color: var(--alloy-color-text-secondary);
    cursor: pointer;
  }
  .nav-item:hover {
    background: var(--alloy-color-surface-subtle);
    color: var(--alloy-color-text-primary);
  }
  .nav-item[aria-current='page'] {
    background: var(--alloy-color-surface-hover);
    color: var(--alloy-color-text-primary);
    font-weight: var(--alloy-typography-weight-medium);
  }
  .nav-item.danger {
    color: var(--alloy-color-status-error);
  }
  .nav-item:focus-visible,
  .close:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }
  .nav-footer {
    margin-top: auto;
    padding-top: var(--alloy-spacing-scale-2);
    border-top: 1px solid var(--alloy-color-border-subtle);
  }
  .content {
    display: flex;
    flex: 1;
    min-width: 0;
    flex-direction: column;
    background: var(--alloy-color-surface-canvas);
  }
  .header {
    display: flex;
    flex-shrink: 0;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    min-height: var(--alloy-layout-header-height);
    padding: 0 calc(var(--alloy-layout-row-inset) + var(--alloy-layout-row-pad-x));
  }
  .header-title {
    flex: 1;
    min-width: 0;
    margin: 0;
    overflow: hidden;
    font-size: var(--alloy-typography-size-lg);
    font-weight: var(--alloy-typography-weight-semibold);
    white-space: nowrap;
    text-overflow: ellipsis;
    color: var(--alloy-color-text-primary);
  }
  .close {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    padding: 0;
    border: 0;
    border-radius: var(--alloy-radii-md);
    background: transparent;
    color: var(--alloy-color-text-secondary);
    cursor: pointer;
  }
  .close:hover {
    background: var(--alloy-color-surface-hover);
    color: var(--alloy-color-text-primary);
  }
  .scroll {
    flex: 1;
    overflow-y: auto;
  }
  .back {
    display: none;
  }
  @media (max-width: 719px) {
    .backdrop {
      padding: 0;
    }
    .panel {
      border-radius: 0;
    }
    .nav {
      width: 100%;
    }
    .panel[data-view='nav'] .content {
      display: none;
    }
    .panel[data-view='content'] .nav {
      display: none;
    }
    .back {
      display: inline-flex;
      align-items: center;
      justify-content: center;
      width: 40px;
      height: 40px;
      padding: 0;
      border: 0;
      border-radius: var(--alloy-radii-full);
      background: transparent;
      color: var(--alloy-color-text-secondary);
      cursor: pointer;
    }
    .column {
      padding: var(--alloy-spacing-scale-6) var(--alloy-layout-gutter) var(--alloy-spacing-scale-6);
    }
  }
  .column {
    max-width: 640px;
    margin: 0 auto;
    padding: var(--alloy-spacing-scale-6) var(--alloy-layout-gutter);
  }
</style>
