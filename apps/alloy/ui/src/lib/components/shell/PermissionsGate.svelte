<script lang="ts">
  import { Button, Modal } from '$lib/components/common';
  import PermissionsPanel from '../settings/PermissionsPanel.svelte';
  import { shouldPrompt } from '$lib/content/permissionsModel';
  import { permissionsStore } from '$lib/stores/permissions.svelte';
  import { prefs } from '$lib/stores/prefs.svelte';

  interface Props {
    /** Opens Settings > Permissions, for the user who wants the full page. */
    onopensettings?: () => void;
  }
  let { onopensettings }: Props = $props();

  // Read the states once when the signed-in shell appears, without prompting.
  $effect(() => {
    void permissionsStore.refresh();
  });
  let dismissed = $state(false);

  // Shown only after the first read, while a required permission is missing and the user has not dismissed it.
  const open = $derived(
    !dismissed &&
    permissionsStore.loaded &&
    shouldPrompt(permissionsStore.rows, prefs.permissionsPromptDismissed)
  );

  function dismiss() {
    dismissed = true;
    prefs.set('permissionsPromptDismissed', true);
  }
</script>

<Modal {open} title="Allow what Alloy needs" onclose={dismiss}>
  <div class="gate">
    <p class="intro">
      Alloy needs a few permissions to tell you about new messages. Allow them here; you can change any of them later in
      Settings.
    </p>
    <PermissionsPanel />
    <div class="actions">
      {#if onopensettings}
        <Button variant="ghost" onclick={() => { dismiss(); onopensettings?.(); }}>All permissions</Button>
      {/if}
      <Button variant="primary" onclick={dismiss}>Continue</Button>
    </div>
  </div>
</Modal>

<style>
  .gate {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-4);
  }
  .intro {
    margin: 0;
    color: var(--alloy-color-text-secondary);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--alloy-spacing-scale-2);
  }
  @media (max-width: 899px) {
    .actions :global(button) {
      min-height: 44px;
      min-width: 44px;
    }
  }
</style>
