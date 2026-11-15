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

  /* The section heading shares the settings type scale: 15px semibold title, 12px muted
     description, then 8px to the first row. Rows carry hairlines instead of a plate, so the
     group draws no box of its own. */
  .title {
    margin: 0;
    font-size: var(--alloy-font-size-title);
    font-weight: var(--alloy-typography-weight-semibold);
    color: var(--alloy-color-text-primary);
  }

  .description {
    margin: 0;
    font-size: var(--alloy-font-size-preview);
    color: var(--alloy-color-text-muted);
  }

  .body {
    display: flex;
    flex-direction: column;
  }
</style>
