import { client } from '../ipc/index.js';
import type { Account } from '../generated/index.js';
import { errorCode, errorMessage, expect } from './errors.js';
import { membersStore } from './members.svelte.js';
import { pinsStore } from './pins.svelte.js';
import { draftsStore } from './drafts.svelte.js';
import { roomInfo } from './roomInfo.svelte.js';
import { prefs } from './prefs.svelte.js';
import { liveLocation } from './liveLocation.svelte.js';
import { pushStore } from './push.svelte.js';
import { openExternal } from '../platform/links.js';

export type Phase = 'booting' | 'onboarding' | 'ready' | 'no-engine';

/** The engine gives a started OIDC login ten minutes; the waiting screen stops waiting with it. */
const OIDC_WAIT_TIMEOUT_MS = 10 * 60 * 1000;

/** Account/session lifecycle: restore on start, login, logout. */
class SessionStore {
  phase = $state<Phase>('booting');
  account = $state<Account | null>(null);
  busy = $state(false);
  error = $state<string | null>(null);
  /** True between opening the browser for an OIDC login and its completion, cancellation or expiry. */
  oidcWaiting = $state(false);

  private started = false;
  private oidcTimer: ReturnType<typeof setTimeout> | null = null;

  /** Restore a previous session if one exists, else show onboarding. Idempotent. */
  async boot() {
    if (this.started) return;
    this.started = true;
    client.subscribe((e) => {
      if (e.type !== 'AuthState') return;
      this.account = e.payload.account;
      if (e.payload.account) {
        // An OIDC login finishes in the engine's loopback listener and only reaches the UI as this
        // event, so it must bring the app to the signed-in shell.
        this.stopOidcWait();
        if (this.phase !== 'ready') {
          prefs.beginAccount(e.payload.account.id);
          this.phase = 'ready';
        }
      } else if (this.phase === 'ready') {
        this.phase = 'onboarding';
      }
    });
    try {
      const accounts = expect(await client.listAccounts(), 'AccountsList').payload.accounts;
      for (const a of accounts) {
        try {
          const res = expect(await client.restoreSession(a.id), 'SessionRestored');
          this.account = res.payload.account;
          // Bind the local preference copy to this account before the shell reads it.
          prefs.beginAccount(res.payload.account.id);
          this.phase = 'ready';
          return;
        } catch {
          // Stored session is gone or invalid; try the next account, else fall through.
        }
      }
    } catch (e) {
      // A browser build with no engine cannot log in to anything: say so instead of showing a form
      // that can only fail, and never substitute demo data.
      if (errorCode(e) === 'no_engine') {
        this.phase = 'no-engine';
        return;
      }
      this.error = errorMessage(e);
    }
    this.phase = 'onboarding';
  }

  async discover(input: string) {
    const res = expect(await client.discoverHomeserver(input), 'HomeserverInfo');
    return res.payload;
  }

  async login(homeserverUrl: string, username: string, password: string) {
    this.busy = true;
    this.error = null;
    try {
      const res = expect(
        await client.loginPassword(homeserverUrl, username, password),
        'LoggedIn'
      );
      this.account = res.payload.account;
      // Bind the local preference copy to this account before the shell reads it.
      prefs.beginAccount(res.payload.account.id);
      this.phase = 'ready';
    } catch (e) {
      this.error = errorMessage(e);
      throw e;
    } finally {
      this.busy = false;
    }
  }

  /**
   * Starts an OIDC (MAS) login and opens the server's authorization page in the system browser.
   * The engine receives the redirect on its loopback listener and emits `AuthState` when done.
   */
  async startOidc(homeserverUrl: string) {
    this.busy = true;
    this.error = null;
    let started = false;
    try {
      const res = expect(await client.startOidcLogin(homeserverUrl), 'OidcUrl');
      started = true;
      await openExternal(res.payload.authorization_url);
      this.oidcWaiting = true;
      this.startOidcTimer();
    } catch (e) {
      // Without a browser there is nothing that can finish this attempt; drop it in the engine.
      if (started) await client.cancelOidcLogin().catch(() => {});
      this.error = errorMessage(e);
      throw e;
    } finally {
      this.busy = false;
    }
  }

  /** Abandons the pending OIDC login, both on the waiting screen and in the engine. */
  async cancelOidc() {
    this.stopOidcWait();
    await client.cancelOidcLogin().catch(() => {});
  }

  private startOidcTimer() {
    this.clearOidcTimer();
    this.oidcTimer = setTimeout(() => {
      this.oidcTimer = null;
      this.oidcWaiting = false;
      void client.cancelOidcLogin().catch(() => {});
    }, OIDC_WAIT_TIMEOUT_MS);
  }

  private clearOidcTimer() {
    if (this.oidcTimer !== null) {
      clearTimeout(this.oidcTimer);
      this.oidcTimer = null;
    }
  }

  private stopOidcWait() {
    this.clearOidcTimer();
    this.oidcWaiting = false;
  }

  async logout() {
    const acc = this.account;
    if (!acc) return;
    this.stopOidcWait();
    // The share ends first: after logout the engine could no longer tell the room it stopped.
    await liveLocation.stop();
    // Disable background push before logging out so homeserver pusher is cleanly removed.
    await pushStore.disableOnSignOut().catch(() => {});
    // Drop this account's synced preferences and stamp before the engine session goes away.
    await prefs.signOut().catch(() => {});
    await client.logout(acc.id);
    this.account = null;
    membersStore.reset();
    pinsStore.reset();
    draftsStore.reset();
    roomInfo.reset();
    this.phase = 'onboarding';
  }
}

export const session = new SessionStore();
