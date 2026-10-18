<script lang="ts">
  import { displayNameFor } from '$lib/platform/names';
  import { membersStore } from '$lib/stores/members.svelte';
  import { prefs } from '$lib/stores/prefs.svelte';
  import { typingStore } from '$lib/stores/typing.svelte';

  let { roomId }: { roomId: string } = $props();

  const names = $derived(
    typingStore.usersIn(roomId).map((userId) => {
      const member = membersStore.membersOf(roomId).find((m) => m.user_id === userId);
      return displayNameFor(userId, null, member?.display_name);
    }),
  );

  const label = $derived.by(() => {
    if (names.length === 0) return '';
    if (names.length === 1) return `${names[0]} is typing...`;
    if (names.length === 2) return `${names[0]} and ${names[1]} are typing...`;
    return 'Several people are typing...';
  });
</script>

{#if prefs.showTyping}
  <!-- The row keeps its height while idle so the timeline does not shift when someone starts or stops typing. -->
  <div class="typing" aria-live="polite">
    {#if label}
      <span class="dots" aria-hidden="true"><span></span><span></span><span></span></span>
      <span class="label">{label}</span>
    {/if}
  </div>
{/if}

<style>
  .typing {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    min-block-size: calc(var(--alloy-typography-size-sm) + 2 * var(--alloy-spacing-scale-1));
    padding-inline: var(--alloy-layout-gutter);
    color: var(--alloy-color-text-muted);
    font-size: var(--alloy-typography-size-sm);
    min-width: 0;
  }

  .label {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .dots {
    display: inline-flex;
    gap: var(--alloy-spacing-scale-1);
    flex: none;
  }

  .dots span {
    inline-size: var(--alloy-spacing-scale-1);
    block-size: var(--alloy-spacing-scale-1);
    border-radius: var(--alloy-radii-full);
    background: currentColor;
    animation: typing-dot var(--alloy-motion-duration-slow) var(--alloy-motion-easing-ease-in-out) infinite;
  }

  .dots span:nth-child(2) {
    animation-delay: calc(var(--alloy-motion-duration-slow) / 3);
  }

  .dots span:nth-child(3) {
    animation-delay: calc(var(--alloy-motion-duration-slow) * 2 / 3);
  }

  @keyframes typing-dot {
    0%,
    80%,
    100% {
      opacity: 0.3;
    }
    40% {
      opacity: 1;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .dots span {
      animation: none;
      opacity: 0.6;
    }
  }
</style>
