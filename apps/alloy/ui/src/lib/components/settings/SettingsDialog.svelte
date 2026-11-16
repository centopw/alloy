<script lang="ts">
  import { untrack, type Component } from 'svelte';
  import {
    Icon,
    X,
    ChevronLeft,
    ChevronRight,
    User,
    ShieldCheck,
    Palette,
    Bell,
    SlidersHorizontal,
    Info,
  } from '$lib/icons';
  import { pushBack } from '$lib/platform/backstack';
  import { handleModalTabKey } from '$lib/platform/focusTrap';
  import { session } from '$lib/stores/session.svelte';
  import { viewport } from '$lib/stores/viewport.svelte';
  import {
    capabilities,
    currentEnv,
    filterSettingsGroups,
    SETTINGS_GROUPS_DEF,
    type RawSettingsGroup,
    type SettingsSectionId,
  } from '$lib/platform/capabilities';
  import { SettingGroup } from './controls';
import Account from './sections/Account.svelte';
import Networks from './sections/Networks.svelte';
import Appearance from './sections/Appearance.svelte';
import Notifications from './sections/Notifications.svelte';
import Preferences from './sections/Preferences.svelte';
import Keyboard from './sections/Keyboard.svelte';
import SidebarPrefs from './sections/SidebarPrefs.svelte';
import SecurityPrivacy from './sections/SecurityPrivacy.svelte';
import HelpAbout from './sections/HelpAbout.svelte';
import Tags from './sections/Tags.svelte';
import Permissions from './sections/Permissions.svelte';
import Debug from './sections/Debug.svelte';
import DevicesSyncLive from './sync/DevicesSyncLive.svelte';

  import { t } from '$lib/i18n';
  interface Props {
    open: boolean;
    onclose: () => void;
    /** Page to show when the dialog opens (for example 'tags' from the room list's tag menu). */
    initialSection?: SettingsSectionId;
  }
  let { open, onclose, initialSection }: Props = $props();

  /**
   * Devices & Sync is a section of its own, contributed here rather than through
   * `SETTINGS_GROUPS_DEF` so this slice does not have to widen the shared section-id union. It sits
   * under Security & Privacy in the user-settings group.
   */
  const DEVICES_SYNC = 'devices-sync' as const;
  type Section = SettingsSectionId | typeof DEVICES_SYNC;

  type NavIcon = Component<{ size?: number; strokeWidth?: number; class?: string }>;

  const CANONICAL_SECTIONS: ReadonlySet<SettingsSectionId> = new Set([
    'account',
    'security-privacy',
    'appearance',
    'notifications',
    'preferences',
    'help-about',
  ]);

  const NAV_ICONS: Partial<Record<SettingsSectionId, { icon: NavIcon; phoneIcon?: NavIcon }>> = {
    account: { icon: User },
    'security-privacy': { icon: ShieldCheck },
    appearance: { icon: Palette },
    notifications: { icon: Bell },
    preferences: { icon: SlidersHorizontal },
    'help-about': { icon: Info },
  };

  /** Maps legacy section IDs to their consolidated parent so deep links continue to work. */
  function normalizeSection(id: Section): Section {
    switch (id) {
      case 'networks':
        return 'account';
      case 'sidebar':
        return 'appearance';
      case 'permissions':
        return 'notifications';
      case 'tags':
        return 'preferences';
      case 'keyboard':
      case 'debug':
        return 'help-about';
      default:
        return id;
    }
  }

  const GROUPS: RawSettingsGroup<NavIcon>[] = SETTINGS_GROUPS_DEF.map((group) => ({
    ...group,
    items: group.items
      .filter((item) => CANONICAL_SECTIONS.has(item.id))
      .map((item) => ({
        ...item,
        icon: NAV_ICONS[item.id]?.icon,
        phoneIcon: NAV_ICONS[item.id]?.phoneIcon,
      })),
  })).filter((group) => group.items.length > 0);

  const caps = $derived(capabilities(currentEnv(viewport.isPhone)));
  const visibleGroups = $derived(filterSettingsGroups(GROUPS, caps));

  // Nav groups are the platform-filtered ones plus the locally contributed Devices & Sync page.
  const navGroups = $derived.by(() => {
    const groups: { label: string; items: { id: Section; label: string; icon?: NavIcon }[] }[] =
      visibleGroups.map((group) => ({
        label: group.label,
        items: group.items.map((item) => ({ id: item.id, label: item.label, icon: item.icon })),
      }));
    const devices = {
      id: DEVICES_SYNC,
      label: t('settings-devices-sync'),
      icon: ShieldCheck,
    };
    const user = groups.find((group) =>
      group.items.some((entry) => entry.id === 'security-privacy')
    );
    if (user) {
      const at = user.items.findIndex((entry) => entry.id === 'security-privacy');
      user.items.splice(at + 1, 0, devices);
    } else if (groups.length > 0) {
      groups[0].items.push(devices);
    } else {
      groups.push({ label: '', items: [devices] });
    }
    return groups;
  });
  const allSections = $derived(navGroups.flatMap((group) => group.items));

  let section = $state<Section>('account');

  // If the stored section is no longer available after a viewport change, fall back to the first available section.
  $effect(() => {
    if (allSections.length > 0 && !allSections.some((item) => item.id === section)) {
      section = allSections[0].id;
    }
  });

  // Shared header band: the content column title matches the nav title for the selected section.
  const sectionLabel = $derived(allSections.find((item) => item.id === section)?.label ?? '');
  let mobileView = $state<'nav' | 'content'>('nav');

  function openSection(id: Section) {
    section = normalizeSection(id);
    mobileView = 'content';
  }

  // Android Back closes the dialog; inside a section it first returns to the section list.
  $effect(() => {
    if (!open) return;
    // Opening on a specific page goes straight to it (on phones too); otherwise start at the list.
    const rawTarget = untrack(() => initialSection);
    const target = rawTarget ? normalizeSection(rawTarget) : undefined;
    if (target && untrack(() => allSections).some((item) => item.id === target)) {
      section = target;
      mobileView = 'content';
    } else {
      mobileView = 'nav';
    }
    return pushBack(() => onclose());
  });
  $effect(() => {
    if (!open || mobileView !== 'content') return;
    return pushBack(() => (mobileView = 'nav'));
  });
  let panel: HTMLElement | undefined = $state();

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
      if (session.addingAccount) return;
      if (e.key === 'Escape') {
        e.preventDefault();
        e.stopPropagation();
        onclose();
        return;
      }
      if (e.key === 'Tab' && handleModalTabKey(e, panel, document.activeElement)) {
        e.preventDefault();
      }
    }
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  });
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
        <header class="nav-header">
          <h2 id="settings-title" class="nav-title">{t('common-settings')}</h2>
          <button type="button" class="nav-close" aria-label={t('settings-dialog-close-settings')} onclick={onclose}>
            <Icon icon={X} size={20} />
          </button>
        </header>

        {#each navGroups as group (group.label)}
          <div class="group">
            <h3 class="group-label">{group.label}</h3>
            {#each group.items as item (item.id)}
              <button
                type="button"
                class="nav-item"
                aria-current={section === item.id ? 'page' : undefined}
                onclick={() => openSection(item.id)}
              >
                {#if item.icon}
                  <span class="nav-icon"><Icon icon={item.icon} size={20} /></span>
                {/if}
                <span class="nav-label">{item.label}</span>
                <span class="nav-chevron"><Icon icon={ChevronRight} size={20} /></span>
              </button>
            {/each}
          </div>
        {/each}
      </nav>

      <div class="content" role="region" aria-labelledby="settings-section-title">
        <header class="header">
          <button type="button" class="back" aria-label={t('settings-dialog-back-to-settings')} onclick={() => (mobileView = 'nav')}>
            <Icon icon={ChevronLeft} size={20} />
          </button>
          <h2 id="settings-section-title" class="header-title">{sectionLabel}</h2>
          <button type="button" class="close" aria-label={t('settings-dialog-close-settings')} onclick={onclose}>
            <Icon icon={X} size={20} />
          </button>
        </header>

        <div class="scroll">
          <div class="column">
            {#if section === 'account'}
              <Account {onclose} />
              <Networks />
            {:else if section === 'security-privacy'}
              <SecurityPrivacy />
            {:else if section === 'devices-sync'}
              <DevicesSyncLive />
            {:else if section === 'appearance'}
              <Appearance />
              <SidebarPrefs />
            {:else if section === 'notifications'}
              <Notifications onOpenPermissions={() => openSection('notifications')} />
              <Permissions />
            {:else if section === 'preferences'}
              <Preferences />
              <SettingGroup title={t('sidebar-tags')}>
                <Tags />
              </SettingGroup>
            {:else}
              {#if caps.keyboardShortcuts}
                <Keyboard />
              {/if}
              <HelpAbout />
              <Debug />
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
    /* Under an overlay title bar the traffic lights float over the window's top-left corner, so the
       panel starts below them instead of being drawn behind them. */
    padding: max(var(--alloy-spacing-scale-4), calc(var(--alloy-titlebar-lights-inset) / 4 + 14px))
      var(--alloy-spacing-scale-4) var(--alloy-spacing-scale-4);
    background: color-mix(in srgb, var(--alloy-color-surface-canvas) 55%, transparent);
    -webkit-backdrop-filter: blur(10px) saturate(140%);
    backdrop-filter: blur(10px) saturate(140%);
  }
  .panel {
    display: flex;
    width: 100%;
    height: 100%;
    /* A settings window, not a second app: cap it so the 720px reading column sits in a
       deliberate frame instead of floating in a full-bleed pane. */
    max-width: 1040px;
    max-height: 760px;
    margin: auto;
    overflow: hidden;
    background: var(--alloy-glass-sheen), var(--alloy-glass-fill-strong);
    -webkit-backdrop-filter: var(--alloy-glass-filter-strong);
    backdrop-filter: var(--alloy-glass-filter-strong);
    border: 1px solid var(--alloy-glass-rim);
    border-radius: var(--alloy-glass-radius-shell);
    box-shadow: var(--alloy-glass-shadow), var(--alloy-glass-highlight);
    outline: none;
  }
  .nav {
    display: flex;
    flex-direction: column;
    gap: 0;
    width: 240px;
    min-height: 0;
    flex-shrink: 0;
    padding: 0 var(--alloy-layout-gutter) var(--alloy-layout-gutter);
    overflow-y: auto;
    overscroll-behavior: contain;
    -webkit-overflow-scrolling: touch;
    touch-action: pan-y;
    background: color-mix(in srgb, var(--alloy-color-surface-panel) 42%, transparent);
    border-right: 1px solid var(--alloy-glass-rim);
  }
  .nav-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--alloy-spacing-scale-2);
    min-height: var(--alloy-layout-header-height);
    padding: 0 var(--alloy-spacing-scale-2);
  }
  .nav-title {
    margin: 0;
    min-width: 0;
    overflow: hidden;
    font-size: var(--alloy-font-size-title);
    font-weight: var(--alloy-typography-weight-semibold);
    white-space: nowrap;
    text-overflow: ellipsis;
    color: var(--alloy-color-text-primary);
  }
  .group {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-1);
  }
  /* Groups are separated by air; the first one starts on the same 24px offset as the section
     column, so the two panes' first rows line up under the shared header band. */
  .group + .group {
    margin-top: var(--alloy-spacing-scale-2);
  }
  .group:first-of-type .group-label {
    padding-top: var(--alloy-spacing-scale-6);
  }
  .group-label {
    margin: 0;
    padding: var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-1);
    font-size: var(--alloy-typography-size-xs);
    font-weight: var(--alloy-typography-weight-semibold);
    letter-spacing: var(--alloy-letter-spacing-caps);
    text-transform: uppercase;
    color: var(--alloy-color-text-muted);
  }
  .nav-item {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    width: 100%;
    min-height: var(--alloy-layout-control-height);
    margin: 0;
    padding: var(--alloy-spacing-scale-2);
    border: 1px solid transparent;
    border-radius: var(--alloy-glass-radius-control);
    background: transparent;
    font: inherit;
    font-size: var(--alloy-font-size-row);
    text-align: left;
    color: var(--alloy-color-text-secondary);
    cursor: pointer;
    transition:
      background-color var(--alloy-motion-duration-fast) var(--alloy-motion-easing-ease),
      color var(--alloy-motion-duration-fast) var(--alloy-motion-easing-ease),
      transform var(--alloy-motion-duration-instant) var(--alloy-motion-easing-ease);
  }
  @media (hover: hover) {
    .nav-item:hover {
      background: var(--alloy-glass-control);
      color: var(--alloy-color-text-primary);
    }
  }
  .nav-item:active {
    background: var(--alloy-glass-control-hover);
    color: var(--alloy-color-text-primary);
    transform: scale(0.99);
  }
  .nav-item[aria-current='page'] {
    border-color: color-mix(in srgb, var(--alloy-color-accent-primary) 26%, transparent);
    background: color-mix(in srgb, var(--alloy-color-accent-primary) 14%, transparent);
    color: var(--alloy-color-text-primary);
    font-weight: var(--alloy-typography-weight-medium);
    box-shadow: var(--alloy-glass-highlight);
  }
  .nav-label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .nav-item:focus-visible,
  .nav-close:focus-visible,
  .close:focus-visible,
  .back:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }
  .nav-close,
  .close,
  .back {
    transition:
      background-color var(--alloy-motion-duration-fast) var(--alloy-motion-easing-ease),
      color var(--alloy-motion-duration-fast) var(--alloy-motion-easing-ease),
      transform var(--alloy-motion-duration-instant) var(--alloy-motion-easing-ease);
  }
  @media (hover: hover) {
    .nav-close:hover,
    .close:hover,
    .back:hover {
      background: var(--alloy-glass-control);
      color: var(--alloy-color-text-primary);
    }
  }
  .nav-close:active,
  .close:active,
  .back:active {
    background: var(--alloy-glass-control-hover);
    color: var(--alloy-color-text-primary);
    transform: scale(0.95);
  }
  .content {
    display: flex;
    flex: 1;
    min-width: 0;
    min-height: 0;
    flex-direction: column;
    overscroll-behavior: contain;
    touch-action: pan-y;
    background: transparent;
  }
  /* One measure, one gutter: the reading column is capped at 720px of content inside its 24px
     side padding, and the header band above it uses the same 24px gutter, so the title, the
     section headings and every row start on the same line whatever the scrollbar does. */
  .header {
    display: flex;
    flex-shrink: 0;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    min-height: var(--alloy-layout-header-height);
    padding: 0 var(--alloy-spacing-scale-6);
  }
  .header-title {
    flex: 1;
    min-width: 0;
    margin: 0;
    overflow: hidden;
    font-size: var(--alloy-font-size-title);
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
  @media (pointer: coarse) {
    .close {
      width: var(--alloy-touch-target);
      height: var(--alloy-touch-target);
    }
  }
  .scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    overscroll-behavior: contain;
    -webkit-overflow-scrolling: touch;
    touch-action: pan-y;
  }
  .back {
    display: none;
  }
  .nav-icon,
  .nav-chevron,
  .nav-close {
    display: none;
  }
  @media (max-width: 899px) {
    .backdrop {
      padding: 0;
      background: var(--alloy-canvas-art);
      -webkit-backdrop-filter: none;
      backdrop-filter: none;
    }
    .panel {
      height: 100dvh;
      max-width: none;
      max-height: 100dvh;
      margin: 0;
      padding: var(--alloy-safe-area-top) var(--alloy-safe-area-right) 0 var(--alloy-safe-area-left);
      border: 0;
      border-radius: 0;
      box-shadow: none;
    }
    .nav {
      width: 100%;
      gap: 0;
      padding: 0 0 calc(var(--alloy-spacing-scale-4) + var(--alloy-safe-area-bottom));
      background: var(--alloy-glass-sheen), var(--alloy-glass-fill);
      border-right: 0;
    }
    .nav-header {
      min-height: 56px;
      padding: 0 var(--alloy-spacing-scale-2) 0 var(--alloy-layout-gutter);
    }
    .nav-close {
      display: inline-flex;
      align-items: center;
      justify-content: center;
      flex-shrink: 0;
      width: 44px;
      height: 44px;
      padding: 0;
      border: 0;
      border-radius: var(--alloy-radii-full);
      background: transparent;
      color: var(--alloy-color-text-secondary);
      cursor: pointer;
    }
    .group:first-of-type .group-label {
      padding-top: var(--alloy-spacing-scale-4);
    }
    .group-label {
      padding: var(--alloy-spacing-scale-4) var(--alloy-layout-gutter) var(--alloy-spacing-scale-2);
    }
    .nav-item {
      gap: var(--alloy-spacing-scale-3);
      min-height: 56px;
      padding: 0 var(--alloy-layout-gutter);
      border-bottom: 1px solid var(--alloy-color-border-subtle);
      border-radius: 0;
      color: var(--alloy-color-text-primary);
    }
    .nav-item:active {
      background: var(--alloy-color-surface-hover);
    }
    .nav-icon,
    .nav-chevron {
      display: inline-flex;
      flex-shrink: 0;
    }
    .nav-icon {
      color: var(--alloy-color-text-secondary);
    }
    .nav-chevron {
      color: var(--alloy-color-text-muted);
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
      flex-shrink: 0;
      width: 44px;
      height: 44px;
      padding: 0;
      border: 0;
      border-radius: var(--alloy-radii-full);
      background: transparent;
      color: var(--alloy-color-text-primary);
      cursor: pointer;
    }
    .header {
      min-height: 56px;
      padding: 0 var(--alloy-spacing-scale-2) 0 var(--alloy-spacing-scale-4);
    }
    .close {
      display: none;
    }
    .scroll {
      padding-bottom: var(--alloy-safe-area-bottom);
    }
    .column {
      max-width: none;
      margin: 0;
      padding: var(--alloy-spacing-scale-6) var(--alloy-layout-gutter);
    }
  }
  /* One measure, one gutter: the column is a 768px box carrying 24px side padding, so the reading
     width is 720px and the header band's 24px gutter starts every title, heading and row on the
     same line. */
  .column {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-6);
    max-width: 768px;
    margin: 0;
    padding: var(--alloy-spacing-scale-6);
  }
</style>
