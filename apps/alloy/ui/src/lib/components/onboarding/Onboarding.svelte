<script lang="ts">
  import { Button, TextField } from '../common';
  import { ChevronLeft, ExternalLink, Icon, MessageSquare, ShieldCheck, Smartphone } from '$lib/icons';
  import { session } from '$lib/stores/session.svelte';
  import { errorMessage, errorReason, oidcErrorMessage } from '$lib/stores/errors';

  type Step = 'welcome' | 'server' | 'login';
  /** How the account signs in: on the server's own page (OIDC/MAS), or with a password here. */
  type Method = 'oidc' | 'password';

  let step = $state<Step>('welcome');
  let method = $state<Method>('password');
  let serverInput = $state('matrix.org');
  let homeserverUrl = $state('');
  let homeserverName = $state('');
  let supportsOidc = $state(false);
  let username = $state('');
  let password = $state('');
  let localError = $state<string | null>(null);
  let checking = $state(false);

  const serverHost = $derived.by(() => {
    try {
      return new URL(homeserverUrl).host;
    } catch {
      return homeserverName;
    }
  });

  function go(next: Step) {
    localError = null;
    step = next;
  }

  function useMethod(next: Method) {
    localError = null;
    method = next;
  }

  async function continueServer(e: Event) {
    e.preventDefault();
    localError = null;
    checking = true;
    try {
      const info = await session.discover(serverInput.trim());
      homeserverUrl = info.url;
      homeserverName = info.name;
      supportsOidc = info.supports_oidc;
      method = info.supports_oidc ? 'oidc' : 'password';
      step = 'login';
    } catch (err) {
      localError = `Couldn't reach that server. ${errorMessage(err)}`;
    } finally {
      checking = false;
    }
  }

  async function startOidc() {
    localError = null;
    try {
      await session.startOidc(homeserverUrl);
    } catch (err) {
      localError = oidcErrorMessage(err);
      // The server cannot do browser sign-in after all; show the password form instead of a dead end.
      if (supportsOidc && errorReason(err) === 'unsupported') {
        method = 'password';
        supportsOidc = false;
      }
    }
  }

  async function cancelOidc() {
    localError = null;
    await session.cancelOidc();
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
            <p class="hint">Your homeserver hosts your rooms and messages.</p>
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
          <div class="step-head">
            <p class="eyebrow">Step 2 of 2 · {homeserverName}</p>
            <h2>{method === 'oidc' ? 'Sign in through your browser' : 'Sign in'}</h2>
            {#if method === 'oidc'}
              <p class="hint">
                {#if session.oidcWaiting}
                  Finish signing in at {serverHost} in the browser window. This screen updates by
                  itself.
                {:else}
                  Your account on {serverHost} signs in on its own page, so Alloy never sees your
                  password.
                {/if}
              </p>
            {:else}
              <p class="hint">Use your Matrix ID and password for this homeserver.</p>
            {/if}
          </div>
          {#if method === 'oidc'}
            {#if session.oidcWaiting}
              <p class="waiting" role="status">
                <span class="pulse" aria-hidden="true"></span>
                Waiting for the browser…
              </p>
            {/if}
            {#if localError}<p class="error" role="alert">{localError}</p>{/if}
            <div class="actions">
              <Button
                variant="ghost"
                type="button"
                onclick={() => (session.oidcWaiting ? cancelOidc() : go('server'))}
              >
                {#if !session.oidcWaiting}
                  <Icon icon={ChevronLeft} size={16} strokeWidth={1.5} />
                {/if}
                {session.oidcWaiting ? 'Cancel' : 'Back'}
              </Button>
              <Button type="button" onclick={startOidc} disabled={session.busy}>
                <Icon icon={ExternalLink} size={16} strokeWidth={1.5} />
                {session.busy ? 'Opening…' : session.oidcWaiting ? 'Open again' : 'Open browser'}
              </Button>
            </div>
            {#if !session.oidcWaiting}
              <button class="link" type="button" onclick={() => useMethod('password')}>
                Use a username and password instead
              </button>
            {/if}
          {:else}
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
            {#if supportsOidc}
              <button class="link" type="button" onclick={() => useMethod('oidc')}>
                Sign in through your browser instead
              </button>
            {/if}
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

  /* A second way in, not a competing action. */
  .link {
    align-self: flex-start;
    padding: 0;
    border: 0;
    background: none;
    color: var(--alloy-color-accent-primary);
    font: inherit;
    font-size: var(--alloy-typography-size-sm);
    cursor: pointer;
  }
  .link:hover {
    text-decoration: underline;
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
    .actions :global(button) {
      width: 100%;
    }
  }
</style>
