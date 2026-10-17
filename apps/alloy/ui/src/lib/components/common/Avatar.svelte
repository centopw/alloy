<script lang="ts">
  import { getInitials } from '$lib/identity/initials';
  import { getUserColorValue } from '$lib/identity/userColor';

  interface Props {
    userId?: string;
    displayName?: string | null;
    src?: string | null;
    size?: 'xs' | 'sm' | 'md' | 'lg' | 'xl';
    presence?: 'online' | 'away' | 'busy' | 'offline' | null;
    alt?: string;
    class?: string;
  }

  let {
    userId,
    displayName,
    src = null,
    size = 'md',
    presence = null,
    alt,
    class: className = ''
  }: Props = $props();

  let imgFailed = $state(false);

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

  const currentSize = $derived(sizePixels[size] || 36);
  const currentFontSize = $derived(fontSizes[size] || 14);
  const currentDotSize = $derived(dotSizes[size] || 10);
  const initials = $derived(getInitials(displayName, userId));
  const userColor = $derived(getUserColorValue(userId));
  const effectiveAlt = $derived(alt || displayName || userId || 'Avatar');
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
    border-radius: var(--alloy-radii-full, 9999px);
    background-color: {userColor};
    color: #ffffff;
    font-size: {currentFontSize}px;
    font-weight: var(--alloy-typography-weight-semibold, 600);
    user-select: none;
    overflow: visible;
  "
  role="img"
  aria-label={effectiveAlt}
>
  {#if src && !imgFailed}
    <img
      {src}
      alt={effectiveAlt}
      onerror={() => (imgFailed = true)}
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
        border-radius: 50%;
        background-color: var(--alloy-color-status-{presence}, #71717a);
        border: 2px solid var(--alloy-color-surface-canvas, #18181b);
        box-sizing: content-box;
      "
    ></span>
  {/if}
</div>
