import type { EngineCommand, EngineEvent } from '../generated/index.js';
import type { Transport, EventHandler, Unsubscribe } from './transport.js';
import { NoEngineError } from './transport.js';
import { TauriTransport } from './tauri.js';
import { WsTransport } from './ws.js';
import { AlloyClient } from './client.js';
import { connectivity } from '../stores/connectivity.svelte.js';

/** What a page can talk to. `mock` exists only in development builds; see `openTransport`. */
export type TransportChoice =
  | { kind: 'tauri' }
  | { kind: 'ws'; url: string }
  | { kind: 'mock' }
  | { kind: 'none' };

/** Where the page is running and what it was asked to connect to. Injected so tests can cover it. */
export interface PageTarget {
  tauri: boolean;
  /** The `?engine=` value, if any. */
  bridge: string | null;
  /** The `?token=` value that belongs to the bridge. */
  token: string | null;
  /** `import.meta.env.DEV` at the call site: demo data is a development aid, never a shipped path. */
  dev: boolean;
}

/**
 * Decides which transport a page should use.
 * Tauri IPC when the packaged webview is present, the bridge WebSocket when `?engine=` was given,
 * the in-memory demo engine only in a development build, otherwise nothing.
 */
export function chooseTransport(page: PageTarget): TransportChoice {
  if (page.tauri) return { kind: 'tauri' };
  if (page.bridge) {
    try {
      const url = new URL(page.bridge);
      if (page.token) url.searchParams.set('token', page.token);
      return { kind: 'ws', url: url.toString() };
    } catch {
      // A malformed ?engine= is not a reason to fall back to demo data.
    }
  }
  return page.dev ? { kind: 'mock' } : { kind: 'none' };
}

function pageTarget(): PageTarget {
  if (typeof window === 'undefined') return { tauri: false, bridge: null, token: null, dev: false };
  const params = new URLSearchParams(window.location.search);
  return {
    tauri: '__TAURI_INTERNALS__' in window,
    bridge: params.get('engine'),
    token: params.get('token'),
    dev: import.meta.env.DEV,
  };
}

/** Browser build with nothing to connect to: every call fails with one typed error. */
export class UnavailableTransport implements Transport {
  invoke(_command: EngineCommand): Promise<never> {
    return Promise.reject(new NoEngineError());
  }

  subscribe(_handler: EventHandler): Unsubscribe {
    return () => {};
  }
}

/** Builds the transport a choice names. */
export async function openTransport(choice: TransportChoice): Promise<Transport> {
  switch (choice.kind) {
    case 'tauri':
      return new TauriTransport();
    case 'ws':
      return new WsTransport(choice.url);
    case 'mock': {
      // The statically-false DEV check is what keeps dev-transport.ts (and the mock data it holds)
      // out of a production bundle.
      if (import.meta.env.DEV) {
        const { createMockTransport } = await import('./dev-transport.js');
        return createMockTransport();
      }
      return new UnavailableTransport();
    }
    case 'none':
      return new UnavailableTransport();
  }
}

/**
 * A transport whose concrete implementation resolves asynchronously (the dev mock is a dynamic
 * import). Calls and subscriptions made before it resolves are queued on it.
 */
class DeferredTransport implements Transport {
  #inner: Transport | null = null;
  #ready: Promise<Transport>;
  #handlers = new Set<EventHandler>();
  #unsubs = new Map<EventHandler, Unsubscribe>();

  constructor(choice: TransportChoice) {
    this.#ready = openTransport(choice).then((inner) => {
      this.#inner = inner;
      for (const handler of this.#handlers) {
        this.#unsubs.set(handler, inner.subscribe(handler));
      }
      this.#handlers.clear();
      return inner;
    });
  }

  async invoke(command: EngineCommand) {
    return (await this.#ready).invoke(command);
  }

  subscribe(handler: EventHandler): Unsubscribe {
    if (this.#inner) return this.#inner.subscribe(handler);
    this.#handlers.add(handler);
    return () => {
      this.#handlers.delete(handler);
      const unsubscribe = this.#unsubs.get(handler);
      this.#unsubs.delete(handler);
      unsubscribe?.();
    };
  }
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
const transport = observed(new DeferredTransport(chooseTransport(pageTarget())));
export const client = new AlloyClient(transport);

// Re-export types
export type { Transport, EventHandler, Unsubscribe } from './transport.js';
export { NoEngineError } from './transport.js';
export { TauriTransport } from './tauri.js';
export { WsTransport } from './ws.js';
export { AlloyClient, applyDiff } from './client.js';
