<script lang="ts">
  import { onMount } from "svelte";
  import Hud from "../dev/Hud.svelte";
  import Timeline from "../timeline/Timeline.svelte";
  import Composer from "../composer/Composer.svelte";
  import { Avatar, Toggle } from "../common";
  import Icon from "../../icons/Icon.svelte";
  import { X, ChevronLeft } from "../../icons";
  import type { Space, Room, Member } from "../../mock/types.js";
  import { generateMockSpaces, generateMockMembers } from "../../mock/generator.js";
  import { themeStore, type Theme } from "../../theme/theme.svelte";

  // Theming: light/dark toggle (AMOLED lives in the dev HUD / settings)
  let isDark = $derived(themeStore.activeTheme !== "alloy-light");
  function setDark(dark: boolean) {
    themeStore.setTheme(dark ? "alloy-dark" : "alloy-light");
  }

  let timeline: Timeline | undefined = $state();

  // Shell Layout & State
  const spaces: Space[] = generateMockSpaces();
  const members: Member[] = generateMockMembers();

  let activeSpaceId = $state(spaces[0].id);
  let activeSpace = $derived(spaces.find((s) => s.id === activeSpaceId) || spaces[0]);
  let activeRoom = $state<Room>(spaces[0].rooms[0]);

  let showRightDrawer = $state(true);
  let hudVisible = $state(true);

  // Mobile Single-Pane navigation (<680px)
  type MobilePane = "spaces" | "rooms" | "timeline" | "drawer";
  let activeMobilePane = $state<MobilePane>("timeline");

  function selectSpace(s: Space) {
    activeSpaceId = s.id;
    if (s.rooms.length > 0) {
      activeRoom = s.rooms[0];
    }
    activeMobilePane = "rooms";
  }

  function selectRoom(r: Room) {
    activeRoom = r;
    activeMobilePane = "timeline";
  }

  onMount(() => {
    // Theme initialized via themeStore
  });
</script>

