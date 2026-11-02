<script lang="ts">
  import { ConfirmDialog } from '../common';
  import { timelineStore } from '$lib/stores/timeline.svelte';

  import { t } from '$lib/i18n';
  const pending = $derived(timelineStore.pendingReport);
</script>

<ConfirmDialog
  open={pending !== null}
  title={t('report-confirm-report-this-message')}
  message="The message is sent to the room's moderators and your homeserver's administrators for review. The sender is not told."
  confirmLabel={t('report-confirm-report')}
  danger
  onconfirm={async () => {
    if (pending) await timelineStore.report(pending);
  }}
  onclose={() => (timelineStore.pendingReport = null)}
/>
