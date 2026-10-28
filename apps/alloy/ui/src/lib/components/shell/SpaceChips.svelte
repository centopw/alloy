<script lang="ts">
  import { Avatar } from '../common';
  import Icon from '$lib/icons/Icon.svelte';
  import { House, Plus } from '$lib/icons';
  import { roomsStore, roomTitle } from '$lib/stores/rooms.svelte';
  import { ui } from '$lib/stores/ui.svelte';

  interface Props {
    onnew?: () => void;
  }
  let { onnew }: Props = $props();

  const spaces = $derived(roomsStore.rooms.filter((r) => r.is_space && r.membership === 'Joined'));
  const homeActive = $derived(ui.scope === 'home');
</script>

{#if spaces.length > 0}
<div class="strip" role="toolbar" aria-label="Spaces">
  <button
    type="button"
    class="chip"
    class:active={homeActive}
    aria-label="Home"
    aria-current={homeActive ? 'true' : undefined}
    onclick={() => (ui.scope = 'home')}
  >
    <Icon icon={House} size={20} />
    <span class="text">Home</span>
  </button>
  {#each spaces as space (space.id)}
    {@const title = roomTitle(space)}
    {@const active = ui.scope === space.id}
    <button
      type="button"
      class="chip"
      class:active
      aria-label={title}
      aria-current={active ? 'true' : undefined}
      onclick={() => (ui.scope = space.id)}
    >
      <Avatar userId={space.id} displayName={title} src={space.avatar_url} size={24} shape="circle" />
      <span class="text">{title}</span>
    </button>
  {/each}
  <button type="button" class="add" aria-label="Create or join" onclick={() => onnew?.()}>
    <Icon icon={Plus} size={20} />
  </button>
</div>
{/if}

<style>
  .strip {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    box-sizing: border-box;
    min-height: 52px;
    padding: var(--alloy-spacing-scale-2) var(--alloy-layout-gutter);
    overflow-x: auto;
    overflow-y: hidden;
    overscroll-behavior: contain;
    -webkit-overflow-scrolling: touch;
    scroll-snap-type: x proximity;
    scrollbar-width: none;
    flex: 0 0 auto;
  }

  .strip::-webkit-scrollbar {
    display: none;
  }

  .chip,
  .add {
    flex: 0 0 auto;
    scroll-snap-align: start;
    min-height: 44px;
    min-width: 44px;
    box-sizing: border-box;
    border: 1px solid transparent;
    font: inherit;
    cursor: pointer;
    -webkit-tap-highlight-color: transparent;
  }

  .chip {
    display: inline-flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    max-width: 220px;
    padding: 0 var(--alloy-spacing-scale-3) 0 var(--alloy-spacing-scale-2);
    border-radius: var(--alloy-radius-full);
    background: var(--alloy-glass-sheen), var(--alloy-glass-fill-soft);
    -webkit-backdrop-filter: var(--alloy-glass-filter);
    backdrop-filter: var(--alloy-glass-filter);
    box-shadow: var(--alloy-glass-highlight);
    color: var(--alloy-color-text-secondary);
    font-size: var(--alloy-typography-size-sm);
    font-weight: var(--alloy-typography-weight-medium);
  }

  .chip.active {
    background: color-mix(in srgb, var(--alloy-color-accent-primary) 16%, transparent);
    border-color: color-mix(in srgb, var(--alloy-color-accent-primary) 30%, transparent);
    color: var(--alloy-color-text-primary);
  }

  .chip :global(svg) {
    margin-left: var(--alloy-spacing-scale-1);
  }

  .text {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .add {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 44px;
    padding: 0;
    border-radius: var(--alloy-radius-full);
    background: var(--alloy-glass-sheen), var(--alloy-glass-fill-soft);
    -webkit-backdrop-filter: var(--alloy-glass-filter);
    backdrop-filter: var(--alloy-glass-filter);
    box-shadow: var(--alloy-glass-highlight);
    color: var(--alloy-color-text-secondary);
  }

  .chip:focus-visible,
  .add:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }

  @media (hover: hover) {
    .chip:not(.active):hover,
    .add:hover {
      background: var(--alloy-glass-control-hover);
      color: var(--alloy-color-text-primary);
    }
  }
</style>
