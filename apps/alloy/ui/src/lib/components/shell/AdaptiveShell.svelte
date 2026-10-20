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
  import ChangelogDialog from '$lib/components/updates/ChangelogDialog.svelte';

  let paletteOpen = $state(false);
  let settingsOpen = $state(false);
  let settingsSection = $state<'tags' | undefined>();
  function openSettings(section?: 'tags') {
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
    // Needs the server, so it only succeeds when the connection is back; everything that failed meanwhile reloads.
    const stopConnectivity = connectivity.start(() => client.invoke({ action: 'GetTagCatalog' }));
    const stopReconnect = connectivity.onReconnect(() => {
      void tags.load();
      void prefs.load();
      void notificationSettings.load();
    });
    return () => {
      notifier.stop();
      stopReconnect();
      stopConnectivity();
      void roomsStore.stop();
      stopUpdates();
      tags.reset();
      roomsStore.clearReadMarks();
      prefs.signOut();
      teardownPrefs();
    };
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
      <SpaceRail onnew={() => (newRoomOpen = true)} />
      <Sidebar onselect={pick} onnew={() => (newRoomOpen = true)} onbrowse={() => (directoryOpen = true)} onsettings={openSettings} />
    {/if}
  </div>

  {#if breakpoint !== 'narrow'}
    <ResizeHandle
      value={layout.sidebarWidth}
      min={LAYOUT_LIMITS.sidebarWidth.min}
      max={LAYOUT_LIMITS.sidebarWidth.max}
      defaultValue={LAYOUT_LIMITS.sidebarWidth.default}
      label="Resize sidebar"
      edge="end"
      onchange={(v) => layout.set('sidebarWidth', v)}
    />
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
        <h2>Select a conversation</h2>
        <p>Press Ctrl K to jump to one quickly.</p>
      </div>
    {/if}
  </main>

  {#if sideLayout === 'column'}
    <ResizeHandle
      value={layout.detailsWidth}
      min={LAYOUT_LIMITS.detailsWidth.min}
      max={LAYOUT_LIMITS.detailsWidth.max}
      defaultValue={LAYOUT_LIMITS.detailsWidth.default}
      label="Resize side panel"
      edge="start"
      onchange={(v) => layout.set('detailsWidth', v)}
    />
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
<VerificationDialog />
{#if import.meta.env.DEV}<Hud bind:visible={hudVisible} />{/if}

<style>
  .frame {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
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
    grid-template-columns: calc(var(--alloy-layout-rail-width) + var(--sidebar-width, 240px)) 6px minmax(0, 1fr);
    box-sizing: border-box;
    overflow: hidden;
    /* Keep content clear of the status bar, gesture bar and display cutouts on phones. */
    padding: var(--alloy-safe-area-top) var(--alloy-safe-area-right) var(--alloy-safe-area-bottom)
      var(--alloy-safe-area-left);
    background: var(--alloy-color-surface-canvas);
    color: var(--alloy-color-text-primary);
  }
  .shell[data-side='column'] {
    grid-template-columns: calc(var(--alloy-layout-rail-width) + var(--sidebar-width, 240px)) 6px minmax(0, 1fr) 6px var(--details-width, 320px);
  }

  /* The rail (68px) and the sidebar are separate components; this wrapper lays them out. The
     sidebar width covers both, so the rail sits inside the resizable left column. */
  .nav {
    display: grid;
    grid-template-columns: var(--alloy-layout-rail-width) minmax(0, 1fr);
    min-width: 0;
    min-height: 0;
  }

  .chat {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    background: var(--alloy-color-surface-canvas);
  }

  .side {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
    background: var(--alloy-color-surface-panel);
    border-left: 1px solid var(--alloy-color-border-subtle);
  }
  .shell[data-side='overlay'] .side {
    position: absolute;
    top: var(--alloy-spacing-scale-2);
    right: var(--alloy-spacing-scale-2);
    bottom: var(--alloy-spacing-scale-2);
    z-index: 10;
    width: min(var(--details-width, 320px), calc(100% - 2 * var(--alloy-spacing-scale-2)));
    border: 1px solid var(--alloy-color-border-subtle);
    border-radius: var(--alloy-radii-md);
  }

  .thread-host {
    display: grid;
    grid-template-rows: minmax(0, 1fr);
    min-width: 0;
    min-height: 0;
    overflow: hidden;
  }
  .shell[data-thread='column'] .thread-host {
    border-left: 1px solid var(--alloy-color-border-subtle);
  }
  .shell[data-thread='overlay'] .thread-host {
    position: fixed;
    inset: 0;
    z-index: 20;
    padding: var(--alloy-safe-area-top) var(--alloy-safe-area-right) var(--alloy-safe-area-bottom)
      var(--alloy-safe-area-left);
    background: var(--alloy-color-surface-canvas);
  }

  .empty {
    margin: auto;
    text-align: center;
    color: var(--alloy-color-text-secondary);
  }
  .empty h2 {
    margin: 0 0 var(--alloy-spacing-scale-2);
    color: var(--alloy-color-text-primary);
  }

  /* Phone (narrow): one screen at a time. The list screen is the space strip, the room list and the
     bottom tab bar; the chat screen is the conversation. Only inner lists scroll. */
  .shell[data-layout='narrow'] {
    grid-template-columns: minmax(0, 1fr);
    grid-template-rows: minmax(0, 1fr);
    /* The tab bar pads the bottom safe area itself on the list screen. */
    padding-bottom: 0;
  }
  .shell[data-layout='narrow'][data-pane='chat'] {
    padding-bottom: var(--alloy-safe-area-bottom);
  }
  .shell[data-layout='narrow'] .nav {
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .shell[data-layout='narrow'] .list-body {
    display: grid;
    grid-template-rows: minmax(0, 1fr);
    flex: 1 1 0;
    min-height: 0;
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
