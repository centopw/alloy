<script lang="ts">
  import { Button } from '../common';
  import { session } from '$lib/stores/session.svelte';
  import { errorMessage } from '$lib/stores/errors';

  type Step = 'server' | 'login';

  let step = $state<Step>('server');
  let serverInput = $state('matrix.org');
  let homeserverUrl = $state('');
  let homeserverName = $state('');
  let username = $state('');
  let password = $state('');
  let localError = $state<string | null>(null);
  let checking = $state(false);

  async function continueServer(e: Event) {
    e.preventDefault();
    localError = null;
    checking = true;
    try {
      const info = await session.discover(serverInput.trim());
      homeserverUrl = info.url;
      homeserverName = info.name;
      step = 'login';
    } catch (err) {
      localError = `Couldn't reach that server. ${errorMessage(err)}`;
    } finally {
      checking = false;
    }
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
  <div class="card">
    <h1 class="brand">Alloy</h1>
    {#if step === 'server'}
      <p class="lead">Choose the server your account lives on.</p>
      <form onsubmit={continueServer}>
        <label>
          <span>Server</span>
          <input
            type="text"
            bind:value={serverInput}
            placeholder="matrix.org or https://chat.example.com"
            autocomplete="off"
            autocapitalize="off"
            spellcheck="false"
            required
          />
        </label>
        {#if localError}<p class="error" role="alert">{localError}</p>{/if}
        <Button type="submit" size="lg" disabled={checking || !serverInput.trim()}>
          {checking ? 'Checking...' : 'Continue'}
        </Button>
      </form>
    {:else}
      <p class="lead">Sign in to <strong>{homeserverName}</strong></p>
      <form onsubmit={signIn}>
        <label>
          <span>Username</span>
          <input
            type="text"
            bind:value={username}
            autocomplete="username"
            autocapitalize="off"
            spellcheck="false"
            required
          />
        </label>
        <label>
          <span>Password</span>
          <input type="password" bind:value={password} autocomplete="current-password" required />
        </label>
        {#if localError}<p class="error" role="alert">{localError}</p>{/if}
        <Button type="submit" size="lg" disabled={session.busy || !username || !password}>
          {session.busy ? 'Signing in...' : 'Sign in'}
        </Button>
        <Button variant="ghost" onclick={() => (step = 'server')}>Use a different server</Button>
      </form>
    {/if}
  </div>
</main>

<style>
  .onboarding {
    min-height: 100vh;
    display: grid;
    place-items: center;
    background: var(--alloy-color-surface-canvas);
    padding: var(--alloy-spacing-scale-4, 16px);
  }
  .card {
    width: min(420px, 100%);
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-4, 16px);
  }
  .brand {
    margin: 0;
    font-size: 2rem;
    letter-spacing: -0.02em;
  }
  .lead {
    margin: 0;
    color: var(--alloy-color-text-secondary);
  }
  form {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-3, 12px);
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: 0.85rem;
    color: var(--alloy-color-text-secondary);
  }
  input {
    font: inherit;
    color: var(--alloy-color-text-primary);
    background: var(--alloy-color-surface-elevated, var(--alloy-color-surface-canvas));
    border: 1px solid var(--alloy-color-border-subtle, rgba(128, 128, 128, 0.35));
    border-radius: var(--alloy-radii-md, 10px);
    padding: 10px 12px;
  }
  input:focus-visible {
    outline: 2px solid var(--alloy-color-accent-primary);
    outline-offset: 1px;
  }
  .error {
    margin: 0;
    color: var(--alloy-color-status-error, #e5484d);
    font-size: 0.9rem;
  }
</style>
