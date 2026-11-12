<script lang="ts">
  import Button from '../common/Button.svelte';
  import { connectivity, isReconnecting } from '$lib/stores/connectivity.svelte';
  import { networkName, roomsStore } from '$lib/stores/rooms.svelte';

  import { t } from '$lib/i18n';

  /** Accounts whose connector is not live, with the copy that explains why. */
  const trouble = $derived(
    connectivity.disconnectedAccounts.map((accountId) => {
      const account = roomsStore.accounts.find((a) => a.id === accountId) ?? null;
      const name = account?.display_name?.trim() || (account ? networkName(account.network) : null);
      const status = connectivity.statusOf(accountId);
      const text =
        status === 'Disconnected'
          ? t('chat-connection-disconnected', { account: name ?? t('chat-network-account') })
          : status === 'Terminated'
            ? t('chat-connection-terminated', { account: name ?? t('chat-network-account') })
            : isReconnecting(status)
              ? t('chat-connection-reconnecting', { account: name ?? t('chat-network-account') })
              : t('chat-connection-connecting', { account: name ?? t('chat-network-account') });
      return { accountId, text };
    })
  );

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
    {:else}
      <ul class="accounts">
        {#each trouble as row (row.accountId)}
          <li class="text">{row.text}</li>
        {/each}
      </ul>
    {/if}
    <Button variant="ghost" size="sm" onclick={() => void connectivity.retryNow()}>{t('connection-banner-retry-now')}</Button>
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
