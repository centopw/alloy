<script lang="ts">
  import { openExternal } from '$lib/platform/links';
  import pkg from '../../../../../package.json';

  const REPOSITORY_URL = 'https://github.com/centopw/alloy';

  function openRepository(e: MouseEvent) {
    e.preventDefault();
    void openExternal(REPOSITORY_URL);
  }
</script>

<section class="section" aria-labelledby="about-title">
  <h2 id="about-title" class="title">About</h2>

  <div class="card">
    <div class="row">
      <span class="label">Application</span>
      <span class="value">Alloy</span>
    </div>
    <div class="row">
      <span class="label">Version</span>
      <span class="value">{pkg.version}</span>
    </div>
    <div class="row">
      <span class="label">Licence</span>
      <span class="value">Apache-2.0</span>
    </div>
    <div class="row">
      <span class="label">Source</span>
      <a class="link" href={REPOSITORY_URL} onclick={openRepository}>{REPOSITORY_URL}</a>
    </div>
  </div>
</section>

<style>
  .section {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-4);
  }
  .title {
    margin: 0;
    font-size: var(--alloy-typography-size-xl);
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
    padding: var(--alloy-spacing-scale-3) var(--alloy-spacing-scale-4);
    font-size: var(--alloy-typography-size-base);
    color: var(--alloy-color-text-primary);
  }
  .row + .row {
    border-top: 1px solid var(--alloy-color-border-subtle);
  }
  .label {
    color: var(--alloy-color-text-secondary);
  }
  .value {
    overflow-wrap: anywhere;
    text-align: right;
  }
  .link {
    color: var(--alloy-color-text-link);
    overflow-wrap: anywhere;
    text-align: right;
    border-radius: var(--alloy-radii-sm);
  }
  .link:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }
</style>
