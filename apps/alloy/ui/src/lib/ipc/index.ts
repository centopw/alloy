import type { Transport } from './transport.js';
import { TauriTransport } from './tauri.js';
import { MockTransport } from './mock.js';
import { AlloyClient } from './client.js';

/**
 * Auto-detect and create the appropriate transport.
 * Tauri when `window.__TAURI_INTERNALS__` is present, else mock.
 */
function createTransport(): Transport {
  if (typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window) {
    return new TauriTransport();
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
export { AlloyClient, applyDiff } from './client.js';
