<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { Button } from '../common';
  import LoginStep from './LoginStep.svelte';
  import TelegramAppForm from '../settings/sections/TelegramAppForm.svelte';
  import { session, networkLabel } from '$lib/stores/session.svelte';
  import { errorMessage } from '$lib/stores/errors';
  import { openExternal } from '$lib/platform/links';
  import type { LoginInputDto, Network, NetworkInfoDto } from '$lib/generated/index.js';

  import { t } from '$lib/i18n';

  const TELEGRAM_TERMS = 'https://core.telegram.org/api/terms';
  const MESSENGER_LOCKS = 'https://github.com/mautrix/meta/issues/44';

  /** True while this instance is the "Add account" overlay above the signed-in app. */
  const overlay = $derived(session.phase === 'ready');
  const attempt = $derived(session.login);

  let loadingNetworks = $state(true);
  let networksError = $state<string | null>(null);
  /** The network whose setup form replaced the list: an unready network is not a dead end. */
  let setupFor = $state<NetworkInfoDto | null>(null);
  let live = true;

  onMount(() => {
    void start();
  });

  onDestroy(() => {
    live = false;
    // Any unmount path (window closed, settings dismissed, shell swapped) must not leave the
    // engine holding an attempt nothing can answer.
    void session.closeAddAccount();
  });

  async function start() {
    loadingNetworks = true;
    networksError = null;
    try {
      const networks = await session.loadNetworks();
      // Reconnect opens onboarding already knowing the network; start its login straight away,
      // unless it still needs setup — then the list offers that form instead of a failed attempt.
      const preferred = session.preferredNetwork;
      if (!live) return;
      const info = networks.find((candidate) => candidate.network === preferred);
      if (preferred && info?.ready) {
        session.preferredNetwork = null;
        await session.beginLogin(preferred).catch(() => {});
      }
    } catch (e) {
      if (live) networksError = errorMessage(e);
    } finally {
      if (live) loadingNetworks = false;
    }
  }

  async function choose(network: Network) {
    if (session.loginBusy) return;
    await session.beginLogin(network).catch(() => {});
  }

  /** A ready network starts its login; one that still needs setup opens its form inline. */
  function openNetwork(info: NetworkInfoDto) {
    if (session.loginBusy) return;
    if (info.ready) void choose(info.network);
    else if (info.setup) setupFor = info;
  }

  /** The pair is sealed: start the login it unblocked instead of returning to the list. */
  async function saveAndStart() {
    const network = setupFor?.network;
    setupFor = null;
    if (network) await session.beginLogin(network).catch(() => {});
  }

  function submit(input: LoginInputDto) {
    void session.submitLogin(input).catch(() => {});
  }

  function networkDetail(network: Network): string {
    switch (network) {
      case 'telegram':
        return t('onboarding-network-telegram-detail');
      case 'messenger':
        return t('onboarding-network-messenger-detail');
      case 'fake':
        return t('onboarding-network-fake-detail');
    }
  }
</script>

<svelte:element
  this={overlay ? 'div' : 'main'}
  class="onboarding"
  role={overlay ? 'dialog' : undefined}
  aria-modal={overlay ? 'true' : undefined}
  aria-label={overlay ? t('onboarding-connect-account') : undefined}
