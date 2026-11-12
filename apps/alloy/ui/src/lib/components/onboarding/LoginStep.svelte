<script lang="ts">
  import { untrack } from 'svelte';
  import { Button, TextField } from '../common';
  import { openExternal } from '$lib/platform/links';
  import { toDataURL } from 'qrcode';
  import type {
    CookieFieldDto,
    LoginFailureDto,
    LoginInputDto,
    LoginStepDto
  } from '$lib/generated/index.js';

  import { t } from '$lib/i18n';

  interface Props {
    /** The step the engine returned; the parent remounts this component when it changes. */
    step: LoginStepDto;
    /** The network's display name, for copy that has to name it. */
    networkName: string;
    busy: boolean;
    /** A transport-level failure from the last call, shown above the step. */
    error: string | null;
    onsubmit: (input: LoginInputDto) => void;
    oncancel: () => void;
    onretry: () => void;
    onback: () => void;
  }

  let { step, networkName, busy, error, onsubmit, oncancel, onretry, onback }: Props = $props();

  let phone = $state('');
  let code = $state('');
  let password = $state('');
  let username = $state('');
  let credentialPassword = $state('');
  let cookieValues = $state<Record<string, string>>(untrack(() => cookieSeed(step)));

  function cookieSeed(value: LoginStepDto): Record<string, string> {
    if (value.type !== 'cookies') return {};
    return Object.fromEntries(value.fields.map((field) => [field.name, '']));
  }

  /** Digits and a leading `+`: the protocol wants international format. */
  function normalisePhone(value: string): string {
    return value.replace(/[^\d+]/g, '');
  }

  const phoneValid = $derived(/^\+\d{6,15}$/.test(normalisePhone(phone)));
  const codeValid = $derived(
    step.type === 'code'
      ? step.length === null
        ? /^\d{3,}$/.test(code)
        : new RegExp(`^\\d{${step.length}}$`).test(code)
      : false
  );
  const passwordValid = $derived(password.length > 0);
  const credentialsValid = $derived(username.trim().length > 0 && credentialPassword.length > 0);
  const cookiesValid = $derived(
    step.type === 'cookies'
      ? step.fields.every(
          (field) => !field.required || (cookieValues[field.name] ?? '').trim().length > 0
        )
      : false
  );

  function submitPhone(e: SubmitEvent) {
    e.preventDefault();
    if (!phoneValid || busy) return;
    onsubmit({ type: 'phone_number', phone: normalisePhone(phone) });
  }

  function submitCode(e: SubmitEvent) {
    e.preventDefault();
    if (!codeValid || busy) return;
    onsubmit({ type: 'code', code });
  }

  function submitPassword(e: SubmitEvent) {
    e.preventDefault();
    if (!passwordValid || busy) return;
    onsubmit({ type: 'password', password });
  }

  function submitCredentials(e: SubmitEvent) {
    e.preventDefault();
    if (!credentialsValid || busy) return;
    onsubmit({ type: 'credentials', username: username.trim(), password: credentialPassword });
  }

  function submitCookies(e: SubmitEvent) {
    e.preventDefault();
    if (step.type !== 'cookies' || !cookiesValid || busy) return;
    const values: Record<string, string> = {};
    for (const field of step.fields) values[field.name] = cookieValues[field.name] ?? '';
    onsubmit({ type: 'cookies', values });
  }

  function refreshQr() {
    if (busy) return;
    onsubmit({ type: 'qr_refresh' });
  }

  function codeHint(via: 'app' | 'sms' | 'email' | 'other'): string {
    switch (via) {
      case 'app':
        return t('onboarding-code-sent-app', { network: networkName });
      case 'sms':
        return t('onboarding-code-sent-sms');
      case 'email':
        return t('onboarding-code-sent-email');
      case 'other':
        return t('onboarding-code-sent-other');
    }
  }

  function fieldName(field: string): string {
    if (field === 'code') return t('onboarding-code-label');
    if (field === 'password') return t('onboarding-password-label');
    if (field === 'phone' || field === 'phone_number') return t('onboarding-phone-label');
    return t('onboarding-failure-field-fallback');
  }

  function failureText(reason: LoginFailureDto): string {
    switch (reason.type) {
      case 'invalid_input':
        return reason.field
          ? t('onboarding-failure-invalid-field', { field: fieldName(reason.field) })
          : t('onboarding-failure-invalid');
      case 'expired':
        return t('onboarding-failure-expired');
      case 'not_supported':
        return t('onboarding-failure-not-supported');
      case 'rate_limited':
        return t('onboarding-failure-rate-limited', {
          seconds: Math.max(1, Math.ceil(reason.retry_after_ms / 1000))
        });
      case 'network':
        return t('onboarding-failure-network');
      case 'account_locked':
        return t('onboarding-failure-account-locked', { network: networkName });
    }
  }

  /** The engine's own text, when it adds something the local copy cannot say. */
  function failureDetail(reason: LoginFailureDto): string | null {
    switch (reason.type) {
      case 'invalid_input':
      case 'network':
      case 'account_locked':
        return reason.message.trim().length > 0 ? reason.message : null;
      default:
        return null;
    }
  }

  // QR rendering: the payload is an opaque string; only the image crosses into the DOM.
  let qrDataUrl = $state<string | null>(null);
  let qrFailed = $state(false);
  $effect(() => {
    if (step.type !== 'qr') {
      qrDataUrl = null;
      qrFailed = false;
      return;
    }
    const payload = step.payload;
    let live = true;
    qrFailed = false;
    void toDataURL(payload, { margin: 1, width: 216, errorCorrectionLevel: 'M' })
      .then((url: string) => {
        if (live) qrDataUrl = url;
      })
      .catch(() => {
        if (live) {
          qrDataUrl = null;
          qrFailed = true;
        }
      });
    return () => {
      live = false;
    };
  });

  // The countdown ticks only while a QR step is on screen.
  let now = $state(Date.now());
  $effect(() => {
    if (step.type !== 'qr') return;
    const timer = setInterval(() => {
      now = Date.now();
    }, 1000);
    return () => clearInterval(timer);
  });
  const secondsLeft = $derived(
    step.type === 'qr' ? Math.max(0, Math.ceil((step.expires_at - now) / 1000)) : 0
  );

  function openCookiesHelp() {
    if (step.type === 'cookies' && step.help_url) void openExternal(step.help_url);
  }

  function cookieFieldLabel(field: CookieFieldDto): string {
    return field.required
      ? `${field.label} (${t('onboarding-required')})`
      : field.label;
  }
