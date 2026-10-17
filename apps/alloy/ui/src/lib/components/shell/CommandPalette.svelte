<script lang="ts">
  import { tick } from 'svelte';
  import { Avatar } from '../common';
  import { roomsStore, roomTitle } from '$lib/stores/rooms.svelte';

  interface Props {
    open: boolean;
    onclose: () => void;
    onpick: (roomId: string) => void;
  }
  let { open, onclose, onpick }: Props = $props();

  let query = $state('');
  let index = $state(0);
  let input: HTMLInputElement | undefined = $state();

  const results = $derived(
    roomsStore.rooms
      .filter((r) => r.membership === 'Joined' && !r.is_space)
      .filter((r) => roomTitle(r).toLowerCase().includes(query.trim().toLowerCase()))
      .slice(0, 8)
  );

  $effect(() => {
    if (open) {
      query = '';
      index = 0;
      void tick().then(() => input?.focus());
    }
  });

  $effect(() => {
    void query;
    index = 0;
  });

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape') onclose();
    else if (e.key === 'ArrowDown') {
      e.preventDefault();
      index = Math.min(index + 1, results.length - 1);
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      index = Math.max(index - 1, 0);
    } else if (e.key === 'Enter' && results[index]) {
      e.preventDefault();
      onpick(results[index].id);
    }
  }
</script>

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="backdrop" onclick={onclose}>
    <div class="palette" role="dialog" aria-modal="true" aria-label="Quick switcher" tabindex="-1" onclick={(e) => e.stopPropagation()}>
      <input
        bind:this={input}
        bind:value={query}
        onkeydown={onKey}
        placeholder="Jump to a conversation"
        aria-label="Jump to a conversation"
        role="combobox"
        aria-expanded="true"
        aria-controls="palette-results"
      />
      <ul id="palette-results" role="listbox">
        {#each results as r, i (r.id)}
          <li role="option" aria-selected={i === index}>
            <button type="button" class:sel={i === index} onclick={() => onpick(r.id)}>
              <Avatar userId={r.id} displayName={roomTitle(r)} size="sm" />
              <span>{roomTitle(r)}</span>
            </button>
          </li>
        {:else}
          <li class="none">No matching conversations</li>
        {/each}
      </ul>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.45);
    display: flex;
    justify-content: center;
    align-items: flex-start;
    padding-top: 12vh;
    z-index: 50;
  }
  .palette {
    width: min(520px, 92vw);
    background: var(--alloy-color-surface-elevated);
    border: 1px solid var(--alloy-color-border-prominent);
    border-radius: var(--alloy-radii-lg);
    box-shadow: var(--alloy-elevation-lg);
    overflow: hidden;
  }
  input {
    width: 100%;
    border: 0;
    outline: 0;
    background: transparent;
    color: var(--alloy-color-text-primary);
    font: inherit;
    font-size: var(--alloy-typography-size-lg);
    padding: var(--alloy-spacing-scale-4);
    border-bottom: 1px solid var(--alloy-color-border-subtle);
  }
  ul {
    list-style: none;
    margin: 0;
    padding: var(--alloy-spacing-scale-2);
  }
  button {
    display: flex;
    gap: var(--alloy-spacing-scale-3);
    align-items: center;
    width: 100%;
    border: 0;
    background: transparent;
    color: inherit;
    font: inherit;
    padding: var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-3);
    border-radius: var(--alloy-radii-md);
    cursor: pointer;
    text-align: left;
  }
  button.sel,
  button:hover {
    background: var(--alloy-color-surface-hover);
  }
  .none {
    padding: var(--alloy-spacing-scale-4);
    text-align: center;
    color: var(--alloy-color-text-secondary);
  }
</style>
