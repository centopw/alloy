<script lang="ts">
  import { onMount, tick, untrack } from 'svelte';
  import Sidebar from './Sidebar.svelte';
  import ChatHeader from './ChatHeader.svelte';
  import MobileTabBar from './MobileTabBar.svelte';
  import RoomInfoPanel from './RoomInfoPanel.svelte';
  import RoomSearchPanel from './RoomSearchPanel.svelte';
  import CommandPalette from './CommandPalette.svelte';
  import Timeline from '../timeline/Timeline.svelte';
  import Composer from '../composer/Composer.svelte';
  import UploadStrip from '../composer/UploadStrip.svelte';
  import SelectionBar from '../timeline/SelectionBar.svelte';
  import ForwardDialog from '../timeline/ForwardDialog.svelte';
  import DeleteConfirm from '../timeline/DeleteConfirm.svelte';
  import { timelineStore } from '$lib/stores/timeline.svelte';
  import { startMuteTimer } from '$lib/stores/muteTimer.svelte';
  import ResizeHandle from '../common/ResizeHandle.svelte';
  import SettingsDialog from '../settings/SettingsDialog.svelte';
  import Hud from '../dev/Hud.svelte';
  import ConnectionBanner from './ConnectionBanner.svelte';
  import { connectivity } from '$lib/stores/connectivity.svelte';
  import { client } from '$lib/ipc';
  import { pushBack } from '$lib/platform/backstack';
  import { capabilities, currentEnv } from '$lib/platform/capabilities';
  import { roomsStore, roomTitle } from '$lib/stores/rooms.svelte';
  import { tags } from '$lib/stores/tags.svelte';
  import { roomInfo } from '$lib/stores/roomInfo.svelte';
  import { layout, LAYOUT_LIMITS } from '$lib/stores/layout.svelte';
  import { ui } from '$lib/stores/ui.svelte';
  import { prefs, installPrefsEffects } from '$lib/stores/prefs.svelte';
  import { notifier } from '$lib/stores/notifier.svelte';
  import { viewport } from '$lib/stores/viewport.svelte';
  import { updatesStore } from '$lib/stores/updates.svelte';
  import { focusComposer, shouldFocusComposer } from '$lib/platform/typeToFocus';
  import { refreshStore } from '$lib/stores/refresh.svelte';
  import ChangelogDialog from '$lib/components/updates/ChangelogDialog.svelte';
  import { listen } from '@tauri-apps/api/event';
  import { isTauriShell } from '$lib/platform/notify';
  import PermissionsGate from './PermissionsGate.svelte';
  import Icon from '$lib/icons/Icon.svelte';
  import { Search } from '$lib/icons';
  import { shortcutLabel } from '$lib/platform/shortcuts';
  import { focusChatEntry, focusRoomList, focusRoomRow, focusSkipTarget } from '$lib/platform/shellFocus';
  import { applyWindowTitle } from '$lib/platform/windowTitle';

  import { t } from '$lib/i18n';
  let paletteOpen = $state(false);
  let settingsOpen = $state(false);
  let settingsSection = $state<'tags' | 'permissions' | undefined>();
  function openSettings(section?: 'tags' | 'permissions') {
    settingsSection = section;
    settingsOpen = true;
  }
  let pane = $state<'list' | 'chat'>('list');
  // Focus bookkeeping for the phone's one-screen-at-a-time layout: the room the chat was opened
  // from (so Back can return to its row) and whether the window is focused (the window title only
  // carries the unread count while the app is in the background).
  let chatOriginId = $state<string | null>(null);
  let windowFocused = $state(typeof document === 'undefined' ? true : document.hasFocus());
  let hudVisible = $state(false);
  let innerWidth = $state(1280);

  // ADR 0002 breakpoints: wide >= 1100px, medium 900-1099px, narrow < 900px.
  const breakpoint = $derived<'wide' | 'medium' | 'narrow'>(
    innerWidth >= 1100 ? 'wide' : innerWidth >= 900 ? 'medium' : 'narrow'
  );

  const active = $derived(roomsStore.active);
  const activeId = $derived(roomsStore.active?.id ?? null);

  // Room search and room info share the side slot. Opening one closes the other in the ui store.
  const infoPanel = $derived<'room-search' | 'room-info' | 'none'>(
    !activeId ? 'none' : ui.roomSearchOpen ? 'room-search' : ui.roomInfoOpen ? 'room-info' : 'none'
  );
  // Wide: a side column. Medium: a rounded overlay over the chat. Narrow: the same overlay, shown
  // only while a chat is open (the list pane has no chat).
  const infoMode = $derived<'column' | 'overlay' | 'none'>(
    infoPanel === 'none'
      ? 'none'
      : breakpoint === 'wide'
        ? 'column'
        : breakpoint === 'medium' || pane === 'chat'
          ? 'overlay'
          : 'none'
  );

  // Phones (<= 899px) show one pane at a time and add a bottom tab bar.
  const phone = $derived(viewport.isPhone);
  const caps = $derived(capabilities(currentEnv(viewport.isPhone)));
  const tabActive = $derived<'chats' | 'settings'>(settingsOpen ? 'settings' : 'chats');
  const unreadTotal = $derived(roomsStore.rooms.reduce((sum, r) => sum + roomsStore.unreadOf(r), 0));

  // Skip links: only offer a pane that is actually on screen. On phones the list and the
  // conversation replace each other, so the link to the hidden pane is not rendered at all. The
  // desktop conversation pane is always mounted (it carries the empty state when no room is open).
  const roomListVisible = $derived(!phone ? !layout.sidebarCollapsed : pane === 'list');
  const conversationVisible = $derived(phone ? pane === 'chat' : true);

  // The window/task-switcher title names the open room and shows unread messages only while the
  // app is in the background (a focused window already shows its badges).
  $effect(() => {
    if (typeof document === 'undefined') return;
    applyWindowTitle({
      roomName: active ? roomTitle(active) : null,
      unreadCount: unreadTotal,
      focused: windowFocused,
      appName: t('brand-name'),
    });
  });

  function showChats() {
    settingsOpen = false;
  }

  onMount(() => {
    // The window title carries unread messages only while the window is in the background.
    const syncWindowFocus = () => {
      windowFocused = document.hasFocus() && !document.hidden;
    };
    syncWindowFocus();
    window.addEventListener('focus', syncWindowFocus);
    window.addEventListener('blur', syncWindowFocus);
    document.addEventListener('visibilitychange', syncWindowFocus);
    const teardownPrefs = installPrefsEffects();
    void prefs.load();
    void roomsStore.start();
    const stopUpdates = updatesStore.start();
    void tags.load();
    notifier.start();
    // Notification clicks name the chat to open: a DOM event from web notifications, a Tauri event
    // from desktop ones. Both carry the chat id as a string.
    const onOpenRoom = (e: Event) => {
      const detail = e instanceof CustomEvent ? e.detail : undefined;
      const roomId = typeof detail === 'string' ? detail.trim() : '';
      if (roomId) {
        pick(roomId);
      }
    };
    window.addEventListener('alloy-open-room', onOpenRoom);
    // Desktop notification clicks come from the Rust side as a Tauri event, not a DOM event.
    // `disposed` covers teardown before `listen` resolves: the late unlisten is called at once.
    let disposed = false;
    let unlistenNativeRoom: (() => void) | null = null;
    if (isTauriShell()) {
      listen<string>('alloy-open-room', (e) => {
        onOpenRoom(new CustomEvent('alloy-open-room', { detail: e.payload }));
      })
        .then((fn) => {
          if (disposed) fn();
          else unlistenNativeRoom = fn;
        })
        .catch((err: unknown) => {
          console.warn('[shell] could not listen for notification clicks', err);
        });
    }
    const stopConnectivity = connectivity.start(() => client.invoke({ action: 'GetTagCatalog' }));
    const stopReconnect = connectivity.onReconnect(() => {
      void tags.load();
      void prefs.load();
    });
    return () => {
      window.removeEventListener('focus', syncWindowFocus);
      window.removeEventListener('blur', syncWindowFocus);
      document.removeEventListener('visibilitychange', syncWindowFocus);
      window.removeEventListener('alloy-open-room', onOpenRoom);
      disposed = true;
      unlistenNativeRoom?.();
      notifier.stop();
      stopConnectivity();
      void roomsStore.stop();
      stopUpdates();
      tags.reset();
      roomInfo.reset();
      // Cached chat timelines belong to the signed-in account, not to the install.
      timelineStore.clearCache();
      roomsStore.clearReadMarks();
      teardownPrefs();
    };
  });

  // Timed mutes end on their own; this restores the chat's notifications.
  $effect(() => startMuteTimer());

  // Room info and room search belong to the room they were opened for.
  $effect(() => {
    void activeId;
    untrack(() => {
      if (ui.roomInfoOpen) ui.closeRoomInfo();
      if (ui.roomSearchOpen) ui.closeRoomSearch();
    });
  });

  // On phones an open room info or room search overlay takes a Back entry, so Back closes it before the chat.
  $effect(() => {
    if (breakpoint !== 'narrow' || infoMode !== 'overlay') return;
    if (infoPanel === 'room-search') return pushBack(() => ui.closeRoomSearch());
    if (infoPanel === 'room-info') return pushBack(() => ui.closeRoomInfo());
    return;
  });

  // On phones the system Back button returns from a chat to the room list instead of leaving the app.
  let releaseChatBack: (() => void) | null = null;

  /**
   * Opens the conversation. On a phone the list is hidden with `display: none`, which drops the
   * focused room row out of the page, so focus moves to the room heading (or the composer when no
   * heading is rendered). TalkBack announces the new screen when the heading takes focus.
   */
  async function openChat(roomId: string) {
    pane = 'chat';
    chatOriginId = roomId;
    if (breakpoint === 'narrow' && !releaseChatBack) {
      releaseChatBack = pushBack(() => {
        releaseChatBack = null;
        void backToList();
      });
    }
    if (breakpoint !== 'narrow') return;
    await tick();
    focusChatEntry();
  }

  /** Returns from the conversation to the phone room list and puts focus back on its room row. */
  async function backToList() {
    const origin = chatOriginId;
    if (releaseChatBack) {
      releaseChatBack();
      releaseChatBack = null;
    }
    pane = 'list';
    if (breakpoint !== 'narrow' || origin === null) return;
    await tick();
    focusRoomRow(origin);
  }

  function pick(roomId: string) {
    showChats();
    roomsStore.select(roomId);
    paletteOpen = false;
    void openChat(roomId);
  }

  /** Skip links: move focus to a landmark instead of relying on the hash-scroll behaviour. */
  function skipTo(id: string) {
    focusSkipTarget(id);
  }

  function onKeydown(e: KeyboardEvent) {
    // Typing anywhere in a conversation starts a message; the keystroke follows focus into the field.
    if (activeId && !paletteOpen && shouldFocusComposer(e)) focusComposer();
    const mod = e.metaKey || e.ctrlKey;
    if (mod && e.key.toLowerCase() === 'k') {
      e.preventDefault();
      paletteOpen = !paletteOpen;
    } else if (mod && e.key.toLowerCase() === 'f' && activeId && caps.keyboardShortcuts) {
      e.preventDefault();
      ui.toggleRoomSearch();
    } else if (mod && e.key === ',') {
      e.preventDefault();
      openSettings();
    } else if ((mod && e.key.toLowerCase() === 'r') || e.key === 'F5') {
      // A page reload would drop the engine connection state; refresh sync and the open chat instead.
      e.preventDefault();
      void refreshStore.run();
    }
  }
