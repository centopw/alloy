<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    label: string;
    description?: string;
    reason?: string;
    forId?: string;
    stacked?: boolean;
    busy?: boolean;
    children: Snippet;
  }

  let {
    label,
    description,
    reason,
    forId,
    stacked = false,
    busy = false,
    children,
  }: Props = $props();
</script>

<div class="setting-row" class:stacked aria-busy={busy || undefined}>
  <div class="text">
    {#if forId}
      <label class="label" for={forId}>{label}</label>
    {:else}
      <span class="label">{label}</span>
    {/if}
    {#if description}
      <span class="description">{description}</span>
    {/if}
    {#if reason}
      <span class="reason">{reason}</span>
    {/if}
  </div>
  {@render children()}
</div>

<style>
  .setting-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--alloy-spacing-scale-4);
    min-height: var(--alloy-layout-control-height);
    padding: var(--alloy-spacing-scale-2) 0;
    border-top: 1px solid var(--alloy-color-border-subtle);
  }

  .setting-row:first-child {
    border-top: none;
  }

  .setting-row.stacked {
    flex-direction: column;
    align-items: stretch;
    gap: var(--alloy-spacing-scale-2);
  }

  .text {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-1);
    min-width: 0;
  }

  .setting-row.stacked .text {
    flex: none;
  }

  .label {
    font-size: var(--alloy-font-size-row);
    font-weight: var(--alloy-typography-weight-medium);
    color: var(--alloy-color-text-primary);
  }

  .description {
    font-size: var(--alloy-font-size-preview);
    color: var(--alloy-color-text-muted);
  }

  .reason {
    font-size: var(--alloy-typography-size-xs);
    color: var(--alloy-color-text-muted);
  }

  @media (max-width: 899px), (pointer: coarse) {
    .setting-row:has(> :global(.alloy-segmented-control)) {
      flex-direction: column;
      align-items: stretch;
      gap: var(--alloy-spacing-scale-2);
    }

    .setting-row:has(> :global(.alloy-segmented-control)) .text {
      flex: none;
    }
  }
</style>
