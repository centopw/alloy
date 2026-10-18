<script lang="ts">
  import Toggle from '../../common/Toggle.svelte';

  interface Props {
    label: string;
    description?: string;
    checked: boolean;
    onchange: (value: boolean) => void;
    disabled?: boolean;
    disabledReason?: string;
  }

  let { label, description, checked, onchange, disabled = false, disabledReason }: Props = $props();
</script>

<div class="setting-row">
  <div class="text">
    <span class="label">{label}</span>
    {#if description}
      <span class="description">{description}</span>
    {/if}
    {#if disabled && disabledReason}
      <span class="reason">{disabledReason}</span>
    {/if}
  </div>
  <Toggle {checked} {label} {disabled} {onchange} />
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

  .reason {
    font-size: var(--alloy-typography-size-xs);
    color: var(--alloy-color-text-muted);
  }
</style>
