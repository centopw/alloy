import type { Transport } from './transport.js';
import { TauriTransport } from './tauri.js';
import { MockTransport } from './mock.js';
import { WsTransport } from './ws.js';
import { AlloyClient } from './client.js';
import { connectivity } from '../stores/connectivity.svelte.js';

/**
 * Auto-detect and create the appropriate transport.
 * Tauri when `window.__TAURI_INTERNALS__` is present, the bridge WebSocket when `?engine=ws://...`
 * is given, else the in-memory mock.
 */
function createTransport(): Transport {
  if (typeof window !== 'undefined') {
    if ('__TAURI_INTERNALS__' in window) return new TauriTransport();
    // `?engine=ws://127.0.0.1:8765&token=...` points the web UI at a running `alloy-bridge`;
    // the token must match that bridge's `--token` / `ALLOY_BRIDGE_TOKEN`.
    const params = new URLSearchParams(window.location.search);
    const bridge = params.get('engine');
    if (bridge) {
      const url = new URL(bridge);
      const token = params.get('token');
      if (token) url.searchParams.set('token', token);
      return new WsTransport(url.toString());
    }
  }
  return new MockTransport();
}

/** Tells the connectivity store about every engine call that fails because the server cannot be reached. */
function observed(inner: Transport): Transport {
  return {
    invoke: async (command) => {
      try {
        return await inner.invoke(command);
      } catch (e) {
        connectivity.noteFailure(e);
        throw e;
      }
    },
    subscribe: (handler) => inner.subscribe(handler),
  };
}

/**
 * Global client instance.
 */
const transport = observed(createTransport());
export const client = new AlloyClient(transport);

// Re-export types
export type { Transport, EventHandler, Unsubscribe } from './transport.js';
export { TauriTransport } from './tauri.js';
export { MockTransport } from './mock.js';
export { WsTransport } from './ws.js';
export { AlloyClient, applyDiff } from './client.js';
