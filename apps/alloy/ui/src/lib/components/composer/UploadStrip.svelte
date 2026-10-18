<script lang="ts">
  import { formatBytes } from '$lib/content/format';
  import { uploadsStore } from '$lib/stores/uploads.svelte';

  interface Props {
    roomId: string;
  }
  let { roomId }: Props = $props();

  const uploads = $derived(uploadsStore.inRoom(roomId));
</script>

{#if uploads.length > 0}
  <ul class="uploads" aria-label="Uploads in progress">
    {#each uploads as upload (upload.txnId)}
      {@const progress = uploadsStore.progressFor(upload.txnId)}
      {@const percent = progress ? Math.round(progress.fraction * 100) : 0}
      <li class="upload">
        <span class="name">{upload.name}</span>
        <div
          class="track"
          role="progressbar"
          aria-label="Uploading {upload.name}"
          aria-valuemin="0"
          aria-valuemax="100"
          aria-valuenow={percent}
          aria-valuetext={progress ? `${formatBytes(progress.sent)} of ${formatBytes(progress.total)}` : 'Starting'}
        >
          <div class="fill" style="width: {percent}%"></div>
        </div>
        <span class="meta">{progress ? `${percent}%` : 'Starting'}</span>
      </li>
    {/each}
  </ul>
{/if}

<style>
  .uploads {
    list-style: none;
    margin: 0 var(--alloy-layout-gutter) var(--alloy-spacing-scale-2);
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-2);
  }
  .upload {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-3);
    padding: var(--alloy-spacing-scale-2) var(--alloy-layout-row-pad-x);
    border-radius: var(--alloy-radii-lg);
    background: var(--alloy-color-surface-subtle);
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-text-primary);
  }
  .name {
    flex: 0 1 40%;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .track {
    flex: 1;
    height: 6px;
    border-radius: var(--alloy-radii-full);
    background: var(--alloy-color-surface-hover);
    overflow: hidden;
  }
  .fill {
    height: 100%;
    border-radius: var(--alloy-radii-full);
    background: var(--alloy-color-accent-primary);
  }
  .meta {
    color: var(--alloy-color-text-secondary);
    font-variant-numeric: tabular-nums;
  }
</style>
