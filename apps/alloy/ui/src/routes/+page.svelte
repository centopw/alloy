<script lang="ts">
  import { onMount } from 'svelte';
  import AdaptiveShell from '$lib/components/shell/AdaptiveShell.svelte';
  import Onboarding from '$lib/components/onboarding/Onboarding.svelte';
  import { session } from '$lib/stores/session.svelte';

  onMount(() => {
    void session.boot();
  });
</script>

{#if session.phase === 'booting'}
  <main class="splash" aria-busy="true"><span>Alloy</span></main>
{:else if session.phase === 'onboarding'}
  <Onboarding />
{:else}
  <AdaptiveShell />
{/if}

<style>
  .splash {
    min-height: 100dvh;
    display: grid;
    place-items: center;
    color: var(--alloy-color-text-secondary);
    font-size: var(--alloy-typography-size-xxl);
  }
</style>
