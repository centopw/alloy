<script lang="ts">
  import { SegmentedControl, Toggle } from '$lib/components/common';
  import { themeStore } from '$lib/theme/theme.svelte';
  import type { Density } from '$lib/theme/theme.svelte';

  const dark = $derived(themeStore.activeTheme !== 'alloy-light');
  const amoled = $derived(themeStore.activeTheme === 'alloy-amoled');

  function setDark(on: boolean) {
    themeStore.setTheme(on ? (amoled ? 'alloy-amoled' : 'alloy-dark') : 'alloy-light');
  }
  function setAmoled(on: boolean) {
    themeStore.setTheme(on ? 'alloy-amoled' : 'alloy-dark');
  }

  const densityOptions: { value: Density; label: string }[] = [
    { value: 'compact', label: 'Compact' },
    { value: 'normal', label: 'Normal' },
    { value: 'cozy', label: 'Cozy' },
  ];
</script>

<section class="section">
  <div class="card">
    <div class="row">
      <span>Follow system appearance</span>
      <Toggle
        label="Follow system appearance"
        checked={themeStore.followSystem}
        onchange={(v) => themeStore.setFollowSystem(v)}
      />
    </div>
    <div class="row">
      <span>Dark mode</span>
      <Toggle
        label="Dark mode"
        checked={dark}
        disabled={themeStore.followSystem}
        onchange={setDark}
      />
    </div>
    <div class="row">
      <span>Pure black background</span>
      <Toggle
        label="Pure black background"
        checked={amoled}
        disabled={!dark || themeStore.followSystem}
        onchange={setAmoled}
      />
    </div>
  </div>

  <div class="block">
    <h3 class="subtitle">Density</h3>
    <SegmentedControl
      label="Density"
      options={densityOptions}
      value={themeStore.density}
      onchange={(v) => themeStore.setDensity(v as Density)}
    />
  </div>
</section>

<style>
  .section {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-layout-section-gap);
  }
  .subtitle {
    margin: 0;
    font-size: var(--alloy-typography-size-lg);
    font-weight: var(--alloy-typography-weight-semibold);
    color: var(--alloy-color-text-primary);
  }
  .card {
    background: var(--alloy-color-surface-subtle);
    border-radius: var(--alloy-radii-lg);
    overflow: hidden;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--alloy-spacing-scale-3);
    padding: var(--alloy-spacing-scale-3) var(--alloy-layout-gutter);
    font-size: var(--alloy-typography-size-base);
    color: var(--alloy-color-text-primary);
  }
  .row + .row {
    border-top: 1px solid var(--alloy-color-border-subtle);
  }
  .block {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-2);
  }
</style>
