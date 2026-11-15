<script lang="ts">
  import { Select, type SelectOption } from '$lib/components/common';

  interface Props {
    label: string;
    description?: string;
    value: string;
    options: SelectOption[];
    onchange: (value: string) => void;
    disabled?: boolean;
  }

  let { label, description, value, options, onchange, disabled = false }: Props = $props();

  const selectId = $props.id();
</script>

<div class="setting-row">
  <div class="text">
    <label class="label" for={selectId}>{label}</label>
    {#if description}
      <span class="description">{description}</span>
    {/if}
  </div>
  <Select
    id={selectId}
    {label}
    {value}
    {options}
    {onchange}
    {disabled}
    class="select-control"
  />
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

  .text {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-1);
    min-width: 0;
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

  :global(.select-control) {
    flex-shrink: 0;
    max-width: 55%;
  }
</style>
