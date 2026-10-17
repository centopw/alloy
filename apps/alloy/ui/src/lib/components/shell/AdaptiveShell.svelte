<script lang="ts">
  import { onMount } from 'svelte';
  import { Avatar } from '../common';
  import Icon from '$lib/icons/Icon.svelte';
  import { ChevronLeft } from '$lib/icons';
  import RoomList from './RoomList.svelte';
  import CommandPalette from './CommandPalette.svelte';
  import Timeline from '../timeline/Timeline.svelte';
  import Composer from '../composer/Composer.svelte';
  import NewRoomDialog from './NewRoomDialog.svelte';
  import SettingsDialog from '../settings/SettingsDialog.svelte';
  import VerificationDialog from '../settings/VerificationDialog.svelte';
  import Hud from '../dev/Hud.svelte';
  import { roomsStore, roomTitle } from '$lib/stores/rooms.svelte';
  import { session } from '$lib/stores/session.svelte';
  import { verification } from '$lib/stores/verification.svelte';

  let paletteOpen = $state(false);
  let settingsOpen = $state(false);
  let newRoomOpen = $state(false);
  let pane = $state<'list' | 'chat'>('list');
  let hudVisible = $state(false);

  const active = $derived(roomsStore.active);
  // A string-valued derived: unlike `active` (a new object on every room update) it only changes
  // when the open room changes, so the timeline is not reopened on each unread/preview update.
  const activeId = $derived(roomsStore.active?.id ?? null);

  onMount(() => {
    verification.listen();
    void roomsStore.start();
    void verification.refreshRecovery();
    return () => {
      void roomsStore.stop();
    };
  });

  function pick(roomId: string) {
    roomsStore.select(roomId);
    paletteOpen = false;
    pane = 'chat';
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

<svelte:window onkeydown={onKeydown} />

<div class="shell" data-pane={pane}>
  <nav class="rail" aria-label="Account">
    <div class="brand" aria-hidden="true">A</div>
    <div class="grow"></div>
    <button type="button" class="account" aria-label="Settings" title="Settings" onclick={() => (settingsOpen = true)}>
      <Avatar
        userId={session.account?.user_id}
        displayName={session.account?.display_name ?? session.account?.user_id}
        size="md"
      />
    </button>
  </nav>

  <div class="list-pane">
    <RoomList onselect={pick} onnew={() => (newRoomOpen = true)} />
  </div>

  <main class="chat-pane">
    {#if active && activeId}
      <button type="button" class="back" onclick={() => (pane = 'list')}>
        <Icon icon={ChevronLeft} size={20} />
        <span>Conversations</span>
      </button>
      <Timeline roomId={activeId} title={roomTitle(active)} isDirect={active.is_direct} encrypted={active.encrypted} />
      <Composer roomId={activeId} />
    {:else}
      <div class="empty">
        <h2>Select a conversation</h2>
        <p>Press Ctrl K to jump to one quickly.</p>
      </div>
    {/if}
  </main>
</div>

<CommandPalette open={paletteOpen} onclose={() => (paletteOpen = false)} onpick={pick} />
<NewRoomDialog open={newRoomOpen} onclose={() => (newRoomOpen = false)} onopened={pick} />
<SettingsDialog open={settingsOpen} onclose={() => (settingsOpen = false)} />
<VerificationDialog />
{#if import.meta.env.DEV}<Hud bind:visible={hudVisible} />{/if}

<style>
  .shell {
    display: grid;
    grid-template-columns: 64px minmax(260px, 340px) 1fr;
    height: 100vh;
    background: var(--alloy-color-surface-canvas);
    color: var(--alloy-color-text-primary);
  }
  .rail {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--alloy-spacing-scale-3);
    padding: var(--alloy-spacing-scale-3) 0;
    background: var(--alloy-color-surface-canvas);
    border-right: 1px solid var(--alloy-color-border-subtle);
  }
  .brand {
    width: 40px;
    height: 40px;
    border-radius: var(--alloy-radii-lg);
    background: var(--alloy-color-accent-primary);
    color: var(--alloy-color-text-on-accent);
    display: grid;
    place-items: center;
    font-weight: var(--alloy-typography-weight-bold);
  }
  .grow {
    flex: 1;
  }
  .account {
    border: 0;
    background: transparent;
    padding: 0;
    cursor: pointer;
    border-radius: var(--alloy-radii-full);
  }
  .list-pane {
    min-height: 0;
  }
  .chat-pane {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }
  .back {
    display: none;
  }
  .empty {
    margin: auto;
    text-align: center;
    color: var(--alloy-color-text-secondary);
  }
  .empty h2 {
    color: var(--alloy-color-text-primary);
    margin: 0 0 var(--alloy-spacing-scale-2);
  }

  @media (max-width: 680px) {
    .shell {
      grid-template-columns: 1fr;
    }
    .rail {
      display: none;
    }
    .shell[data-pane='list'] .chat-pane {
      display: none;
    }
    .shell[data-pane='chat'] .list-pane {
      display: none;
    }
    .back {
      display: flex;
      align-items: center;
      gap: var(--alloy-spacing-scale-1);
      border: 0;
      background: var(--alloy-color-surface-panel);
      color: var(--alloy-color-accent-primary);
      font: inherit;
      padding: var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-3);
      cursor: pointer;
    }
  }
</style>
