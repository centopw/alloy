import { invoke } from '@tauri-apps/api/core';

/**
 * Launch-at-login only exists in the Tauri desktop shell. Android has no equivalent here, and the
 * browser build has no shell to register with.
 */
export function isAutostartSupported(): boolean {
  if (typeof window === 'undefined' || !('__TAURI_INTERNALS__' in window)) return false;
  if (typeof navigator !== 'undefined' && /android/i.test(navigator.userAgent)) return false;
  return true;
}

/** Whether the OS currently launches Alloy at login. Resolves false where autostart is unsupported. */
export async function getAutostart(): Promise<boolean> {
  if (!isAutostartSupported()) return false;
  return invoke<boolean>('autostart_get');
}

/** Registers or unregisters Alloy as a login item. No-op where autostart is unsupported. */
export async function setAutostart(enabled: boolean): Promise<void> {
  if (!isAutostartSupported()) return;
  await invoke<void>('autostart_set', { enabled });
}
