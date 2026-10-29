<script lang="ts">
  import { profileCard } from '$lib/stores/profileCard.svelte';
  import { roomsStore } from '$lib/stores/rooms.svelte';
  import { pushBack } from '$lib/platform/backstack';
  import UserProfileCard from './UserProfileCard.svelte';

  const CARD_WIDTH = 320;
  const MARGIN = 8;
  const NARROW_QUERY = '(max-width: 899px)';

  let narrow = $state(false);
  let cardEl = $state<HTMLElement | null>(null);
  let cardHeight = $state(0);
  let viewportWidth = $state(0);
  let viewportHeight = $state(0);
  let returnFocus: HTMLElement | null = null;

  $effect(() => {
    const mql = window.matchMedia(NARROW_QUERY);
    const update = () => {
      narrow = mql.matches;
      viewportWidth = window.innerWidth;
      viewportHeight = window.innerHeight;
    };
    update();
    mql.addEventListener('change', update);
    window.addEventListener('resize', update);
    return () => {
      mql.removeEventListener('change', update);
      window.removeEventListener('resize', update);
    };
  });

  const position = $derived.by(() => {
    const at = profileCard.at;
    if (!at) return { left: MARGIN, top: MARGIN };
    const maxLeft = Math.max(MARGIN, viewportWidth - CARD_WIDTH - MARGIN);
    const maxTop = Math.max(MARGIN, viewportHeight - cardHeight - MARGIN);
    return {
      left: Math.min(Math.max(MARGIN, at.x), maxLeft),
      top: Math.min(Math.max(MARGIN, at.y), maxTop),
    };
  });

  // Track the card's rendered height so the popover can stay inside the viewport.
  $effect(() => {
    const el = cardEl;
    if (!el) return;
    const ro = new ResizeObserver(() => {
      cardHeight = el.offsetHeight;
    });
    ro.observe(el);
    cardHeight = el.offsetHeight;
    return () => ro.disconnect();
  });

  // Focus management: move focus into the card on open, restore it on close.
  $effect(() => {
    if (!profileCard.isOpen) return;
    returnFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    queueMicrotask(() => cardEl?.focus());
    return () => {
      const target = returnFocus;
      returnFocus = null;
      target?.focus?.();
    };
  });

  // Escape and outside pointerdown close the card.
  $effect(() => {
    if (!profileCard.isOpen) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.stopPropagation();
        profileCard.close();
      }
    };
    const onPointer = (e: PointerEvent) => {
      const target = e.target as Node | null;
      if (cardEl && target && cardEl.contains(target)) return;
      profileCard.close();
    };
    window.addEventListener('keydown', onKey);
    document.addEventListener('pointerdown', onPointer, true);
    return () => {
      window.removeEventListener('keydown', onKey);
      document.removeEventListener('pointerdown', onPointer, true);
    };
  });

  // Narrow layouts show a bottom sheet; Android back closes it instead of leaving the app.
  $effect(() => {
    if (!profileCard.isOpen || !narrow) return;
    return pushBack(() => profileCard.close());
  });

  function onMessage(roomId: string) {
    roomsStore.select(roomId);
    profileCard.close();
  }
</script>

{#if profileCard.isOpen && profileCard.userId}
  {#if narrow}
    <button type="button" class="scrim" aria-label="Dismiss profile" onclick={() => profileCard.close()}></button>
  {/if}
  <div
    bind:this={cardEl}
    class="host"
    class:sheet={narrow}
    style={narrow ? undefined : `left: ${position.left}px; top: ${position.top}px; width: ${CARD_WIDTH}px;`}
    role="dialog"
    aria-modal={narrow ? 'true' : 'false'}
    aria-label="User profile"
    tabindex="-1"
  >
    {#if narrow}
      <div class="grabber" aria-hidden="true"></div>
    {/if}
    <UserProfileCard
      userId={profileCard.userId}
      roomId={profileCard.roomId}
      onclose={() => profileCard.close()}
      onmessage={onMessage}
    />
  </div>
{/if}

<style>
  .host {
    position: fixed;
    z-index: 1000;
    box-sizing: border-box;
    max-height: calc(100vh - 2 * var(--alloy-space-2));
    overflow: auto;
    background: var(--alloy-glass-sheen), var(--alloy-glass-fill-strong);
    -webkit-backdrop-filter: var(--alloy-glass-filter-strong);
    backdrop-filter: var(--alloy-glass-filter-strong);
    color: var(--alloy-color-text-primary);
    border: 1px solid var(--alloy-glass-rim);
    border-radius: var(--alloy-glass-radius-panel);
    box-shadow: var(--alloy-glass-shadow), var(--alloy-glass-highlight);
  }

  .host:focus {
    outline: none;
  }

  .host.sheet {
    left: 0;
    right: 0;
    bottom: 0;
    top: auto;
    width: 100%;
    max-height: 90dvh;
    overscroll-behavior: contain;
    border-radius: var(--alloy-glass-radius-panel) var(--alloy-glass-radius-panel) 0 0;
    border-bottom: none;
    padding-bottom: calc(var(--alloy-spacing-scale-4) + var(--alloy-safe-area-bottom));
  }

  .grabber {
    width: 40px;
    height: 4px;
    margin: var(--alloy-space-2) auto 0;
    border-radius: var(--alloy-radii-full);
    background: var(--alloy-color-border-prominent);
  }

  /* Phone: a scrim behind the bottom sheet. Tapping it dismisses the profile. */
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 999;
    padding: 0;
    border: none;
    background: var(--alloy-color-scrim);
    cursor: pointer;
  }
</style>
