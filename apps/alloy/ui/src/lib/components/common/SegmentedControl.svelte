<script lang="ts">
  interface Option {
    value: string;
    label: string;
  }

  interface Props {
    options: Option[];
    value?: string;
    label?: string;
    disabled?: boolean;
    onchange?: (value: string) => void;
    class?: string;
  }

  let {
    options,
    value = $bindable(options[0]?.value || ''),
    label = 'Segmented options',
    disabled = false,
    onchange,
    class: className = ''
  }: Props = $props();

  let buttonElements: (HTMLButtonElement | null)[] = $state([]);

  function selectOption(optValue: string, focusIndex?: number) {
    if (disabled) return;
    value = optValue;
    onchange?.(optValue);
    if (focusIndex !== undefined && buttonElements[focusIndex]) {
      buttonElements[focusIndex]?.focus();
    }
  }

  function handleKeyDown(event: KeyboardEvent, index: number) {
    if (disabled) return;
    const len = options.length;
    if (len === 0) return;

    let nextIndex = index;

    if (event.key === 'ArrowRight' || event.key === 'ArrowDown') {
      event.preventDefault();
      nextIndex = (index + 1) % len;
    } else if (event.key === 'ArrowLeft' || event.key === 'ArrowUp') {
      event.preventDefault();
      nextIndex = (index - 1 + len) % len;
    } else if (event.key === 'Home') {
      event.preventDefault();
      nextIndex = 0;
    } else if (event.key === 'End') {
      event.preventDefault();
      nextIndex = len - 1;
    }

    if (nextIndex !== index) {
      selectOption(options[nextIndex].value, nextIndex);
    }
  }
</script>

<div
  role="radiogroup"
  aria-label={label}
  class="alloy-segmented-control {className}"
  style="
    display: inline-flex;
    align-items: center;
    background-color: var(--alloy-color-surface-subtle, #27272a);
    border: 1px solid var(--alloy-color-border-subtle, #3f3f46);
    border-radius: var(--alloy-radii-md, 8px);
    padding: 3px;
    gap: 2px;
    user-select: none;
  "
>
  {#each options as opt, i}
    {@const isSelected = value === opt.value}
    <button
      type="button"
      role="radio"
      aria-checked={isSelected}
      aria-label={opt.label}
      tabindex={isSelected ? 0 : -1}
      disabled={disabled}
      bind:this={buttonElements[i]}
      onclick={() => selectOption(opt.value, i)}
      onkeydown={(e) => handleKeyDown(e, i)}
      class="alloy-segmented-option"
      style="
        display: inline-flex;
        align-items: center;
        justify-content: center;
        padding: 4px 10px;
        font-size: var(--alloy-typography-size-sm, 12px);
        font-weight: {isSelected ? 'var(--alloy-typography-weight-semibold, 600)' : 'var(--alloy-typography-weight-regular, 400)'};
        color: {isSelected ? 'var(--alloy-color-text-primary, #ffffff)' : 'var(--alloy-color-text-secondary, #a1a1aa)'};
        background-color: {isSelected ? 'var(--alloy-color-surface-panel, #3f3f46)' : 'transparent'};
        border: none;
        border-radius: var(--alloy-radii-sm, 4px);
        cursor: {disabled ? 'not-allowed' : 'pointer'};
        box-shadow: {isSelected ? 'var(--alloy-elevation-sm, 0 1px 2px rgba(0,0,0,0.15))' : 'none'};
        transition: background-color var(--alloy-motion-duration-fast, 150ms) ease, color var(--alloy-motion-duration-fast, 150ms) ease;
        outline: none;
        white-space: nowrap;
      "
    >
      {opt.label}
    </button>
  {/each}
</div>

<style>
  .alloy-segmented-option:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary, #4f46e5);
    outline-offset: 1px;
    z-index: 1;
  }

  /*
   * Phone and coarse pointers: segments are touch targets (44px high) and share the full width of
   * the control as equal flex items. Inline styles set no width or height, so these rules apply.
   */
  @media (max-width: 899px), (pointer: coarse) {
    .alloy-segmented-control {
      width: 100%;
      box-sizing: border-box;
    }
    .alloy-segmented-option {
      flex: 1 1 0;
      min-width: 0;
      min-height: var(--alloy-touch-target);
    }
  }
</style>
