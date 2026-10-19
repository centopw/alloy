<script lang="ts">
  import type { HTMLTextareaAttributes } from 'svelte/elements';

  interface Props extends Omit<HTMLTextareaAttributes, 'value'> {
    value?: string;
    invalid?: boolean;
    el?: HTMLTextAreaElement | null;
    rows?: number;
    maxRows?: number;
    autoGrow?: boolean;
    class?: string;
  }

  let {
    value = $bindable(''),
    invalid = false,
    el = $bindable(null),
    rows = 3,
    maxRows,
    autoGrow = false,
    class: className = '',
    oninput,
    ...rest
  }: Props = $props();

  const shouldAutoGrow = $derived(autoGrow || maxRows !== undefined);

  function adjustHeight() {
    if (!el || !shouldAutoGrow) return;
    el.style.height = 'auto';
    if (maxRows !== undefined) {
      const cs = window.getComputedStyle(el);
      const lh = parseFloat(cs.lineHeight) || 20;
      const pt = parseFloat(cs.paddingTop) || 0;
      const pb = parseFloat(cs.paddingBottom) || 0;
      const bt = parseFloat(cs.borderTopWidth) || 0;
      const bb = parseFloat(cs.borderBottomWidth) || 0;
      const maxHeight = lh * maxRows + pt + pb + bt + bb;
      if (el.scrollHeight > maxHeight) {
        el.style.height = `${maxHeight}px`;
        el.style.overflowY = 'auto';
      } else {
        el.style.height = `${el.scrollHeight}px`;
        el.style.overflowY = 'hidden';
      }
    } else {
      el.style.height = `${el.scrollHeight}px`;
    }
  }

  $effect(() => {
    void value;
    if (shouldAutoGrow) {
      adjustHeight();
    }
  });

  function handleInput(e: Event & { currentTarget: EventTarget & HTMLTextAreaElement }) {
    if (shouldAutoGrow) {
      adjustHeight();
    }
    oninput?.(e);
  }
</script>

<textarea
  bind:this={el}
  bind:value
  {rows}
  class={['text-area', className].filter(Boolean).join(' ')}
  class:invalid
  class:grows={shouldAutoGrow}
  aria-invalid={invalid ? 'true' : undefined}
  oninput={handleInput}
  {...rest}
></textarea>

<style>
  .text-area {
    box-sizing: border-box;
    width: 100%;
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
    resize: vertical;
  }

  /* An auto-growing field sizes itself, so it has no manual resize grip. */
  .text-area.grows {
    resize: none;
  }

  .text-area::placeholder {
    color: var(--alloy-color-text-muted);
  }

  .text-area:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 1px;
    border-color: var(--alloy-color-accent-primary);
  }

  .text-area:disabled {
    opacity: 0.5;
    cursor: not-allowed;
    color: var(--alloy-color-text-muted);
  }

  .text-area.invalid,
  .text-area[aria-invalid='true'] {
    border-color: var(--alloy-color-status-error);
  }

  .text-area.invalid:focus-visible {
    outline-color: var(--alloy-color-status-error);
    border-color: var(--alloy-color-status-error);
  }

  @media (max-width: 899px) {
    .text-area {
      font-size: 16px;
    }
  }
</style>
