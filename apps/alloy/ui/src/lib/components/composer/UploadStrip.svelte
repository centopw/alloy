<script lang="ts">
  import { formatBytes } from '$lib/content/format';
  import { hasKnownSize, uploadPercent, uploadsStore } from '$lib/stores/uploads.svelte';

  interface Props {
    roomId: string;
  }
  let { roomId }: Props = $props();

  const uploads = $derived(uploadsStore.inRoom(roomId));

  /*
   * A progress record only means something once the engine reports a size. The engine emits a
   * `0 of 0` event as soon as the upload starts, and treating that as real progress froze the bar
   * at 0% with no sweep — the "looks stuck" report. Until `total > 0`, stay indeterminate.
   */
</script>

{#if uploads.length > 0}
  <ul class="uploads" aria-label="Uploads in progress">
    {#each uploads as upload (upload.txnId)}
      {@const progress = uploadsStore.progressFor(upload.txnId)}
      {@const hasSize = hasKnownSize(progress)}
      {@const percent = uploadPercent(progress)}
      <li class="upload">
        <span class="name">{upload.name}</span>
        <div
          class="track"
          class:indeterminate={!hasSize}
          role="progressbar"
          aria-label="Uploading {upload.name}"
          aria-valuemin="0"
          aria-valuemax="100"
          aria-valuenow={hasSize ? percent : undefined}
          aria-valuetext={hasSize ? `${formatBytes(progress.sent)} of ${formatBytes(progress.total)}` : 'Uploading'}
        >
          <div class="fill" style={hasSize ? `width: ${percent}%` : undefined}></div>
        </div>
        <span class="meta">{hasSize ? `${percent}%` : 'Uploading…'}</span>
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
    border: 1px solid var(--alloy-glass-rim);
    border-radius: var(--alloy-glass-radius-control);
    background: var(--alloy-glass-sheen), var(--alloy-glass-fill-strong);
    -webkit-backdrop-filter: var(--alloy-glass-filter);
    backdrop-filter: var(--alloy-glass-filter);
    box-shadow: var(--alloy-glass-highlight);
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
  /* No byte count yet: the engine is still encrypting or buffering, so the bar sweeps instead of sitting at 0. */
  .track.indeterminate .fill {
    width: 40%;
    animation: upload-sweep 1.1s ease-in-out infinite;
  }
  @keyframes upload-sweep {
    0% {
      transform: translateX(-110%);
    }
    100% {
      transform: translateX(260%);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .track.indeterminate .fill {
      animation: none;
      width: 100%;
      opacity: 0.4;
    }
  }
  .meta {
    color: var(--alloy-color-text-secondary);
    font-variant-numeric: tabular-nums;
  }
  @media (max-width: 899px) {
    .upload {
      min-height: 44px;
    }
  }
</style>
