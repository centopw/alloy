import { isNetworkError } from './errors.js';

/** How often a lost connection is probed again. */
export const PROBE_INTERVAL_MS = 5000;

/**
 * Whether the app can reach the homeserver.
 *
 * Two signals feed it: the browser's own online flag (instant, but only says the device has a link) and
 * network failures seen on engine calls (the server is unreachable even though the device is "online").
 * Local engine calls keep succeeding while offline, so a success never clears a failure: only the probe
 * does, which is a call that needs the server. When the connection comes back every registered hook runs,
 * so data that failed to load is fetched again.
 */
class Connectivity {
  browserOnline = $state(typeof navigator === 'undefined' ? true : navigator.onLine);
  unreachable = $state(false);
  #wasOffline = false;
  #hooks = new Set<() => void>();
  #probe: (() => Promise<unknown>) | null = null;
  #probing = false;

  get offline(): boolean {
    return !this.browserOnline || this.unreachable;
  }

  /** An engine call failed: remember it when the cause was the network, not the server's answer. */
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

  /** Probes the server now instead of waiting for the next tick. */
  async retryNow() {
    if (!this.#probe || this.#probing) return;
    this.#probing = true;
    try {
      await this.#probe();
      this.unreachable = false;
    } catch (e) {
      if (isNetworkError(e)) this.unreachable = true;
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
  }

  #sync() {
    const now = this.offline;
    const reconnected = this.#wasOffline && !now;
    this.#wasOffline = now;
    if (reconnected) for (const hook of this.#hooks) hook();
  }
}

export const connectivity = new Connectivity();
