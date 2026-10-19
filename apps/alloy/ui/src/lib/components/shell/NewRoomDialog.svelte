<script lang="ts">
  import { Modal, Button, Toggle, SegmentedControl, TextField } from '../common';
  import { client } from '$lib/ipc';
  import { roomsStore } from '$lib/stores/rooms.svelte';
  import { errorMessage } from '$lib/stores/errors';

  interface Props {
    open: boolean;
    onclose: () => void;
    onopened: (roomId: string) => void;
  }
  let { open, onclose, onopened }: Props = $props();

  let mode = $state<'create' | 'join'>('create');
  let name = $state('');
  let encrypted = $state(true);
  let target = $state('');
  let busy = $state(false);
  let error = $state<string | null>(null);

  $effect(() => {
    if (open) {
      name = '';
      target = '';
      error = null;
      busy = false;
    }
  });

  async function submit(e: Event) {
    e.preventDefault();
    busy = true;
    error = null;
    try {
      let roomId: string;
      if (mode === 'create') {
        const res = await client.createRoom(name.trim(), encrypted, null);
        if (res.type !== 'RoomCreated') throw new Error('unexpected response');
        roomId = res.payload.room_id;
      } else {
        const res = await client.joinRoom(target.trim());
        if (res.type !== 'RoomJoined') throw new Error('unexpected response');
        roomId = res.payload.room_id;
      }
      roomsStore.select(roomId);
      onopened(roomId);
      onclose();
    } catch (err) {
      error = errorMessage(err);
    } finally {
      busy = false;
    }
  }
</script>

<Modal {open} title="New conversation" {onclose} width="440px">
    <div class="mode">
      <SegmentedControl
        label="Create or join"
        value={mode}
        options={[{ value: 'create', label: 'Create room' }, { value: 'join', label: 'Join room' }]}
        onchange={(v) => (mode = v as 'create' | 'join')}
      />
    </div>
  <form onsubmit={submit}>
    {#if mode === 'create'}
      <label>
        <span>Room name</span>
        <TextField type="text" bind:value={name} required maxlength={100} autocomplete="off" />
      </label>
      <div class="row"><span>End-to-end encrypted</span><Toggle label="End-to-end encrypted" checked={encrypted} onchange={(v) => (encrypted = v)} /></div>
    {:else}
      <label>
        <span>Room address or ID</span>
        <TextField type="text" bind:value={target} placeholder="#room:server.org" required autocomplete="off" autocapitalize="off" spellcheck={false} />
      </label>
    {/if}
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    <div class="actions">
      <Button variant="ghost" onclick={onclose}>Cancel</Button>
      <Button type="submit" disabled={busy || (mode === 'create' ? !name.trim() : !target.trim())}>
        {busy ? 'Working...' : mode === 'create' ? 'Create' : 'Join'}
      </Button>
    </div>
  </form>
</Modal>

<style>
  form {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-3);
    margin-top: var(--alloy-layout-section-gap);
  }
  .mode {
    display: flex;
  }
  .mode > :global(div) {
    flex: 1;
    min-width: 0;
  }
  .mode :global(button) {
    flex: 1;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-2);
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-text-secondary);
  }
  .row {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--alloy-spacing-scale-2);
  }
  .error {
    margin: 0;
    color: var(--alloy-color-status-error);
  }
  @media (max-width: 899px) {
    .row {
      min-height: 44px;
    }
    .actions {
      gap: var(--alloy-spacing-scale-2);
    }
    .actions :global(button) {
      flex: 1;
      min-height: 44px;
    }
  }
</style>
