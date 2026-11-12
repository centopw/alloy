<script lang="ts">
  import { getInitials } from '$lib/identity/initials';
  import {
    AVATAR_WHITE,
    getUserColorValue,
    getUserColorVar,
    pickAvatarOnColor
  } from '$lib/identity/userColor';
  import { themeStore } from '$lib/theme/theme.svelte';

  import { t } from '$lib/i18n';
  interface Props {
    userId?: string;
    displayName?: string | null;
    /** A URL the webview can load directly (already resolved through the media cache). */
    src?: string | null;
    /** Named preset, or a diameter in CSS pixels. */
    size?: 'xs' | 'sm' | 'md' | 'lg' | 'xl' | number;
    /** `circle` for users and spaces; `rounded` for rooms. */
    shape?: 'circle' | 'rounded';
    presence?: 'online' | 'away' | 'busy' | 'offline' | null;
    alt?: string;
    class?: string;
  }

  let {
    userId,
    displayName,
    src = null,
    size = 'md',
    shape = 'circle',
    presence = null,
    alt,
    class: className = ''
  }: Props = $props();

  /** The image URL that last failed to load, so a failed source falls back to initials. */
  let failedSrc = $state<string | null>(null);

  const sizePixels: Record<string, number> = {
    xs: 20,
    sm: 28,
    md: 36,
    lg: 48,
    xl: 64
  };

  const fontSizes: Record<string, number> = {
    xs: 10,
    sm: 12,
    md: 14,
    lg: 18,
    xl: 24
  };

  const dotSizes: Record<string, number> = {
    xs: 6,
    sm: 8,
    md: 10,
    lg: 12,
    xl: 14
  };

  const currentSize = $derived(typeof size === 'number' ? size : (sizePixels[size] ?? 36));
  const currentFontSize = $derived(
    typeof size === 'number' ? Math.round(size * 0.38) : (fontSizes[size] ?? 14)
  );
  const currentDotSize = $derived(
    typeof size === 'number'
      ? Math.min(14, Math.max(6, Math.round(size * 0.22)))
      : (dotSizes[size] ?? 10)
  );
  const radius = $derived(
    shape === 'rounded'
      ? `min(calc(${currentSize}px * 0.28), var(--alloy-radii-lg))`
      : 'var(--alloy-radii-full)'
  );
  const initials = $derived(getInitials(displayName, userId));
  const userColor = $derived(getUserColorValue(userId));
  const effectiveAlt = $derived(alt || displayName || userId || 'Avatar');
  /**
   * A11Y-20: `role="img"` makes descendants presentational, so presence has to live in the
   * accessible name instead of a child label. The name is announced once: the inner <img> is
   * decorative (`alt=""`) and the dot is `aria-hidden`.
   */
  const accessibleName = $derived(
    presence ? `${effectiveAlt}, ${t('avatar-presence-status', { presence })}` : effectiveAlt
  );

  /** A11Y-01: initials colour comes from the resolved fill, not a fixed white. */
  let rootEl = $state<HTMLElement | undefined>();
  let onColor = $state<string>(AVATAR_WHITE);

  /** Resolve the avatar fill from computed style (the inline background is `var(--alloy-color-user-N)`). */
  function resolveAvatarFill(element: HTMLElement): string | null {
    const computed = getComputedStyle(element).backgroundColor;
    if (computed && computed !== 'transparent' && computed !== 'rgba(0, 0, 0, 0)') return computed;
    if (typeof document === 'undefined') return null;
    return getComputedStyle(document.documentElement).getPropertyValue(getUserColorVar(userId)).trim() || null;
  }

  $effect(() => {
    // Re-run after a theme switch (the palette has changed by then) and when a reusable row hands
    // this instance a different user. Both are reactive reads, not values the effect uses.
    void themeStore.activeTheme;
    void userId;
    if (!rootEl) return;
    onColor = pickAvatarOnColor(resolveAvatarFill(rootEl));
  });

  const imageSrc = $derived(src || null);
</script>

<div
  class="alloy-avatar {className}"
  style="
    position: relative;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: {currentSize}px;
    height: {currentSize}px;
    min-width: {currentSize}px;
    min-height: {currentSize}px;
    border-radius: {radius};
    background-color: {userColor};
    color: {onColor};
    font-size: {currentFontSize}px;
    font-weight: var(--alloy-typography-weight-semibold);
    user-select: none;
    overflow: visible;
  "
  bind:this={rootEl}
  role="img"
  aria-label={accessibleName}
>
  {#if imageSrc && imageSrc !== failedSrc}
    <img
      src={imageSrc}
      alt=""
      onerror={() => (failedSrc = imageSrc)}
      style="
        width: 100%;
        height: 100%;
        object-fit: cover;
        border-radius: inherit;
      "
    />
  {:else}
    <span class="alloy-avatar-initials" style="line-height: 1;">
      {initials}
    </span>
  {/if}

  {#if presence}
    <span
      class="alloy-avatar-presence"
      data-presence={presence}
      aria-hidden="true"
      style="
        position: absolute;
        bottom: 0;
        right: 0;
        width: {currentDotSize}px;
        height: {currentDotSize}px;
        border-radius: var(--alloy-radii-full);
        background-color: var(--alloy-color-status-{presence});
        border: 2px solid var(--alloy-color-surface-canvas);
        box-sizing: content-box;
      "
    ></span>
  {/if}
</div>
