<script lang="ts">
  import { ConfirmDialog } from '../common';
  import { timelineStore } from '$lib/stores/timeline.svelte';

  const pending = $derived(timelineStore.pendingDelete ?? []);
  const others = $derived(pending.some((i) => !i.is_own));
  const plural = $derived(pending.length > 1);

  const message = $derived(
    (plural ? 'These messages are removed for everyone in the room.' : 'This message is removed for everyone in the room.') +
      (others ? ' You are deleting messages written by someone else.' : ''),
  );
</script>

<ConfirmDialog
  open={pending.length > 0}
  title="Delete for everyone?"
  {message}
  confirmLabel={plural ? `Delete ${pending.length} messages` : 'Delete'}
  danger
  onconfirm={() => timelineStore.redactItems(pending)}
  onclose={() => (timelineStore.pendingDelete = null)}
/>
