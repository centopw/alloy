import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

/**
 * How this copy of the app was installed, detected once by the backend from the bundle path.
 * `homebrew` means a cask owns the bundle, so only Homebrew may move it; `translocated` means the
 * app runs from a quarantined disk-image path it cannot write to.
 */
export type InstallMethod = 'manual' | 'homebrew' | 'translocated';

/** Cask token that ships Alloy. */
export const HOMEBREW_CASK = 'alloy';
/** Tap hosting the cask, needed on a machine that has not tapped it yet. */
export const HOMEBREW_TAP = 'centopw/tap';
export const HOMEBREW_UPGRADE_COMMAND = `brew upgrade --cask ${HOMEBREW_CASK}`;
export const HOMEBREW_TAP_COMMAND = `brew tap ${HOMEBREW_TAP}`;

/** Result of a `check_update` probe. `available: false` means the app is up to date. */
export interface UpdateInfo {
  available: boolean;
  version: string;
  /** Release notes as markdown, taken from the release manifest. */
  notes: string | null;
  /** True when the app can download, verify and install the update in place (desktop). */
  can_install: boolean;
  /** Release page. The only action available when `can_install` is false (Android). */
  url: string;
  /** True when the update is already downloaded and only waits for `applyUpdate`. */
  downloaded: boolean;
  /** Who installed this copy; decides whether the UI may offer the in-place install. */
  install_method: InstallMethod;
}

/** Payload of the `update://progress` event emitted while `download_update` runs. */
export interface UpdateProgress {
  downloaded: number;
  total: number | null;
}

export const UPDATE_PROGRESS_EVENT = 'update://progress';
export const UPDATE_DOWNLOADED_EVENT = 'update://downloaded';

function inTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

/** Ask the backend whether a newer release exists. Resolves null outside Tauri. */
export async function checkForUpdate(): Promise<UpdateInfo | null> {
  if (!inTauri()) return null;
  return invoke<UpdateInfo>('check_update');
}

/**
 * Download and verify the pending update without installing it. `onProgress` receives every
 * `update://progress` event for the duration of the download. Rejects with a message string on
 * failure, and rejects when the platform cannot install in place. No-op outside Tauri.
 */
export async function downloadUpdate(onProgress: (progress: UpdateProgress) => void): Promise<void> {
  if (!inTauri()) return;
  const unlisten = await listen<UpdateProgress>(UPDATE_PROGRESS_EVENT, (event) => onProgress(event.payload));
  try {
    await invoke<void>('download_update');
  } finally {
    unlisten();
  }
}

/**
 * Install the downloaded update (downloading first if needed) and restart the app. On success the
 * app restarts itself, so the promise may never resolve. Rejects with a message string on failure.
 * No-op outside Tauri.
 */
export async function applyUpdate(): Promise<void> {
  if (!inTauri()) return;
  await invoke<void>('apply_update');
}

/**
 * Subscribe to the `update://downloaded` event. Returns a teardown. No-op outside Tauri.
 */
export function onUpdateDownloaded(cb: () => void): () => void {
  if (!inTauri()) return () => {};
  let unlisten: UnlistenFn | null = null;
  let disposed = false;
  void listen(UPDATE_DOWNLOADED_EVENT, () => cb()).then((fn) => {
    if (disposed) fn();
    else unlisten = fn;
  });
  return () => {
    disposed = true;
    unlisten?.();
  };
}
