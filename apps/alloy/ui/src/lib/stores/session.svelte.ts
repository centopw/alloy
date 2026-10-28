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

export type Phase = 'booting' | 'onboarding' | 'ready' | 'no-engine';

/** Account/session lifecycle: restore on start, login, logout. */
class SessionStore {
  phase = $state<Phase>('booting');
  account = $state<Account | null>(null);
  busy = $state(false);
  error = $state<string | null>(null);

  private started = false;

  /** Restore a previous session if one exists, else show onboarding. Idempotent. */
  async boot() {
    if (this.started) return;
    this.started = true;
    client.subscribe((e) => {
      if (e.type === 'AuthState') {
        this.account = e.payload.account;
        if (!e.payload.account && this.phase === 'ready') this.phase = 'onboarding';
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

  async logout() {
    const acc = this.account;
    if (!acc) return;
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
