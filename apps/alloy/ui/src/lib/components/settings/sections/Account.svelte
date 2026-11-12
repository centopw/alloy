<script lang="ts">
  import { Button } from '$lib/components/common';
  import Onboarding from '$lib/components/onboarding/Onboarding.svelte';
  import { session, networkLabel } from '$lib/stores/session.svelte';
  import type { Account, AccountId, ConnectionStatus } from '$lib/generated/index.js';

  import { t } from '$lib/i18n';

  interface Props {
    /** Closes the settings dialog; used once the last account is gone and the section is empty. */
    onclose?: () => void;
  }

  let { onclose }: Props = $props();

  const accounts = $derived(session.accounts);

  function statusLabel(status: ConnectionStatus): string {
    if (typeof status === 'object') {
      return t('account-status-reconnecting', { attempt: status.Reconnecting.attempt });
    }
    switch (status) {
      case 'Connecting':
        return t('account-status-connecting');
      case 'Syncing':
        return t('account-status-syncing');
      case 'Connected':
        return t('account-state-active');
      case 'Disconnected':
        return t('account-status-disconnected');
      case 'Terminated':
        return t('account-status-terminated');
    }
    return t('account-status-disconnected');
  }

  /** Lifecycle state first (a dead session is not "connected"), then the connector's live status. */
  function stateLabel(account: Account): string {
    switch (account.state) {
      case 'needs_relogin':
        return t('account-state-needs-relogin');
      case 'disabled':
        return t('account-state-disabled');
      case 'pending':
        return t('account-state-pending');
      case 'active': {
        const status = session.statuses[account.id];
        return status ? statusLabel(status) : t('account-state-active');
      }
    }
    return t('account-state-active');
  }

  function accountName(account: Account): string {
    return account.display_name ?? account.remote_user_id ?? t('account-unnamed');
  }

  async function logOut(accountId: AccountId) {
    await session.logout(accountId).catch(() => {});
    // With nothing left the section has no content and the app is back on onboarding.
    if (session.accounts.length === 0) onclose?.();
  }
</script>

<section class="section">
  <div class="section-head">
    <div class="section-text">
      <h2 class="section-title">{t('account-connected-accounts')}</h2>
      <p class="muted">{t('account-connected-accounts-hint')}</p>
    </div>
    <Button size="sm" onclick={() => session.openAddAccount()}>{t('account-add')}</Button>
  </div>

  {#if accounts.length === 0}
    <p class="muted">{t('account-no-account-is-connected')}</p>
  {:else}
    <ul class="accounts">
      {#each accounts as account (account.id)}
        <li class="account">
          <div class="identity">
            <span class="name">{accountName(account)}</span>
            <span class="meta">
              {networkLabel(account.network)} · {stateLabel(account)}
            </span>
          </div>
          <div class="row-actions">
            {#if account.state === 'needs_relogin'}
              <Button
                size="sm"
                disabled={session.busy}
                onclick={() => session.openReconnect(account.network)}
                >{t('account-reconnect')}</Button
              >
            {/if}
            <Button
              variant="ghost"
              size="sm"
              disabled={session.busy}
              onclick={() => void logOut(account.id)}
              >{t('account-log-out')}</Button
            >
          </div>
        </li>
      {/each}
    </ul>
  {/if}

  {#if session.error}<p class="error" role="alert">{session.error}</p>{/if}
</section>

{#if session.addingAccount}
  <Onboarding />
{/if}

<style>
  .section {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-4);
  }
  .section-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: var(--alloy-layout-gutter);
  }
  .section-text {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-1);
    min-width: 0;
  }
  .section-title {
    margin: 0;
    font-size: var(--alloy-typography-size-lg);
    font-weight: var(--alloy-typography-weight-semibold);
    color: var(--alloy-color-text-primary);
  }
  .muted {
    margin: 0;
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-text-secondary);
  }
  .accounts {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    background: var(--alloy-color-surface-subtle);
    border-radius: var(--alloy-radii-lg);
    overflow: hidden;
  }
  .account {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--alloy-layout-gutter);
    padding: var(--alloy-spacing-scale-3) var(--alloy-layout-row-pad-x);
    border-top: 1px solid var(--alloy-color-border-subtle);
  }
  .account:first-child {
    border-top: none;
  }
  .identity {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-1);
    min-width: 0;
  }
  .name {
    font-size: var(--alloy-typography-size-base);
    font-weight: var(--alloy-typography-weight-medium);
    color: var(--alloy-color-text-primary);
    overflow-wrap: anywhere;
  }
  .meta {
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-text-secondary);
  }
  .row-actions {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    flex-shrink: 0;
  }
  .error {
    margin: 0;
    color: var(--alloy-color-status-error);
    font-size: var(--alloy-typography-size-sm);
  }
  @media (max-width: 899px) {
    .section-head {
      flex-direction: column;
      align-items: stretch;
    }
    .account {
      flex-direction: column;
      align-items: stretch;
      min-height: 56px;
      padding: var(--alloy-spacing-scale-2) var(--alloy-layout-row-pad-x);
    }
    .row-actions {
      justify-content: flex-end;
    }
  }
</style>
