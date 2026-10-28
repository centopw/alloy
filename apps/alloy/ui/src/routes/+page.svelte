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
{:else if session.phase === 'no-engine'}
  <main class="splash no-engine">
    <section role="alert">
      <h1>No local engine</h1>
      <p>
        This page is not connected to Alloy's engine, so there is nothing to sign in to. Open the
        packaged app instead; development builds can point at a bridge with
        <code>?engine=ws://…</code>.
      </p>
    </section>
  </main>
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
  .no-engine section {
    max-width: 44ch;
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-3);
    padding: var(--alloy-spacing-scale-6);
    text-align: center;
  }
  .no-engine h1 {
    margin: 0;
    color: var(--alloy-color-text-primary);
    font-size: var(--alloy-font-size-title);
    font-weight: 600;
  }
  .no-engine p {
    margin: 0;
    font-size: var(--alloy-font-size-row);
    line-height: 1.5;
  }
  .no-engine code {
    font-family: var(--alloy-typography-font-family-mono);
    font-size: var(--alloy-font-size-preview);
    overflow-wrap: anywhere;
  }
</style>
