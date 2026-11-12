<script lang="ts">
  import { Icon, X, Copy, Check } from '$lib/icons';
  import { getUserColorValue } from '$lib/identity/userColor';
  import { userProfile } from '$lib/stores/userProfile.svelte';
  import { resolveAvatar } from '$lib/stores/avatars.svelte';
  import { errorMessage } from '$lib/stores/errors';
  import { membersStore } from '$lib/stores/members.svelte';
  import { roomsStore } from '$lib/stores/rooms.svelte';
  import { viewport } from '$lib/stores/viewport.svelte';
  import type { UserProfileDto } from '$lib/generated/protocol';
  import { Avatar, Button } from '../common';

  import { t } from '$lib/i18n';
  interface Props {
    userId: string;
    /** The account that knows this user; profiles are account-scoped. Defaults to the open chat's. */
    accountId?: string;
    roomId: string | null;
    onclose: () => void;
    /** Called with the direct-chat room id when the user already has one with this person. */
    onmessage?: (roomId: string) => void;
    /** Called with the user id when Mention is pressed. Mention is hidden when omitted. */
    onmention?: (userId: string) => void;
  }

  let { userId, accountId, roomId, onclose, onmessage, onmention }: Props = $props();

  const account = $derived(accountId ?? roomsStore.active?.account_id ?? '');

  let profile = $state<UserProfileDto | null>(null);
  let avatarSrc = $state<string | null>(null);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let actionError = $state<string | null>(null);
  let copied = $state(false);

  // Roles are a property of the member row, not of the profile: read them from the room's roster.
  const role = $derived(
    roomId ? (membersStore.membersOf(roomId).find((m) => m.user_id === userId)?.role ?? null) : null
  );

  $effect(() => {
    const id = userId;
    const owner = account;
    let cancelled = false;
    loading = true;
    error = null;
    profile = null;
    avatarSrc = null;
    actionError = null;
    userProfile
      .get(owner, id)
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

  $effect(() => {
    const source = profile?.avatar ?? null;
    let cancelled = false;
    avatarSrc = null;
    void resolveAvatar(source, 64).then((url) => {
      if (!cancelled) avatarSrc = url;
    });
    return () => {
      cancelled = true;
    };
  });

  const displayName = $derived(profile?.display_name || userId);

  async function copyUserId() {
    try {
      await navigator.clipboard.writeText(userId);
      copied = true;
      setTimeout(() => (copied = false), 1500);
    } catch (e) {
      actionError = errorMessage(e);
    }
  }

  function message() {
    const dm = profile?.dm_room_id;
    if (dm) onmessage?.(dm);
  }
</script>

<div class="card">
  <div class="top">
    <Avatar
      userId={userId}
      displayName={displayName}
      src={avatarSrc}
      size="xl"
      shape="circle"
    />
    <button type="button" class="icon-btn" aria-label={t('user-profile-card-close-profile')} title={t('user-profile-card-close-profile')} onclick={onclose}>
      <Icon icon={X} size={viewport.isPhone ? 20 : 16} />
    </button>
  </div>

  {#if loading}
    <p class="status" role="status">{t('user-profile-card-loading-profile')}</p>
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
          aria-label={t('user-profile-card-copy-user-id')}
          title={copied ? t('user-profile-card-copied') : t('user-profile-card-copy-user-id')}
          onclick={copyUserId}
        >
          <Icon icon={copied ? Check : Copy} size={viewport.isPhone ? 20 : 16} />
        </button>
      </div>
    </div>

    <dl class="facts">
      {#if role}
        <div class="fact">
          <dt>{t('user-profile-card-role')}</dt>
          <dd>{role}</dd>
        </div>
      {/if}
    </dl>
  {/if}

  {#if actionError}
    <p class="notice" role="alert">{actionError}</p>
  {/if}

  {#if profile && !profile.is_me}
    <div class="actions">
      {#if profile.dm_room_id && onmessage}
        <Button variant="primary" size="md" onclick={message}>
          {#snippet children()}{t('user-profile-card-message')}{/snippet}
        </Button>
      {/if}
      {#if onmention}
        <Button variant="ghost" size="md" onclick={() => onmention(userId)}>
          {#snippet children()}{t('user-profile-card-mention')}{/snippet}
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

  /* Phone: the card is the body of a bottom sheet; controls are touch-sized and actions stack full width. */
  @media (max-width: 899px) {
    .card {
      padding: var(--alloy-layout-section-gap) var(--alloy-layout-gutter);
      font-size: var(--alloy-typography-size-lg);
    }
    .icon-btn {
      width: 44px;
      height: 44px;
    }
    .icon-btn:hover {
      background: transparent;
      color: var(--alloy-text-secondary);
    }
    .icon-btn:active {
      background: var(--alloy-color-surface-hover);
      color: var(--alloy-text-primary);
    }
    .mxid,
    .status,
    .notice {
      font-size: var(--alloy-typography-size-sm);
    }
    .fact {
      min-height: 44px;
      align-items: center;
    }
    .actions {
      flex-direction: column;
    }
    .actions :global(button) {
      width: 100%;
    }
  }
</style>
