<script lang="ts">
  import { Avatar } from '../common';
  import Icon from '$lib/icons/Icon.svelte';
  import { CheckCheck, Copy, House, PanelLeft, Settings } from '$lib/icons';
  import type { RoomSummary } from '$lib/generated/protocol';
  import { copyItems } from './roomMenus';
  import { contextmenu } from '$lib/platform/contextmenu';
  import type { MenuItem } from '$lib/stores/contextmenu.svelte';
  import { roomsStore, roomTitle } from '$lib/stores/rooms.svelte';
  import { session } from '$lib/stores/session.svelte';
  import { profileCard } from '$lib/stores/profileCard.svelte';
  import { ui } from '$lib/stores/ui.svelte';

  interface Props {
    onsettings?: () => void;
    /** True while the room list is hidden; the rail then offers the way back. */
    collapsed?: boolean;
    onexpand?: () => void;
  }
  let { onsettings, collapsed = false, onexpand }: Props = $props();

  const spaces = $derived(roomsStore.rooms.filter((r) => r.is_space && r.membership === 'Joined'));
  const homeActive = $derived(ui.scope === 'home');
  const account = $derived(session.account);
  const accountName = $derived(account?.display_name?.trim() || account?.user_id || '');

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

  // The rail answers "where is something waiting?" before the list has to.
  const homeUnread = $derived(unreadRooms(null).length);

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

  function accountMenu(userId: string): MenuItem[] {
    return [
      { id: 'settings', label: 'Settings', icon: Settings, onselect: () => onsettings?.() },
      { separator: true },
      { id: 'copy-user-id', label: 'Copy user ID', icon: Copy, onselect: () => void navigator.clipboard?.writeText(userId).catch(() => undefined) },
    ];
  }

  /** Opens the signed-in user's profile card beside the avatar. */
  function openAccount(e: MouseEvent) {
    if (!account) return;
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    profileCard.open(account.user_id, null, { x: r.right + 10, y: r.top });
  }
</script>

