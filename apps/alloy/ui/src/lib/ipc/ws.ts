import type { EngineCommand, EngineEvent, EngineResponse } from '../generated/index.js';
import type { EventHandler, Transport, Unsubscribe } from './transport.js';

type Frame =
  | { id: number; ok: EngineResponse }
  | { id: number; err: { code: string; message: string } }
  | { event: EngineEvent };

/**
 * Transport over the loopback `alloy-bridge` WebSocket. Same JSON as the Tauri IPC commands;
 * used by browser-based E2E tests and as the starting point for the web target.
 */
export class WsTransport implements Transport {
  private socket: WebSocket;
  private ready: Promise<void>;
  private nextId = 1;
  private pending = new Map<number, { resolve: (r: EngineResponse) => void; reject: (e: unknown) => void }>();
  private handlers = new Set<EventHandler>();

  constructor(url: string) {
    this.socket = new WebSocket(url);
    this.ready = new Promise((resolve, reject) => {
      this.socket.onopen = () => resolve();
      this.socket.onerror = () => reject(new Error(`cannot reach engine bridge at ${url}`));
    });
    this.socket.onmessage = (m) => this.onFrame(JSON.parse(String(m.data)) as Frame);
    this.socket.onclose = () => {
      for (const p of this.pending.values()) p.reject(new Error('engine bridge closed'));
      this.pending.clear();
    };
  }

  private onFrame(f: Frame) {
    if ('event' in f) {
      for (const h of this.handlers) {
        try {
          h(f.event);
        } catch (e) {
          console.error('[WsTransport] handler error', e);
        }
      }
      return;
    }
    const p = this.pending.get(f.id);
    if (!p) return;
    this.pending.delete(f.id);
    if ('ok' in f) p.resolve(f.ok);
    else p.reject(f.err);
  }

  async invoke(command: EngineCommand): Promise<EngineResponse> {
    await this.ready;
    const id = this.nextId++;
    return new Promise<EngineResponse>((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.socket.send(JSON.stringify({ id, command }));
    });
  }

  subscribe(handler: EventHandler): Unsubscribe {
    this.handlers.add(handler);
    return () => this.handlers.delete(handler);
  }
}
