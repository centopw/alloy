<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    title: string;
    description?: string;
    children: Snippet;
  }

  let { title, description, children }: Props = $props();
</script>

<section class="group">
  <header class="header">
    <h3 class="title">{title}</h3>
    {#if description}
      <p class="description">{description}</p>
    {/if}
  </header>
  <div class="body">
    {@render children()}
  </div>
</section>

<style>
  .group {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-2);
  }

  .header {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-1);
  }

  /* An eyebrow, not a row: flush with the card's edge, tracked caps and the quieter text colour.
     That is what separates "Theme" from the switch labels underneath it at a glance. */
  .title {
    margin: 0;
    font-size: var(--alloy-font-size-preview);
    font-weight: var(--alloy-typography-weight-semibold);
    letter-spacing: var(--alloy-letter-spacing-caps);
    text-transform: uppercase;
    color: var(--alloy-color-text-secondary);
  }

  .description {
    margin: 0;
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-text-secondary);
  }

  .body {
    display: flex;
    flex-direction: column;
    background: var(--alloy-color-surface-subtle);
    border-radius: var(--alloy-radii-lg);
    overflow: hidden;
  }
</style>
