<script lang="ts">
  import { Button, TextField } from '../common';
  import {
    ChevronDown,
    ChevronLeft,
    ChevronUp,
    ExternalLink,
    Icon,
    MessageSquare,
    ShieldCheck,
    Smartphone
  } from '$lib/icons';
  import { session } from '$lib/stores/session.svelte';
  import { errorMessage, oidcErrorMessage } from '$lib/stores/errors';

  type Step = 'welcome' | 'server' | 'login';

  /** The host a server URL names, as the copy shows it. */
  function hostOf(url: string): string {
    try {
      return new URL(url).host;
    } catch {
      return url;
    }
  }

  let step = $state<Step>('welcome');
  // A session that could not be restored leaves its server here, so the user retypes the password, not the address.
  let serverInput = $state(session.lastHomeserver ? hostOf(session.lastHomeserver) : 'matrix.org');
  let homeserverUrl = $state('');
  let homeserverName = $state('');
  let supportsOidc = $state(false);
  let username = $state('');
  let password = $state('');
  let localError = $state<string | null>(null);
  let checking = $state(false);
  /** The password form sits behind a disclosure when the server offers browser sign-in. */
  let passwordOpen = $state(false);
  let waitingTitle: HTMLElement | undefined = $state();

  const serverHost = $derived.by(() => {
    try {
      return new URL(homeserverUrl).host;
    } catch {
      return homeserverName;
    }
  });

  // The waiting panel is a new context; a screen reader should hear what the app is waiting for.
  $effect(() => {
    if (session.oidcWaiting) waitingTitle?.focus();
  });

  function go(next: Step) {
    localError = null;
    step = next;
  }

  async function continueServer(e: Event) {
    e.preventDefault();
    localError = null;
    session.clearOidcFailure();
    checking = true;
    try {
      const info = await session.discover(serverInput.trim());
      homeserverUrl = info.url;
      homeserverName = info.name;
      supportsOidc = info.supports_oidc;
      passwordOpen = false;
      step = 'login';
    } catch (err) {
      localError = `Couldn't reach that server. ${errorMessage(err)}`;
    } finally {
      checking = false;
    }
  }

  /** Starts (or retries) the browser sign-in; the store reports any failure for the panel below. */
  async function startOidc() {
    await session.startOidc(homeserverUrl).catch(() => {});
  }

  async function cancelOidc() {
    await session.cancelOidc();
  }

  function differentServer() {
    session.clearOidcFailure();
    go('server');
  }

  async function signIn(e: Event) {
    e.preventDefault();
    localError = null;
    try {
      await session.login(homeserverUrl, username.trim(), password);
    } catch (err) {
      localError = errorMessage(err);
    }
  }
</script>

