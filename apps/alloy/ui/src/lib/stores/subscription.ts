import type { EngineEvent, SubscriptionId } from '../generated/index.js';

/**
 * Engine events can arrive before the command response that carries their subscription id.
 * `Subscription` buffers events until the id is known, then replays the matching ones.
 */
export class Subscription<E extends EngineEvent> {
  private id: SubscriptionId | null = null;
  private buffer: E[] = [];
  private closed = false;

  constructor(
    private readonly idOf: (e: E) => SubscriptionId,
    private readonly apply: (e: E) => void
  ) {}

  /** Feed every matching-type event here. */
  push(e: E) {
    if (this.closed) return;
    if (this.id === null) {
      this.buffer.push(e);
    } else if (this.idOf(e) === this.id) {
      this.apply(e);
    }
  }

  /** Called once the subscribe response delivered the id. */
  attach(id: SubscriptionId) {
    if (this.closed) return;
    this.id = id;
    const pending = this.buffer;
    this.buffer = [];
    for (const e of pending) if (this.idOf(e) === id) this.apply(e);
  }

  get subscriptionId() {
    return this.id;
  }

  close() {
    this.closed = true;
    this.buffer = [];
  }
}

/**
 * Coalesce bursts of events into one UI update per frame. The engine emits one event per diff,
 * and a reset arrives as `Clear` + `Append`; applying them in separate frames would flash an
 * empty list and unmount whatever depends on it.
 */
export class FrameBatcher<T> {
  private queue: T[] = [];
  private scheduled = false;

  constructor(private readonly flush: (events: T[]) => void) {}

  push(e: T) {
    this.queue.push(e);
    if (this.scheduled) return;
    this.scheduled = true;
    const run = () => {
      this.scheduled = false;
      const batch = this.queue;
      this.queue = [];
      this.flush(batch);
    };
    if (typeof requestAnimationFrame === 'function') requestAnimationFrame(run);
    else setTimeout(run, 0);
  }

  clear() {
    this.queue = [];
  }
}
