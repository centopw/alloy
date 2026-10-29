<script lang="ts">
  import { Button, ConfirmDialog } from '../../common';
  import TagEditDialog from '../../tags/TagEditDialog.svelte';
  import { Icon, Pencil, Trash2, ChevronUp, ChevronDown, Plus } from '$lib/icons';
  import type { RoomTagDef } from '$lib/generated/index.js';
  import { tags } from '$lib/stores/tags.svelte.js';
  import { TAG_COLOUR_VARS } from '$lib/stores/tags.js';

  let editOpen = $state(false);
  let editing = $state<RoomTagDef | null>(null);
  let pendingDelete = $state<RoomTagDef | null>(null);

  function openNew() {
    editing = null;
    editOpen = true;
  }

  function openEdit(tag: RoomTagDef) {
    editing = tag;
    editOpen = true;
  }
</script>

<section class="tags">
  <p class="intro">
    Tags group rooms in the sidebar. Assign them from a room's context menu. Deleting a tag removes it
    from every room.
  </p>

  {#if tags.error}<p class="error" role="alert">{tags.error}</p>{/if}

  {#if tags.defs.length === 0}
    <p class="empty">No tags yet.</p>
  {:else}
    <ul class="list">
      {#each tags.defs as tag, index (tag.id)}
        <li class="row">
          <span class="dot" style:background-color={TAG_COLOUR_VARS[tag.colour]}></span>
          <span class="name">{tag.name}</span>
          <div class="actions">
            <button
              type="button"
              class="icon-btn"
              aria-label="Move {tag.name} up"
              disabled={index === 0}
              onclick={() => tags.move(tag.id, -1)}
            >
              <Icon icon={ChevronUp} size={20} />
            </button>
            <button
              type="button"
              class="icon-btn"
              aria-label="Move {tag.name} down"
              disabled={index === tags.defs.length - 1}
              onclick={() => tags.move(tag.id, 1)}
            >
              <Icon icon={ChevronDown} size={20} />
            </button>
            <button type="button" class="icon-btn" aria-label="Edit {tag.name}" onclick={() => openEdit(tag)}>
              <Icon icon={Pencil} size={20} />
            </button>
            <button
              type="button"
              class="icon-btn danger"
              aria-label="Delete {tag.name}"
              onclick={() => (pendingDelete = tag)}
            >
              <Icon icon={Trash2} size={20} />
            </button>
          </div>
        </li>
      {/each}
    </ul>
  {/if}

  <div class="footer">
    <Button onclick={openNew}>
      {#snippet children()}<Icon icon={Plus} size={16} /> New tag{/snippet}
    </Button>
  </div>
</section>

<TagEditDialog open={editOpen} tag={editing} onclose={() => (editOpen = false)} />

<ConfirmDialog
  open={pendingDelete !== null}
  title="Delete tag"
  message={`Delete the tag "${pendingDelete?.name ?? ''}"? It is removed from every room.`}
  confirmLabel="Delete"
  danger
  onconfirm={async () => {
    if (pendingDelete) await tags.remove(pendingDelete.id);
  }}
  onclose={() => (pendingDelete = null)}
/>

<style>
  .tags {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-layout-section-gap);
  }
  .intro,
  .empty {
    margin: 0;
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-text-secondary);
  }
  .error {
    margin: 0;
    color: var(--alloy-color-status-error);
  }
  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-1);
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    min-height: 44px;
    padding: 0 var(--alloy-spacing-scale-2);
    border-radius: var(--alloy-radii-lg);
    background: var(--alloy-color-surface-subtle);
  }
  .dot {
    width: 10px;
    height: 10px;
    border-radius: var(--alloy-radii-full);
    flex: 0 0 auto;
  }
  .name {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .actions {
    display: flex;
    gap: var(--alloy-spacing-scale-1);
  }
  .icon-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    border: 0;
    border-radius: var(--alloy-radii-sm);
    background: transparent;
    color: var(--alloy-color-text-secondary);
    cursor: pointer;
  }
  .icon-btn:hover:not(:disabled) {
    background: var(--alloy-color-surface-hover);
    color: var(--alloy-color-text-primary);
  }
  .icon-btn:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .icon-btn.danger:hover:not(:disabled) {
    color: var(--alloy-color-status-error);
  }
  .icon-btn:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }
  .footer {
    display: flex;
    justify-content: flex-start;
  }
  @media (max-width: 899px) {
    .icon-btn {
      width: 44px;
      height: 44px;
    }
  }
</style>
