<script lang="ts">
  import { ConfirmDialog } from '../common';
  import { timelineStore } from '$lib/stores/timeline.svelte';

  const pending = $derived(timelineStore.pendingReport);
</script>

<ConfirmDialog
  open={pending !== null}
  title="Report this message?"
  message="The message is sent to the room's moderators and your homeserver's administrators for review. The sender is not told."
  confirmLabel="Report"
  danger
  onconfirm={async () => {
    if (pending) await timelineStore.report(pending);
  }}
  onclose={() => (timelineStore.pendingReport = null)}
/>