</script>

<svelte:window onkeydown={onKeydown} bind:innerWidth />

<div class="frame" class:offline={connectivity.offline}>
{#if conversationVisible || roomListVisible}
  <!-- First Tab stop: keyboard users jump over the navigation entirely (A11Y-06). Each link is
       only rendered while the pane it targets is actually on screen; phones show one pane at a
       time. The links are visually hidden until focused so they add no chrome to the shell. -->
  <nav class="skip-links" aria-label={t('adaptive-shell-skip-links')}>
    {#if conversationVisible}
      <a
        class="skip-link"
        href="#conversation"
        onclick={(e) => {
          e.preventDefault();
          skipTo('conversation');
        }}>{t('adaptive-shell-skip-to-conversation')}</a>
    {/if}
    {#if roomListVisible}
      <a
        class="skip-link"
        href="#room-list"
        onclick={(e) => {
          e.preventDefault();
          focusRoomList();
        }}>{t('adaptive-shell-skip-to-room-list')}</a>
    {/if}
  </nav>
{/if}
<ConnectionBanner />
<ChangelogDialog />
<ForwardDialog />
<DeleteConfirm />
<div
  class="shell"
  data-layout={breakpoint}
  data-pane={pane}
  data-side={infoMode}
  data-sidebar={layout.sidebarCollapsed ? 'collapsed' : 'expanded'}
  style:--sidebar-width="{layout.sidebarWidth}px"
  style:--details-width="{layout.detailsWidth}px"
>
  <div class="nav">
    <div class="list-body" id="room-list" tabindex="-1">
      <Sidebar onselect={pick} onsettings={openSettings} />
    </div>
    {#if phone}
      <MobileTabBar
        active={tabActive}
        onchats={showChats}
        onsettings={() => openSettings()}
        unread={unreadTotal}
      />
    {/if}
  </div>

  {#if breakpoint !== 'narrow'}
    <!-- The handle stays mounted while the list is hidden: dragging the left edge opens the
         list again, so collapsing is never a one-way door. -->
    <div class="split split-sidebar">
      <ResizeHandle
        value={layout.sidebarWidth}
        min={LAYOUT_LIMITS.sidebarWidth.min}
        max={LAYOUT_LIMITS.sidebarWidth.max}
        defaultValue={LAYOUT_LIMITS.sidebarWidth.default}
        label={t('adaptive-shell-resize-sidebar')}
        edge="end"
        collapseBelow={LAYOUT_LIMITS.sidebarWidth.min - 40}
        collapsed={layout.sidebarCollapsed}
        onexpand={() => layout.setSidebarCollapsed(false)}
        onchange={(v) => {
          if (layout.sidebarCollapsed) layout.setSidebarCollapsed(false);
          layout.set('sidebarWidth', v);
        }}
        oncollapse={() => layout.setSidebarCollapsed(true)}
      />
    </div>
  {/if}

  <main class="chat" id="conversation" tabindex="-1">
    {#if active && activeId}
      {#if timelineStore.selecting}
        <!-- Selection replaces the header; keep the view's h1 in the outline for the duration. -->
        <h1 class="visually-hidden">{roomTitle(active)}</h1>
        <SelectionBar />
      {:else}
        <ChatHeader room={active} onback={backToList} />
      {/if}
      <Timeline roomId={activeId} title={roomTitle(active)} isDirect={active.is_direct} />
      <UploadStrip roomId={activeId} />
      <Composer roomId={activeId} />
    {:else}
      <div class="empty">
        <h1 class="eyebrow">{t('brand-name')}</h1>
        <h2>{t('adaptive-shell-no-conversation-open')}</h2>
        <p>{t('adaptive-shell-search-every-room-and-person-or-pick-one-from-th')}</p>
        <button type="button" class="jump" onclick={() => (paletteOpen = true)}>
          <Icon icon={Search} size={16} />
          <span>{t('adaptive-shell-quick-switcher')}</span>
          <kbd>{shortcutLabel('K')}</kbd>
        </button>
      </div>
    {/if}
  </main>

  {#if infoMode === 'column'}
    <div class="split split-details">
      <ResizeHandle
        value={layout.detailsWidth}
        min={LAYOUT_LIMITS.detailsWidth.min}
        max={LAYOUT_LIMITS.detailsWidth.max}
        defaultValue={LAYOUT_LIMITS.detailsWidth.default}
        label={t('adaptive-shell-resize-side-panel')}
        edge="start"
        onchange={(v) => layout.set('detailsWidth', v)}
      />
    </div>
  {/if}

  {#if infoMode !== 'none' && active && activeId}
    <div class="side">
      {#if infoPanel === 'room-search'}
        <RoomSearchPanel roomId={activeId} title={roomTitle(active)} onclose={() => ui.closeRoomSearch()} />
      {:else}
        <RoomInfoPanel roomId={activeId} initialTab={ui.roomInfoTab} onclose={() => ui.closeRoomInfo()} onsearch={() => ui.openRoomSearch()} />
      {/if}
    </div>
  {/if}
</div>
</div>

<CommandPalette open={paletteOpen} onclose={() => (paletteOpen = false)} onpick={pick} />
<SettingsDialog open={settingsOpen} initialSection={settingsSection} onclose={() => (settingsOpen = false)} />
<PermissionsGate onopensettings={() => openSettings('permissions')} />
{#if import.meta.env.DEV}<Hud bind:visible={hudVisible} />{/if}

<style>
  /* The canvas is the material source: floating glass panes refract this wash (ADR 0014). */
  .frame {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    background: var(--alloy-canvas-art);
    background-attachment: fixed;
  }
  /* The offline banner already clears the status bar, so the shell does not add the inset twice. */
  .frame.offline .shell {
    padding-top: 0;
  }
  /* First Tab stop (A11Y-06): off-screen until focused, so keyboard users can jump over the
     navigation without the links adding chrome for pointer users. */
  .skip-links {
    position: fixed;
    top: var(--alloy-spacing-scale-2);
    left: var(--alloy-spacing-scale-2);
    z-index: 30;
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-1);
    transform: translateY(calc(-100% - var(--alloy-spacing-scale-8)));
    transition: transform var(--alloy-motion-duration-fast) var(--alloy-motion-easing-ease);
  }
  .skip-links:focus-within {
    transform: none;
  }
  .skip-link {
    display: inline-flex;
    align-items: center;
    min-height: var(--alloy-touch-target, 44px);
    padding: 0 var(--alloy-spacing-scale-4);
    border: 1px solid var(--alloy-glass-rim-strong);
    border-radius: var(--alloy-radii-lg);
    background: var(--alloy-glass-fill-strong);
    -webkit-backdrop-filter: var(--alloy-glass-filter);
    backdrop-filter: var(--alloy-glass-filter);
    box-shadow: var(--alloy-glass-shadow), var(--alloy-glass-highlight);
    color: var(--alloy-color-text-primary);
    font-size: var(--alloy-font-size-row);
    font-weight: var(--alloy-typography-weight-semibold);
    text-decoration: none;
  }
  .skip-link:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }
  /* A heading that only the accessibility tree sees (selection mode keeps the view's h1). */
  .visually-hidden {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
    white-space: nowrap;
    border: 0;
  }
  /* Skip-link targets are focused programmatically: no ring for pointer clicks, but a visible one
     when the focus came from the keyboard. */
  .chat:focus,
  .list-body:focus {
    outline: none;
  }
  .chat:focus-visible,
  .list-body:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: -2px;
  }
  @media (prefers-reduced-motion: reduce) {
    .skip-links {
      transition: none;
    }
  }
  .shell {
    position: relative;
    flex: 1;
    min-height: 0;
    display: grid;
    /* Chrome and conversation sit side by side between hairlines, so the chat keeps the pixels. */
    grid-template-columns: var(--sidebar-width, 240px) minmax(0, 1fr);
    box-sizing: border-box;
    overflow: hidden;
    /* Keep content clear of the status bar, gesture bar and display cutouts on phones. */
    padding: var(--alloy-safe-area-top) var(--alloy-safe-area-right) var(--alloy-safe-area-bottom)
      var(--alloy-safe-area-left);
    color: var(--alloy-color-text-primary);
  }
  .shell[data-side='column'] {
    grid-template-columns: var(--sidebar-width, 240px) minmax(0, 1fr) var(--details-width, 320px);
  }
  /* The chat list collapsed: the conversation takes the freed pixels. */
  .shell[data-layout='wide'][data-sidebar='collapsed'],
  .shell[data-layout='medium'][data-sidebar='collapsed'] {
    grid-template-columns: 0 minmax(0, 1fr);
  }
  .shell[data-layout='wide'][data-sidebar='collapsed'][data-side='column'],
  .shell[data-layout='medium'][data-sidebar='collapsed'][data-side='column'] {
    grid-template-columns: 0 minmax(0, 1fr) var(--details-width, 320px);
  }
  .shell[data-layout='wide'][data-sidebar='collapsed'] .nav,
  .shell[data-layout='medium'][data-sidebar='collapsed'] .nav {
    display: none;
  }
  /* Handles straddle the hairline; the wrapper gives them a hit area without a gutter between panes. */
  .split {
    position: absolute;
    top: 0;
    bottom: 0;
    display: flex;
    width: 10px;
    z-index: 8;
  }
  .split-sidebar {
    left: calc(var(--sidebar-width, 240px) - 5px);
  }
  /* Hidden list: the handle sits on the left edge, the boundary the drag collapsed to, so the
     same edge that closed the list is the one that opens it. */
  .shell[data-layout='wide'][data-sidebar='collapsed'] .split-sidebar,
  .shell[data-layout='medium'][data-sidebar='collapsed'] .split-sidebar {
    left: 0;
  }
  .split-details {
    right: calc(var(--details-width, 320px) - 5px);
  }

  /* Navigation chrome: the chat list surface. */
  .nav {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    min-width: 0;
    min-height: 0;
    overflow: visible;
    border-right: 1px solid var(--alloy-chat-rim);
    background: var(--alloy-chrome-surface);
    -webkit-backdrop-filter: var(--alloy-chrome-filter);
    backdrop-filter: var(--alloy-chrome-filter);
  }
  .list-body {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
  }

  /* Conversation: the only near-opaque surface, because reading must not depend on the wash. */
  .chat {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
    --alloy-layout-gutter: 12px;
    background: var(--alloy-chat-surface);
    -webkit-backdrop-filter: var(--alloy-chat-filter);
    backdrop-filter: var(--alloy-chat-filter);
  }

  .side {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
    border-left: 1px solid var(--alloy-chat-rim);
    background: var(--alloy-chrome-surface-strong);
    -webkit-backdrop-filter: var(--alloy-chrome-filter);
    backdrop-filter: var(--alloy-chrome-filter);
  }
  .shell[data-side='overlay'] .side {
    position: absolute;
    top: var(--alloy-shell-edge);
    right: var(--alloy-shell-edge);
    bottom: var(--alloy-shell-edge);
    z-index: 10;
    width: min(var(--details-width, 320px), calc(100% - 2 * var(--alloy-shell-edge)));
    border: 1px solid var(--alloy-glass-rim);
    border-radius: var(--alloy-glass-radius-panel);
    box-shadow: var(--alloy-glass-shadow);
  }

  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--alloy-spacing-scale-2);
    margin: auto;
    padding: var(--alloy-spacing-scale-7);
    max-width: 420px;
    text-align: center;
    color: var(--alloy-color-text-secondary);
  }
  .empty .eyebrow {
    margin: 0 0 var(--alloy-spacing-scale-1);
    font-size: var(--alloy-font-size-meta);
    font-weight: var(--alloy-typography-weight-semibold);
    letter-spacing: 0.16em;
    text-transform: uppercase;
    color: var(--alloy-color-accent-primary);
  }
  .empty h2 {
    margin: 0;
    font-size: var(--alloy-typography-size-xxl);
    font-weight: var(--alloy-typography-weight-semibold);
    letter-spacing: -0.02em;
    color: var(--alloy-color-text-primary);
  }
  .empty p {
    margin: 0;
    max-width: 34ch;
    font-size: var(--alloy-typography-size-base);
    line-height: var(--alloy-typography-line-height-normal);
  }
  .jump {
    display: inline-flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    height: 40px;
    margin-top: var(--alloy-spacing-scale-3);
    padding: 0 var(--alloy-spacing-scale-2) 0 var(--alloy-spacing-scale-4);
    border: 1px solid var(--alloy-glass-rim);
    border-radius: var(--alloy-radii-full);
    background: var(--alloy-glass-control);
    color: var(--alloy-color-text-secondary);
    font: inherit;
    font-size: var(--alloy-font-size-row);
    cursor: pointer;
    box-shadow: var(--alloy-glass-highlight);
    transition:
      background var(--alloy-motion-duration-fast) var(--alloy-motion-easing-ease),
      border-color var(--alloy-motion-duration-fast) var(--alloy-motion-easing-ease),
      color var(--alloy-motion-duration-fast) var(--alloy-motion-easing-ease);
  }
  .jump:hover {
    border-color: var(--alloy-glass-rim-strong);
    background: var(--alloy-glass-control-hover);
    color: var(--alloy-color-text-primary);
  }
  .jump:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }
  .jump kbd {
    padding: 3px 6px; /* token-audit: kbd chip inset, set by the 10.5px meta text */
    border: 1px solid var(--alloy-glass-rim);
    border-radius: var(--alloy-radii-sm);
    background: color-mix(in srgb, var(--alloy-color-text-primary) 4%, transparent);
    font-family: inherit;
    font-size: var(--alloy-font-size-meta);
    line-height: 1;
    color: var(--alloy-color-text-muted);
  }

  /* Phone (narrow): one screen at a time. The list screen is the chat list and the bottom tab bar;
     the chat screen is the conversation. Only inner lists scroll. */
  .shell[data-layout='narrow'] {
    grid-template-columns: minmax(0, 1fr);
    grid-template-rows: minmax(0, 1fr);
    /* The tab bar pads the bottom safe area itself on the list screen. */
    padding: var(--alloy-safe-area-top) var(--alloy-safe-area-right) 0 var(--alloy-safe-area-left);
  }
  .shell[data-layout='narrow'][data-pane='chat'] {
    padding-bottom: var(--alloy-safe-area-bottom);
  }
  /* Phones are edge-to-edge, but the list still belongs to the chrome layer (ADR 0014):
     the same lightly transparent panel and 26px chrome filter as the desktop panes. The
     earlier canvas-at-66% veil measured 3.73:1 for muted meta over the brightest bloom. */
  .shell[data-layout='narrow'] .nav {
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
    border: 0;
    border-radius: 0;
    box-shadow: none;
  }
  /* Phones get the roomier phone gutter back; the desktop 12px is for a narrow chat column. */
  .shell[data-layout='narrow'] .chat {
    --alloy-layout-gutter: var(--alloy-spacing-scale-4);
  }
  .shell[data-layout='narrow'] .list-body {
    display: grid;
    grid-template-rows: minmax(0, 1fr);
    flex: 1 1 0;
    min-height: 0;
    border: 0;
    border-radius: 0;
    background: none;
    -webkit-backdrop-filter: none;
    backdrop-filter: none;
    box-shadow: none;
  }
  .shell[data-layout='narrow'][data-pane='list'] .chat {
    display: none;
  }
  .shell[data-layout='narrow'][data-pane='chat'] .nav {
    display: none;
  }
  /* Room info takes the whole screen, sliding in from the right. */
  .shell[data-layout='narrow'][data-side='overlay'] .side {
    position: fixed;
    inset: 0;
    z-index: 20;
    width: auto;
    border: 0;
    border-radius: 0;
    background: var(--alloy-glass-sheen), var(--alloy-glass-fill-strong);
    -webkit-backdrop-filter: var(--alloy-glass-filter-strong);
    backdrop-filter: var(--alloy-glass-filter-strong);
    box-shadow: none;
    padding: var(--alloy-safe-area-top) var(--alloy-safe-area-right) var(--alloy-safe-area-bottom)
      var(--alloy-safe-area-left);
    animation: alloy-panel-in var(--alloy-motion-duration-normal) ease-out;
  }
  @keyframes alloy-panel-in {
    from {
      transform: translateX(100%);
    }
    to {
      transform: none;
    }
  }
</style>
