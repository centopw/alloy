<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import SpaceRail from './SpaceRail.svelte';
  import Sidebar from './Sidebar.svelte';
  import ChatHeader from './ChatHeader.svelte';
  import MembersPanel from './MembersPanel.svelte';
  import RoomInfoPanel from './RoomInfoPanel.svelte';
  import CommandPalette from './CommandPalette.svelte';
  import Timeline from '../timeline/Timeline.svelte';
  import ThreadPanel from '../timeline/ThreadPanel.svelte';
  import Composer from '../composer/Composer.svelte';
  import UploadStrip from '../composer/UploadStrip.svelte';
  import ResizeHandle from '../common/ResizeHandle.svelte';
  import NewRoomDialog from './NewRoomDialog.svelte';
  import DirectoryDialog from './DirectoryDialog.svelte';
  import SettingsDialog from '../settings/SettingsDialog.svelte';
  import VerificationDialog from '../settings/VerificationDialog.svelte';
  import Hud from '../dev/Hud.svelte';
  import { pushBack } from '$lib/platform/backstack';
  import { roomsStore, roomTitle } from '$lib/stores/rooms.svelte';
  import { layout, LAYOUT_LIMITS } from '$lib/stores/layout.svelte';
  import { ui } from '$lib/stores/ui.svelte';
  import { threadStore } from '$lib/stores/thread.svelte';
  import { verification } from '$lib/stores/verification.svelte';

  let paletteOpen = $state(false);
  let settingsOpen = $state(false);
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
  // A string-valued derived: unlike `active` (a new object on every room update) it only changes
  // when the open room changes, so the timeline is not reopened on each unread/preview update.
  const activeId = $derived(roomsStore.active?.id ?? null);

  // An open thread takes the side column (wide) or the full overlay (below 1100) and hides the
  // room info and members panels while it is open.
  const threadOpen = $derived(Boolean(threadStore.roomId && threadStore.rootEventId));
  const threadMode = $derived<'column' | 'overlay' | 'none'>(
    !threadOpen ? 'none' : breakpoint === 'wide' ? 'column' : 'overlay'
  );

  // Room info and members share one side slot; room info wins when both flags are set.
  const infoPanel = $derived<'room-info' | 'members' | 'none'>(
    !activeId || threadOpen ? 'none' : ui.roomInfoOpen ? 'room-info' : ui.membersOpen ? 'members' : 'none'
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

  onMount(() => {
    verification.listen();
    void roomsStore.start();
    void verification.refreshRecovery();
    return () => {
      void roomsStore.stop();
    };
  });

  // A thread belongs to the room it was opened in. Untracked so the effect runs only when the
  // active room changes, not on every thread state update.
  $effect(() => {
    const roomId = activeId;
    untrack(() => {
      if (threadStore.roomId !== null && threadStore.roomId !== roomId) void threadStore.close();
    });
  });

  // Room info belongs to the room it was opened for, the same way a thread does.
  $effect(() => {
    void activeId;
    untrack(() => {
      if (ui.roomInfoOpen) ui.closeRoomInfo();
    });
  });

  // On phones an open thread takes a Back entry, so Back closes the thread before the chat.
  $effect(() => {
    if (!threadOpen || breakpoint !== 'narrow') return;
    return pushBack(() => threadStore.close());
  });

  // On phones an open room info overlay takes a Back entry, so Back closes it before the chat.
  $effect(() => {
    if (breakpoint !== 'narrow' || infoPanel !== 'room-info' || infoMode !== 'overlay') return;
    return pushBack(() => ui.closeRoomInfo());
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
    const mod = e.metaKey || e.ctrlKey;
    if (mod && e.key.toLowerCase() === 'k') {
      e.preventDefault();
      paletteOpen = !paletteOpen;
    } else if (mod && e.key === ',') {
      e.preventDefault();
      settingsOpen = true;
    }
  }
</script>

<svelte:window onkeydown={onKeydown} bind:innerWidth />

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
    <SpaceRail onnew={() => (newRoomOpen = true)} />
    <Sidebar onselect={pick} onnew={() => (newRoomOpen = true)} onbrowse={() => (directoryOpen = true)} onsettings={() => (settingsOpen = true)} />
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
      <ChatHeader room={active} onback={backToList} onsearch={() => (paletteOpen = true)} />
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

  {#if infoMode !== 'none' && activeId}
    <div class="side">
      {#if infoPanel === 'room-info'}
        <RoomInfoPanel roomId={activeId} initialTab={ui.roomInfoTab} onclose={() => ui.closeRoomInfo()} />
      {:else}
        <MembersPanel
          roomId={activeId}
          onclose={infoMode === 'overlay' ? () => (ui.membersOpen = false) : undefined}
        />
      {/if}
    </div>
  {/if}
  {#if threadMode !== 'none'}
    <div class="thread-host">
      <ThreadPanel onclose={() => void threadStore.close()} />
    </div>
  {/if}
</div>

<CommandPalette open={paletteOpen} onclose={() => (paletteOpen = false)} onpick={pick} />
<NewRoomDialog open={newRoomOpen} onclose={() => (newRoomOpen = false)} onopened={pick} />
<DirectoryDialog open={directoryOpen} onclose={() => (directoryOpen = false)} onjoined={pick} />
<SettingsDialog open={settingsOpen} onclose={() => (settingsOpen = false)} />
<VerificationDialog />
{#if import.meta.env.DEV}<Hud bind:visible={hudVisible} />{/if}

<style>
  .shell {
    position: relative;
    display: grid;
    grid-template-columns: var(--sidebar-width, 312px) 6px minmax(0, 1fr);
    height: 100dvh;
    overflow: hidden;
    /* Keep content clear of the status bar, gesture bar and display cutouts on phones. */
    padding: var(--alloy-safe-area-top) var(--alloy-safe-area-right) var(--alloy-safe-area-bottom)
      var(--alloy-safe-area-left);
    background: var(--alloy-color-surface-canvas);
    color: var(--alloy-color-text-primary);
  }
  .shell[data-side='column'] {
    grid-template-columns: var(--sidebar-width, 312px) 6px minmax(0, 1fr) 6px var(--details-width, 340px);
  }

  /* The rail (72px) and the sidebar are separate components; this wrapper lays them out. The
     sidebar width covers both, so the rail sits inside the resizable left column. */
  .nav {
    display: grid;
    grid-template-columns: 72px minmax(0, 1fr);
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
    width: min(var(--details-width, 340px), calc(100% - 2 * var(--alloy-spacing-scale-2)));
    border: 1px solid var(--alloy-color-border-subtle);
    border-radius: var(--alloy-radii-lg);
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

  /* Narrow: one pane at a time. The list pane is the rail plus sidebar; the chat pane is the conversation. */
  .shell[data-layout='narrow'] {
    grid-template-columns: minmax(0, 1fr);
  }
  .shell[data-layout='narrow'] .nav {
    grid-template-columns: 72px minmax(0, 1fr);
  }
  .shell[data-layout='narrow'][data-pane='list'] .chat {
    display: none;
  }
  .shell[data-layout='narrow'][data-pane='chat'] .nav {
    display: none;
  }
  .shell[data-layout='narrow'][data-side='overlay'] .side {
    left: var(--alloy-spacing-scale-2);
    width: auto;
  }
</style>