<main class="onboarding">
  <section class="panel" data-step={step}>
    <div class="story">
      <div class="lockup">
        <span class="brand-mark" aria-hidden="true">A</span>
        <span class="wordmark">Alloy</span>
      </div>
      <div class="pitch">
        <h1>Your conversations,<br />without the noise.</h1>
        <p class="lede">
          Alloy is a calm, private chat client for Matrix. No algorithms, no ads — just your rooms,
          on a server you choose.
        </p>
      </div>
      <ul class="points">
        <li>
          <Icon icon={ShieldCheck} size={20} strokeWidth={1.5} />
          <span><strong>End-to-end encrypted</strong> by default, with device verification built in.</span>
        </li>
        <li>
          <Icon icon={MessageSquare} size={20} strokeWidth={1.5} />
          <span><strong>Rooms, threads and spaces</strong> that stay in order and out of your way.</span>
        </li>
        <li>
          <Icon icon={Smartphone} size={20} strokeWidth={1.5} />
          <span><strong>Your account, anywhere</strong> — sign in on desktop or phone, synced live.</span>
        </li>
      </ul>
      <p class="footnote">Open source · Apache-2.0</p>
    </div>

    <div class="step">
      {#if step === 'welcome'}
        <div class="intro">
          <p class="eyebrow">Welcome to Alloy</p>
          <h2>Sign in to Matrix</h2>
          {#if session.restoreNotice}
            <p class="notice" role="status">{session.restoreNotice}</p>
          {/if}
          <p class="hint">
            You’ll pick your homeserver, then your account. It takes about a minute.
          </p>
        </div>
        <div class="cta">
          <Button size="lg" onclick={() => go('server')}>Get started</Button>
        </div>
      {:else}
        <div class="progress" aria-hidden="true">
          <span class="bar" class:on={step === 'server' || step === 'login'}></span>
          <span class="bar" class:on={step === 'login'}></span>
        </div>

        {#if step === 'server'}
          <div class="step-head">
            <p class="eyebrow">Step 1 of 2</p>
            <h2>Where is your account?</h2>
            <p class="hint">Your homeserver hosts your rooms and messages. You’ll sign in there.</p>
          </div>
          <form onsubmit={continueServer}>
            <label>
              <span>Homeserver</span>
              <TextField
                type="text"
                bind:value={serverInput}
                placeholder="matrix.org"
                autocomplete="off"
                autocapitalize="off"
                spellcheck={false}
                autofocus
                required
              />
            </label>
            {#if localError}<p class="error" role="alert">{localError}</p>{/if}
            <div class="actions">
              <Button variant="ghost" type="button" onclick={() => go('welcome')}>
                <Icon icon={ChevronLeft} size={16} strokeWidth={1.5} />
                Back
              </Button>
              <Button type="submit" disabled={checking || !serverInput.trim()}>
                {checking ? 'Checking…' : 'Continue'}
              </Button>
            </div>
          </form>
        {:else}
          {#if session.oidcWaiting}
            <div class="step-head">
              <p class="eyebrow">Step 2 of 2 · {homeserverName}</p>
              <h2 class="waiting-title" tabindex="-1" bind:this={waitingTitle}>
                Waiting for your browser
              </h2>
              <p class="hint">
                We opened a browser window. Approve the sign-in for {serverHost}, then come back
                here.
              </p>
            </div>
            <p class="waiting" role="status">
              <span class="pulse" aria-hidden="true"></span>
              Waiting for approval…
            </p>
            <div class="actions waiting-actions">
              <Button variant="ghost" type="button" onclick={startOidc} disabled={session.busy}>
                <Icon icon={ExternalLink} size={16} strokeWidth={1.5} />
                {session.busy ? 'Opening…' : 'Open browser again'}
              </Button>
              <Button variant="ghost" type="button" onclick={cancelOidc}>Cancel</Button>
            </div>
            <p class="footnote-inline">
              Nothing happened? You can close the browser tab and try again.
            </p>
          {:else if supportsOidc}
            <div class="step-head">
              <p class="eyebrow">Step 2 of 2 · {homeserverName}</p>
              <h2>Sign in</h2>
              <p class="hint">You’ll approve sign-in on {serverHost} and come back here.</p>
            </div>
            {#if session.oidcFailure}
              <p class="error" role="alert">
                {oidcErrorMessage(session.oidcFailure, serverHost)}
              </p>
              <div class="actions">
                <Button variant="ghost" type="button" onclick={differentServer}>
                  Use a different server
                </Button>
                <Button type="button" onclick={startOidc} disabled={session.busy}>
                  {session.busy ? 'Opening…' : 'Try again'}
                </Button>
              </div>
            {:else}
              <div class="browser-path">
                <div class="browser-cta">
                  <Button size="lg" type="button" onclick={startOidc} disabled={session.busy}>
                    <Icon icon={ExternalLink} size={16} strokeWidth={1.5} />
                    {session.busy ? 'Opening browser…' : 'Continue in browser'}
                  </Button>
                </div>
                <p class="trust">
                  <Icon icon={ShieldCheck} size={16} strokeWidth={1.5} />
                  Your password stays on {serverHost}, never in Alloy.
                </p>
              </div>
              <div class="divider" aria-hidden="true"></div>
              <button
                class="disclosure"
                type="button"
                aria-expanded={passwordOpen}
                onclick={() => (passwordOpen = !passwordOpen)}
              >
                Use a password instead
                <Icon icon={passwordOpen ? ChevronUp : ChevronDown} size={16} strokeWidth={1.5} />
              </button>
              {#if passwordOpen}
                <form onsubmit={signIn}>
                  <label>
                    <span>Username</span>
                    <TextField
                      type="text"
                      bind:value={username}
                      placeholder="alice"
                      autocomplete="username"
                      autocapitalize="off"
                      spellcheck={false}
                      required
                    />
                  </label>
                  <label>
                    <span>Password</span>
                    <TextField
                      type="password"
                      bind:value={password}
                      autocomplete="current-password"
                      required
                    />
                  </label>
                  {#if localError}<p class="error" role="alert">{localError}</p>{/if}
                  <div class="actions">
                    <Button variant="ghost" type="button" onclick={() => go('server')}>
                      <Icon icon={ChevronLeft} size={16} strokeWidth={1.5} />
                      Back
                    </Button>
                    <Button type="submit" disabled={session.busy || !username || !password}>
                      {session.busy ? 'Signing in…' : 'Sign in'}
                    </Button>
                  </div>
                </form>
              {/if}
            {/if}
            {#if !passwordOpen && !session.oidcFailure}
              <div class="back-row">
                <Button variant="ghost" type="button" onclick={() => go('server')}>
                  <Icon icon={ChevronLeft} size={16} strokeWidth={1.5} />
                  Back
                </Button>
              </div>
            {/if}
          {:else}
            <div class="step-head">
              <p class="eyebrow">Step 2 of 2 · {homeserverName}</p>
              <h2>Sign in</h2>
              <p class="hint">
                This server doesn’t support browser sign-in. Use your Matrix ID and password for
                this homeserver.
              </p>
            </div>
            <form onsubmit={signIn}>
              <label>
                <span>Username</span>
                <TextField
                  type="text"
                  bind:value={username}
                  placeholder="alice"
                  autocomplete="username"
                  autocapitalize="off"
                  spellcheck={false}
                  autofocus
                  required
                />
              </label>
              <label>
                <span>Password</span>
                <TextField
                  type="password"
                  bind:value={password}
                  autocomplete="current-password"
                  required
                />
              </label>
              {#if localError}<p class="error" role="alert">{localError}</p>{/if}
              <div class="actions">
                <Button variant="ghost" type="button" onclick={() => go('server')}>
                  <Icon icon={ChevronLeft} size={16} strokeWidth={1.5} />
                  Back
                </Button>
                <Button type="submit" disabled={session.busy || !username || !password}>
                  {session.busy ? 'Signing in…' : 'Sign in'}
                </Button>
              </div>
            </form>
          {/if}
        {/if}
      {/if}
    </div>
  </section>
</main>

<style>
  .onboarding {
    box-sizing: border-box;
    min-height: 100dvh;
    display: grid;
    place-items: center;
    padding: calc(var(--alloy-spacing-scale-6) + var(--alloy-safe-area-top))
      var(--alloy-layout-gutter)
      calc(var(--alloy-spacing-scale-6) + var(--alloy-safe-area-bottom));
    background: var(--alloy-canvas-art);
  }

  /* One floating sheet over the aurora; the story column anchors the brand, the step column does the work. */
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
    display: flex;
    gap: var(--alloy-spacing-scale-3);
    color: var(--alloy-color-text-secondary);
    font-size: var(--alloy-typography-size-sm);
    line-height: var(--alloy-typography-line-height-normal, 1.45);
  }
  .points :global(svg) {
    flex: none;
    margin-top: 2px;
    color: var(--alloy-color-accent-primary);
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
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-6);
    padding: var(--alloy-spacing-scale-8);
    background: color-mix(in srgb, var(--alloy-color-surface-panel) 55%, transparent);
  }
  /* The welcome step is only a heading and one action; centre it against the taller story. */
  .panel[data-step='welcome'] .step {
    justify-content: center;
  }
  .intro,
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

  .progress {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--alloy-spacing-scale-1);
  }
  .bar {
    height: 3px;
    border-radius: var(--alloy-radii-full);
    background: color-mix(in srgb, var(--alloy-color-text-primary) 12%, transparent);
    transition: background-color 160ms ease;
  }
  .bar.on {
    background: var(--alloy-color-accent-primary);
  }

  form {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-4);
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
  .cta {
    width: 100%;
  }
  .cta :global(button) {
    width: 100%;
  }
  .error {
    margin: 0;
    color: var(--alloy-color-status-error);
    font-size: var(--alloy-typography-size-sm);
  }

  /* Waiting for the browser: quiet, but alive. */
  .waiting {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    margin: 0;
    font-size: var(--alloy-typography-size-sm);
    color: var(--alloy-color-text-secondary);
  }
  .pulse {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: var(--alloy-radii-full);
    background: var(--alloy-color-accent-primary);
    animation: pulse 1.4s ease-in-out infinite;
  }
  @keyframes pulse {
    0%,
    100% {
      opacity: 0.3;
    }
    50% {
      opacity: 1;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .pulse {
      animation: none;
    }
  }
  /* The heading receives focus so a screen reader hears the new state; it is not an input. */
  .waiting-title:focus {
    outline: none;
  }
  .footnote-inline {
    margin: 0;
    font-size: var(--alloy-typography-size-xs);
    color: var(--alloy-color-text-secondary);
  }

  /* Browser sign-in: one primary action, the trust note, then the password fallback. */
  .browser-path {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-2);
  }
  .browser-cta :global(button) {
    width: 100%;
  }
  .trust {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
    margin: 0;
    font-size: var(--alloy-typography-size-xs);
    color: var(--alloy-color-text-secondary);
  }
  .trust :global(svg) {
    flex: none;
    color: var(--alloy-color-accent-primary);
  }
  .divider {
    height: 1px;
    background: var(--alloy-glass-rim);
  }
  .disclosure {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
    padding: var(--alloy-spacing-scale-2) 0;
    border: 0;
    background: none;
    color: var(--alloy-color-text-secondary);
    font: inherit;
    font-size: var(--alloy-typography-size-sm);
    cursor: pointer;
  }
  .disclosure:hover {
    color: var(--alloy-color-text-primary);
  }
  .back-row {
    display: flex;
  }
  .notice {
    margin: 0;
    padding: var(--alloy-spacing-scale-2) var(--alloy-spacing-scale-3);
    border-left: 2px solid var(--alloy-color-accent-primary);
    border-radius: var(--alloy-radii-sm);
    background: var(--alloy-color-accent-subtle);
    color: var(--alloy-color-text-secondary);
    font-size: var(--alloy-typography-size-sm);
  }

  /* Phone: the sheet becomes the screen. The pitch only stays on the welcome step, so forms get the room. */
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
    .panel:not([data-step='welcome']) .story {
      display: none;
    }
    .panel:not([data-step='welcome']) .step {
      min-height: 100dvh;
    }
    /* Hero fills the space above the action, which stays at thumb height. */
    .panel[data-step='welcome'] .story {
      flex: 1 1 auto;
      justify-content: center;
    }
    .panel[data-step='welcome'] .step {
      flex: none;
      justify-content: flex-start;
    }
    .step h2 {
      font-size: 1.5rem;
    }
    .hint,
    label {
      font-size: var(--alloy-typography-size-base);
    }
    .pitch h1 {
      font-size: 1.6rem;
    }
    .footnote {
      display: none;
    }
    .step {
      flex: 1 1 auto;
      padding: var(--alloy-spacing-scale-6) var(--alloy-spacing-scale-5)
        calc(var(--alloy-spacing-scale-6) + var(--alloy-safe-area-bottom));
      background: none;
    }
    form :global(button[type='submit']) {
      min-height: 48px;
    }
    .actions {
      flex-direction: column-reverse;
      align-items: stretch;
    }
    /* Waiting actions stack in reading order with Cancel at thumb height, per the sign-in spec. */
    .waiting-actions {
      flex-direction: column;
    }
    .disclosure {
      min-height: 44px;
    }
    .actions :global(button) {
      width: 100%;
    }
  }
</style>