>
  <section class="panel" data-step={attempt ? 'login' : 'network'}>
    <div class="story">
      <div class="lockup">
        <span class="brand-mark" aria-hidden="true">A</span>
        <span class="wordmark">{t('brand-name')}</span>
      </div>
      <div class="pitch">
        <h1>{t('onboarding-connect-to-your-accounts')}</h1>
        <p class="lede">{t('onboarding-on-device-lede')}</p>
      </div>
      <ul class="points">
        <li><strong>{t('onboarding-point-direct-title')}</strong> {t('onboarding-point-direct')}</li>
        <li><strong>{t('onboarding-point-local-title')}</strong> {t('onboarding-point-local')}</li>
        <li><strong>{t('onboarding-point-leave-title')}</strong> {t('onboarding-point-leave')}</li>
      </ul>
      <p class="footnote">{t('onboarding-open-source-apache-2-0')}</p>
    </div>

    <div class="step">
      {#if overlay}
        <button type="button" class="panel-close" onclick={() => void session.closeAddAccount()}>
          {t('common-close')}
        </button>
      {/if}

      {#if attempt}
        <p class="eyebrow">{networkLabel(attempt.network)}</p>
        {#key attempt.step}
          <LoginStep
            step={attempt.step}
            networkName={networkLabel(attempt.network)}
            busy={session.loginBusy}
            error={session.loginError}
            onsubmit={submit}
            onswitch={(to) => void session.switchLoginMethod(to).catch(() => {})}
            oncancel={() => void session.cancelLogin()}
            onretry={() => void session.retryLogin().catch(() => {})}
            onback={() => session.dismissLogin()}
          />
        {/key}
      {:else}
        <div class="step-head">
          <p class="eyebrow">
            {setupFor ? networkLabel(setupFor.network) : t('onboarding-connect-account')}
          </p>
          <h2>
            {setupFor ? t('onboarding-telegram-setup-title') : t('onboarding-choose-network')}
          </h2>
          <p class="hint">
            {setupFor
              ? t('onboarding-telegram-setup-hint')
              : t('onboarding-choose-network-hint')}
          </p>
        </div>

        {#if setupFor}
          <TelegramAppForm onSaved={saveAndStart} />
          <div class="actions">
            <Button variant="ghost" type="button" onclick={() => (setupFor = null)}>
              {t('common-back')}
            </Button>
          </div>
        {:else}
          <p class="notice">
            {t('onboarding-unofficial-notice')}
            <button type="button" class="link" onclick={() => void openExternal(TELEGRAM_TERMS)}>
              {t('onboarding-unofficial-telegram')}
            </button>
            <button type="button" class="link" onclick={() => void openExternal(MESSENGER_LOCKS)}>
              {t('onboarding-unofficial-messenger')}
            </button>
          </p>

          {#if networksError || session.error || session.loginError}
            <p class="error" role="alert">
              {networksError ?? session.error ?? session.loginError}
            </p>
          {/if}

          {#if loadingNetworks}
            <p class="hint" role="status">{t('onboarding-loading-networks')}</p>
          {:else if session.networks.length === 0}
            <div class="actions">
              <Button variant="ghost" onclick={() => void start()}>{t('common-retry')}</Button>
            </div>
          {:else}
            <ul class="networks">
              {#each session.networks as info (info.network)}
                <li>
                  <button
                    type="button"
                    class="network"
                    disabled={session.loginBusy}
                    onclick={() => openNetwork(info)}
                  >
                    <span class="network-name">{networkLabel(info.network)}</span>
                    <span class="network-detail">{networkDetail(info.network)}</span>
                    {#if !info.ready}
                      <span class="network-setup">{t('settings-networks-needs-setup')}</span>
                    {/if}
                  </button>
                </li>
              {/each}
            </ul>
          {/if}
        {/if}
      {/if}
    </div>
  </section>
</svelte:element>

<style>
  .onboarding {
    box-sizing: border-box;
    position: fixed;
    inset: 0;
    z-index: 70;
    overflow-y: auto;
    display: grid;
    place-items: center;
    padding: calc(var(--alloy-spacing-scale-6) + var(--alloy-safe-area-top))
      var(--alloy-layout-gutter)
      calc(var(--alloy-spacing-scale-6) + var(--alloy-safe-area-bottom));
    background: var(--alloy-canvas-art);
  }

  /* One floating sheet; the story column anchors the brand, the step column does the work. */
  .panel {
    box-sizing: border-box;
    width: min(940px, 100%);
    display: grid;
    grid-template-columns: minmax(0, 1.05fr) minmax(0, 0.95fr);
    border-radius: var(--alloy-glass-radius-shell);
    border: 1px solid var(--alloy-glass-rim);
    background: var(--alloy-glass-sheen), var(--alloy-glass-fill-strong);
    backdrop-filter: var(--alloy-glass-filter-strong);
    box-shadow: var(--alloy-glass-shadow);
    overflow: hidden;
  }

  .story {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-7);
    padding: var(--alloy-spacing-scale-8);
    border-right: 1px solid var(--alloy-glass-rim);
  }
  .lockup {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-3);
  }
  .brand-mark {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    border-radius: var(--alloy-radii-lg);
    background: linear-gradient(145deg, var(--alloy-color-accent-secondary), var(--alloy-color-accent-primary));
    color: var(--alloy-color-text-on-accent);
    font-size: var(--alloy-font-size-title);
    font-weight: var(--alloy-typography-weight-bold);
    letter-spacing: -0.02em;
    line-height: 1;
    box-shadow:
      var(--alloy-brand-highlight),
      0 6px 14px -8px color-mix(in srgb, var(--alloy-color-accent-primary) 80%, transparent);
  }
  .wordmark {
    font-size: var(--alloy-font-size-title);
    font-weight: var(--alloy-typography-weight-semibold);
    letter-spacing: -0.01em;
    color: var(--alloy-color-text-primary);
  }
  .pitch {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-3);
  }
  h1 {
    margin: 0;
    font-size: clamp(1.7rem, 2.4vw, 2.25rem);
    line-height: 1.15;
    letter-spacing: -0.03em;
    color: var(--alloy-color-text-primary);
  }
  .lede {
    margin: 0;
    max-width: 34ch;
    color: var(--alloy-color-text-secondary);
    line-height: var(--alloy-typography-line-height-relaxed, 1.55);
  }
  .points {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-4);
  }
  .points li {
    color: var(--alloy-color-text-secondary);
    font-size: var(--alloy-typography-size-sm);
    line-height: var(--alloy-typography-line-height-normal, 1.45);
  }
  .points strong {
    display: block;
    font-weight: var(--alloy-typography-weight-semibold);
    color: var(--alloy-color-text-primary);
  }
  .footnote {
    margin: auto 0 0;
    font-size: var(--alloy-typography-size-xs);
    color: var(--alloy-color-text-secondary);
    opacity: 0.75;
  }

  .step {
    box-sizing: border-box;
    position: relative;
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-5);
    padding: var(--alloy-spacing-scale-8);
    background: color-mix(in srgb, var(--alloy-color-surface-panel) 55%, transparent);
  }
  .panel-close {
    align-self: flex-end;
    padding: var(--alloy-spacing-scale-1) var(--alloy-spacing-scale-2);
    border: 0;
    border-radius: var(--alloy-radii-md);
    background: none;
    color: var(--alloy-color-text-secondary);
    font: inherit;
    font-size: var(--alloy-typography-size-sm);
    cursor: pointer;
  }
  .panel-close:hover {
    background: var(--alloy-color-surface-hover);
    color: var(--alloy-color-text-primary);
  }
  .panel-close:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }
  .step-head {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-2);
  }
  .eyebrow {
    margin: 0;
    font-size: var(--alloy-typography-size-xs);
    font-weight: var(--alloy-typography-weight-semibold);
    letter-spacing: var(--alloy-letter-spacing-caps, 0.08em);
    text-transform: uppercase;
    color: var(--alloy-color-accent-primary);
  }
  h2 {
    margin: 0;
    font-size: var(--alloy-typography-size-xl);
    font-weight: var(--alloy-typography-weight-semibold);
    letter-spacing: -0.01em;
    color: var(--alloy-color-text-primary);
  }
  .hint {
    margin: 0;
    color: var(--alloy-color-text-secondary);
    font-size: var(--alloy-typography-size-sm);
    line-height: var(--alloy-typography-line-height-normal, 1.45);
  }
  .error {
    margin: 0;
    color: var(--alloy-color-status-error);
    font-size: var(--alloy-typography-size-sm);
  }
  .notice {
    margin: 0;
    padding: var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-3);
    border-left: 2px solid var(--alloy-color-accent-primary);
    border-radius: var(--alloy-radii-sm);
    background: var(--alloy-color-accent-subtle);
    color: var(--alloy-color-text-secondary);
    font-size: var(--alloy-typography-size-sm);
    line-height: var(--alloy-typography-line-height-normal, 1.45);
  }
  .link {
    display: inline;
    padding: 0;
    border: 0;
    background: none;
    color: var(--alloy-color-accent-primary);
    font: inherit;
    text-decoration: underline;
    cursor: pointer;
  }
  .link:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }
  .networks {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-2);
  }
  .network {
    box-sizing: border-box;
    width: 100%;
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-1);
    padding: var(--alloy-spacing-scale-3) var(--alloy-spacing-scale-4);
    border: 1px solid var(--alloy-color-border-subtle);
    border-radius: var(--alloy-radii-md);
    background: var(--alloy-color-surface-elevated);
    color: var(--alloy-color-text-primary);
    font: inherit;
    text-align: start;
    cursor: pointer;
  }
  .network:not(:disabled):hover {
    border-color: var(--alloy-color-border-prominent);
    background: var(--alloy-color-surface-hover);
  }
  .network:disabled {
    cursor: not-allowed;
    opacity: 0.5;
  }
  .network:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }
  .network-name {
    font-size: var(--alloy-typography-size-base);
    font-weight: var(--alloy-typography-weight-semibold);
  }
  .network-detail {
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-text-secondary);
  }
  .network-setup {
    font-size: var(--alloy-typography-size-sm);
    font-weight: var(--alloy-typography-weight-medium);
    color: var(--alloy-color-accent-primary);
  }
  .actions {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-3);
  }

  /* Phone: the sheet becomes the screen and the story column steps aside. */
  @media (max-width: 899px) {
    .onboarding {
      padding: 0;
      place-items: stretch;
    }
    .panel {
      width: 100%;
      min-height: 100dvh;
      display: flex;
      flex-direction: column;
      border: 0;
      border-radius: 0;
      box-shadow: none;
    }
    .story {
      flex: none;
      border-right: 0;
      border-bottom: 1px solid var(--alloy-glass-rim);
      padding: calc(var(--alloy-spacing-scale-6) + var(--alloy-safe-area-top))
        var(--alloy-spacing-scale-5) var(--alloy-spacing-scale-6);
    }
    .panel[data-step='login'] .story {
      display: none;
    }
    .step {
      flex: 1 1 auto;
      padding: var(--alloy-spacing-scale-6) var(--alloy-spacing-scale-5)
        calc(var(--alloy-spacing-scale-6) + var(--alloy-safe-area-bottom));
      background: none;
    }
    .step h2 {
      font-size: 1.5rem;
    }
    .hint,
    .network-detail {
      font-size: var(--alloy-typography-size-base);
    }
    h1 {
      font-size: 1.6rem;
    }
    .footnote {
      display: none;
    }
    .network {
      min-height: 56px;
    }
    .panel-close,
    .link {
      min-height: 44px;
      display: inline-flex;
      align-items: center;
    }
  }
</style>
