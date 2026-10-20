import {
  pushState,
  pushSupported,
  enablePush,
  disablePush,
  pickPushApp,
  onPushState,
  type PushState,
  UNSUPPORTED_PUSH_STATE,
} from '../platform/push';

/**
 * Reactive push notification store.
 * Fed by the native Android PushBridge via alloy-push-state events.
 */
export class PushStore {
  state = $state<PushState>(UNSUPPORTED_PUSH_STATE);
  busy = $state(false);

  private unsubscribe: (() => void) | null = null;
  private stateWaiters: Array<(state: PushState) => void> = [];

  constructor(initialState?: PushState) {
    if (initialState) {
      this.state = initialState;
    } else if (pushSupported()) {
      this.state = pushState();
    }
  }

  get supported(): boolean {
    return this.state.supported;
  }

  get enabled(): boolean {
    return this.state.enabled;
  }

  get registered(): boolean {
    return this.state.registered;
  }

  get distributor(): string | null {
    return this.state.distributor;
  }

  get distributors(): string[] {
    return this.state.distributors;
  }

  get endpoint(): string | null {
    return this.state.endpoint;
  }

  get gateway(): string | null {
    return this.state.gateway;
  }

  get permission(): 'granted' | 'denied' | 'unknown' {
    return this.state.permission;
  }

  get error(): string | null {
    return this.state.error;
  }

  applyState(next: PushState) {
    this.state = next;
    this.busy = false;
    const waiters = [...this.stateWaiters];
    for (const waiter of waiters) {
      waiter(next);
    }
  }

  start() {
    this.refresh();
    if (this.unsubscribe) return;
    this.unsubscribe = onPushState((newState) => {
      this.applyState(newState);
    });
  }

  stop() {
    this.unsubscribe?.();
    this.unsubscribe = null;
    this.stateWaiters = [];
  }

  refresh() {
    if (pushSupported()) {
      this.applyState(pushState());
    }
  }

  toggle(enabled: boolean) {
    this.busy = true;
    if (enabled) {
      enablePush();
    } else {
      disablePush();
    }
  }

  pick() {
    pickPushApp();
  }

  /**
   * Called during account sign out.
   * Disables push and waits up to timeoutMs for the bridge to acknowledge,
   * never throwing or blocking sign out on failure.
   */
  async disableOnSignOut(timeoutMs = 3000): Promise<void> {
    if (!this.supported || !this.enabled) return;

    return new Promise<void>((resolve) => {
      let settled = false;

      const finish = () => {
        if (!settled) {
          settled = true;
          this.stateWaiters = this.stateWaiters.filter((w) => w !== waiter);
          resolve();
        }
      };

      const timer = setTimeout(finish, timeoutMs);

      const waiter = (s: PushState) => {
        if (!s.enabled) {
          clearTimeout(timer);
          finish();
        }
      };

      this.stateWaiters.push(waiter);

      try {
        disablePush();
      } catch {
        clearTimeout(timer);
        finish();
      }
    });
  }
}

export const pushStore = new PushStore();
