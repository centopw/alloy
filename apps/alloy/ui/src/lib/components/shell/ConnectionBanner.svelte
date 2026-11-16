<script lang="ts">
  import Button from '../common/Button.svelte';
  import { connectivity } from '$lib/stores/connectivity.svelte';
  import { client } from '$lib/ipc';
  import { networkName, roomsStore } from '$lib/stores/rooms.svelte';

  import { t } from '$lib/i18n';

  /** Accounts whose connector is not live, with the copy that explains why. */
  const trouble = $derived(
    connectivity.disconnectedAccounts.map((accountId) => {
      const account = roomsStore.accounts.find((a) => a.id === accountId) ?? null;
      const name = account?.display_name?.trim() || (account ? networkName(account.network) : null);
      const who = name ?? t('chat-network-account');
      const status = connectivity.statusOf(accountId);
      // The connector retries on its own and counts the failed attempts, so the banner reports the
      // attempt it is on and when the next one is due instead of a bare "reconnecting".
      const reconnecting =
        typeof status === 'object' && status !== null && 'Reconnecting' in status
          ? status.Reconnecting
          : null;
      const text = reconnecting
        ? reconnecting.attempt > 1
          ? t('chat-connection-reconnecting-attempt', {
              account: who,
              attempt: reconnecting.attempt,
              seconds: Math.max(1, Math.round(reconnecting.next_retry_millis / 1000)),
            })
          : t('chat-connection-reconnecting', { account: who })
        : status === 'Disconnected'
          ? t('chat-connection-disconnected', { account: who })
          : status === 'Terminated'
            ? t('chat-connection-terminated', { account: who })
            : t('chat-connection-connecting', { account: who });
      return { accountId, text };
    })
  );

  /** Accounts whose connector is being restarted right now, so the button can say so. */
  let restarting = $state<Record<string, boolean>>({});
  let restartError = $state<string | null>(null);

  /**
   * Restarts one account's connector.
   *
   * "Retry now" only probes whether the engine answers, which it always does when the engine is
   * local — it cannot revive a connector that gave up on its own. This asks the engine to start
   * that account's connector again, which is the only thing that brings it back.
   */
  async function reconnect(accountId: string) {
    restarting = { ...restarting, [accountId]: true };
    restartError = null;
    try {
      await client.invoke({ action: 'ReconnectAccount', params: { account_id: accountId } });
    } catch (err) {
      restartError = err instanceof Error ? err.message : String(err);
    } finally {
      restarting = { ...restarting, [accountId]: false };
    }
  }

  const text = $derived(
    connectivity.browserOnline
      ? t('chat-connection-unreachable')
      : t('chat-connection-offline')
  );
</script>

{#if connectivity.offline || trouble.length > 0}
  <div class="banner" role="status" aria-live="polite">
    {#if connectivity.offline}
      <span class="text">{text}</span>
      <!-- A send made while the network is down is queued, not lost; saying so stops a user
           retyping a message that is already waiting. -->
      <span class="text queued">{t('connection-banner-messages-wait')}</span>
    {:else}
      <ul class="accounts">
        {#each trouble as row (row.accountId)}
          <li class="account">
            <span class="text">{row.text}</span>
            <Button
              variant="ghost"
              size="sm"
              disabled={restarting[row.accountId]}
              onclick={() => void reconnect(row.accountId)}
            >
              {restarting[row.accountId]
                ? t('connection-banner-trying')
                : t('connection-banner-reconnect')}
            </Button>
          </li>
        {/each}
      </ul>
    {/if}
    {#if restartError}
      <span class="text error" role="alert">{restartError}</span>
    {/if}
    <Button
      variant="ghost"
      size="sm"
      disabled={connectivity.probing}
      onclick={() => void connectivity.retryNow()}
    >
      {connectivity.probing ? t('connection-banner-trying') : t('connection-banner-retry-now')}
    </Button>
  </div>
{/if}

<style>
  .banner {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: var(--alloy-spacing-scale-3);
    flex-shrink: 0;
    /* The shell drops its own top inset while this bar is shown, so the bar keeps clear of the status bar. */
    padding: calc(var(--alloy-safe-area-top) + var(--alloy-spacing-scale-1)) var(--alloy-spacing-scale-3) var(--alloy-spacing-scale-1);
    background: var(--alloy-glass-sheen), var(--alloy-glass-fill-strong);
    -webkit-backdrop-filter: var(--alloy-glass-filter);
    backdrop-filter: var(--alloy-glass-filter);
    border-bottom: 1px solid var(--alloy-glass-rim);
    box-shadow: var(--alloy-glass-highlight);
    color: var(--alloy-color-text-secondary);
    font-size: var(--alloy-typography-size-sm, 12px);
    text-align: center;
  }
  .text {
    min-width: 0;
  }
  .account {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: var(--alloy-spacing-scale-2);
  }
  .error {
    color: var(--alloy-color-status-error);
  }
  .accounts {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    margin: 0;
    padding: 0;
    list-style: none;
  }
</style>
