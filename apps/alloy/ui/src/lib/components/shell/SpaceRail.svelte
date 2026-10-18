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

<nav class="rail" aria-label="Spaces">
  <ul class="entries">
    <li>
      <span class="pill" class:shown={homeActive} aria-hidden="true"></span>
      <button
        type="button"
        class="home"
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
    width: 4px;
    height: 0;
    transform: translateY(-50%);
    border-radius: var(--alloy-radii-full);
    background: var(--alloy-color-accent-primary);
    transition: height 120ms ease;
  }
  .pill.shown {
    height: 40px;
  }
  .home,
  .space,
  .create {
    display: grid;
    place-items: center;
    flex: none;
    width: 48px;
    height: 48px;
    padding: 0;
    border: 0;
    border-radius: var(--alloy-radii-full);
    background: transparent;
    color: inherit;
    cursor: pointer;
  }
  .home {
    background: var(--alloy-color-accent-primary);
    color: var(--alloy-color-text-on-accent);
  }
  .home[aria-current='true'] {
    border-radius: var(--alloy-radii-lg);
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
