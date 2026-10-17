<script lang="ts">
  import { getUserColorValue } from '$lib/identity/userColor';

  interface Props {
    userId: string;
    displayName?: string | null;
    class?: string;
  }

  let {
    userId,
    displayName = null,
    class: className = ''
  }: Props = $props();

  const userColor = $derived(getUserColorValue(userId));
  const effectiveName = $derived(displayName || userId);
</script>

<span
  class="alloy-user-name {className}"
  style="
    color: {userColor};
    font-weight: var(--alloy-typography-weight-semibold, 600);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    display: inline-block;
  "
  data-user-id={userId}
>
  {effectiveName}
</span>
