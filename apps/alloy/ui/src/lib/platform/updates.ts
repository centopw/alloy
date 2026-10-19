import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

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
