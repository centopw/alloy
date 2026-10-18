<script lang="ts">
  import { Icon, X, Copy, Check } from '$lib/icons';
  import { getUserColorValue } from '$lib/identity/userColor';
  import { userProfile } from '$lib/stores/userProfile.svelte';
  import { errorMessage } from '$lib/stores/errors';
  import type { UserProfileDto } from '$lib/generated/protocol';
  import { Avatar, Button } from '../common';

  interface Props {
    userId: string;
    roomId: string | null;
    onclose: () => void;
    /** Called with the direct-message room id after Message succeeds. */
    onmessage?: (roomId: string) => void;
    /** Called with the user id when Mention is pressed. Mention is hidden when omitted. */
    onmention?: (userId: string) => void;
  }

  let { userId, roomId, onclose, onmessage, onmention }: Props = $props();

  let profile = $state<UserProfileDto | null>(null);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let busy = $state(false);
  let actionError = $state<string | null>(null);
  let copied = $state(false);

  $effect(() => {
    const id = userId;
    const room = roomId;
    let cancelled = false;
    loading = true;
    error = null;
    profile = null;
    actionError = null;
    userProfile
      .get(id, room)
      .then((p) => {
        if (!cancelled) profile = p;
      })
      .catch((e: unknown) => {
        if (!cancelled) error = errorMessage(e);
      })
      .finally(() => {
        if (!cancelled) loading = false;
      });
    return () => {
      cancelled = true;
    };
  });

  const displayName = $derived(profile?.display_name || userId);

  function roleLabel(powerLevel: number | null): string {
    if (powerLevel === null) return 'Member';
    if (powerLevel >= 100) return 'Admin';
    if (powerLevel >= 50) return 'Moderator';
    return 'Member';
  }

  function membershipLabel(membership: string | null): string {
    if (!membership) return 'Not in this room';
    const lower = membership.toLowerCase();
    return lower.charAt(0).toUpperCase() + lower.slice(1);
  }

  async function copyUserId() {
    try {
      await navigator.clipboard.writeText(userId);
      copied = true;
      setTimeout(() => (copied = false), 1500);
    } catch (e) {
      actionError = errorMessage(e);
    }
  }

  async function message() {
    if (busy) return;
    busy = true;
    actionError = null;
    try {
      const dmRoomId = await userProfile.startDirectMessage(userId);
      onmessage?.(dmRoomId);
    } catch (e) {
      actionError = errorMessage(e);
    } finally {
      busy = false;
    }
  }
</script>

<div class="card">
  <div class="top">
    <Avatar
      userId={userId}
      displayName={displayName}
      src={profile?.avatar_url ?? null}
      size="xl"
      shape="circle"
    />
    <button type="button" class="icon-btn" aria-label="Close profile" title="Close profile" onclick={onclose}>
      <Icon icon={X} size={16} />
    </button>
  </div>

  {#if loading}
    <p class="status" role="status">Loading profile…</p>
  {:else if error}
    <p class="notice" role="alert">{error}</p>
  {:else if profile}
    <div class="identity">
      <p class="name" style="color: {getUserColorValue(userId)};">{displayName}</p>
      <div class="mxid-row">
        <span class="mxid" title={userId}>{userId}</span>
        <button
          type="button"
          class="icon-btn"
          aria-label="Copy user ID"
          title={copied ? 'Copied' : 'Copy user ID'}
          onclick={copyUserId}
        >
          <Icon icon={copied ? Check : Copy} size={16} />
        </button>
      </div>
    </div>

    <dl class="facts">
      <div class="fact">
        <dt>Role</dt>
        <dd>{roleLabel(profile.power_level)}</dd>
      </div>
      <div class="fact">
        <dt>Membership</dt>
        <dd>{membershipLabel(profile.membership)}</dd>
      </div>
    </dl>
  {/if}

  {#if actionError}
    <p class="notice" role="alert">{actionError}</p>
  {/if}

  {#if profile && !profile.is_me}
    <div class="actions">
      <Button variant="primary" size="md" disabled={busy} onclick={message}>
        {#snippet children()}Message{/snippet}
      </Button>
      {#if onmention}
        <Button variant="ghost" size="md" onclick={() => onmention(userId)}>
          {#snippet children()}Mention{/snippet}
        </Button>
      {/if}
    </div>
  {/if}
</div>

<style>
  .card {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-space-3);
    padding: var(--alloy-layout-gutter);
    color: var(--alloy-text-primary);
    font-family: var(--alloy-typography-font-family-sans);
    font-size: var(--alloy-typography-size-sm);
  }

  .top {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
  }

  .identity {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-space-1);
    min-width: 0;
  }

  .name {
    margin: 0;
    font-size: var(--alloy-typography-size-lg);
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .mxid-row {
    display: flex;
    align-items: center;
    gap: var(--alloy-space-1);
    min-width: 0;
  }

  .mxid {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--alloy-text-muted);
    font-size: var(--alloy-typography-size-xs);
  }

  .icon-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    width: 28px;
    height: 28px;
    padding: 0;
    border: none;
    border-radius: var(--alloy-radii-md);
    background: transparent;
    color: var(--alloy-text-secondary);
    cursor: pointer;
  }

  .icon-btn:hover {
    background: var(--alloy-color-surface-hover);
    color: var(--alloy-text-primary);
  }

  .facts {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-space-2);
    margin: 0;
  }

  .fact {
    display: flex;
    justify-content: space-between;
    gap: var(--alloy-space-2);
  }

  .fact dt {
    color: var(--alloy-text-secondary);
  }

  .fact dd {
    margin: 0;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--alloy-space-2);
  }

  .status {
    margin: 0;
    color: var(--alloy-text-muted);
    font-size: var(--alloy-typography-size-xs);
  }

  .notice {
    margin: 0;
    padding: var(--alloy-space-2);
    border-radius: var(--alloy-radii-sm);
    background: var(--alloy-color-surface-subtle);
    color: var(--alloy-color-status-error);
    font-size: var(--alloy-typography-size-xs);
  }
</style>
