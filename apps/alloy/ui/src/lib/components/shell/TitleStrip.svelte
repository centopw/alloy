<script lang="ts">
  import ChangelogDialog from '$lib/components/updates/ChangelogDialog.svelte';
  import UpdatePill from '$lib/components/updates/UpdatePill.svelte';
  import { updatesStore } from '$lib/stores/updates.svelte';
</script>

<!-- The strip exists only while an update is on offer; the dialog is also opened from Help & About. -->
{#if updatesStore.pending}
  <header class="strip" data-tauri-drag-region>
    <span class="name">Alloy</span>
    <div class="end">
      <UpdatePill />
    </div>
  </header>
{/if}
<ChangelogDialog />

<style>
  .strip {
    display: flex;
    flex-shrink: 0;
    align-items: center;
    justify-content: space-between;
    gap: var(--alloy-spacing-scale-3);
    height: var(--alloy-layout-header-height);
    min-width: 0;
    padding: 0 var(--alloy-layout-gutter);
    border-bottom: 1px solid var(--alloy-color-border-subtle);
    background: var(--alloy-color-surface-elevated);
  }

  .name {
    color: var(--alloy-color-text-secondary);
    font-size: var(--alloy-typography-size-sm);
    font-weight: var(--alloy-typography-weight-semibold);
    letter-spacing: 0.02em;
  }

  .end {
    display: flex;
    align-items: center;
    min-width: 0;
  }
</style>
