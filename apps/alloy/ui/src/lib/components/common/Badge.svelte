<script lang="ts">
  interface Props {
    count?: number | string;
    variant?: 'default' | 'muted';
    max?: number;
    class?: string;
  }

  let {
    count,
    variant = 'default',
    max = 99,
    class: className = ''
  }: Props = $props();

  const displayCount = $derived.by(() => {
    if (typeof count === 'number') {
      if (count > max) return `${max}+`;
      return count.toString();
    }
    return count ?? '';
  });

  const isMuted = $derived(variant === 'muted');
</script>

{#if count !== undefined && count !== null && count !== 0 && count !== '0'}
  <span
    class="alloy-badge {className}"
    data-variant={variant}
    style="
      display: inline-flex;
      align-items: center;
      justify-content: center;
      min-width: 18px;
      height: 18px;
      padding: 0 5px;
      border-radius: var(--alloy-radii-full, 9999px);
      font-size: var(--alloy-typography-size-xs, 11px);
      font-weight: var(--alloy-typography-weight-bold, 700);
      line-height: 1;
      background-color: {isMuted ? 'var(--alloy-color-surface-hover)' : 'var(--alloy-color-accent-fill)'};
      color: {isMuted ? 'var(--alloy-color-text-secondary)' : 'var(--alloy-color-text-on-accent)'};
      user-select: none;
      box-sizing: border-box;
    "
  >
    {displayCount}
  </span>
{/if}
