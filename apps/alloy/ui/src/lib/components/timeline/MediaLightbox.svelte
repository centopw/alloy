<script lang="ts">
  /**
   * A picture at full size, over the conversation.
   *
   * Clicking a picture used to hand its file path to the operating system, which opened whatever
   * the OS associates with it — or nothing, on a build where the opener refuses local paths. The
   * lightbox keeps the picture inside the app: one surface, one close control, and the download
   * path the message already owns.
   */
  import Icon from '$lib/icons/Icon.svelte';
  import { Download, X } from '$lib/icons';
  import { t } from '$lib/i18n';

  interface Props {
    src: string;
    alt: string;
    onclose: () => void;
    /** Saves the file through the caller's download path; omitted when the caller cannot save. */
    ondownload?: (() => void) | null;
  }

  let { src, alt, onclose, ondownload = null }: Props = $props();

  let rootEl = $state<HTMLDivElement | null>(null);
  let closeEl = $state<HTMLButtonElement | null>(null);
  let opener: Element | null = null;

  $effect(() => {
    // Remember where the keyboard was, focus the close control, and give it back on the way out.
    opener = document.activeElement;
    closeEl?.focus();
    return () => {
      if (opener instanceof HTMLElement) opener.focus();
    };
  });

  function onkeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      event.preventDefault();
      onclose();
      return;
    }
    // Keep Tab inside the overlay: it is modal, and the page behind it is not reachable by pointer.
    if (event.key !== 'Tab' || !rootEl) return;
    const buttons = Array.from(rootEl.querySelectorAll<HTMLElement>('button'));
    if (buttons.length === 0) return;
    const first = buttons[0];
    const last = buttons[buttons.length - 1];
    const active = document.activeElement;
    const inside = active instanceof Node && rootEl.contains(active);
    if (event.shiftKey && (active === first || !inside)) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && active === last) {
      event.preventDefault();
      first.focus();
    }
  }
</script>

<svelte:window onkeydown={onkeydown} />

<!-- The backdrop is a real control so clicking it needs no handler on a static element; the picture
     and the controls sit above it. -->
<div class="lightbox" role="dialog" aria-modal="true" aria-label={alt} bind:this={rootEl}>
  <!-- The backdrop closes on click but is not a second tab stop: Escape and the close control do
       the same job for the keyboard, and two controls named "Close" only add noise. -->
  <button
    type="button"
    class="scrim"
    tabindex="-1"
    aria-label={t('common-close')}
    onclick={onclose}
  ></button>
  <div class="stage">
    <img {src} {alt} />
  </div>
  <div class="controls">
    {#if ondownload}
      <button
        type="button"
        class="control"
        aria-label={t('media-download', { name: alt })}
        data-tooltip={t('media-view-download')}
        onclick={() => ondownload?.()}
      >
        <Icon icon={Download} size={20} />
      </button>
    {/if}
    <button
      type="button"
      class="control close"
      bind:this={closeEl}
      aria-label={t('common-close')}
      data-tooltip={t('common-close')}
      onclick={onclose}
    >
      <Icon icon={X} size={20} />
    </button>
  </div>
</div>

<style>
  .lightbox {
    position: fixed;
    inset: 0;
    z-index: 80;
    display: grid;
    place-items: center;
    padding: var(--alloy-spacing-scale-6);
  }
  .scrim {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    padding: 0;
    border: 0;
    background: color-mix(in srgb, var(--alloy-color-surface-canvas) 92%, transparent);
    -webkit-backdrop-filter: blur(12px);
    backdrop-filter: blur(12px);
    cursor: default;
  }
  .scrim:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: -4px;
  }
  .stage {
    position: relative;
    display: grid;
    place-items: center;
    max-width: 100%;
    max-height: 100%;
  }
  img {
    display: block;
    max-width: min(92vw, 1600px);
    max-height: 86vh;
    border-radius: var(--alloy-radii-lg);
    box-shadow: var(--alloy-glass-shadow-soft);
    /* The picture is content, not a control: no selection handles, no drag ghost. */
    user-select: none;
    -webkit-user-drag: none;
  }
  .controls {
    position: absolute;
    top: var(--alloy-spacing-scale-4);
    right: var(--alloy-spacing-scale-4);
    display: flex;
    gap: var(--alloy-spacing-scale-2);
  }
  .control {
    display: inline-grid;
    place-items: center;
    width: 40px;
    height: 40px;
    padding: 0;
    border: 1px solid var(--alloy-glass-rim);
    border-radius: var(--alloy-radii-full);
    background: var(--alloy-glass-fill-strong);
    color: var(--alloy-color-text-primary);
    cursor: pointer;
  }
  .control:hover {
    background: var(--alloy-glass-control-hover);
  }
  .control:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }
  @media (max-width: 899px) {
    .lightbox {
      padding: var(--alloy-spacing-scale-2);
    }
    .control {
      width: 44px;
      height: 44px;
    }
    img {
      max-height: 80vh;
    }
  }
</style>
