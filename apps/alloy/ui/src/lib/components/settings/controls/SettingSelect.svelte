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

  :global(.select-control) {
    flex-shrink: 0;
    max-width: 55%;
  }

  @media (max-width: 899px) {
    .setting-row {
      min-height: 56px;
      padding: var(--alloy-spacing-scale-2) var(--alloy-layout-row-pad-x);
    }
    .label {
      font-size: var(--alloy-typography-size-lg);
    }
    .text {
      flex: 1;
    }
  }
</style>
