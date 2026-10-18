<script lang="ts">
  import { Avatar } from '$lib/components/common';
  import { session } from '$lib/stores/session.svelte';
</script>

<section class="section">
  {#if session.account}
    <div class="profile">
      <Avatar
        userId={session.account.user_id}
        displayName={session.account.display_name ?? session.account.user_id}
        src={session.account.avatar_url}
        size="xl"
      />
      <div class="identity">
        <p class="name">{session.account.display_name ?? session.account.user_id}</p>
        <p class="meta">{session.account.user_id}</p>
      </div>
    </div>

    <div class="card">
      <div class="row">
        <span class="row-label">Homeserver</span>
        <span class="row-value">{session.account.homeserver_url}</span>
      </div>
    </div>
  {:else}
    <p class="muted">No account is signed in.</p>
  {/if}

  <h3 class="subtitle">Devices</h3>
  <div class="card">
    <p class="row note">
      Device management is not available in Alloy yet. This device's encryption keys are managed
      under Security.
    </p>
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
  .profile {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-4);
  }
  .identity {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-1);
  }
  .name {
    margin: 0;
    font-size: var(--alloy-typography-size-lg);
    font-weight: var(--alloy-typography-weight-semibold);
    color: var(--alloy-color-text-primary);
    overflow-wrap: anywhere;
  }
  .meta {
    margin: 0;
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-text-secondary);
    overflow-wrap: anywhere;
  }
  .muted {
    margin: 0;
    color: var(--alloy-color-text-muted);
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
  .row-label {
    color: var(--alloy-color-text-secondary);
  }
  .row-value {
    min-width: 0;
    overflow-wrap: anywhere;
    text-align: right;
  }
  .note {
    margin: 0;
    color: var(--alloy-color-text-secondary);
    justify-content: flex-start;
    line-height: var(--alloy-typography-line-height-normal);
  }
</style>
