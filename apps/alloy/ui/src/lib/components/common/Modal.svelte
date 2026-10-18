<script lang="ts">
  import type { Snippet } from 'svelte';
  import { tick } from 'svelte';
  import Icon from '$lib/icons/Icon.svelte';
  import { X } from '$lib/icons';
  import { pushBack } from '$lib/platform/backstack';

  interface Props {
    open: boolean;
    title: string;
    onclose: () => void;
    children: Snippet;
    width?: string;
  }
  let { open, title, onclose, children, width = '520px' }: Props = $props();

  let dialog: HTMLDivElement | undefined = $state();
  let opener: HTMLElement | null = null;

  // Android Back closes the dialog.
  $effect(() => {
    if (!open) return;
    return pushBack(() => onclose());
  });

  $effect(() => {
    if (open) {
      opener = document.activeElement as HTMLElement | null;
      void tick().then(() => dialog?.focus());
    } else {
      opener?.focus?.();
      opener = null;
    }
  });

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.stopPropagation();
      onclose();
    } else if (e.key === 'Tab' && dialog) {
      // Keep focus inside the dialog.
      const f = dialog.querySelectorAll<HTMLElement>(
        'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])'
      );
      if (f.length === 0) return;
      const first = f[0];
      const last = f[f.length - 1];
      if (e.shiftKey && document.activeElement === first) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && document.activeElement === last) {
        e.preventDefault();
        first.focus();
      }
    }
  }
</script>

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="backdrop" onclick={onclose}>
    <div
      class="dialog"
      role="dialog"
      aria-modal="true"
      aria-label={title}
      tabindex="-1"
      style="width: min({width}, 94vw);"
      bind:this={dialog}
      onclick={(e) => e.stopPropagation()}
      onkeydown={onKey}
    >
      <header>
        <h2>{title}</h2>
        <button type="button" aria-label="Close" onclick={onclose}><Icon icon={X} size={20} /></button>
      </header>
      <div class="content">{@render children()}</div>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.5);
    display: grid;
    place-items: center;
    z-index: 60;
  }
  .dialog {
    max-height: 88vh;
    display: flex;
    flex-direction: column;
    background: var(--alloy-color-surface-elevated);
    color: var(--alloy-color-text-primary);
    border: 1px solid var(--alloy-color-border-prominent);
    border-radius: var(--alloy-radii-lg);
    box-shadow: var(--alloy-elevation-lg);
    outline: 0;
  }
  header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    height: var(--alloy-layout-header-height);
    padding: 0 var(--alloy-layout-gutter);
    border-bottom: 1px solid var(--alloy-color-border-subtle);
  }
  h2 {
    margin: 0;
    font-size: var(--alloy-typography-size-lg);
  }
  header button {
    border: 0;
    background: transparent;
    color: var(--alloy-color-text-secondary);
    cursor: pointer;
    border-radius: var(--alloy-radii-full);
    width: 32px;
    height: 32px;
    display: grid;
    place-items: center;
  }
  header button:hover {
    background: var(--alloy-color-surface-hover);
  }
  .content {
    padding: var(--alloy-layout-gutter);
    overflow-y: auto;
  }
</style>
