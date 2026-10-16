import { invoke, Channel } from '@tauri-apps/api/core';
import type { EngineCommand, EngineResponse, EngineEvent } from '../generated/index.js';
import type { Transport, EventHandler, Unsubscribe } from './transport.js';

/**
 * Tauri IPC Transport implementation.
 * Communicates with Tauri Rust backend using `alloy_invoke` and `alloy_subscribe`.
 */
export class TauriTransport implements Transport {
  private handlers = new Set<EventHandler>();
  private channel: Channel<EngineEvent> | null = null;
  private subscribed = false;

  async invoke(command: EngineCommand): Promise<EngineResponse> {
    return await invoke<EngineResponse>('alloy_invoke', { command });
  }

  subscribe(handler: EventHandler): Unsubscribe {
    this.handlers.add(handler);
    this.ensureSubscribed();

    return () => {
      this.handlers.delete(handler);
    };
  }

  private ensureSubscribed() {
    if (this.subscribed) return;
    this.subscribed = true;

    this.channel = new Channel<EngineEvent>();
    this.channel.onmessage = (event: EngineEvent) => {
      for (const handler of this.handlers) {
        try {
          handler(event);
        } catch (err) {
          console.error('[TauriTransport] Error in event handler:', err);
        }
      }
    };

    invoke('alloy_subscribe', { onEvent: this.channel }).catch((err) => {
      console.error('[TauriTransport] Failed to subscribe to backend events:', err);
      this.subscribed = false;
    });
  }
}
