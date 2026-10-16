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
