import { isAndroid } from '../platform/capabilities';
import {
  PERMISSION_CHANGED_EVENT,
  openPermissionSettings,
  readPermission,
  requestPermission,
  supportsPermissionSettings,
  type PermissionId,
  type PermissionState,
} from '../platform/permissions';
import { specsFor, summarize, type PermissionSpec, type PermissionSummary } from '../content/permissionsModel';
import { pushStore } from './push.svelte';

export { supportsPermissionSettings };

export interface PermissionRow extends PermissionSpec {
  state: PermissionState;
  /** Whether "Open settings" does something on this platform. */
  canOpenSettings: boolean;
}

/** Live state of everything the app needs the system to allow. Read without asking; refreshed when it may change. */
class PermissionsStore {
  #states = $state<Partial<Record<PermissionId, PermissionState>>>({});
  #loaded = $state(false);
  #watchers = 0;
  #stop: (() => void) | null = null;

  /** True after the first read finished, so a screen does not flash "not asked" while it is still reading. */
  get loaded(): boolean {
    return this.#loaded;
  }

  get rows(): PermissionRow[] {
    return specsFor(isAndroid()).map((spec) => ({
      ...spec,
      state: this.#stateOf(spec.id),
      canOpenSettings: supportsPermissionSettings(spec.id),
    }));
  }

  get summary(): PermissionSummary {
    return summarize(this.rows);
  }

  #stateOf(id: PermissionId): PermissionState {
    if (id === 'push-app') {
      if (!pushStore.supported) return 'unavailable';
      // A distributor must be installed, and the user must have turned the feature on and been registered.
      if (pushStore.distributors.length === 0) return 'prompt';
      return pushStore.registered ? 'granted' : 'prompt';
    }
    return this.#states[id] ?? 'unknown';
  }

  /** Reads every platform permission again. Never prompts. */
  async refresh(): Promise<void> {
    const ids = specsFor(isAndroid())
      .map((spec) => spec.id)
      .filter((id) => id !== 'push-app');
    const next: Partial<Record<PermissionId, PermissionState>> = {};
    await Promise.all(
      ids.map(async (id) => {
        try {
          next[id] = await readPermission(id);
        } catch {
          next[id] = 'unknown';
        }
      })
    );
    this.#states = next;
    this.#loaded = true;
  }

  /** Shows the system prompt for one permission, then reads the states again. */
  async request(id: PermissionId): Promise<void> {
    if (id === 'push-app') {
      pushStore.pick();
      return;
    }
    try {
      await requestPermission(id);
    } finally {
      await this.refresh();
    }
  }

  async openSettings(id: PermissionId): Promise<boolean> {
    return openPermissionSettings(id);
  }

  /**
   * Keeps the states current while a screen is showing: window focus (the user came back from system settings),
   * tab visibility, and the Android shell's answer event. Returns the stop function; watchers nest.
   */
  watch(): () => void {
    this.#watchers += 1;
    if (this.#watchers === 1) {
      const update = () => void this.refresh();
      const onVisible = () => {
        if (document.visibilityState === 'visible') update();
      };
      window.addEventListener('focus', update);
      window.addEventListener(PERMISSION_CHANGED_EVENT, update);
      document.addEventListener('visibilitychange', onVisible);
      this.#stop = () => {
        window.removeEventListener('focus', update);
        window.removeEventListener(PERMISSION_CHANGED_EVENT, update);
        document.removeEventListener('visibilitychange', onVisible);
      };
      update();
    }
    let stopped = false;
    return () => {
      if (stopped) return;
      stopped = true;
      this.#watchers -= 1;
      if (this.#watchers === 0) {
        this.#stop?.();
        this.#stop = null;
      }
    };
  }
}

export const permissionsStore = new PermissionsStore();
