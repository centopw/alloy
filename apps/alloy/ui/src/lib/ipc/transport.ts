import type { EngineCommand, EngineResponse, EngineEvent } from '../generated/index.js';

/**
 * Unsubscribe function returned by subscribe().
 */
export type Unsubscribe = () => void;

/**
 * Event handler callback.
 */
export type EventHandler = (event: EngineEvent) => void;

/**
 * A page with no engine to talk to: a browser build without Tauri IPC and without a `?engine=`
 * bridge. It is a deployment fault, never a data source — the UI must say so instead of inventing
 * demo content.
 */
export class NoEngineError extends Error {
  readonly code = 'no_engine';

  constructor(
    message = 'Alloy could not find a local engine. Open the packaged app, or point a development build at a bridge with ?engine=ws://…'
  ) {
    super(message);
    this.name = 'NoEngineError';
  }
}

/**
 * Core IPC transport interface.
 * Implementations may use Tauri IPC, WebWorker, or mock data.
 */
export interface Transport {
  /**
   * Invoke a command and wait for response.
   */
  invoke(command: EngineCommand): Promise<EngineResponse>;

  /**
   * Subscribe to events. Returns an unsubscribe function.
   */
  subscribe(handler: EventHandler): Unsubscribe;
}
