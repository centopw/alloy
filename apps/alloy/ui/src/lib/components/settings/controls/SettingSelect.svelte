<script lang="ts">
  interface Option {
    value: string;
    label: string;
  }

  interface Props {
    label: string;
    description?: string;
    value: string;
    options: Option[];
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
  <select
    id={selectId}
    class="select"
    {disabled}
    {value}
    onchange={(event) => onchange(event.currentTarget.value)}
  >
    {#each options as option (option.value)}
      <option value={option.value}>{option.label}</option>
    {/each}
  </select>
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

  .select {
    padding: var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-3);
    border: 1px solid var(--alloy-color-border-prominent);
    border-radius: var(--alloy-radii-md);
    background: var(--alloy-color-surface-elevated);
    color: var(--alloy-color-text-primary);
    font: inherit;
    font-size: var(--alloy-typography-size-sm);
  }

  .select:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }

  .select:disabled {
    color: var(--alloy-color-text-muted);
    cursor: not-allowed;
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
    .select {
      flex-shrink: 0;
      max-width: 55%;
      min-height: 44px;
      font-size: 16px;
    }
  }
</style>
