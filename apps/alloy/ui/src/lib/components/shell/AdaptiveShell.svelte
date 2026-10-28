<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import SpaceRail from './SpaceRail.svelte';
  import Sidebar from './Sidebar.svelte';
  import ChatHeader from './ChatHeader.svelte';
  import SpaceChips from './SpaceChips.svelte';
  import MobileTabBar from './MobileTabBar.svelte';
  import RoomInfoPanel from './RoomInfoPanel.svelte';
  import RoomSearchPanel from './RoomSearchPanel.svelte';
  import CommandPalette from './CommandPalette.svelte';
  import Timeline from '../timeline/Timeline.svelte';
  import ThreadPanel from '../timeline/ThreadPanel.svelte';
  import Composer from '../composer/Composer.svelte';
  import UploadStrip from '../composer/UploadStrip.svelte';
  import PinnedBar from '../timeline/PinnedBar.svelte';
  import SelectionBar from '../timeline/SelectionBar.svelte';
  import ForwardDialog from '../timeline/ForwardDialog.svelte';
  import DeleteConfirm from '../timeline/DeleteConfirm.svelte';
  import ReportConfirm from '../timeline/ReportConfirm.svelte';
  import LiveLocationBanner from '../timeline/LiveLocationBanner.svelte';
  import { timelineStore } from '$lib/stores/timeline.svelte';
  import { startMuteTimer } from '$lib/stores/muteTimer.svelte';
  import ResizeHandle from '../common/ResizeHandle.svelte';
  import NewRoomDialog from './NewRoomDialog.svelte';
  import DirectoryDialog from './DirectoryDialog.svelte';
  import SettingsDialog from '../settings/SettingsDialog.svelte';
  import VerificationDialog from '../settings/VerificationDialog.svelte';
  import Hud from '../dev/Hud.svelte';
  import ConnectionBanner from './ConnectionBanner.svelte';
  import { connectivity } from '$lib/stores/connectivity.svelte';
  import { client } from '$lib/ipc';
  import { pushBack } from '$lib/platform/backstack';
  import { capabilities, currentEnv } from '$lib/platform/capabilities';
  import { roomsStore, roomTitle } from '$lib/stores/rooms.svelte';
  import { tags } from '$lib/stores/tags.svelte';
  import { layout, LAYOUT_LIMITS } from '$lib/stores/layout.svelte';
  import { ui } from '$lib/stores/ui.svelte';
  import { threadStore } from '$lib/stores/thread.svelte';
  import { verification } from '$lib/stores/verification.svelte';
  import { prefs, installPrefsEffects } from '$lib/stores/prefs.svelte';
  import { notifier } from '$lib/stores/notifier.svelte';
  import { notificationSettings } from '$lib/stores/notificationSettings.svelte';
  import { viewport } from '$lib/stores/viewport.svelte';
  import { updatesStore } from '$lib/stores/updates.svelte';
  import { focusComposer, shouldFocusComposer } from '$lib/platform/typeToFocus';
  import { extractOpenRoomId, setPushPreview, takePendingRoom } from '$lib/platform/push';
  import { pushStore } from '$lib/stores/push.svelte';
  import { refreshStore } from '$lib/stores/refresh.svelte';
  import ChangelogDialog from '$lib/components/updates/ChangelogDialog.svelte';
  import { listen } from '@tauri-apps/api/event';
  import { isTauriShell } from '$lib/platform/notify';
  import PermissionsGate from './PermissionsGate.svelte';
  import Icon from '$lib/icons/Icon.svelte';
  import { Search } from '$lib/icons';
  import { shortcutLabel } from '$lib/platform/shortcuts';

  let paletteOpen = $state(false);
  let settingsOpen = $state(false);
  let settingsSection = $state<'tags' | 'permissions' | undefined>();
  function openSettings(section?: 'tags' | 'permissions') {
    settingsSection = section;
    settingsOpen = true;
  }
  let newRoomOpen = $state(false);
  let directoryOpen = $state(false);
  let pane = $state<'list' | 'chat'>('list');
  let hudVisible = $state(false);
  let innerWidth = $state(1280);

  // ADR 0002 breakpoints: wide >= 1100px, medium 900-1099px, narrow < 900px.
  const breakpoint = $derived<'wide' | 'medium' | 'narrow'>(
    innerWidth >= 1100 ? 'wide' : innerWidth >= 900 ? 'medium' : 'narrow'
  );

  const active = $derived(roomsStore.active);
  const activeId = $derived(roomsStore.active?.id ?? null);
  // An open thread takes the side column (wide) or the full overlay (below 1100) and hides the
  // room info panel while it is open.
  const threadOpen = $derived(Boolean(threadStore.roomId && threadStore.rootEventId));
  const threadMode = $derived<'column' | 'overlay' | 'none'>(
    !threadOpen ? 'none' : breakpoint === 'wide' ? 'column' : 'overlay'
  );

  // Room search and room info share the side slot. Opening one closes the other in the ui store.
  const infoPanel = $derived<'room-search' | 'room-info' | 'none'>(
    !activeId || threadOpen ? 'none' : ui.roomSearchOpen ? 'room-search' : ui.roomInfoOpen ? 'room-info' : 'none'
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

  // The thread and the info slot are mutually exclusive, so at most one of them is the side panel.
  const sideLayout = $derived<'column' | 'overlay' | 'none'>(threadMode !== 'none' ? threadMode : infoMode);

  // Phones (<= 899px) swap the rail for a space strip and add a bottom tab bar.
  const phone = $derived(viewport.isPhone);
  const caps = $derived(capabilities(currentEnv(viewport.isPhone)));
  const tabActive = $derived<'chats' | 'discover' | 'settings'>(
    directoryOpen ? 'discover' : settingsOpen ? 'settings' : 'chats'
  );
  const unreadTotal = $derived(
    roomsStore.rooms.reduce(
      (sum, r) => sum + (r.is_space ? 0 : roomsStore.unreadOf(r)),
      0
    )
  );

  function showChats() {
    directoryOpen = false;
    settingsOpen = false;
  }

  onMount(() => {
    const teardownPrefs = installPrefsEffects();
    void prefs.load();
    verification.listen();
    void roomsStore.start();
    const stopUpdates = updatesStore.start();
    void tags.load();
    void verification.refreshRecovery();
    void notificationSettings.load();
    notifier.start();
    pushStore.start();
    const pendingRoom = takePendingRoom();
    if (pendingRoom) {
      pick(pendingRoom);
    }
    const onOpenRoom = (e: Event) => {
      const roomId = extractOpenRoomId(e instanceof CustomEvent ? e.detail : undefined);
      if (roomId) {
        pick(roomId);
        // The shell has the room now; drop the copy the Android side kept for a page that was not ready.
        takePendingRoom();
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
      void notificationSettings.load();
    });
    return () => {
      window.removeEventListener('alloy-open-room', onOpenRoom);
      disposed = true;
      unlistenNativeRoom?.();
      pushStore.stop();
      notifier.stop();
      stopConnectivity();
      void roomsStore.stop();
      stopUpdates();
      tags.reset();
      roomsStore.clearReadMarks();
      void prefs.signOut();
      teardownPrefs();
    };
  });

  // The notifications Android posts while Alloy is closed follow "Show message preview" too.
  $effect(() => {
    if (pushStore.supported) setPushPreview(prefs.notifyPreview);
  });

  // Timed mutes end on their own; this restores the room's notifications.
  $effect(() => startMuteTimer());

  // A thread belongs to the room it was opened in. Untracked so the effect runs only when the
  // active room changes, not on every thread state update.
  $effect(() => {
    const roomId = activeId;
    untrack(() => {
      if (threadStore.roomId !== null && threadStore.roomId !== roomId) void threadStore.close();
    });
  });

  // Room info and room search belong to the room they were opened for, the same way a thread does.
  $effect(() => {
    void activeId;
    untrack(() => {
      if (ui.roomInfoOpen) ui.closeRoomInfo();
      if (ui.roomSearchOpen) ui.closeRoomSearch();
    });
  });
  // An open thread takes the side slot, so room search closes when a thread opens.
  $effect(() => {
    if (threadOpen && ui.roomSearchOpen) ui.closeRoomSearch();
  });

  // On phones an open thread takes a Back entry, so Back closes the thread before the chat.
  $effect(() => {
    if (!threadOpen || breakpoint !== 'narrow') return;
    return pushBack(() => threadStore.close());
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

  function openChat() {
    pane = 'chat';
    if (breakpoint === 'narrow' && !releaseChatBack) {
      releaseChatBack = pushBack(() => {
        releaseChatBack = null;
        pane = 'list';
      });
    }
  }

  function backToList() {
    if (releaseChatBack) {
      releaseChatBack();
      releaseChatBack = null;
    }
    pane = 'list';
  }

  function pick(roomId: string) {
    showChats();
    roomsStore.select(roomId);
    paletteOpen = false;
    openChat();
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
<ConnectionBanner />
<ChangelogDialog />
<ForwardDialog />
<DeleteConfirm />
<ReportConfirm />
<div
  class="shell"
  data-layout={breakpoint}
  data-pane={pane}
  data-side={sideLayout}
  data-thread={threadMode}
  data-sidebar={layout.sidebarCollapsed ? 'collapsed' : 'expanded'}
  style:--sidebar-width="{layout.sidebarWidth}px"
  style:--details-width="{layout.detailsWidth}px"
>
  <div class="nav">
    {#if phone}
      <SpaceChips onnew={() => (newRoomOpen = true)} />
      <div class="list-body">
        <Sidebar onselect={pick} onnew={() => (newRoomOpen = true)} onbrowse={() => (directoryOpen = true)} onsettings={openSettings} />
      </div>
      <MobileTabBar
        active={tabActive}
        onchats={showChats}
        ondiscover={() => (directoryOpen = true)}
        onsettings={() => openSettings()}
        unread={unreadTotal}
      />
    {:else}
      <SpaceRail
        onsettings={openSettings}
        collapsed={layout.sidebarCollapsed}
        onexpand={() => layout.setSidebarCollapsed(false)}
      />
      <div class="list-body">
        <Sidebar
          onselect={pick}
          onnew={() => (newRoomOpen = true)}
          onbrowse={() => (directoryOpen = true)}
          onsettings={openSettings}
          oncollapse={() => layout.setSidebarCollapsed(true)}
        />
      </div>
    {/if}
  </div>

  {#if breakpoint !== 'narrow' && !layout.sidebarCollapsed}
    <div class="split split-sidebar">
      <ResizeHandle
        value={layout.sidebarWidth}
        min={LAYOUT_LIMITS.sidebarWidth.min}
        max={LAYOUT_LIMITS.sidebarWidth.max}
        defaultValue={LAYOUT_LIMITS.sidebarWidth.default}
        label="Resize sidebar"
        edge="end"
        collapseBelow={LAYOUT_LIMITS.sidebarWidth.min - 40}
        onchange={(v) => layout.set('sidebarWidth', v)}
        oncollapse={() => layout.setSidebarCollapsed(true)}
      />
    </div>
  {/if}

  <main class="chat">
    {#if active && activeId}
      {#if timelineStore.selecting}
        <SelectionBar />
      {:else}
        <ChatHeader room={active} onback={backToList} />
      {/if}
      <PinnedBar roomId={activeId} />
      <LiveLocationBanner />
      <Timeline roomId={activeId} title={roomTitle(active)} isDirect={active.is_direct} encrypted={active.encrypted} />
      <UploadStrip roomId={activeId} />
      <Composer roomId={activeId} />
    {:else}
      <div class="empty">
        <p class="eyebrow">Alloy</p>
        <h2>No conversation open</h2>
        <p>Search every room and person, or pick one from the sidebar.</p>
        <button type="button" class="jump" onclick={() => (paletteOpen = true)}>
          <Icon icon={Search} size={16} />
          <span>Quick switcher</span>
          <kbd>{shortcutLabel('K')}</kbd>
        </button>
      </div>
    {/if}
  </main>

  {#if sideLayout === 'column'}
    <div class="split split-details">
      <ResizeHandle
        value={layout.detailsWidth}
        min={LAYOUT_LIMITS.detailsWidth.min}
        max={LAYOUT_LIMITS.detailsWidth.max}
        defaultValue={LAYOUT_LIMITS.detailsWidth.default}
        label="Resize side panel"
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
  {#if threadMode !== 'none'}
    <div class="thread-host">
      <ThreadPanel onclose={() => void threadStore.close()} />
    </div>
  {/if}
</div>
</div>

<CommandPalette open={paletteOpen} onclose={() => (paletteOpen = false)} onpick={pick} />
<NewRoomDialog open={newRoomOpen} onclose={() => (newRoomOpen = false)} onopened={pick} />
<DirectoryDialog open={directoryOpen} onclose={() => (directoryOpen = false)} onjoined={pick} />
<SettingsDialog open={settingsOpen} initialSection={settingsSection} onclose={() => (settingsOpen = false)} />
<PermissionsGate onopensettings={() => openSettings('permissions')} />
<VerificationDialog />
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
  .shell {
    position: relative;
    flex: 1;
    min-height: 0;
    display: grid;
    /* Chrome and conversation sit side by side between hairlines, so the chat keeps the pixels. */
    grid-template-columns: calc(var(--alloy-layout-rail-width) + var(--sidebar-width, 240px))
      minmax(0, 1fr);
    box-sizing: border-box;
    overflow: hidden;
    /* Keep content clear of the status bar, gesture bar and display cutouts on phones. */
    padding: var(--alloy-safe-area-top) var(--alloy-safe-area-right) var(--alloy-safe-area-bottom)
      var(--alloy-safe-area-left);
    color: var(--alloy-color-text-primary);
  }
  .shell[data-side='column'] {
    grid-template-columns: calc(var(--alloy-layout-rail-width) + var(--sidebar-width, 240px))
      minmax(0, 1fr) var(--details-width, 320px);
  }
  /* The room list collapsed to the rail: the conversation takes the freed pixels. */
  .shell[data-layout='wide'][data-sidebar='collapsed'],
  .shell[data-layout='medium'][data-sidebar='collapsed'] {
    grid-template-columns: var(--alloy-layout-rail-width) minmax(0, 1fr);
  }
  .shell[data-layout='wide'][data-sidebar='collapsed'][data-side='column'],
  .shell[data-layout='medium'][data-sidebar='collapsed'][data-side='column'] {
    grid-template-columns: var(--alloy-layout-rail-width) minmax(0, 1fr) var(--details-width, 320px);
  }
  .shell[data-layout='wide'][data-sidebar='collapsed'] .nav,
  .shell[data-layout='medium'][data-sidebar='collapsed'] .nav {
    grid-template-columns: var(--alloy-layout-rail-width);
  }
  .shell[data-layout='wide'][data-sidebar='collapsed'] .nav .list-body,
  .shell[data-layout='medium'][data-sidebar='collapsed'] .nav .list-body {
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
    left: calc(var(--alloy-layout-rail-width) + var(--sidebar-width, 240px) - 5px);
  }
  .split-details {
    right: calc(var(--details-width, 320px) - 5px);
  }

  /* Navigation chrome: the rail and the room list share one lightly transparent surface. */
  .nav {
    display: grid;
    grid-template-columns: var(--alloy-layout-rail-width) minmax(0, 1fr);
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

  .thread-host {
    display: grid;
    grid-template-rows: minmax(0, 1fr);
    min-width: 0;
    min-height: 0;
    overflow: hidden;
  }
  /* The thread is a third pane between hairlines, not a card floating over the conversation. */
  .shell[data-thread='column'] .thread-host {
    border-left: 1px solid var(--alloy-chat-rim);
    background: var(--alloy-chrome-surface-strong);
    -webkit-backdrop-filter: var(--alloy-chrome-filter);
    backdrop-filter: var(--alloy-chrome-filter);
  }
  .shell[data-thread='overlay'] .thread-host {
    position: fixed;
    inset: 0;
    z-index: 20;
    padding: var(--alloy-safe-area-top) var(--alloy-safe-area-right) var(--alloy-safe-area-bottom)
      var(--alloy-safe-area-left);
    background: var(--alloy-canvas-art);
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
    padding: 3px 6px;
    border: 1px solid var(--alloy-glass-rim);
    border-radius: 5px;
    background: color-mix(in srgb, var(--alloy-color-text-primary) 4%, transparent);
    font-family: inherit;
    font-size: var(--alloy-font-size-meta);
    line-height: 1;
    color: var(--alloy-color-text-muted);
  }

  /* Phone (narrow): one screen at a time. The list screen is the space strip, the room list and the
     bottom tab bar; the chat screen is the conversation. Only inner lists scroll. */
  .shell[data-layout='narrow'] {
    grid-template-columns: minmax(0, 1fr);
    grid-template-rows: minmax(0, 1fr);
    /* The tab bar pads the bottom safe area itself on the list screen. */
    padding: var(--alloy-safe-area-top) var(--alloy-safe-area-right) 0 var(--alloy-safe-area-left);
  }
  .shell[data-layout='narrow'][data-pane='chat'] {
    padding-bottom: var(--alloy-safe-area-bottom);
  }
  /* Phones are edge-to-edge: the canvas runs behind the chrome and the chrome itself is glass.
     The list needs a readable veil over the aurora, not a transparent sheet. */
  .shell[data-layout='narrow'] .nav {
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
    border: 0;
    border-radius: 0;
    background: color-mix(in srgb, var(--alloy-color-surface-canvas) 66%, transparent);
    -webkit-backdrop-filter: var(--alloy-glass-filter);
    backdrop-filter: var(--alloy-glass-filter);
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
  .shell[data-layout='narrow'][data-thread='overlay'] .thread-host {
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
