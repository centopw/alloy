<script lang="ts">
  import type { Network } from '$lib/generated';

  interface Props {
    network: Network;
    /** Accessible name: the network's name. */
    label: string;
    size?: number;
  }

  let { network, label, size = 12 }: Props = $props();

  /** Monochrome network glyphs on a 24px grid. A network without one renders nothing. */
  const GLYPHS: Partial<Record<Network, string>> = {
    telegram:
      'M21.9 4.3 18.7 19.4c-.2 1-.9 1.3-1.8.8l-4.9-3.6-2.4 2.3c-.3.3-.5.5-1 .5l.4-5 9.1-8.2c.4-.4-.1-.5-.6-.2L6.2 13.1 1.4 11.6c-1-.3-1-1 .2-1.5L20.6 2.8c.9-.3 1.6.2 1.3 1.5z',
    messenger:
      'M12 2C6.36 2 2 6.13 2 11.7c0 2.91 1.19 5.44 3.14 7.17.16.15.26.35.27.57l.05 1.78c.02.57.6.94 1.12.71l1.98-.87c.17-.07.36-.09.53-.04.91.25 1.88.38 2.91.38 5.64 0 10-4.13 10-9.7S17.64 2 12 2zM5.99 14.54l2.94-4.66c.47-.74 1.47-.93 2.17-.4l2.34 1.75c.21.16.51.16.72 0l3.16-2.4c.42-.32.97.19.69.63l-2.94 4.66c-.47.74-1.47.93-2.17.4l-2.34-1.75a.6.6 0 0 0-.72 0l-3.16 2.4c-.42.32-.97-.18-.69-.63z',
  };
  const path = $derived(GLYPHS[network]);
</script>

{#if path}
  <svg class="network-icon" width={size} height={size} viewBox="0 0 24 24" role="img" aria-label={label}>
    <title>{label}</title>
    <path d={path} fill="currentColor" fill-rule="evenodd" />
  </svg>
{/if}
