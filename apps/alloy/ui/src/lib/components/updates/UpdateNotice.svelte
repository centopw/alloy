<script lang="ts">
  // Bits UI supplies the progressbar role and aria-valuenow; ADR 0012 replaced the hand-rolled
  // aria-hidden track. Restyle through `class`; the vendored primitive stays unmodified.
  import { Progress } from '$lib/components/ui/progress/index.js';
  import Icon from '$lib/icons/Icon.svelte';
  import { X } from '$lib/icons';
  import { contextmenu } from '$lib/platform/contextmenu';
  import type { MenuItem } from '$lib/stores/contextmenu.svelte';
  import { updatesStore } from '$lib/stores/updates.svelte';

  import { t } from '$lib/i18n';
  const visible = $derived(updatesStore.pending !== null || updatesStore.busy);
  /**
   * A narrow chat list cannot hold the title, the second line and a labelled action at once; the
   * title is what used to be cut mid-word. Under this width the bar drops both and becomes the
   * action itself, so one click still does the thing the bar is about.
   */
  const NARROW_PX = 300;
  let narrow = $state(false);
  let noticeEl = $state<HTMLDivElement | null>(null);

  $effect(() => {
    const el = noticeEl;
    if (!el) return;
    const measure = () => {
      narrow = el.clientWidth > 0 && el.clientWidth < NARROW_PX;
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  });

  /** The bar's own click: on a narrow list it applies the update, otherwise it opens what's new. */
  function activate() {
    if (narrow) void updatesStore.primary();
    else updatesStore.openChangelog();
  }
  const percent = $derived(
    updatesStore.progress === null ? null : Math.round(updatesStore.progress * 100),
  );

  /** Two quiet lines: what is happening, and what the user may do about it. */
  const copy = $derived.by(() => {
    const version = updatesStore.info?.version;
    switch (updatesStore.phase) {
      case 'error':
        return { title: t('update-notice-update-failed'), detail: t('update-notice-keep-using-alloy'), action: t('common-retry') };
      case 'installing':
        return { title: t('update-notice-installing-update'), detail: t('update-notice-almost-done'), action: null };
      case 'downloading':
        return {
          title: t('update-notice-downloading-update'),
          detail: percent === null ? t('update-notice-working-in-background') : `${percent}%`,
          action: null,
        };
      case 'ready':
        return { title: t('update-notice-update-ready'), detail: t('update-notice-restart-to-apply'), action: t('update-notice-restart') };
      default:
        return {
          title: t('update-notice-update-available'),
          detail: version ? t('changelog-dialog-version', { version }) : t('update-notice-see-whats-new'),
          action: updatesStore.info?.can_install === false ? t('update-notice-open') : t('update-notice-download'),
        };
    }
  });

  /**
   * The bar's label when the list is too narrow to show the title and an action side by side. It
   * has to say what the click does: "Restart to apply" when the update is downloaded, otherwise the
   * action word itself. Everything comes from the bundles, so it follows the locale.
   */
  const narrowLabel = $derived(
    copy.action === null
      ? copy.title
      : updatesStore.phase === 'ready'
        ? t('update-notice-restart-to-apply')
        : copy.action,
  );

  function menuItems(): MenuItem[] {
    const items: MenuItem[] = [
      { id: 'whats-new', label: t('help-about-whats-new'), onselect: () => updatesStore.openChangelog() },
      {
        id: 'primary',
        label: updatesStore.primaryLabel,
        disabled: updatesStore.busy,
        onselect: () => void updatesStore.primary(),
      },
      { id: 'remind-later', label: t('update-notice-remind-me-later'), onselect: () => updatesStore.remindLater() },
    ];
    // Once the download has started, skipping would leave a downloaded update unused.
    if (!updatesStore.info?.downloaded && !updatesStore.busy) {
      items.push({ id: 'skip-version', label: t('changelog-dialog-skip-this-version'), onselect: () => updatesStore.skipVersion() });
    }
    items.push({ id: 'check', label: t('help-about-check-for-updates'), onselect: () => void updatesStore.check() });
    return items;
  }
</script>

{#if visible}
  <div class="notice" class:failed={updatesStore.phase === 'error'} bind:this={noticeEl} use:contextmenu={menuItems}>
    <button
      type="button"
      class="body"
      class:narrow
      aria-label={narrow && copy.action ? `${copy.title}. ${copy.action}` : undefined}
      onclick={activate}
    >
      <span class="dot" aria-hidden="true"></span>
      <span class="text">
        {#if narrow && copy.action}
          <!-- A narrow list cannot hold "what" and "do" side by side, and hiding the action left a
               bar that said "Update ready" with nothing to press. The action's own label carries
               both: "Restart to apply update" says what and what to do. -->
          <span class="title action-title">{narrowLabel}</span>
        {:else}
          <span class="title">{copy.title}</span>
          <span class="detail">{copy.detail}</span>
        {/if}
      </span>
    </button>
    {#if copy.action}
      <button type="button" class="action" onclick={() => void updatesStore.primary()}>{copy.action}</button>
    {/if}
    {#if !updatesStore.busy}
      <button
        type="button"
        class="dismiss"
        aria-label={t('update-notice-remind-me-later')}
        data-tooltip={t('update-notice-remind-me-later')}
        onclick={() => updatesStore.remindLater()}
      >
        <Icon icon={X} size={16} />
      </button>
    {/if}
    {#if updatesStore.phase === 'downloading'}
      <!--
        Known size: a determinate bar. Unknown size (null) keeps correct indeterminate ARIA from
        Bits UI but renders an empty track, so the track itself pulses to read as "still working"
        rather than stalled — the same intent as the old 30% placeholder.
      -->
      <Progress
        value={percent}
        aria-label={percent === null ? t('update-notice-working-on-update') : t('update-notice-downloading-update')}
        class="absolute inset-x-0 bottom-0 h-[2px] w-full overflow-hidden rounded-none
          bg-[var(--alloy-color-border-subtle)]
          [&_[data-slot=progress-indicator]]:bg-[var(--alloy-color-accent-primary)]
          [&_[data-slot=progress-indicator]]:duration-200
          {percent === null ? 'animate-pulse' : ''}"
      />
    {/if}
  </div>
{/if}

<style>
  .notice {
    position: relative;
    display: flex;
    flex-shrink: 0;
    align-items: center;
    gap: var(--alloy-spacing-scale-1);
    /* Inset on all four sides: the old bottom margin of zero left the bar glued to the window edge,
       where a rounded window corner cut it in half. */
    margin: var(--alloy-layout-row-inset);
    padding: 0 var(--alloy-spacing-scale-1) 0 0;
    overflow: hidden;
    border-radius: var(--alloy-radii-lg);
    background: var(--alloy-color-accent-subtle);
  }

  .body {
    display: flex;
    flex: 1;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    min-width: 0;
    /* One compact line: the bar sits under the chat list and every pixel it keeps is a row the
       user cannot see. The touch target grows back to 44px on a phone, below. */
    min-height: 36px;
    padding: 0 var(--alloy-spacing-scale-1) 0 var(--alloy-layout-row-pad-x);
    border: 0;
    background: transparent;
    color: inherit;
    font: inherit;
    text-align: start;
    cursor: pointer;
  }

  .dot {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: var(--alloy-radii-full);
    background: var(--alloy-color-accent-primary);
  }

  .failed .dot {
    background: var(--alloy-color-text-muted);
  }

  .text {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .title,
  .detail {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .title {
    color: var(--alloy-color-text-primary);
    font-size: var(--alloy-typography-size-sm);
    font-weight: var(--alloy-typography-weight-semibold);
  }

  .detail {
    color: var(--alloy-color-text-secondary);
    font-size: var(--alloy-typography-size-xs);
  }

  .action,
  .dismiss {
    flex: none;
    min-height: 36px;
    border: 0;
    border-radius: var(--alloy-radii-md);
    background: transparent;
    font: inherit;
    cursor: pointer;
  }

  .action {
    padding: 0 var(--alloy-spacing-scale-2);
    color: var(--alloy-color-text-link);
    font-size: var(--alloy-typography-size-sm);
    font-weight: var(--alloy-typography-weight-semibold);
  }

  .dismiss {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 36px;
    color: var(--alloy-color-text-muted);
  }

  /* A narrow chat list cannot hold the title, the second line and a labelled action at once; the
     title was what got cut mid-word ("Update a…"). Both yield, and the component makes the bar
     itself apply the update. */
  @container (max-width: 300px) {
    .detail,
    .action {
      display: none;
    }
  }

  /* Phone: the bar is a full-width row again, so it keeps the 44px touch target and both lines. */
  @media (max-width: 899px) {
    .body {
      min-height: 44px;
      padding: var(--alloy-spacing-scale-1) var(--alloy-spacing-scale-1) var(--alloy-spacing-scale-1)
        var(--alloy-layout-row-pad-x);
    }
    .action,
    .dismiss {
      min-height: 44px;
    }
    .dismiss {
      min-width: 44px;
    }
    .detail {
      display: block;
    }
  }

  .action:hover,
  .dismiss:hover {
    background: var(--alloy-color-surface-hover);
  }

  .dismiss:hover {
    color: var(--alloy-color-text-primary);
  }

  .body:focus-visible,
  .action:focus-visible,
  .dismiss:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: -2px;
  }
</style>
