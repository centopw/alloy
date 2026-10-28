import { client } from '../ipc/index.js';
import type { Account } from '../generated/index.js';
import { errorCode, errorMessage, errorReason, expect } from './errors.js';
import { membersStore } from './members.svelte.js';
import { pinsStore } from './pins.svelte.js';
import { draftsStore } from './drafts.svelte.js';
import { roomInfo } from './roomInfo.svelte.js';
import { prefs } from './prefs.svelte.js';
import { liveLocation } from './liveLocation.svelte.js';
import { pushStore } from './push.svelte.js';
import { openExternal } from '../platform/links.js';

export type Phase = 'booting' | 'onboarding' | 'ready' | 'no-engine';

/** Why a browser sign-in stopped, keyed by the engine's stable reason. */
export interface OidcFailure {
  reason: string;
  message: string;
}

/** The engine gives a started OIDC login ten minutes; the waiting screen stops waiting with it. */
const OIDC_WAIT_TIMEOUT_MS = 10 * 60 * 1000;

/** The host of a homeserver URL, as the sign-in copy names it. */
function hostOf(url: string): string {
  try {
    return new URL(url).host;
  } catch {
    return url;
  }
}

/** Account/session lifecycle: restore on start, login, logout. */
class SessionStore {
  phase = $state<Phase>('booting');
  account = $state<Account | null>(null);
  busy = $state(false);
  error = $state<string | null>(null);
  /** True between opening the browser for an OIDC login and its completion, cancellation or expiry. */
  oidcWaiting = $state(false);
  /** Set when a browser sign-in stopped without an account, for the onboarding surface to explain. */
  oidcFailure = $state<OidcFailure | null>(null);
  /** A stored session that could not be restored, so onboarding can say why it is showing. */
  restoreNotice = $state<string | null>(null);
  /** The last server that had a session, kept in memory to prefill the server step. */
  lastHomeserver = $state<string | null>(null);

  private started = false;
  private oidcTimer: ReturnType<typeof setTimeout> | null = null;

  /** Restore a previous session if one exists, else show onboarding. Idempotent. */
  async boot() {
    if (this.started) return;
    this.started = true;
    client.subscribe((e) => {
      if (e.type === 'AuthFailed') {
        // Only a browser sign-in the app started can be waiting; anything else has no surface.
        if (this.oidcWaiting) {
          this.stopOidcWait();
          this.oidcFailure = { reason: e.payload.reason, message: e.payload.message };
        }
        return;
      }
      if (e.type !== 'AuthState') return;
      this.account = e.payload.account;
      if (e.payload.account) {
        // An OIDC login finishes in the engine's loopback listener and only reaches the UI as this
        // event, so it must bring the app to the signed-in shell.
        this.stopOidcWait();
        this.oidcFailure = null;
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
          this.lastHomeserver = a.homeserver_url;
          this.restoreNotice = `Your session ended. Sign in again to ${hostOf(a.homeserver_url)}.`;
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
   * The engine receives the redirect on its loopback listener; a completion arrives as
   * `AuthState`, a failure as `AuthFailed`, and both end the waiting state here.
   */
  async startOidc(homeserverUrl: string) {
    this.busy = true;
    this.error = null;
    this.oidcFailure = null;
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
      this.oidcFailure = {
        reason: started ? 'launch_failed' : (errorReason(e) ?? 'exchange_failed'),
        message: errorMessage(e),
      };
      this.error = errorMessage(e);
      throw e;
    } finally {
      this.busy = false;
    }
  }

  /** Abandons the pending OIDC login, both on the waiting screen and in the engine. */
  async cancelOidc() {
    this.stopOidcWait();
    this.oidcFailure = null;
    await client.cancelOidcLogin().catch(() => {});
  }

  /** Drops the last failure, e.g. when the user moves to a different server. */
  clearOidcFailure() {
    this.oidcFailure = null;
  }

  private startOidcTimer() {
    this.clearOidcTimer();
    this.oidcTimer = setTimeout(() => {
      this.oidcTimer = null;
      this.oidcWaiting = false;
      this.oidcFailure = { reason: 'expired', message: 'the sign-in window timed out' };
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
