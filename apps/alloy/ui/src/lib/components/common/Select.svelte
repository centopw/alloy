<script lang="ts">
  import Icon from '$lib/icons/Icon.svelte';
  import { ChevronDown } from '$lib/icons';
  import { contextMenu, type MenuItem } from '$lib/stores/contextmenu.svelte';

  export interface SelectOption {
    value: string;
    label: string;
    disabled?: boolean;
  }

  interface Props {
    value?: string;
    options: SelectOption[];
    onchange?: (value: string) => void;
    label?: string;
    disabled?: boolean;
    id?: string;
    class?: string;
  }

  let {
    value = $bindable(''),
    options,
    onchange,
    label,
    disabled = false,
    id,
    class: className = '',
  }: Props = $props();

  let buttonEl: HTMLButtonElement | undefined = $state();
  let isOpen = $state(false);
  let closedAt = 0;

  const selectedOption = $derived(options.find((o) => o.value === value));
  const selectedLabel = $derived(selectedOption ? selectedOption.label : value);

  function open() {
    if (disabled || !buttonEl || isOpen) return;
    const rect = buttonEl.getBoundingClientRect();

    const items: MenuItem[] = options.map((opt) => ({
      id: opt.value,
      label: opt.label,
      disabled: opt.disabled,
      selected: opt.value === value,
      onselect: () => {
        value = opt.value;
        onchange?.(opt.value);
        buttonEl?.focus();
      },
    }));

    isOpen = true;
    contextMenu.open(
      items,
      { x: rect.left, y: rect.bottom },
      {
        anchor: {
          x: rect.left,
          y: rect.top,
          width: rect.width,
          height: rect.height,
        },
        matchWidth: true,
        ariaLabel: label ?? 'Select an option',
        onclose: () => {
          isOpen = false;
          closedAt = Date.now();
        },
      },
    );
  }

  function toggle() {
    if (disabled) return;
    if (isOpen) {
      contextMenu.close();
    } else {
      if (Date.now() - closedAt < 100) return;
      open();
    }
  }

  function onButtonKeydown(e: KeyboardEvent) {
    if (disabled) return;
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      if (!isOpen) {
        open();
      }
    } else if (e.key === ' ' || e.key === 'Enter') {
      e.preventDefault();
      toggle();
    } else if (e.key === 'Escape' && isOpen) {
      e.preventDefault();
      contextMenu.close();
      buttonEl?.focus();
    }
  }
</script>

<button
  bind:this={buttonEl}
  type="button"
  {id}
  class={['select-trigger', className].filter(Boolean).join(' ')}
  aria-label={selectedLabel ? `${label}: ${selectedLabel}` : label}
  aria-haspopup="menu"
  aria-expanded={isOpen}
  disabled={disabled}
  onclick={toggle}
  onkeydown={onButtonKeydown}
>
  <span class="label">{selectedLabel}</span>
  <span class="chevron" class:open={isOpen}>
    <Icon icon={ChevronDown} size={16} />
  </span>
</button>

<style>
  .select-trigger {
    box-sizing: border-box;
    display: inline-flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--alloy-spacing-scale-2);
    min-height: 36px;
    padding: var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-3);
    border: 1px solid var(--alloy-color-border-prominent);
    border-radius: var(--alloy-radii-md);
    background: var(--alloy-color-surface-elevated);
    color: var(--alloy-color-text-primary);
    font-family: var(--alloy-typography-font-family-sans);
    font-size: var(--alloy-typography-size-sm);
    line-height: var(--alloy-typography-line-height-normal);
    text-align: left;
    cursor: pointer;
    user-select: none;
    appearance: none;
    -webkit-appearance: none;
    outline: none;
  }

  .select-trigger:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 1px;
    border-color: var(--alloy-color-accent-primary);
  }

  .select-trigger:disabled {
    color: var(--alloy-color-text-muted);
    cursor: not-allowed;
    opacity: 0.5;
  }

  .label {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .chevron {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: none;
    color: var(--alloy-color-text-secondary);
    transition: transform var(--alloy-motion-duration-fast) ease;
  }

  .chevron.open {
    transform: rotate(180deg);
  }

  @media (max-width: 899px) {
    .select-trigger {
      min-height: 44px;
      font-size: 16px;
    }
  }
</style>
