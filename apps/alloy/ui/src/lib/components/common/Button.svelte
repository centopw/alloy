<script lang="ts">
  import type { Component, Snippet } from 'svelte';
  import { Icon } from '$lib/icons';

  interface Props {
    variant?: 'primary' | 'ghost' | 'danger';
    size?: 'sm' | 'md' | 'lg';
    disabled?: boolean;
    type?: 'button' | 'submit' | 'reset';
    icon?: Component<{ size?: number; strokeWidth?: number; class?: string }>;
    ariaLabel?: string;
    tooltip?: string;
    shortcut?: string;
    onclick?: (event: MouseEvent) => void;
    children?: Snippet;
    class?: string;
  }

  let {
    variant = 'primary',
    size = 'md',
    disabled = false,
    type = 'button',
    icon,
    ariaLabel,
    tooltip,
    shortcut,
    onclick,
    children,
    class: className = ''
  }: Props = $props();

  const iconSizes: Record<string, 16 | 20 | 24> = {
    sm: 16,
    md: 20,
    lg: 24
  };
</script>

<button
  {type}
  {disabled}
  aria-label={ariaLabel}
  data-tooltip={tooltip}
  data-shortcut={shortcut}
  {onclick}
  class="alloy-button variant-{variant} size-{size} {className}"
  class:icon-only={!!icon && !children}
>
  {#if icon}
    <Icon {icon} size={iconSizes[size] || 20} strokeWidth={1.5} />
  {/if}
  {#if children}
    {@render children()}
  {/if}
</button>

<style>
  .alloy-button {
    box-sizing: border-box;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px; /* token-audit: control rhythm between the 4px and 8px layout steps */
    height: var(--alloy-layout-control-height);
    padding: 0 14px; /* token-audit: control inset, sized by the button not the layout grid */
    border: 1px solid transparent;
    border-radius: var(--alloy-radii-md);
    font-family: inherit;
    font-size: var(--alloy-typography-size-base, 14px);
    font-weight: var(--alloy-typography-weight-medium, 500);
    line-height: inherit;
    white-space: nowrap;
    user-select: none;
    cursor: pointer;
    transition: background-color var(--alloy-motion-duration-fast, 150ms) ease, border-color var(--alloy-motion-duration-fast, 150ms) ease;
  }
  .alloy-button:disabled {
    cursor: not-allowed;
    opacity: 0.5;
  }
  .size-sm {
    height: var(--alloy-layout-control-height-sm);
    padding: 0 10px; /* token-audit: control inset, sized by the button not the layout grid */
    font-size: var(--alloy-typography-size-sm, 12px);
  }
  .size-lg {
    padding: 0 18px; /* token-audit: control inset, sized by the button not the layout grid */
    font-size: var(--alloy-typography-size-lg, 16px);
  }
  /* Icon-only buttons are square at their height. */
  .alloy-button.icon-only {
    width: var(--alloy-layout-control-height);
    padding: 0;
  }
  .alloy-button.icon-only.size-sm {
    width: var(--alloy-layout-control-height-sm);
  }
  .variant-primary {
    background-color: var(--alloy-color-accent-fill);
    color: var(--alloy-color-text-on-accent);
    border-color: transparent;
  }
  .variant-primary:not(:disabled):hover {
    background-color: var(--alloy-color-accent-hover);
  }
  .variant-ghost {
    background-color: transparent;
    color: var(--alloy-color-text-primary);
    border-color: transparent;
  }
  .variant-ghost:not(:disabled):hover {
    background-color: var(--alloy-color-surface-hover);
  }
  .variant-danger {
    background-color: var(--alloy-color-status-error);
    color: var(--alloy-color-text-on-accent);
    border-color: transparent;
  }
  .variant-danger:not(:disabled):hover {
    filter: brightness(0.9);
  }
  /* Touch: every button is a full-size target, whatever its desktop size. */
  @media (max-width: 899px) {
    .alloy-button {
      min-height: 44px;
      min-width: 44px;
    }
  }
  .alloy-button:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }
</style>
