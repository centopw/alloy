<script lang="ts">
  import { getInitials } from '$lib/identity/initials';
  import { getUserColorValue } from '$lib/identity/userColor';
  import { avatarUrl } from '$lib/stores/avatars.svelte';

  interface Props {
    userId?: string;
    displayName?: string | null;
    /** An `mxc://` URI (resolved through the media cache) or a URL the webview can load directly. */
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

  /** Resolved URL for an `mxc://` source; null while loading or on failure. */
  let mediaUrl = $state<string | null>(null);
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

  const imageSrc = $derived(src?.startsWith('mxc://') ? mediaUrl : (src || null));

  // Resolve mxc sources at the rendered size. Clearing first keeps the initials
  // visible while a new source loads, so a stale image never shows for the new user.
  $effect(() => {
    const source = src;
    const px = currentSize;
    mediaUrl = null;
    if (!source?.startsWith('mxc://')) return;
    let cancelled = false;
    avatarUrl(source, px).then((url) => {
      if (!cancelled) mediaUrl = url;
    });
    return () => {
      cancelled = true;
    };
  });
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
    color: var(--alloy-color-text-on-accent);
    font-size: {currentFontSize}px;
    font-weight: var(--alloy-typography-weight-semibold);
    user-select: none;
    overflow: visible;
  "
  role="img"
  aria-label={effectiveAlt}
>
  {#if imageSrc && imageSrc !== failedSrc}
    <img
      src={imageSrc}
      alt={effectiveAlt}
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
      aria-label="Status: {presence}"
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
