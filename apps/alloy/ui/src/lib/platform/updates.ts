import { invoke } from '@tauri-apps/api/core';

/** Result of a `check_update` probe. `available: false` means the app is up to date. */
export interface UpdateInfo {
  available: boolean;
  version: string;
  notes: string | null;
  /** True when the app can download, verify and install the update in place (desktop). */
  can_install: boolean;
  /** Release page. The only action available when `can_install` is false (Android). */
  url: string;
}

/** Ask the backend whether a newer release exists. Resolves null outside Tauri. */
export async function checkForUpdate(): Promise<UpdateInfo | null> {
  if (typeof window === 'undefined' || !('__TAURI_INTERNALS__' in window)) return null;
  return invoke<UpdateInfo>('check_update');
}

/**
 * Download, verify and install the pending update. On success the app restarts itself, so the
 * promise may never resolve. Rejects with a message string on failure. No-op outside Tauri.
 */
export async function installUpdate(): Promise<void> {
  if (typeof window === 'undefined' || !('__TAURI_INTERNALS__' in window)) return;
  await invoke<void>('install_update');
}
