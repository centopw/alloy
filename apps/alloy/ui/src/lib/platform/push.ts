/**
 * Typed wrapper around window.AlloyPush (Kotlin PushBridge).
 * Provides background push notification support on Android via UnifiedPush.
 * On non-Android platforms, all functions return/indicate unsupported.
 */

export interface AlloyPushBridge {
  state(): string;
  enable(): void;
  disable(): void;
  pick(): void;
  pendingRoom?(): string;
  takePendingRoom?(): string;
}

declare global {
  interface Window {
    AlloyPush?: AlloyPushBridge;
    __alloyPendingRoom?: string | null;
  }
}

export type PushPermission = 'granted' | 'denied' | 'unknown';

export interface PushState {
  /** True if the native push bridge is available and push is supported. */
  supported: boolean;
  /** User enabled background push in settings. */
  enabled: boolean;
  /** Distributor gave an endpoint and homeserver pusher was registered. */
  registered: boolean;
  /** Display name of the active distributor, or null if none. */
  distributor: string | null;
  /** List of available distributors on this device. */
  distributors: string[];
  /** The current endpoint URL, or null. */
  endpoint: string | null;
  /** The gateway URL used for push, or null. */
  gateway: string | null;
  /** Notification permission status: granted, denied, or unknown. */
  permission: PushPermission;
  /** Error message if registration failed, or null. */
  error: string | null;
}

export const UNSUPPORTED_PUSH_STATE: PushState = {
  supported: false,
  enabled: false,
  registered: false,
  distributor: null,
  distributors: [],
  endpoint: null,
  gateway: null,
  permission: 'unknown',
  error: null,
};

function parsePermission(val: unknown): PushPermission {
  if (val === 'granted' || val === 'denied' || val === 'unknown') {
    return val;
  }
  return 'unknown';
}

function parseDistributors(val: unknown): string[] {
  if (!Array.isArray(val)) return [];
  const result: string[] = [];
  for (const item of val) {
    if (typeof item === 'string' && item.length > 0) {
      result.push(item);
    }
  }
  return result;
}

/**
 * Defensively parses push state payload from JSON string or raw object.
 */
export function parsePushState(raw: unknown): PushState {
  if (raw === null || raw === undefined) {
    return UNSUPPORTED_PUSH_STATE;
  }

  let obj: unknown = raw;
  if (typeof raw === 'string') {
    try {
      obj = JSON.parse(raw);
    } catch {
      return {
        ...UNSUPPORTED_PUSH_STATE,
        error: 'Invalid push state payload',
      };
    }
  }

  if (typeof obj !== 'object' || obj === null) {
    return UNSUPPORTED_PUSH_STATE;
  }

  const record = obj as Record<string, unknown>;
  const supported = typeof record.supported === 'boolean' ? record.supported : true;
  const enabled = typeof record.enabled === 'boolean' ? record.enabled : false;
  const registered = typeof record.registered === 'boolean' ? record.registered : false;
  const distributor = typeof record.distributor === 'string' && record.distributor.length > 0 ? record.distributor : null;
  const distributors = parseDistributors(record.distributors);
  const endpoint = typeof record.endpoint === 'string' && record.endpoint.length > 0 ? record.endpoint : null;
  const gateway = typeof record.gateway === 'string' && record.gateway.length > 0 ? record.gateway : null;
  const permission = parsePermission(record.permission);
  const error = typeof record.error === 'string' && record.error.length > 0 ? record.error : null;

  return {
    supported,
    enabled,
    registered,
    distributor,
    distributors,
    endpoint,
    gateway,
    permission,
    error,
  };
}

/**
 * Returns true if the platform supports background push notifications.
 * True when window.AlloyPush is exposed by the host.
 */
export function pushSupported(): boolean {
  if (typeof window === 'undefined') return false;
  const bridge = window.AlloyPush;
  return typeof bridge?.state === 'function';
}

/**
 * Get the current push state. Returns UNSUPPORTED_PUSH_STATE if the bridge is absent.
 */
export function pushState(): PushState {
  if (typeof window === 'undefined') return UNSUPPORTED_PUSH_STATE;
  const bridge = window.AlloyPush;
  if (typeof bridge?.state !== 'function') return UNSUPPORTED_PUSH_STATE;

  try {
    const raw = bridge.state();
    return parsePushState(raw);
  } catch (e) {
    return {
      ...UNSUPPORTED_PUSH_STATE,
      supported: true,
      error: e instanceof Error ? e.message : 'Failed to query push state',
    };
  }
}

/**
 * Enable background push notifications.
 */
export function enablePush(): void {
  if (typeof window === 'undefined') return;
  const bridge = window.AlloyPush;
  if (typeof bridge?.enable === 'function') {
    bridge.enable();
  }
}

/**
 * Disable background push notifications.
 */
export function disablePush(): void {
  if (typeof window === 'undefined') return;
  const bridge = window.AlloyPush;
  if (typeof bridge?.disable === 'function') {
    bridge.disable();
  }
}

/**
 * Open the distributor picker UI on Android.
 */
export function pickPushApp(): void {
  if (typeof window === 'undefined') return;
  const bridge = window.AlloyPush;
  if (typeof bridge?.pick === 'function') {
    bridge.pick();
  }
}

/**
 * Register a callback to be called when the push state changes via alloy-push-state.
 * Returns an unsubscribe function.
 */
export function onPushState(callback: (state: PushState) => void): () => void {
  if (typeof window === 'undefined') return () => {};

  const listener = (event: Event) => {
    if (event instanceof CustomEvent && event.detail !== undefined) {
      callback(parsePushState(event.detail));
    }
  };

  window.addEventListener('alloy-push-state', listener);
  return () => {
    window.removeEventListener('alloy-push-state', listener);
  };
}

/**
 * Get and clear the pending room ID from a notification tap.
 * Called once at startup to open the room if the app was launched from a notification.
 * Returns the room ID or null if none is pending.
 */
export function takePendingRoom(): string | null {
  if (typeof window === 'undefined') return null;

  const bridge = window.AlloyPush;
  if (bridge) {
    if (typeof bridge.pendingRoom === 'function') {
      try {
        const id = bridge.pendingRoom();
        if (typeof id === 'string' && id.trim().length > 0) {
          return id.trim();
        }
      } catch {
        // Fall through
      }
    }
    if (typeof bridge.takePendingRoom === 'function') {
      try {
        const id = bridge.takePendingRoom();
        if (typeof id === 'string' && id.trim().length > 0) {
          return id.trim();
        }
      } catch {
        // Fall through
      }
    }
  }

  const fallback = window.__alloyPendingRoom;
  if (typeof fallback === 'string' && fallback.trim().length > 0) {
    window.__alloyPendingRoom = null;
    return fallback.trim();
  }

  return null;
}

/**
 * Extracts a valid Matrix room ID from a custom event detail or payload.
 * Pure function for testing and robust event handling.
 */
export function extractOpenRoomId(payload: unknown): string | null {
  if (typeof payload === 'string') {
    const trimmed = payload.trim();
    return trimmed.length > 0 ? trimmed : null;
  }

  if (typeof payload === 'object' && payload !== null) {
    const record = payload as Record<string, unknown>;
    const candidate = record.roomId ?? record.room_id ?? record.id;
    if (typeof candidate === 'string') {
      const trimmed = candidate.trim();
      return trimmed.length > 0 ? trimmed : null;
    }
  }

  return null;
}