<nav class="rail" aria-label="Spaces">
  <div class="brand" aria-hidden="true"><span class="brand-mark">A</span></div>

  <ul class="entries">
    <li>
      <span class="pill" class:shown={homeActive} aria-hidden="true"></span>
      <button
        type="button"
        class="home"
        class:active={homeActive}
        use:contextmenu={homeMenu}
        aria-label={homeUnread > 0 ? `Home, ${homeUnread} unread` : 'Home'}
        title="Home"
        aria-current={homeActive ? 'true' : undefined}
        onclick={() => (ui.scope = 'home')}
      >
        <Icon icon={House} size={20} />
        {#if homeUnread > 0}
          <span class="unread-dot" aria-hidden="true"></span>
        {/if}
      </button>
    </li>
    {#each spaces as space (space.id)}
      {@const title = roomTitle(space)}
      {@const active = ui.scope === space.id}
      {@const unread = unreadRooms(space.id).length}
      <li>
        <span class="pill" class:shown={active} aria-hidden="true"></span>
        <button
          type="button"
          class="space"
          use:contextmenu={() => spaceMenu(space)}
          aria-label={unread > 0 ? `${title}, ${unread} unread` : title}
          title={title}
          aria-current={active ? 'true' : undefined}
          onclick={() => (ui.scope = space.id)}
        >
          <Avatar userId={space.id} displayName={title} src={space.avatar_url} size={36} shape={active ? 'rounded' : 'circle'} />
          {#if unread > 0}
            <span class="unread-dot" aria-hidden="true"></span>
          {/if}
        </button>
      </li>
    {/each}
  </ul>

  {#if account}
    <div class="foot">
      <span class="foot-rule" aria-hidden="true"></span>
      {#if collapsed && onexpand}
        <button
          type="button"
          class="foot-btn"
          aria-label="Show room list"
          title="Show room list"
          onclick={() => onexpand?.()}
        >
          <Icon icon={PanelLeft} size={20} />
        </button>
      {/if}
      <button
        type="button"
        class="foot-btn"
        aria-label="Settings"
        title="Settings"
        onclick={() => onsettings?.()}
      >
        <Icon icon={Settings} size={20} />
      </button>
      <button
        type="button"
        class="foot-btn me"
        aria-label="Your account"
        title={accountName}
        use:contextmenu={() => accountMenu(account.user_id)}
        onclick={openAccount}
      >
        <Avatar userId={account.user_id} displayName={accountName} src={account.avatar_url} size={32} />
      </button>
    </div>
  {/if}
</nav>

<style>
  /* The rail is the spine of the shell: it shares the navigation pane's surface and is separated
     from the room list by a hairline, not by its own card. Identity, spaces, account; the middle is
     allowed to be quiet, because spaces are something you add, not a checklist to fill. */
  .rail {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--alloy-spacing-scale-1);
    min-height: 0;
    height: 100%;
    padding: var(--alloy-spacing-scale-2) 0;
    overflow: hidden;
    border-right: 1px solid var(--alloy-chat-rim);
  }
  .brand {
    display: grid;
    place-items: center;
    flex: none;
    width: 100%;
    height: 34px;
  }
  .brand-mark {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border-radius: var(--alloy-radii-lg);
    background: linear-gradient(145deg, var(--alloy-color-accent-secondary), var(--alloy-color-accent-primary));
    color: var(--alloy-color-text-on-accent);
    font-size: var(--alloy-font-size-row);
    font-weight: var(--alloy-typography-weight-bold);
    letter-spacing: -0.02em;
    line-height: 1;
    box-shadow:
      inset 0 1px 0 rgba(255, 255, 255, 0.35),
      0 6px 14px -8px color-mix(in srgb, var(--alloy-color-accent-primary) 80%, transparent);
  }
  .entries {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex: 0 1 auto;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    width: 100%;
    min-height: 0;
    overflow-y: auto;
    scrollbar-width: none;
  }
  .entries::-webkit-scrollbar {
    display: none;
  }
  .entries li {
    position: relative;
    display: flex;
    justify-content: center;
    width: 100%;
  }
  .pill {
    position: absolute;
    left: 4px;
    top: 50%;
    width: 3px;
    height: 0;
    transform: translateY(-50%);
    border-radius: var(--alloy-radii-full);
    background: var(--alloy-color-accent-primary);
    transition: height var(--alloy-motion-duration-fast) var(--alloy-motion-easing-ease);
  }
  .pill.shown {
    height: 18px;
  }
  .home,
  .space {
    position: relative;
    display: grid;
    place-items: center;
    flex: none;
    width: 40px;
    height: 40px;
    padding: 0;
    border: 0;
    border-radius: var(--alloy-radii-xl);
    background: transparent;
    color: inherit;
    cursor: pointer;
    transition:
      background var(--alloy-motion-duration-fast) var(--alloy-motion-easing-ease),
      color var(--alloy-motion-duration-fast) var(--alloy-motion-easing-ease),
      scale var(--alloy-motion-duration-fast) var(--alloy-motion-easing-ease);
  }
  .home:active,
  .space:active,
  .foot-btn:active {
    scale: 0.94;
  }
  .home {
    background: var(--alloy-glass-control);
    color: var(--alloy-color-text-secondary);
  }
  /* The active space keeps the accent as a tint and a rail, never as a second heavy tile. */
  .home.active {
    background: color-mix(in srgb, var(--alloy-color-accent-primary) 14%, transparent);
    color: var(--alloy-color-accent-primary);
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--alloy-color-accent-primary) 28%, transparent);
  }
  .space:hover,
  .home:not(.active):hover {
    background: var(--alloy-glass-control-hover);
    color: var(--alloy-color-text-primary);
  }
  .home.active:hover {
    background: color-mix(in srgb, var(--alloy-color-accent-primary) 20%, transparent);
    color: var(--alloy-color-accent-primary);
  }
  /* A dot, not a count: the rail says where to look, the list says how much. */
  .unread-dot {
    position: absolute;
    right: 0;
    bottom: 0;
    width: 8px;
    height: 8px;
    border-radius: var(--alloy-radii-full);
    background: var(--alloy-color-accent-primary);
    box-shadow: 0 0 0 2px var(--alloy-color-surface-panel);
  }
  .home:focus-visible,
  .space:focus-visible,
  .foot-btn:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }
  .foot {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--alloy-spacing-scale-1);
    flex: none;
    width: 100%;
    margin-top: auto;
    padding-top: var(--alloy-spacing-scale-2);
  }
  /* A short rule centres the account cluster instead of cutting the rail in half. */
  .foot-rule {
    width: 20px;
    height: 1px;
    margin-bottom: 2px;
    background: var(--alloy-chat-rim);
  }
  .foot-btn {
    display: grid;
    place-items: center;
    width: 36px;
    height: 36px;
    padding: 0;
    border: 0;
    border-radius: var(--alloy-radii-full);
    background: transparent;
    color: var(--alloy-color-text-secondary);
    cursor: pointer;
    transition:
      background var(--alloy-motion-duration-fast) var(--alloy-motion-easing-ease),
      color var(--alloy-motion-duration-fast) var(--alloy-motion-easing-ease),
      box-shadow var(--alloy-motion-duration-fast) var(--alloy-motion-easing-ease),
      scale var(--alloy-motion-duration-fast) var(--alloy-motion-easing-ease);
  }
  .foot-btn:hover {
    background: var(--alloy-glass-control-hover);
    color: var(--alloy-color-text-primary);
  }
  /* The avatar keeps its own circle; the ring is the hover affordance. */
  .foot-btn.me:hover {
    background: transparent;
    box-shadow: 0 0 0 2px var(--alloy-glass-rim-strong);
  }
</style>
