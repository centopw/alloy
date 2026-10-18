<script lang="ts">
  import Button from '../../common/Button.svelte';

  interface Props {
    label: string;
    description?: string;
    buttonLabel: string;
    onclick: () => void;
    danger?: boolean;
    busy?: boolean;
  }

  let { label, description, buttonLabel, onclick, danger = false, busy = false }: Props = $props();
</script>

<div class="setting-row" aria-busy={busy || undefined}>
  <div class="text">
    <span class="label">{label}</span>
    {#if description}
      <span class="description">{description}</span>
    {/if}
  </div>
  <Button variant={danger ? 'danger' : 'ghost'} disabled={busy} {onclick}>
    {buttonLabel}
  </Button>
</div>

<style>
  .setting-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--alloy-layout-gutter);
    padding: var(--alloy-spacing-scale-3) var(--alloy-layout-row-pad-x);
    border-top: 1px solid var(--alloy-color-border-subtle);
  }

  .setting-row:first-child {
    border-top: none;
  }

  .text {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-1);
    min-width: 0;
  }

  .label {
    font-size: var(--alloy-typography-size-base);
    font-weight: var(--alloy-typography-weight-medium);
    color: var(--alloy-color-text-primary);
  }

  .description {
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-text-secondary);
  }
</style>
