<script lang="ts">
  import { Modal, Button, TextField } from '../common';
  import type { RoomTagDef, TagColour } from '$lib/generated/index.js';
  import { tags } from '$lib/stores/tags.svelte.js';
  import { TAG_COLOUR_VARS, TAG_COLOURS } from '$lib/stores/tags.js';

  interface Props {
    open: boolean;
    /** The tag being edited; null creates a new tag. */
    tag: RoomTagDef | null;
    onclose: () => void;
    /** Called with the created tag, so the caller can assign it to a room. */
    onsaved?: (tag: RoomTagDef) => void;
  }
  let { open, tag, onclose, onsaved }: Props = $props();

  let name = $state('');
  let colour = $state<TagColour>('blue');
  let busy = $state(false);
  let wasOpen = false;

  const trimmed = $derived(name.trim());
  const canSave = $derived(!busy && trimmed.length > 0);

  // Reset the form each time the dialog opens, from the tag being edited (or blank for a new one).
  $effect(() => {
    if (!open) {
      wasOpen = false;
      return;
    }
    if (wasOpen) return;
    wasOpen = true;
    name = tag?.name ?? '';
    colour = tag?.colour ?? 'blue';
    tags.error = null;
  });

  async function save(e: SubmitEvent) {
    e.preventDefault();
    if (!canSave) return;
    busy = true;
    if (tag) {
      const ok = await tags.update(tag.id, { name: trimmed, colour });
      if (ok) onclose();
    } else {
      const created = await tags.create(trimmed, colour);
      if (created) {
        onsaved?.(created);
        onclose();
      }
    }
    busy = false;
  }
</script>

<Modal open={open} title={tag ? 'Edit tag' : 'New tag'} {onclose} width="420px">
  <form onsubmit={save}>
    <label class="field">
      <span>Name</span>
      <TextField
        type="text"
        bind:value={name}
        maxlength={64}
        autocomplete="off"
        disabled={busy}
      />
    </label>
    <fieldset class="colours" disabled={busy}>
      <legend>Colour</legend>
      {#each TAG_COLOURS as option (option)}
        <button
          type="button"
          class="swatch"
          class:selected={colour === option}
          style:background-color={TAG_COLOUR_VARS[option]}
          aria-label={option}
          aria-pressed={colour === option}
          onclick={() => (colour = option)}
        ></button>
      {/each}
    </fieldset>
    {#if tags.error}<p class="error" role="alert">{tags.error}</p>{/if}
    <div class="actions">
      <Button variant="ghost" onclick={onclose}>Cancel</Button>
      <Button type="submit" disabled={!canSave}>{busy ? 'Saving...' : 'Save'}</Button>
    </div>
  </form>
</Modal>

<style>
  form {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-layout-section-gap);
    margin-top: var(--alloy-layout-section-gap);
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-1);
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-text-secondary);
  }
  .colours {
    display: flex;
    flex-wrap: wrap;
    gap: var(--alloy-spacing-scale-2);
    border: 0;
    margin: 0;
    padding: 0;
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-text-secondary);
  }
  .colours legend {
    padding: 0;
    margin-bottom: var(--alloy-spacing-scale-1);
  }
  .swatch {
    width: 28px;
    height: 28px;
    border-radius: var(--alloy-radii-full);
    border: 2px solid transparent;
    cursor: pointer;
  }
  .swatch.selected {
    border-color: var(--alloy-color-text-primary);
    box-shadow: inset 0 0 0 2px var(--alloy-color-surface-panel);
  }
  .swatch:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
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
    .swatch {
      width: 44px;
      height: 44px;
    }
    .actions {
      flex-direction: column-reverse;
      justify-content: flex-start;
    }
  }
</style>
