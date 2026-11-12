<script lang="ts">
  import { ConfirmDialog } from '../common';
  import { timelineStore } from '$lib/stores/timeline.svelte';

  import { t } from '$lib/i18n';
  const pending = $derived(timelineStore.pendingDelete ?? []);
  const others = $derived(pending.some((i) => !i.is_own));

  const message = $derived(
    (pending.length > 1
      ? t('chat-delete-for-everyone-many')
      : t('chat-delete-for-everyone-one')) +
      (others ? ` ${t('chat-delete-others-note')}` : ''),
  );
</script>

<ConfirmDialog
  open={pending.length > 0}
  title={t('delete-confirm-delete-for-everyone')}
  {message}
  confirmLabel={pending.length > 1 ? t('chat-delete-count', { count: pending.length }) : t('common-delete')}
  danger
  onconfirm={() => timelineStore.deleteItems(pending)}
  onclose={() => (timelineStore.pendingDelete = null)}
/>