<div class="alloy-shell" data-theme={themeStore.activeTheme}>
  <Hud bind:visible={hudVisible} />

  <!-- 1. SPACES RAIL -->
  <aside class="pane spaces-rail" class:mobile-active={activeMobilePane === "spaces"}>
    <div class="rail-header">
      <div class="logo-badge" title="Alloy Matrix Shell">A</div>
    </div>

    <div class="spaces-list">
      {#each spaces as space}
        <button
          class="space-item"
          class:active={space.id === activeSpaceId}
          onclick={() => selectSpace(space)}
          title={space.name}
        >
          <span class="space-icon">{space.name.slice(0, 1)}</span>
        </button>
      {/each}
    </div>

    <div class="rail-footer">
      <Toggle checked={isDark} label="Dark mode" onchange={setDark} />
    </div>
  </aside>

  <!-- 2. ROOM LIST -->
  <section class="pane room-list" class:mobile-active={activeMobilePane === "rooms"}>
    <div class="room-list-header">
      <div class="mobile-back">
        <button class="nav-back-btn" onclick={() => activeMobilePane = "spaces"}>
          <Icon icon={ChevronLeft} size={16} /> Spaces
        </button>
      </div>
      <div class="space-title">
        <span>{activeSpace.name}</span>
      </div>
      <div class="search-box">
        <input type="text" placeholder="Filter rooms..." class="search-input" />
      </div>
    </div>

    <div class="rooms-scroll">
      <div class="section-label">ROOMS</div>
      {#each activeSpace.rooms as room}
        <button
          class="room-item"
          class:active={room.id === activeRoom.id}
          onclick={() => selectRoom(room)}
        >
          <Avatar userId={room.id} displayName={room.name} size="md" />
          <div class="room-info">
            <div class="room-row-top">
              <span class="room-name">{room.name}</span>
              {#if room.unreadCount > 0}
                <span class="unread-badge">{room.unreadCount}</span>
              {/if}
            </div>
            <div class="room-preview">{room.lastMessage}</div>
          </div>
        </button>
      {/each}
    </div>
  </section>

  <!-- 3. TIMELINE (MAIN PANE) -->
  <main class="pane timeline-pane" class:mobile-active={activeMobilePane === "timeline"}>
    <div class="mobile-nav-bar">
      <button class="nav-back-btn" onclick={() => activeMobilePane = "rooms"}>
        <Icon icon={ChevronLeft} size={16} /> Rooms
      </button>
      <span class="mobile-room-name">{activeRoom.name}</span>
      <button
        class="nav-toggle-btn"
        onclick={() => activeMobilePane = "drawer"}
      >
        Info
      </button>
    </div>

    <div class="timeline-wrapper">
      <Timeline bind:this={timeline} />
      <Composer roomId={activeRoom.id} onSend={(t) => timeline?.appendMessage(t)} />
    </div>
  </main>

  <!-- 4. RIGHT DRAWER (OPTIONAL) -->
  {#if showRightDrawer}
    <aside class="pane right-drawer" class:mobile-active={activeMobilePane === "drawer"}>
      <div class="drawer-header">
        <span class="drawer-title">Room Details</span>
        <button
          class="close-drawer-btn"
          onclick={() => {
            showRightDrawer = false;
            activeMobilePane = "timeline";
          }}
          title="Close details drawer"
        >
          <Icon icon={X} size={16} label="Close" />
        </button>
      </div>

      <div class="drawer-content">
        <div class="drawer-section">
          <div class="detail-label">TOPIC</div>
          <p class="detail-value">{activeRoom.topic || "No topic configured."}</p>
        </div>

        <div class="drawer-section">
          <div class="detail-label">ENCRYPTION STATUS</div>
          <div class="e2ee-status">
            <span class="e2ee-dot"></span>
            <span>E2EE: Vodozemac Megolm (Active)</span>
          </div>
        </div>

        <div class="drawer-section">
          <div class="detail-label">MEMBERS ({members.length})</div>
          <div class="member-list">
            {#each members as m}
              <div class="member-row">
                <span class="status-indicator" class:online={m.status === "online"} class:idle={m.status === "idle"}></span>
                <span class="member-name">{m.displayName}</span>
              </div>
            {/each}
          </div>
        </div>

      </div>
    </aside>
  {/if}

  {#if !showRightDrawer}
    <button
      class="reopen-drawer-btn"
      onclick={() => showRightDrawer = true}
      title="Open room details"
    >
      Details
    </button>
  {/if}
</div>

<style>
  .alloy-shell {
    display: flex;
    width: 100vw;
    height: 100vh;
    overflow: hidden;
    background-color: var(--alloy-color-surface-canvas);
    color: var(--alloy-color-text-primary);
    font-family: var(--alloy-typography-font-family-sans);
    position: relative;
  }

  /* 1. Spaces Rail */
  .spaces-rail {
    width: 64px;
    background-color: var(--alloy-color-surface-panel);
    border-right: 1px solid var(--alloy-color-border-subtle);
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: var(--alloy-spacing-scale-3) 0;
    flex-shrink: 0;
    z-index: 5;
  }

  .rail-header {
    margin-bottom: var(--alloy-spacing-scale-4);
  }

  .logo-badge {
    width: 40px;
    height: 40px;
    border-radius: var(--alloy-radii-lg);
    background: linear-gradient(135deg, var(--alloy-color-accent-primary), #a855f7);
    color: var(--alloy-color-text-on-accent);
    display: flex;
    align-items: center;
    justify-content: center;
    font-weight: 800;
    font-size: 20px;
    box-shadow: var(--alloy-elevation-sm);
  }

  .spaces-list {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-2);
    width: 100%;
    align-items: center;
  }

  .space-item {
    width: 44px;
    height: 44px;
    border-radius: var(--alloy-radii-md);
    background: var(--alloy-color-surface-elevated);
    border: 1px solid var(--alloy-color-border-subtle);
    color: var(--alloy-color-text-primary);
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    font-size: 18px;
    transition: all 0.15s ease;
  }

  .space-item:hover {
    background: var(--alloy-color-surface-hover);
    border-color: var(--alloy-color-border-prominent);
  }

  .space-item.active {
    background: var(--alloy-color-accent-primary);
    color: var(--alloy-color-text-on-accent);
    border-radius: var(--alloy-radii-lg);
    box-shadow: 0 0 10px var(--alloy-color-accent-subtle);
  }

  .rail-footer {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-2);
  }




  /* 2. Room List */
  .room-list {
    width: 250px;
    background-color: var(--alloy-color-surface-subtle);
    border-right: 1px solid var(--alloy-color-border-subtle);
    display: flex;
    flex-direction: column;
    flex-shrink: 0;
    overflow: hidden;
  }

  .room-list-header {
    padding: var(--alloy-spacing-scale-3);
    border-bottom: 1px solid var(--alloy-color-border-subtle);
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-2);
  }

  .space-title {
    font-weight: 700;
    font-size: 14px;
    color: var(--alloy-color-text-primary);
  }

  .search-input {
    width: 100%;
    padding: var(--alloy-spacing-scale-1) var(--alloy-spacing-scale-2);
    border-radius: var(--alloy-radii-sm);
    border: 1px solid var(--alloy-color-border-prominent);
    background: var(--alloy-color-surface-panel);
    color: var(--alloy-color-text-primary);
    font-size: 12px;
    outline: none;
    box-sizing: border-box;
  }

  .search-input:focus {
    border-color: var(--alloy-color-accent-primary);
  }

  .rooms-scroll {
    flex: 1;
    overflow-y: auto;
    padding: var(--alloy-spacing-scale-2);
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .section-label {
    font-size: 10px;
    font-weight: 700;
    color: var(--alloy-color-text-muted);
    padding: var(--alloy-spacing-scale-1) var(--alloy-spacing-scale-2);
    letter-spacing: 0.5px;
  }

  .room-item {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    padding: var(--alloy-spacing-scale-2);
    border-radius: var(--alloy-radii-md);
    background: transparent;
    border: none;
    width: 100%;
    text-align: left;
    cursor: pointer;
    transition: background 0.1s ease;
  }

  .room-item:hover {
    background: var(--alloy-color-surface-elevated);
  }

  .room-item.active {
    background: var(--alloy-color-surface-hover);
  }


  .room-info {
    flex: 1;
    min-width: 0;
  }

  .room-row-top {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .room-name {
    font-weight: 600;
    font-size: 13px;
    color: var(--alloy-color-text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .unread-badge {
    background: var(--alloy-color-status-mention);
    color: var(--alloy-color-text-on-accent);
    font-size: 10px;
    font-weight: 700;
    padding: 1px 6px;
    border-radius: var(--alloy-radii-full);
  }

  .room-preview {
    font-size: 11px;
    color: var(--alloy-color-text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* 3. Timeline Pane */
  .timeline-pane {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    height: 100%;
    position: relative;
    overflow: hidden;
  }

  .timeline-wrapper > :global(.timeline-container) {
    flex: 1;
    min-height: 0;
  }

  .timeline-wrapper {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  /* 4. Right Drawer */
  .right-drawer {
    width: 260px;
    background: var(--alloy-color-surface-panel);
    border-left: 1px solid var(--alloy-color-border-subtle);
    display: flex;
    flex-direction: column;
    flex-shrink: 0;
    overflow-y: auto;
  }

  .drawer-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: var(--alloy-spacing-scale-3);
    border-bottom: 1px solid var(--alloy-color-border-subtle);
  }

  .drawer-title {
    font-weight: 700;
    font-size: 13px;
    color: var(--alloy-color-text-primary);
  }

  .close-drawer-btn {
    background: transparent;
    border: none;
    color: var(--alloy-color-text-muted);
    font-size: 14px;
    cursor: pointer;
  }

  .close-drawer-btn:hover {
    color: var(--alloy-color-text-primary);
  }

  .drawer-content {
    padding: var(--alloy-spacing-scale-3);
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-4);
  }

  .drawer-section {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-1);
  }

  .detail-label {
    font-size: 10px;
    font-weight: 700;
    color: var(--alloy-color-text-muted);
    letter-spacing: 0.5px;
  }

  .detail-value {
    font-size: 12px;
    color: var(--alloy-color-text-secondary);
    margin: 0;
    line-height: 1.4;
  }

  .e2ee-status {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    font-size: 12px;
    color: var(--alloy-color-status-online);
    font-weight: 500;
  }

  .e2ee-dot {
    width: 8px;
    height: 8px;
    border-radius: var(--alloy-radii-full);
    background: var(--alloy-color-status-online);
  }

  .member-list {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-2);
    margin-top: var(--alloy-spacing-scale-1);
  }

  .member-row {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    font-size: 12px;
    color: var(--alloy-color-text-primary);
  }

  .status-indicator {
    width: 7px;
    height: 7px;
    border-radius: var(--alloy-radii-full);
    background: var(--alloy-color-text-muted);
  }

  .status-indicator.online {
    background: var(--alloy-color-status-online);
  }

  .status-indicator.idle {
    background: #f59e0b;
  }

  .reopen-drawer-btn {
    position: absolute;
    top: var(--alloy-spacing-scale-3);
    right: 240px;
    width: 28px;
    height: 28px;
    border-radius: var(--alloy-radii-full);
    background: var(--alloy-color-surface-elevated);
    border: 1px solid var(--alloy-color-border-prominent);
    color: var(--alloy-color-text-secondary);
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 13px;
    cursor: pointer;
    z-index: 10;
  }

  .reopen-drawer-btn:hover {
    background: var(--alloy-color-surface-hover);
    color: var(--alloy-color-text-primary);
  }

  /* Responsive Mobile Navigation Bar (<680px) */
  .mobile-nav-bar,
  .mobile-back {
    display: none;
  }

  .nav-back-btn,
  .nav-toggle-btn {
    background: var(--alloy-color-surface-elevated);
    border: 1px solid var(--alloy-color-border-subtle);
    color: var(--alloy-color-text-primary);
    padding: var(--alloy-spacing-scale-1) var(--alloy-spacing-scale-2);
    border-radius: var(--alloy-radii-sm);
    font-size: 11px;
    cursor: pointer;
  }

  /* Collapsing to single pane under 680px */
  @media (max-width: 680px) {
    .pane {
      display: none !important;
      width: 100% !important;
      height: 100% !important;
    }

    .pane.mobile-active {
      display: flex !important;
    }

    .mobile-nav-bar {
      display: flex;
      align-items: center;
      justify-content: space-between;
      padding: var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-3);
      background: var(--alloy-color-surface-panel);
      border-bottom: 1px solid var(--alloy-color-border-subtle);
      flex-shrink: 0;
    }

    .mobile-back {
      display: block;
    }

    .mobile-room-name {
      font-weight: 700;
      font-size: 13px;
    }

    .reopen-drawer-btn {
      display: none;
    }
  }
</style>
