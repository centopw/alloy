<script lang="ts">
  import type { Component } from 'svelte';

  interface Props {
    icon: Component<{ size?: number; strokeWidth?: number; class?: string }>;
    size?: 16 | 20 | 24;
    strokeWidth?: number;
    label?: string;
    class?: string;
  }

  let {
    icon: IconComponent,
    size = 20,
    strokeWidth = 1.5,
    label,
    class: className = ''
  }: Props = $props();
</script>

<span
  class="alloy-icon-wrapper {className}"
  style="display: inline-flex; align-items: center; justify-content: center; width: {size}px; height: {size}px;"
  role={label ? 'img' : undefined}
  aria-label={label}
  aria-hidden={label ? undefined : 'true'}
>
  <IconComponent {size} {strokeWidth} class="alloy-icon" />
</span>
