<script lang="ts">
  import { onMount } from 'svelte';
  import { Button, TextField } from '$lib/components/common';
  import { openExternal } from '$lib/platform/links';
  import { errorCode, errorMessage } from '$lib/stores/errors';
  import { session } from '$lib/stores/session.svelte';

  import { t } from '$lib/i18n';

  interface Props {
    /** Called after the pair was sealed; onboarding starts the Telegram login from here. */
    onSaved?: () => void;
  }

  let { onSaved }: Props = $props();

  const TELEGRAM_APPS_URL = 'https://my.telegram.org/apps';

  // Two instances never share labels: the section and the onboarding overlay can both be mounted.
  const idField = $props.id();

  let apiId = $state('');
  let apiHash = $state('');
  let showHash = $state(false);
  let busy = $state(false);
  let error = $state<string | null>(null);

  const configured = $derived(session.telegramApp?.configured ?? false);
  const appUrl = $derived(
    session.networks.find((info) => info.network === 'telegram')?.setup?.app_url ??
      TELEGRAM_APPS_URL
  );
  // What the engine accepts: a positive 32-bit id and the 32-hexadecimal hash from
  // my.telegram.org/apps.
  const apiIdNumber = $derived(Number(apiId.trim()));
  const apiIdValid = $derived(
    /^\d+$/.test(apiId.trim()) && apiIdNumber > 0 && apiIdNumber <= 2_147_483_647
  );
  const apiHashValid = $derived(/^[0-9a-fA-F]{32}$/.test(apiHash.trim()));

  function stateLabel(): string {
    const app = session.telegramApp;
    if (!app?.configured) return t('telegram-app-state-none');
    return app.source === 'environment'
      ? t('telegram-app-state-environment')
      : t('telegram-app-state-runtime');
  }

  onMount(() => {
    void seed();
  });

  /** Shows the id in effect so a typo can be corrected without looking it up again. */
  async function seed() {
    if (session.telegramApp === null) {
      await session.refreshTelegramApp().catch((e) => {
        error = errorMessage(e);
      });
    }
    const known = session.telegramApp?.apiId;
    if (known !== null && known !== undefined) apiId = String(known);
  }

  async function save(e: SubmitEvent) {
    e.preventDefault();
    if (!apiIdValid || !apiHashValid || busy) return;
    busy = true;
    error = null;
    try {
      await session.setTelegramApp(apiIdNumber, apiHash.trim());
      // The hash is a credential: it never stays in the DOM once the engine has it.
      apiHash = '';
      showHash = false;
      onSaved?.();
    } catch (err) {
      error =
        errorCode(err) === 'invalid_argument' ? t('telegram-app-invalid') : errorMessage(err);
    } finally {
      busy = false;
    }
  }

  async function clear() {
    if (busy) return;
    busy = true;
    error = null;
    try {
      await session.clearTelegramApp();
    } catch (err) {
      error = errorMessage(err);
    } finally {
      busy = false;
    }
  }
</script>

<div class="telegram-app">
  <p class="hint">
    {t('telegram-app-hint')}
    <button type="button" class="link" onclick={() => void openExternal(appUrl)}>
      {t('telegram-app-open')}
    </button>
  </p>
  <p class="state" aria-live="polite">{stateLabel()}</p>

  <form onsubmit={save}>
    <div class="field">
      <label class="field-label" for="{idField}-api-id">{t('telegram-app-api-id')}</label>
      <TextField
        id="{idField}-api-id"
        bind:value={apiId}
        inputmode="numeric"
        pattern="[0-9]*"
        autocomplete="off"
        spellcheck={false}
        invalid={apiId.trim() !== '' && !apiIdValid}
      />
    </div>

    <div class="field">
      <label class="field-label" for="{idField}-api-hash">{t('telegram-app-api-hash')}</label>
      <div class="secret">
        <TextField
          id="{idField}-api-hash"
          type={showHash ? 'text' : 'password'}
          bind:value={apiHash}
          autocomplete="off"
          spellcheck={false}
          invalid={apiHash.trim() !== '' && !apiHashValid}
        />
        <Button variant="ghost" size="sm" type="button" onclick={() => (showHash = !showHash)}>
          {showHash ? t('telegram-app-hide') : t('telegram-app-show')}
        </Button>
      </div>
    </div>

    {#if error}
      <p class="error" role="alert">{error}</p>
    {/if}

    <div class="actions">
      <Button type="submit" disabled={busy || !apiIdValid || !apiHashValid}>
        {busy ? t('telegram-app-saving') : t('common-save')}
      </Button>
      {#if configured}
        <Button variant="ghost" type="button" disabled={busy} onclick={() => void clear()}>
          {t('telegram-app-clear')}
        </Button>
      {/if}
    </div>
  </form>
</div>

<style>
  .telegram-app {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-3);
    padding: var(--alloy-spacing-scale-3) var(--alloy-layout-row-pad-x);
  }
  .hint,
  .state {
    margin: 0;
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-text-secondary);
    line-height: var(--alloy-typography-line-height-normal, 1.45);
  }
  .state {
    color: var(--alloy-color-text-primary);
    font-weight: var(--alloy-typography-weight-medium);
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
  form {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-3);
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-1);
  }
  .field-label {
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-text-secondary);
  }
  .secret {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
  }
  .error {
    margin: 0;
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-status-error);
    overflow-wrap: anywhere;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
  }
</style>
