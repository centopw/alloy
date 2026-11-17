<script lang="ts">
  import { untrack } from 'svelte';
  import { Button, Select, TextArea, TextField } from '../common';
  import { openExternal } from '$lib/platform/links';
  import { importChromeSession, isChromeSessionSupported } from '$lib/platform/chromeSession';
  import { errorMessage } from '$lib/stores/errors';
  import { toDataURL } from 'qrcode';
  import { cookiesFromPaste } from '$lib/content/curlCookies';
  import { COUNTRIES, defaultCountry, parsePhone } from '$lib/content/phone';
  import type {
    ChallengeKind,
    CookieFieldDto,
    LoginAlternativeDto,
    LoginFailureDto,
    LoginInputDto,
    LoginStepDto
  } from '$lib/generated/index.js';

  import { currentLocale, t } from '$lib/i18n';

  interface Props {
    /** The step the engine returned; the parent remounts this component when it changes. */
    step: LoginStepDto;
    /** The network's display name, for copy that has to name it. */
    networkName: string;
    busy: boolean;
    /** A transport-level failure from the last call, shown above the step. */
    error: string | null;
    onsubmit: (input: LoginInputDto) => void;
    /** Picks one of the step's alternatives (`switch` input) without going through the form. */
    onswitch: (to: LoginAlternativeDto) => void;
    oncancel: () => void;
    onretry: () => void;
    onback: () => void;
  }

  let {
    step,
    networkName,
    busy,
    error,
    onsubmit,
    onswitch,
    oncancel,
    onretry,
    onback,
  }: Props = $props();

  let phone = $state('');
  let code = $state('');
  let password = $state('');
  let username = $state('');
  let credentialPassword = $state('');
  let cookieValues = $state<Record<string, string>>(untrack(() => cookieSeed(step)));

  // Chrome's session is offered on the Mac app only, and only once the person presses it: nothing
  // here reads Chrome or asks the Keychain (ADR 0022).
  const chromeAvailable = isChromeSessionSupported();
  let chromeBusy = $state(false);
  let chromeError = $state<string | null>(null);
  let chromeImported = $state(false);
  let chromeShared = $state(false);

  function cookieSeed(value: LoginStepDto): Record<string, string> {
    if (value.type !== 'cookies') return {};
    return Object.fromEntries(value.fields.map((field) => [field.name, '']));
  }

  // The protocol wants international format; the country comes from the dropdown, or from the
  // number itself when it is typed with a `+`.
  let country = $state(defaultCountry(typeof navigator === 'undefined' ? [] : navigator.languages));
  const dial = $derived(COUNTRIES.find((entry) => entry.iso === country)?.dial ?? '1');
  const parsedPhone = $derived(parsePhone(phone, dial));
  const phoneValid = $derived(/^\+\d{7,15}$/.test(parsedPhone.e164));

  const countryOptions = $derived.by(() => {
    const locale = currentLocale() === 'rtl' ? 'en' : currentLocale();
    const names = new Intl.DisplayNames([locale], { type: 'region', fallback: 'code' });
    return COUNTRIES.map((entry) => ({
      value: entry.iso,
      label: `${names.of(entry.iso) ?? entry.iso} (+${entry.dial})`
    })).sort((a, b) => a.label.localeCompare(b.label, locale));
  });

  // Calling codes several countries share: the one a typed `+` code selects.
  const SHARED_DIAL: Record<string, string> = { '1': 'US', '7': 'RU', '39': 'IT' };

  /** A number typed with its own country code moves the dropdown to that country. */
  function followTypedCountry(e: Event & { currentTarget: HTMLInputElement }) {
    const typed = parsePhone(e.currentTarget.value, dial).dial;
    if (typed === dial) return;
    country = SHARED_DIAL[typed] ?? COUNTRIES.find((entry) => entry.dial === typed)?.iso ?? country;
  }
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
    onsubmit({ type: 'phone_number', phone: parsedPhone.e164 });
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

  function chooseAlternative(to: LoginAlternativeDto) {
    if (busy) return;
    onswitch(to);
  }

  function alternativeLabel(alternative: LoginAlternativeDto): string {
    switch (alternative) {
      case 'cookies':
        return t('onboarding-alternative-cookies');
    }
  }

  // Browser sign-in: one request copied as cURL carries every cookie the step asks for.
  let cookiePaste = $state('');
  const missingCookies = $derived(
    step.type === 'cookies'
      ? step.fields
          .filter((field) => field.required && !(cookieValues[field.name] ?? '').trim())
          .map((field) => field.name)
      : []
  );

  function readCookiePaste(e: Event & { currentTarget: HTMLTextAreaElement }) {
    if (step.type !== 'cookies') return;
    absorbPaste(e.currentTarget.value);
  }

  /** Fills the required cookies from any text that carries them; shared by typing and the clipboard. */
  function absorbPaste(text: string) {
    if (step.type !== 'cookies') return;
    const found = cookiesFromPaste(text);
    for (const field of step.fields) {
      if (found[field.name]) cookieValues[field.name] = found[field.name];
    }
  }

  // Reading the clipboard needs a gesture and the platform's permission; when either refuses, the
  // box below still takes the same text, so a refusal is a hint rather than an error.
  let clipboardBlocked = $state(false);

  async function pasteFromClipboard() {
    if (step.type !== 'cookies' || busy) return;
    clipboardBlocked = false;
    let text = '';
    try {
      text = (await navigator.clipboard?.readText?.()) ?? '';
    } catch {
      clipboardBlocked = true;
      return;
    }
    if (text.trim().length === 0) {
      clipboardBlocked = true;
      return;
    }
    cookiePaste = text;
    absorbPaste(text);
  }

  /**
   * Fills the cookies from the Chrome profile on this Mac. The import is a one-off: what it fills
   * is submitted as the cookie login, and the account's own stored session takes over from there.
   */
  async function useChromeSession() {
    if (step.type !== 'cookies' || busy || chromeBusy) return;
    chromeBusy = true;
    chromeError = null;
    try {
      const imported = await importChromeSession();
      for (const field of step.fields) {
        const value = imported.cookies[field.name];
        if (value) cookieValues[field.name] = value;
      }
      chromeImported = true;
      chromeShared = imported.active;
    } catch (e) {
      chromeError = errorMessage(e);
    } finally {
      chromeBusy = false;
    }
  }

  // The cookie step is Messenger's (see LoginStep::Cookies in alloy-connector); its session
  // cookies are the ones messenger.com sets.
  const COOKIE_SIGN_IN_URL = 'https://www.messenger.com/';

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

  // A challenge can only be cleared on Meta's own site, so the step names the one the connector
  // reported and points there instead of asking for another local answer.
  function challengeTitle(kind: ChallengeKind): string {
    switch (kind) {
      case 'checkpoint':
        return t('onboarding-challenge-checkpoint-title');
      case 'two_factor':
        return t('onboarding-challenge-two-factor-title');
      case 'consent':
        return t('onboarding-challenge-consent-title');
      case 'unsupported_mfa':
        return t('onboarding-challenge-unsupported-mfa-title');
    }
  }

  function challengeBody(kind: ChallengeKind): string {
    switch (kind) {
      case 'checkpoint':
        return t('onboarding-challenge-checkpoint-body');
      case 'two_factor':
        return t('onboarding-challenge-two-factor-body');
      case 'consent':
        return t('onboarding-challenge-consent-body');
      case 'unsupported_mfa':
        return t('onboarding-challenge-unsupported-mfa-body');
    }
  }

  /** The page the challenge has to be cleared on. */
  const CHALLENGE_SITE_URL = 'https://www.facebook.com/';
  /** A challenge only a method this app cannot drive will clear: the fix is a security setting. */
  const CHALLENGE_SECURITY_SETTINGS_URL = 'https://www.facebook.com/settings?tab=security';

  function challengeSiteUrl(kind: ChallengeKind): string {
    switch (kind) {
      case 'checkpoint':
      case 'two_factor':
      case 'consent':
        return CHALLENGE_SITE_URL;
      case 'unsupported_mfa':
        return CHALLENGE_SECURITY_SETTINGS_URL;
    }
  }

  function openChallengeSite() {
    if (step.type !== 'challenge') return;
    void openExternal(challengeSiteUrl(step.kind));
  }

  /**
   * Answers the code a two-factor challenge asks for with the same `code` input a code step uses.
   * The attempt is still open, so the connector takes the answer and continues.
   */
  function submitChallengeCode(e: SubmitEvent) {
    e.preventDefault();
    const answer = code.trim();
    if (busy || answer.length === 0) return;
    onsubmit({ type: 'code', code: answer });
  }

  // An approval step advances on its own: the connector holds each answer for its poll window, so
  // the screen asks again as soon as that window comes back until the network moves on. The small
  // delay keeps a fast answer from turning into a hot loop.
  $effect(() => {
    if (step.type !== 'wait' || busy) return;
    const timer = setTimeout(() => onsubmit({ type: 'continue' }), 1200);
    return () => clearTimeout(timer);
  });

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
      <div class="phone-row">
        <div class="field">
          <span>{t('onboarding-phone-country-label')}</span>
          <Select
            bind:value={country}
            options={countryOptions}
            label={t('onboarding-phone-country-label')}
            searchable
            searchPlaceholder={t('onboarding-phone-country-search')}
          />
        </div>
        <label class="field">
          <span>{t('onboarding-phone-label')}</span>
          <TextField
            type="tel"
            bind:value={phone}
            oninput={followTypedCountry}
            autocomplete="tel"
            inputmode="tel"
            autofocus
            required
          />
        </label>
      </div>
      {#if parsedPhone.e164}
        <p class="hint" role="status">
          {t('onboarding-phone-preview', { phone: parsedPhone.e164 })}
        </p>
      {/if}
      <div class="actions">
        <Button variant="ghost" type="button" onclick={oncancel}>{t('common-cancel')}</Button>
        <Button type="submit" disabled={busy || !phoneValid}>
          {busy ? t('onboarding-working') : t('common-continue')}
        </Button>
      </div>
    </form>
  {:else if step.type === 'wait'}
    <h3>{t('onboarding-wait-title')}</h3>
    <p class="hint" role="status">{step.message}</p>
    <p class="hint">{t('onboarding-wait-hint')}</p>
    <div class="actions">
      <Button variant="ghost" type="button" onclick={oncancel}>{t('common-cancel')}</Button>
      <Button type="button" disabled={busy} onclick={() => onsubmit({ type: 'continue' })}>
        {busy ? t('onboarding-working') : t('onboarding-wait-continue')}
      </Button>
    </div>
    <div class="alternatives">
      <button type="button" class="link" disabled={busy} onclick={() => chooseAlternative('cookies')}>
        {t('onboarding-alternative-cookies')}
      </button>
    </div>
  {:else if step.type === 'choice'}
    <h3>{t('onboarding-choice-title')}</h3>
    <p class="hint">{step.message || t('onboarding-choice-fallback')}</p>
    <div class="choices">
      {#each step.options as option (option)}
        <Button
          type="button"
          variant="secondary"
          disabled={busy}
          onclick={() => onsubmit({ type: 'choice', field: step.field, value: option })}
        >
          {option}
        </Button>
      {/each}
    </div>
    <div class="actions">
      <Button variant="ghost" type="button" onclick={oncancel}>{t('common-cancel')}</Button>
    </div>
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
    <p class="hint">{t('onboarding-cookies-reason', { network: networkName })}</p>
    {#if chromeAvailable && step.fields.some((field) => field.name === 'c_user')}
      <div class="chrome-session">
        <Button
          type="button"
          variant="secondary"
          disabled={busy || chromeBusy}
          onclick={useChromeSession}
        >
          {chromeBusy ? t('onboarding-working') : t('onboarding-chrome-session')}
        </Button>
        {#if chromeImported}
          <p class="hint" role="status">{t('onboarding-chrome-imported')}</p>
          {#if chromeShared}
            <p class="hint">{t('onboarding-chrome-shared')}</p>
          {/if}
        {/if}
        {#if chromeError}
          <p class="error" role="alert">{chromeError}</p>
        {/if}
      </div>
    {/if}
    <ol class="steps">
      <li>
        <button type="button" class="link" onclick={() => void openExternal(COOKIE_SIGN_IN_URL)}>
          {t('onboarding-cookies-open', { network: networkName })}
        </button>
        {t('onboarding-cookies-step-sign-in')}
      </li>
      <li>{t('onboarding-cookies-step-inspect')}</li>
      <li>{t('onboarding-cookies-step-network')}</li>
      <li>{t('onboarding-cookies-step-copy', { network: networkName })}</li>
      <li>{t('onboarding-cookies-step-paste')}</li>
    </ol>
    <form onsubmit={submitCookies}>
      <label>
        <span>{t('onboarding-cookies-paste-label')}</span>
        <TextArea
          bind:value={cookiePaste}
          oninput={readCookiePaste}
          rows={4}
          autocomplete="off"
          autocapitalize="off"
          spellcheck={false}
          autofocus
        />
      </label>
      <div class="paste-actions">
        <button type="button" class="link" disabled={busy} onclick={() => void pasteFromClipboard()}>
          {t('onboarding-cookies-clipboard')}
        </button>
      </div>
      {#if clipboardBlocked}
        <p class="hint" role="status">{t('onboarding-cookies-clipboard-blocked')}</p>
      {/if}
      {#if cookiePaste.trim().length > 0}
        {#if missingCookies.length === 0}
          <p class="hint" role="status">{t('onboarding-cookies-paste-found')}</p>
        {:else}
          <p class="error" role="alert">
            {t('onboarding-cookies-paste-missing', { names: missingCookies.join(', ') })}
          </p>
        {/if}
      {/if}
      <details>
        <summary>{t('onboarding-cookies-manual')}</summary>
        <div class="manual">
          <p class="hint">{t('onboarding-cookies-hint')}</p>
          {#if step.help_url}
            <button type="button" class="link" onclick={openCookiesHelp}>
              {t('onboarding-cookies-help')}
            </button>
          {/if}
          {#each step.fields as field (field.name)}
            <label>
              <span>{cookieFieldLabel(field)}</span>
              <TextField
                type="text"
                bind:value={cookieValues[field.name]}
                autocomplete="off"
                autocapitalize="off"
                spellcheck={false}
              />
            </label>
          {/each}
        </div>
      </details>
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
          {busy ? t('onboarding-working') : t('onboarding-credentials-submit')}
        </Button>
      </div>
    </form>
    {#if step.alternatives.length > 0}
      <div class="alternatives">
        {#each step.alternatives as alternative (alternative)}
          <button
            type="button"
            class="link"
            disabled={busy}
            onclick={() => chooseAlternative(alternative)}
          >
            {alternativeLabel(alternative)}
          </button>
        {/each}
      </div>
    {/if}
  {:else if step.type === 'done'}
    <h3>{t('onboarding-done-title')}</h3>
    <p class="hint" role="status">{t('onboarding-done-hint', { network: networkName })}</p>
  {:else if step.type === 'challenge'}
    <h3>{challengeTitle(step.kind)}</h3>
    <p class="error" role="alert">{challengeBody(step.kind)}</p>
    {#if step.message.trim().length > 0}
      <p class="hint">{step.message}</p>
    {/if}
    {#if step.kind === 'two_factor'}
      <form onsubmit={submitChallengeCode}>
        <label>
          <span>{t('onboarding-code-label')}</span>
          <TextField
            type="text"
            bind:value={code}
            inputmode="numeric"
            autocomplete="one-time-code"
            pattern="[0-9]*"
            autofocus
            required
          />
        </label>
        <div class="actions">
          <Button type="submit" disabled={busy || code.trim().length === 0}>
            {busy ? t('onboarding-working') : t('onboarding-challenge-verify')}
          </Button>
        </div>
      </form>
    {/if}
    <div class="actions">
      <Button variant="ghost" type="button" onclick={onback}>
        {t('onboarding-use-another-network')}
      </Button>
      <Button variant="ghost" type="button" disabled={busy} onclick={onretry}>
        {t('common-retry')}
      </Button>
      <Button type="button" onclick={openChallengeSite}>
        {t('onboarding-challenge-open-facebook')}
      </Button>
    </div>
    <div class="alternatives">
      <button type="button" class="link" disabled={busy} onclick={() => chooseAlternative('cookies')}>
        {t('onboarding-alternative-cookies')}
      </button>
    </div>
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
    color: var(--alloy-color-text-muted);
    font-size: var(--alloy-font-size-preview);
    line-height: var(--alloy-typography-line-height-normal, 1.45);
  }
  form {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-4);
    margin-top: var(--alloy-spacing-scale-2);
  }
  /* Labels sit 13px above their field with 8px of air, matching a settings row label. */
  label,
  .field {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-2);
    min-width: 0;
    font-size: var(--alloy-font-size-row);
    font-weight: var(--alloy-typography-weight-medium);
    color: var(--alloy-color-text-secondary);
  }
  /* The number is typed on one line with its country on wide columns; phones stack the pair. */
  .phone-row {
    display: grid;
    grid-template-columns: minmax(0, 0.85fr) minmax(0, 1.15fr);
    gap: var(--alloy-spacing-scale-2);
  }
  .actions {
    display: flex;
    flex-direction: column-reverse;
    align-items: stretch;
    gap: var(--alloy-spacing-scale-2);
    margin-top: var(--alloy-spacing-scale-2);
  }
  .actions :global(.alloy-button) {
    width: 100%;
    min-height: var(--alloy-layout-control-height);
    touch-action: manipulation;
  }
  /* The network's options are listed in its own order, so this stack is not reversed like .actions. */
  .choices {
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: var(--alloy-spacing-scale-2);
    margin-top: var(--alloy-spacing-scale-2);
  }
  .choices :global(.alloy-button) {
    width: 100%;
    min-height: var(--alloy-layout-control-height);
    touch-action: manipulation;
  }
  .error {
    margin: 0;
    color: var(--alloy-color-status-error);
    font-size: var(--alloy-font-size-preview);
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
  .steps {
    margin: 0;
    padding-inline-start: var(--alloy-spacing-scale-5);
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-2);
    color: var(--alloy-color-text-secondary);
    font-size: var(--alloy-font-size-preview);
    line-height: var(--alloy-typography-line-height-normal, 1.45);
  }
  /* The Chrome import is one action above the manual steps, not a fifth step in their list. */
  .chrome-session {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-2);
    margin-top: var(--alloy-spacing-scale-2);
  }
  .chrome-session :global(.alloy-button) {
    width: 100%;
    min-height: var(--alloy-layout-control-height);
    touch-action: manipulation;
  }
  .steps .link {
    display: inline;
  }
  details {
    font-size: var(--alloy-font-size-preview);
    color: var(--alloy-color-text-secondary);
  }
  summary {
    cursor: pointer;
    touch-action: manipulation;
  }
  @media (hover: hover) {
    summary:hover {
      color: var(--alloy-color-text-primary);
    }
  }
  summary:active {
    color: var(--alloy-color-text-primary);
  }
  summary:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }
  .manual {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-3);
    margin-top: var(--alloy-spacing-scale-3);
  }
  .alternatives {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-2);
    margin-top: var(--alloy-spacing-scale-2);
  }
  .paste-actions {
    display: flex;
    margin-top: var(--alloy-spacing-scale-2);
  }
  .link {
    align-self: flex-start;
    padding: 0;
    border: 0;
    background: none;
    color: var(--alloy-color-accent-primary);
    font: inherit;
    font-size: var(--alloy-font-size-row);
    text-decoration: underline;
    cursor: pointer;
    touch-action: manipulation;
  }
  @media (hover: hover) {
    .link:hover:not(:disabled) {
      color: var(--alloy-color-text-link);
    }
  }
  .link:active:not(:disabled) {
    color: var(--alloy-color-text-link);
    opacity: 0.75;
  }
  .link:disabled {
    cursor: not-allowed;
    opacity: 0.5;
  }
  .link:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 2px;
  }
  @media (max-width: 899px) {
    .phone-row {
      grid-template-columns: minmax(0, 1fr);
    }
  }
  @media (max-width: 899px), (pointer: coarse) {
    .login-step {
      padding-bottom: calc(var(--alloy-spacing-scale-4) + var(--alloy-safe-area-bottom));
    }
    .actions :global(.alloy-button),
    .login-step :global(.select-trigger),
    .login-step :global(.text-field) {
      min-height: var(--alloy-touch-target, 44px);
    }
    summary {
      min-height: var(--alloy-touch-target, 44px);
      padding-block: var(--alloy-spacing-scale-2);
    }
    .link,
    .steps .link {
      min-height: var(--alloy-touch-target, 44px);
      display: inline-flex;
      align-items: center;
      text-align: start;
    }
  }
</style>
