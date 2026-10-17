<script lang="ts">
  import { untrack } from 'svelte';
  import { Icon, X, ChevronLeft } from '$lib/icons';
  import { session } from '$lib/stores/session.svelte';
  import { pushBack } from '$lib/platform/backstack';
  import MyAccount from './sections/MyAccount.svelte';
  import Appearance from './sections/Appearance.svelte';
  import Security from './sections/Security.svelte';
  import Notifications from './sections/Notifications.svelte';
  import Advanced from './sections/Advanced.svelte';
  import About from './sections/About.svelte';

  interface Props {
    open: boolean;
    onclose: () => void;
  }
  let { open, onclose }: Props = $props();

  type Section = 'account' | 'appearance' | 'security' | 'notifications' | 'advanced' | 'about';

  const GROUPS: { label: string; items: { id: Section; label: string }[] }[] = [
    {
      label: 'User settings',
      items: [
        { id: 'account', label: 'My Account' },
        { id: 'security', label: 'Security' },
      ],
    },
    {
      label: 'App settings',
      items: [
        { id: 'appearance', label: 'Appearance' },
        { id: 'notifications', label: 'Notifications' },
        { id: 'advanced', label: 'Advanced' },
      ],
    },
    {
      label: 'Info',
      items: [{ id: 'about', label: 'About' }],
    },
  ];

  let section = $state<Section>('account');
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

      <div class="content">
        <button type="button" class="back" aria-label="Back to settings" onclick={() => (mobileView = 'nav')}>
          <Icon icon={ChevronLeft} size={20} />
        </button>
        <button type="button" class="close" aria-label="Close settings" onclick={onclose}>
          <Icon icon={X} size={20} />
        </button>

        <div class="scroll">
          <div class="column">
            {#if section === 'account'}
              <MyAccount />
            {:else if section === 'appearance'}
              <Appearance />
            {:else if section === 'security'}
              <Security {onclose} />
            {:else if section === 'notifications'}
              <Notifications />
            {:else if section === 'advanced'}
              <Advanced />
            {:else}
              <About />
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
    padding: var(--alloy-spacing-scale-2);
    overflow-y: auto;
    background: var(--alloy-color-surface-panel);
  }
  .nav-title {
    margin: 0;
    padding: var(--alloy-spacing-scale-2);
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
    padding: var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-1);
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
    padding: var(--alloy-spacing-scale-2);
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
    position: relative;
    display: flex;
    flex: 1;
    min-width: 0;
    flex-direction: column;
    background: var(--alloy-color-surface-canvas);
  }
  .close {
    position: absolute;
    top: var(--alloy-spacing-scale-3);
    right: var(--alloy-spacing-scale-3);
    z-index: 1;
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
      position: absolute;
      top: var(--alloy-spacing-scale-3);
      left: var(--alloy-spacing-scale-3);
      z-index: 1;
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
      padding: calc(var(--alloy-spacing-scale-8) * 2.5) var(--alloy-spacing-scale-4) var(--alloy-spacing-scale-6);
    }
  }
  .column {
    max-width: 640px;
    margin: 0 auto;
    padding: var(--alloy-spacing-scale-8) var(--alloy-spacing-scale-6);
  }
</style>
