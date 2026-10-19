<script lang="ts">
  import { Avatar } from '../common';
  import Icon from '$lib/icons/Icon.svelte';
  import { CheckCheck, House, Plus } from '$lib/icons';
  import type { RoomSummary } from '$lib/generated/protocol';
  import { copyItems } from './roomMenus';
  import { contextmenu } from '$lib/platform/contextmenu';
  import type { MenuItem } from '$lib/stores/contextmenu.svelte';
  import { roomsStore, roomTitle } from '$lib/stores/rooms.svelte';
  import { ui } from '$lib/stores/ui.svelte';

  interface Props {
    onnew?: () => void;
  }
  let { onnew }: Props = $props();

  const spaces = $derived(roomsStore.rooms.filter((r) => r.is_space && r.membership === 'Joined'));
  const homeActive = $derived(ui.scope === 'home');

  /** Unread joined rooms inside `space`, or across every room when `space` is null. */
  function unreadRooms(space: string | null) {
    return roomsStore.rooms.filter(
      (r) =>
        !r.is_space &&
        r.membership !== 'Invited' &&
        roomsStore.unreadOf(r) > 0 &&
        (space === null || (roomsStore.spaceParents[r.id] ?? []).includes(space)),
    );
  }

  function markUnread(space: string | null) {
    for (const r of unreadRooms(space)) roomsStore.markRead(r.id);
  }

  function homeMenu(): MenuItem[] {
    return [
      {
        id: 'mark-all',
        label: 'Mark all as read',
        icon: CheckCheck,
        disabled: unreadRooms(null).length === 0,
        onselect: () => markUnread(null),
      },
    ];
  }

  function spaceMenu(space: RoomSummary): MenuItem[] {
    return [
      {
        id: 'mark-space',
        label: 'Mark space as read',
        icon: CheckCheck,
        disabled: unreadRooms(space.id).length === 0,
        onselect: () => markUnread(space.id),
      },
      { separator: true },
      ...copyItems(space),
    ];
  }

  function createMenu(): MenuItem[] {
    return [{ id: 'new-room', label: 'New room...', icon: Plus, onselect: () => onnew?.() }];
  }
</script>

<nav class="rail" aria-label="Spaces">
  <ul class="entries">
    <li>
      <span class="pill" class:shown={homeActive} aria-hidden="true"></span>
      <button
        type="button"
        class="home"
        use:contextmenu={homeMenu}
        aria-label="Home"
        title="Home"
        aria-current={homeActive ? 'true' : undefined}
        onclick={() => (ui.scope = 'home')}
      >
        <Icon icon={House} size={20} />
      </button>
    </li>
    {#each spaces as space (space.id)}
      {@const title = roomTitle(space)}
      {@const active = ui.scope === space.id}
      <li>
        <span class="pill" class:shown={active} aria-hidden="true"></span>
        <button
          type="button"
          class="space"
          use:contextmenu={() => spaceMenu(space)}
          aria-label={title}
          title={title}
          aria-current={active ? 'true' : undefined}
          onclick={() => (ui.scope = space.id)}
        >
          <Avatar userId={space.id} displayName={title} src={space.avatar_url} size={48} shape={active ? 'rounded' : 'circle'} />
        </button>
      </li>
    {/each}
  </ul>
  <button
    type="button"
    class="create"
    use:contextmenu={createMenu}
    aria-label="Create or join"
    title="Create or join"
    onclick={() => onnew?.()}
  >
    <Icon icon={Plus} size={20} />
  </button>
</nav>

<style>
  .rail {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    flex: 1;
    min-height: 0;
    padding: var(--alloy-spacing-scale-3) 0;
    overflow-y: auto;
    background: var(--alloy-color-surface-canvas);
  }
  .entries {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    width: 100%;
  }
  .entries li {
    position: relative;
    display: flex;
    justify-content: center;
    width: 100%;
  }
  .pill {
    position: absolute;
    left: 0;
    top: 50%;
    width: 3px;
    height: 0;
    transform: translateY(-50%);
    border-radius: var(--alloy-radii-full);
    background: var(--alloy-color-accent-primary);
    transition: height 120ms ease;
  }
  .pill.shown {
    height: 24px;
  }
  .home,
  .space,
  .create {
    display: grid;
    place-items: center;
    flex: none;
    width: 44px;
    height: 44px;
    padding: 0;
    border: 0;
    border-radius: var(--alloy-radii-lg);
    background: transparent;
    color: inherit;
    cursor: pointer;
  }
  .home {
    background: var(--alloy-color-accent-fill);
    color: var(--alloy-color-text-on-accent);
  }
  .create {
    background: var(--alloy-color-surface-subtle);
    color: var(--alloy-color-text-secondary);
  }
  .create:hover {
    background: var(--alloy-color-surface-hover);
    color: var(--alloy-color-text-primary);
  }
  .home:focus-visible,
  .space:focus-visible,
  .create:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }
</style>
