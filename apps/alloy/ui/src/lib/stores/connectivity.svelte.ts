import type { AccountId, ConnectionStatus } from '../generated/index.js';
import { isNetworkError } from './errors.js';

/** How often a lost connection is probed again. */
export const PROBE_INTERVAL_MS = 5000;

/** True when a connector is up and the user can expect live updates. */
export function isLiveStatus(status: ConnectionStatus | undefined): boolean {
  return status === 'Connected' || status === 'Syncing' || status === 'Connecting';
}

/** True when the account's connector is waiting to retry after a failure. */
export function isReconnecting(status: ConnectionStatus | undefined): boolean {
  return typeof status === 'object' && status !== null && 'Reconnecting' in status;
}

/**
 * Whether the app can reach its networks.
 *
 * Three signals feed it: the browser's own online flag (instant, but only says the device has a
 * link), the per-account `ConnectionStatus` stream, and network failures seen on engine calls.
 * Local engine calls keep succeeding while offline, so a success never clears a failure: only the
 * probe does, which is a call that goes out to the networks. When the connection comes back every
 * registered hook runs, so data that failed to load is fetched again.
 */
class Connectivity {
  browserOnline = $state(typeof navigator === 'undefined' ? true : navigator.onLine);
  unreachable = $state(false);
  /** Latest connector state per account, as reported by the engine. */
  statuses = $state<Record<AccountId, ConnectionStatus>>({});
  #wasOffline = false;
  #hooks = new Set<() => void>();
  #probe: (() => Promise<unknown>) | null = null;
  #probing = false;

  get offline(): boolean {
    return !this.browserOnline || this.unreachable;
  }

  /** Accounts currently reconnecting or disconnected, in the order they were reported. */
  get disconnectedAccounts(): AccountId[] {
    return Object.entries(this.statuses)
      .filter(([, status]) => !isLiveStatus(status))
      .map(([accountId]) => accountId);
  }

  statusOf(accountId: AccountId): ConnectionStatus | undefined {
    return this.statuses[accountId];
  }

  /** Records one account's connector state. */
  noteStatus(accountId: AccountId, status: ConnectionStatus) {
    this.statuses = { ...this.statuses, [accountId]: status };
  }

  /** Forgets an account's state (logout). */
  forgetAccount(accountId: AccountId) {
    if (!Object.hasOwn(this.statuses, accountId)) return;
    const next = { ...this.statuses };
    delete next[accountId];
    this.statuses = next;
  }

  /** An engine call failed: remember it when the cause was the network, not the engine's answer. */
  noteFailure(e: unknown) {
    if (!isNetworkError(e)) return;
    this.unreachable = true;
    this.#sync();
  }

  /** Runs `hook` each time the connection returns after being lost. Returns the unsubscribe function. */
  onReconnect(hook: () => void): () => void {
    this.#hooks.add(hook);
    return () => this.#hooks.delete(hook);
  }

  /** Probes the networks now instead of waiting for the next tick. */
  async retryNow() {
    if (!this.#probe || this.#probing) return;
    this.#probing = true;
    try {
      await this.#probe();
      this.unreachable = false;
    } catch (e) {
      // A refusal from the engine still means it answered; only a transport failure keeps us offline.
      this.unreachable = isNetworkError(e);
    } finally {
      this.#probing = false;
      this.#sync();
    }
  }

  /** Starts listening for the browser's online/offline events and probing while disconnected. */
  start(probe: () => Promise<unknown>): () => void {
    this.#probe = probe;
    const goOnline = () => {
      this.browserOnline = true;
      this.#sync();
      void this.retryNow();
    };
    const goOffline = () => {
      this.browserOnline = false;
      this.#sync();
    };
    window.addEventListener('online', goOnline);
    window.addEventListener('offline', goOffline);
    const timer = setInterval(() => {
      if (this.offline) void this.retryNow();
    }, PROBE_INTERVAL_MS);
    return () => {
      window.removeEventListener('online', goOnline);
      window.removeEventListener('offline', goOffline);
      clearInterval(timer);
      this.#probe = null;
    };
  }

  /** Forget everything (tests, sign-out). */
  reset() {
    this.unreachable = false;
    this.browserOnline = true;
    this.#wasOffline = false;
    this.statuses = {};
  }

  #sync() {
    const now = this.offline;
    const reconnected = this.#wasOffline && !now;
    this.#wasOffline = now;
    if (reconnected) for (const hook of this.#hooks) hook();
  }
}

export const connectivity = new Connectivity();
