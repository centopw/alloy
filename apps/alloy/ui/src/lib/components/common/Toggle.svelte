<script lang="ts">
  interface Props {
    checked?: boolean;
    label: string;
    disabled?: boolean;
    onchange?: (checked: boolean) => void;
    class?: string;
  }

  let {
    checked = $bindable(false),
    label,
    disabled = false,
    onchange,
    class: className = ''
  }: Props = $props();

  function toggle() {
    if (disabled) return;
    checked = !checked;
    onchange?.(checked);
  }

  function handleKeyDown(event: KeyboardEvent) {
    if (event.key === ' ' || event.key === 'Enter') {
      event.preventDefault();
      toggle();
    }
  }
</script>

<button
  type="button"
  role="switch"
  aria-checked={checked}
  aria-label={label}
  aria-disabled={disabled}
  {disabled}
  tabindex={disabled ? -1 : 0}
  onclick={toggle}
  onkeydown={handleKeyDown}
  class="alloy-toggle {className}"
  style="
    position: relative;
    display: inline-flex;
    align-items: center;
    width: 40px;
    height: 22px;
    padding: 2px;
    border-radius: var(--alloy-radii-full, 9999px);
    background-color: {checked ? 'var(--alloy-color-accent-primary, #4f46e5)' : 'var(--alloy-color-surface-hover, #3f3f46)'};
    border: none;
    cursor: {disabled ? 'not-allowed' : 'pointer'};
    opacity: {disabled ? 0.5 : 1};
    transition: background-color var(--alloy-motion-duration-fast, 150ms) ease;
    outline: none;
    box-sizing: border-box;
  "
>
  <span
    class="alloy-toggle-thumb"
    style="
      display: block;
      width: 18px;
      height: 18px;
      border-radius: 50%;
      background-color: #ffffff;
      box-shadow: var(--alloy-elevation-sm, 0 1px 3px rgba(0, 0, 0, 0.2));
      transform: translateX({checked ? '18px' : '0px'});
      transition: transform var(--alloy-motion-duration-fast, 150ms) cubic-bezier(0.4, 0, 0.2, 1);
    "
  ></span>
</button>

<style>
  @media (prefers-reduced-motion: reduce) {
    .alloy-toggle,
    .alloy-toggle-thumb {
      transition: none !important;
    }
  }
  :global([data-reduced-motion="true"]) .alloy-toggle,
  :global([data-reduced-motion="true"]) .alloy-toggle-thumb {
    transition: none !important;
  }
  .alloy-toggle:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary, #4f46e5);
    outline-offset: 2px;
  }
  /*
   * Phone and coarse pointers: the visible track stays 40x22 (inline style), but the hit area grows to
   * 44px tall and 44px wide through an overlay. The button is position: relative (inline style).
   */
  @media (max-width: 899px), (pointer: coarse) {
    .alloy-toggle::before {
      content: '';
      position: absolute;
      inset: -11px -2px;
    }
  }
</style>