</script>

<div class="login-step">
  {#if error}<p class="error" role="alert">{error}</p>{/if}

  {#if step.type === 'phone_number'}
    <h3>{t('onboarding-phone-title')}</h3>
    <p class="hint">{t('onboarding-phone-hint')}</p>
    <form onsubmit={submitPhone}>
      <label>
        <span>{t('onboarding-phone-label')}</span>
        <TextField
          type="tel"
          bind:value={phone}
          placeholder={t('onboarding-phone-placeholder')}
          autocomplete="tel"
          inputmode="tel"
          autofocus
          required
        />
      </label>
      <div class="actions">
        <Button variant="ghost" type="button" onclick={oncancel}>{t('common-cancel')}</Button>
        <Button type="submit" disabled={busy || !phoneValid}>
          {busy ? t('onboarding-working') : t('common-continue')}
        </Button>
      </div>
    </form>
  {:else if step.type === 'code'}
    <h3>{t('onboarding-code-title')}</h3>
    <p class="hint">{codeHint(step.via)}</p>
    {#if step.length !== null}
      <p class="hint">{t('onboarding-code-length', { count: step.length })}</p>
    {/if}
    <form onsubmit={submitCode}>
      <label>
        <span>{t('onboarding-code-label')}</span>
        <TextField
          type="text"
          bind:value={code}
          inputmode="numeric"
          autocomplete="one-time-code"
          pattern="[0-9]*"
          maxlength={step.length ?? undefined}
          autofocus
          required
        />
      </label>
      <div class="actions">
        <Button variant="ghost" type="button" onclick={oncancel}>{t('common-cancel')}</Button>
        <Button type="submit" disabled={busy || !codeValid}>
          {busy ? t('onboarding-working') : t('common-continue')}
        </Button>
      </div>
    </form>
  {:else if step.type === 'password'}
    <h3>{t('onboarding-password-title')}</h3>
    <p class="hint">{t('onboarding-password-hint')}</p>
    {#if step.hint}
      <p class="hint">{t('onboarding-password-network-hint', { hint: step.hint })}</p>
    {/if}
    <form onsubmit={submitPassword}>
      <label>
        <span>{t('onboarding-password-label')}</span>
        <TextField
          type="password"
          bind:value={password}
          autocomplete="current-password"
          autofocus
          required
        />
      </label>
      <div class="actions">
        <Button variant="ghost" type="button" onclick={oncancel}>{t('common-cancel')}</Button>
        <Button type="submit" disabled={busy || !passwordValid}>
          {busy ? t('onboarding-working') : t('common-continue')}
        </Button>
      </div>
    </form>
  {:else if step.type === 'qr'}
    <h3>{t('onboarding-qr-title')}</h3>
    <p class="hint">{t('onboarding-qr-hint', { network: networkName })}</p>
    <div class="qr">
      {#if qrDataUrl}
        <img src={qrDataUrl} alt={t('onboarding-qr-alt')} width="216" height="216" />
      {:else if qrFailed}
        <p class="error" role="alert">{t('onboarding-qr-failed')}</p>
      {/if}
    </div>
    {#if secondsLeft > 0}
      <p class="hint">
        {t('onboarding-qr-expires', { seconds: secondsLeft })}
      </p>
    {:else}
      <p class="hint" role="status">{t('onboarding-qr-expired')}</p>
    {/if}
    <div class="actions">
      <Button variant="ghost" type="button" onclick={oncancel}>{t('common-cancel')}</Button>
      <Button type="button" disabled={busy} onclick={refreshQr}>
        {secondsLeft > 0 ? t('onboarding-qr-refresh-waiting') : t('onboarding-qr-refresh')}
      </Button>
    </div>
  {:else if step.type === 'cookies'}
    <h3>{t('onboarding-cookies-title')}</h3>
    <p class="hint">{t('onboarding-cookies-hint')}</p>
    {#if step.help_url}
      <button type="button" class="link" onclick={openCookiesHelp}>
        {t('onboarding-cookies-help')}
      </button>
    {/if}
    <form onsubmit={submitCookies}>
      {#each step.fields as field (field.name)}
        <label>
          <span>{cookieFieldLabel(field)}</span>
          <TextField
            type="text"
            bind:value={cookieValues[field.name]}
            autocomplete="off"
            autocapitalize="off"
            spellcheck={false}
            required={field.required}
          />
        </label>
      {/each}
      <div class="actions">
        <Button variant="ghost" type="button" onclick={oncancel}>{t('common-cancel')}</Button>
        <Button type="submit" disabled={busy || !cookiesValid}>
          {busy ? t('onboarding-working') : t('common-continue')}
        </Button>
      </div>
    </form>
  {:else if step.type === 'credentials'}
    <h3>{t('onboarding-credentials-title')}</h3>
    <p class="hint">{t('onboarding-credentials-hint', { network: networkName })}</p>
    <form onsubmit={submitCredentials}>
      <label>
        <span>{step.username_label}</span>
        <TextField
          type="text"
          bind:value={username}
          autocomplete="username"
          autocapitalize="off"
          spellcheck={false}
          autofocus
          required
        />
      </label>
      <label>
        <span>{t('onboarding-password-label')}</span>
        <TextField
          type="password"
          bind:value={credentialPassword}
          autocomplete="current-password"
          required
        />
      </label>
      <div class="actions">
        <Button variant="ghost" type="button" onclick={oncancel}>{t('common-cancel')}</Button>
        <Button type="submit" disabled={busy || !credentialsValid}>
          {busy ? t('onboarding-working') : t('common-continue')}
        </Button>
      </div>
    </form>
  {:else if step.type === 'done'}
    <h3>{t('onboarding-done-title')}</h3>
    <p class="hint" role="status">{t('onboarding-done-hint', { network: networkName })}</p>
  {:else if step.type === 'failed'}
    <h3>{t('onboarding-failed-title')}</h3>
    <p class="error" role="alert">{failureText(step.reason)}</p>
    {#if failureDetail(step.reason)}
      <p class="hint">{failureDetail(step.reason)}</p>
    {/if}
    <div class="actions">
      <Button variant="ghost" type="button" onclick={onback}>
        {t('onboarding-use-another-network')}
      </Button>
      <Button type="button" disabled={busy} onclick={onretry}>{t('common-retry')}</Button>
    </div>
  {/if}
</div>

<style>
  .login-step {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-3);
  }
  h3 {
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
  form {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-4);
    margin-top: var(--alloy-spacing-scale-2);
  }
  label {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-1);
    font-size: var(--alloy-typography-size-sm);
    font-weight: var(--alloy-typography-weight-medium, 500);
    color: var(--alloy-color-text-secondary);
  }
  .actions {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--alloy-spacing-scale-3);
    margin-top: var(--alloy-spacing-scale-2);
  }
  .error {
    margin: 0;
    color: var(--alloy-color-status-error);
    font-size: var(--alloy-typography-size-sm);
  }
  .qr {
    display: grid;
    place-items: center;
    padding: var(--alloy-spacing-scale-4);
    border-radius: var(--alloy-radii-lg);
    background: var(--alloy-color-surface-elevated);
    border: 1px solid var(--alloy-color-border-subtle);
  }
  .qr img {
    /* The code must stay scannable: square pixels, no smoothing, nothing on top of it. */
    image-rendering: pixelated;
    display: block;
    width: 216px;
    height: 216px;
    max-width: 100%;
  }
  .link {
    align-self: flex-start;
    padding: 0;
    border: 0;
    background: none;
    color: var(--alloy-color-accent-primary);
    font: inherit;
    font-size: var(--alloy-typography-size-sm);
    text-decoration: underline;
    cursor: pointer;
  }
  .link:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }
  @media (max-width: 899px) {
    .hint,
    label {
      font-size: var(--alloy-typography-size-base);
    }
    .actions {
      flex-direction: column-reverse;
      align-items: stretch;
    }
    .actions :global(button) {
      width: 100%;
    }
    .link {
      min-height: 44px;
    }
  }
</style>
