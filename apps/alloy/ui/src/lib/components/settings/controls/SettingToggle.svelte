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
  @media (max-width: 899px) {
    .setting-row {
      min-height: 56px;
      padding: var(--alloy-spacing-scale-2) var(--alloy-layout-row-pad-x);
    }
    .label {
      font-size: var(--alloy-typography-size-lg);
    }
    .setting-row :global(.alloy-toggle) {
      flex-shrink: 0;
    }
    /*
     * The shadcn-svelte track is 32x18.4px and carries a 1px border each side, so its padding box is
     * 16.4px tall and 30px wide. A pseudo-element resolves against that padding box: -14px
     * vertically gives 16.4 + 28 = 44.4px and -7px horizontally gives 30 + 14 = 44px, so the tap
     * target clears ADR 0005's 44x44 minimum without changing the visible track.
     */
    .setting-row :global(.alloy-toggle)::before {
      content: '';
      position: absolute;
      inset: -14px -7px;
    }
  }
</style>
