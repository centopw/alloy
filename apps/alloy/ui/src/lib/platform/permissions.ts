import { invoke } from '@tauri-apps/api/core';
import { isAndroid } from './capabilities';
import { isTauriShell } from './notify';

/**
 * What the app can use that the system has to allow.
 * `push-app` and `battery` exist on Android only: a UnifiedPush app to deliver messages while Alloy is closed, and the
 * exemption from battery optimisation that keeps those deliveries on time.
 */
export type PermissionId = 'notifications' | 'microphone' | 'location' | 'push-app' | 'battery';

/**
 * `prompt`: not decided yet. `denied`: refused, only the system settings can change it.
 * `unavailable`: this device or build cannot grant it. `unknown`: the platform does not say until it is requested.
 */
export type PermissionState = 'granted' | 'denied' | 'prompt' | 'unavailable' | 'unknown';

/** The window event the Android shell sends after an answer, or after the user comes back from a settings page. */
export const PERMISSION_CHANGED_EVENT = 'alloy-permission-changed';

/** The Kotlin PermissionsBridge, mounted as window.AlloyPermissions on Android. */
export interface AlloyPermissionsBridge {
  status(name: string): string;
  request(name: string): void;
  openSettings(name: string): void;
}

declare global {
  interface Window {
    AlloyPermissions?: AlloyPermissionsBridge;
  }
}

function bridge(): AlloyPermissionsBridge | null {
  return typeof window !== 'undefined' && window.AlloyPermissions ? window.AlloyPermissions : null;
}

/** Whether this runs inside the Android shell with the native permission bridge. */
export function hasNativePermissions(): boolean {
  return isAndroid() && bridge() !== null;
}

function parseState(value: string): PermissionState {
  return value === 'granted' || value === 'denied' || value === 'prompt' || value === 'unavailable'
    ? value
    : 'unknown';
}

async function webState(name: 'microphone' | 'geolocation'): Promise<PermissionState> {
  if (typeof navigator === 'undefined' || !navigator.permissions) return 'unknown';
  try {
    // `microphone` is not in every TypeScript lib's PermissionName; browsers that lack it throw and land below.
    const status = await navigator.permissions.query({ name: name as PermissionName });
    return status.state === 'denied' ? 'denied' : status.state;
  } catch {
    return 'unknown';
  }
}

/** Reads one permission without asking the user anything. */
export async function readPermission(id: PermissionId): Promise<PermissionState> {
  const native = bridge();
  switch (id) {
    case 'notifications':
      if (native && isAndroid()) return parseState(native.status('notifications'));
      if (isTauriShell()) return parseState(await invoke<string>('notify_permission_state'));
      if (typeof Notification === 'undefined') return 'unavailable';
      return Notification.permission === 'default' ? 'prompt' : Notification.permission;
    case 'microphone':
      if (native && isAndroid()) return parseState(native.status('microphone'));
      if (typeof navigator === 'undefined' || !navigator.mediaDevices?.getUserMedia) return 'unavailable';
      return webState('microphone');
    case 'location':
      if (native && isAndroid()) return parseState(native.status('location'));
      if (typeof navigator === 'undefined' || !navigator.geolocation) return 'unavailable';
      return webState('geolocation');
    case 'battery':
      return native && isAndroid() ? parseState(native.status('battery')) : 'unavailable';
    case 'push-app':
      // Read from the push store by the caller: it holds the distributor list the native bridge reports.
      return 'unknown';
  }
}

/** Shows the system's own prompt for one permission. Resolves once the user answered; callers read the state again. */
export async function requestPermission(id: PermissionId): Promise<void> {
  const native = bridge();
  if (native && isAndroid() && id !== 'push-app') {
    // The answer arrives as PERMISSION_CHANGED_EVENT; the caller re-reads on it.
    native.request(id);
    return;
  }
  switch (id) {
    case 'notifications':
      if (isTauriShell()) await invoke<boolean>('notify_permission');
      else if (typeof Notification !== 'undefined') await Notification.requestPermission();
      return;
    case 'microphone': {
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      for (const track of stream.getTracks()) track.stop();
      return;
    }
    case 'location':
      await new Promise<void>((resolve) => {
        navigator.geolocation.getCurrentPosition(
          () => resolve(),
          () => resolve(),
          { timeout: 15000 }
        );
      });
      return;
    case 'push-app':
    case 'battery':
      return;
  }
}

/**
 * Whether opening the system settings page is supported on this platform.
 * Desktop Tauri supports notifications, microphone, and location.
 * Android supports settings when the native bridge is mounted.
 * Plain browsers cannot open system settings pages.
 */
export function supportsPermissionSettings(id?: PermissionId): boolean {
  if (isAndroid()) {
    return hasNativePermissions();
  }
  if (!isTauriShell()) {
    return false;
  }
  if (!id) {
    return true;
  }
  return id === 'notifications' || id === 'microphone' || id === 'location';
}

/** Opens the system page where the permission can be changed, or returns false when there is none to open. */
export async function openPermissionSettings(id: PermissionId): Promise<boolean> {
  const native = bridge();
  if (native && isAndroid()) {
    native.openSettings(id === 'push-app' ? 'notifications' : id);
    return true;
  }
  if (isTauriShell() && !isAndroid()) {
    if (id === 'push-app' || id === 'battery') {
      return false;
    }
    try {
      await invoke<void>('open_permission_settings', { permission: id });
      return true;
    } catch {
      return false;
    }
  }
  return false;
}
