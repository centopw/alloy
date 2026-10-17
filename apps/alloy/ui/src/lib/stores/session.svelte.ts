import { client } from '../ipc/index.js';
import type { Account } from '../generated/index.js';
import { errorMessage, expect } from './errors.js';
import { membersStore } from './members.svelte.js';

export type Phase = 'booting' | 'onboarding' | 'ready';

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
          this.phase = 'ready';
          return;
        } catch {
          // Stored session is gone or invalid; try the next account, else fall through.
        }
      }
    } catch (e) {
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
    await client.logout(acc.id);
    this.account = null;
    membersStore.reset();
    this.phase = 'onboarding';
  }
}

export const session = new SessionStore();
