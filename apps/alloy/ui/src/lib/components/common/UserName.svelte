<script lang="ts">
  import { getUserColorValue } from '$lib/identity/userColor';
  import { displayNameFor } from '$lib/platform/names';
  import { membersStore } from '$lib/stores/members.svelte';
  import { profileCard } from '$lib/stores/profileCard.svelte';

  interface Props {
    userId: string;
    /** Explicit name (for example from a message sender). Wins over the room member name. */
    displayName?: string | null;
    /** Room used to look up the member display name. */
    roomId?: string | null;
    /** Render a button that opens the user's profile card. */
    interactive?: boolean;
    class?: string;
  }

  let {
    userId,
    displayName = null,
    roomId = null,
    interactive = false,
    class: className = ''
  }: Props = $props();

  const userColor = $derived(getUserColorValue(userId));
  const memberName = $derived(
    membersStore.membersOf(roomId).find((m) => m.user_id === userId)?.display_name ?? null
  );
  const effectiveName = $derived(displayNameFor(userId, displayName, memberName));

  const resetStyle = $derived(
    interactive
      ? 'background: none; border: 0; padding: 0; margin: 0; font: inherit; text-align: start; cursor: pointer; max-width: 100%;'
      : ''
  );
  const textStyle = $derived(`
    ${resetStyle}
    color: ${userColor};
    font-weight: var(--alloy-typography-weight-semibold, 600);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    display: inline-block;
  `);

  /** Centre of an element, used when the card is opened from the keyboard (no pointer position). */
  function centreOf(el: HTMLElement): { x: number; y: number } {
    const r = el.getBoundingClientRect();
    return { x: r.left + r.width / 2, y: r.top + r.height / 2 };
  }

  function onclick(e: MouseEvent) {
    // detail 0 means the click came from the keyboard (Space), which has no pointer coordinates.
    const at =
      e.detail === 0 ? centreOf(e.currentTarget as HTMLElement) : { x: e.clientX, y: e.clientY };
    profileCard.open(userId, roomId ?? null, at);
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key !== 'Enter') return;
    // Handle Enter here so the native click (which would report no pointer position) never fires.
    e.preventDefault();
    profileCard.open(userId, roomId ?? null, centreOf(e.currentTarget as HTMLElement));
  }
</script>

{#if interactive}
  <button
    type="button"
    class="alloy-user-name name {className}"
    style={textStyle}
    title={userId}
    data-user-id={userId}
    {onclick}
    {onkeydown}
  >
    {effectiveName}
  </button>
{:else}
  <span class="alloy-user-name {className}" style={textStyle} title={userId} data-user-id={userId}>
    {effectiveName}
  </span>
{/if}
