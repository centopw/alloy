import type { Transport } from './transport.js';
import { TauriTransport } from './tauri.js';
import { MockTransport } from './mock.js';
import { WsTransport } from './ws.js';
import { AlloyClient } from './client.js';

/**
 * Auto-detect and create the appropriate transport.
 * Tauri when `window.__TAURI_INTERNALS__` is present, the bridge WebSocket when `?engine=ws://...`
 * is given, else the in-memory mock.
 */
function createTransport(): Transport {
  if (typeof window !== 'undefined') {
    if ('__TAURI_INTERNALS__' in window) return new TauriTransport();
    // `?engine=ws://127.0.0.1:8765` points the web UI at a running `alloy-bridge`.
    const bridge = new URLSearchParams(window.location.search).get('engine');
    if (bridge) return new WsTransport(bridge);
  }
  return new MockTransport();
}

/**
 * Global client instance.
 */
const transport = createTransport();
export const client = new AlloyClient(transport);

// Re-export types
export type { Transport, EventHandler, Unsubscribe } from './transport.js';
export { TauriTransport } from './tauri.js';
export { MockTransport } from './mock.js';
export { WsTransport } from './ws.js';
export { AlloyClient, applyDiff } from './client.js';
