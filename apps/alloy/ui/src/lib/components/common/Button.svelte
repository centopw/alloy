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
    onclick,
    children,
    class: className = ''
  }: Props = $props();

  const iconSizes: Record<string, 16 | 20 | 24> = {
    sm: 16,
    md: 20,
    lg: 24
  };

  const paddings: Record<string, string> = {
    sm: '4px 10px',
    md: '8px 14px',
    lg: '10px 18px'
  };

  const fontSizes: Record<string, string> = {
    sm: 'var(--alloy-typography-size-sm, 12px)',
    md: 'var(--alloy-typography-size-base, 14px)',
    lg: 'var(--alloy-typography-size-lg, 16px)'
  };
</script>

<button
  {type}
  {disabled}
  aria-label={ariaLabel}
  {onclick}
  class="alloy-button variant-{variant} size-{size} {className}"
  style="
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: {paddings[size] || paddings.md};
    font-size: {fontSizes[size] || fontSizes.md};
    font-weight: var(--alloy-typography-weight-medium, 500);
    border-radius: var(--alloy-radii-md, 8px);
    cursor: {disabled ? 'not-allowed' : 'pointer'};
    opacity: {disabled ? 0.5 : 1};
    transition: background-color var(--alloy-motion-duration-fast, 150ms) ease, border-color var(--alloy-motion-duration-fast, 150ms) ease;
    user-select: none;
    box-sizing: border-box;
    white-space: nowrap;
  "
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
    border: 1px solid transparent;
  }
  .variant-primary {
    background-color: var(--alloy-color-accent-primary, #4f46e5);
    color: var(--alloy-color-text-on-accent, #ffffff);
    border-color: transparent;
  }
  .variant-primary:not(:disabled):hover {
    background-color: var(--alloy-color-accent-hover, #4338ca);
  }
  .variant-ghost {
    background-color: transparent;
    color: var(--alloy-color-text-primary, #f4f4f5);
    border-color: transparent;
  }
  .variant-ghost:not(:disabled):hover {
    background-color: var(--alloy-color-surface-hover, #27272a);
  }
  .variant-danger {
    background-color: var(--alloy-color-status-error, #dc2626);
    color: #ffffff;
    border-color: transparent;
  }
  .variant-danger:not(:disabled):hover {
    filter: brightness(0.9);
  }
  .alloy-button:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary, #4f46e5);
    outline-offset: 2px;
  }
</style>
