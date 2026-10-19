<script lang="ts">
  import type { Component } from 'svelte';
  import type { HTMLInputAttributes } from 'svelte/elements';
  import Icon from '$lib/icons/Icon.svelte';

  type IconComponent = Component<{ size?: number; strokeWidth?: number; class?: string }>;

  interface Props extends Omit<HTMLInputAttributes, 'value'> {
    value?: string;
    invalid?: boolean;
    el?: HTMLInputElement | null;
    icon?: IconComponent;
    class?: string;
  }

  let {
    value = $bindable(''),
    invalid = false,
    el = $bindable(null),
    icon,
    class: className = '',
    type = 'text',
    ...rest
  }: Props = $props();
</script>

{#if icon}
  <div class={['text-field-wrap', className].filter(Boolean).join(' ')}>
    <span class="icon"><Icon {icon} size={16} /></span>
    <input
      bind:this={el}
      bind:value
      {type}
      class="text-field has-icon"
      class:invalid
      aria-invalid={invalid ? 'true' : undefined}
      {...rest}
    />
  </div>
{:else}
  <input
    bind:this={el}
    bind:value
    {type}
    class={['text-field', className].filter(Boolean).join(' ')}
    class:invalid
    aria-invalid={invalid ? 'true' : undefined}
    {...rest}
  />
{/if}

<style>
  .text-field {
    box-sizing: border-box;
    width: 100%;
    min-height: 36px;
    padding: var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-3);
    border: 1px solid var(--alloy-color-border-prominent);
    border-radius: var(--alloy-radii-md);
    background: var(--alloy-color-surface-elevated);
    color: var(--alloy-color-text-primary);
    font-family: var(--alloy-typography-font-family-sans);
    font-size: var(--alloy-typography-size-sm);
    line-height: var(--alloy-typography-line-height-normal);
    appearance: none;
    -webkit-appearance: none;
    outline: none;
  }

  .text-field::placeholder {
    color: var(--alloy-color-text-muted);
  }

  .text-field:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 1px;
    border-color: var(--alloy-color-accent-primary);
  }

  .text-field:disabled {
    opacity: 0.5;
    cursor: not-allowed;
    color: var(--alloy-color-text-muted);
  }

  .text-field.invalid,
  .text-field[aria-invalid='true'] {
    border-color: var(--alloy-color-status-error);
  }

  .text-field.invalid:focus-visible {
    outline-color: var(--alloy-color-status-error);
    border-color: var(--alloy-color-status-error);
  }

  .text-field-wrap {
    position: relative;
    display: flex;
    align-items: center;
    width: 100%;
  }

  .text-field-wrap .icon {
    position: absolute;
    left: var(--alloy-spacing-scale-3);
    display: inline-flex;
    align-items: center;
    color: var(--alloy-color-text-muted);
    pointer-events: none;
    z-index: 1;
  }

  .text-field-wrap .has-icon {
    padding-left: calc(var(--alloy-spacing-scale-3) * 2 + 16px);
  }

  @media (max-width: 899px) {
    .text-field {
      min-height: 44px;
      font-size: 16px;
    }
  }
</style>
