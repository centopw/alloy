import { invoke } from '@tauri-apps/api/core';
import { client } from '$lib/ipc';
import { isAndroid } from './capabilities';
import { isTauriRuntime, localFileUrl } from './files';

/** The native helper the Android shell injects into the WebView (see `DownloadBridge.kt`). */
interface AndroidDownloadBridge {
  saveToDownloads(path: string, name: string, mime: string): string;
}

function androidBridge(): AndroidDownloadBridge | null {
  const bridge = (window as Window & { AlloyAndroid?: AndroidDownloadBridge }).AlloyAndroid;
  return typeof bridge?.saveToDownloads === 'function' ? bridge : null;
}

/** Whether the Download action is offered: everywhere except an Android build without the native helper. */
export function canDownloadMedia(): boolean {
  return !isTauriRuntime() || !isAndroid() || androidBridge() !== null;
}

/** Fetch a remote media file into the local cache and return its absolute local path. */
export async function fetchMediaPath(uri: string, mimeType: string): Promise<string> {
  const res = await client.getMedia({ kind: 'Remote', data: { uri } }, 'file', {
    mime_type: mimeType,
  });
  if (res.type !== 'MediaRetrieved') throw new Error(`unexpected response: ${res.type}`);
  return res.payload.local_path;
}

/**
 * Save a cached file. Desktop shows the native save dialog and resolves false when cancelled; Android
 * copies into the public Downloads folder; the browser starts a download. Rejects with a readable message on failure.
 */
export async function saveMediaFile(localPath: string, suggestedName: string, mimeType = ''): Promise<boolean> {
  const android = isTauriRuntime() && isAndroid() ? androidBridge() : null;
  if (android) {
    const failure = android.saveToDownloads(localPath, suggestedName, mimeType);
    if (failure) throw new Error(failure);
    return true;
  }
  if (isTauriRuntime()) return invoke<boolean>('save_media', { localPath, suggestedName });
  const link = document.createElement('a');
  link.href = await localFileUrl(localPath);
  link.download = suggestedName;
  link.click();
  return true;
}

/** Fetch a remote media file and save it under `suggestedName`. Resolves false when cancelled. */
export async function downloadMedia(
  uri: string,
  mimeType: string,
  suggestedName: string
): Promise<boolean> {
  return saveMediaFile(await fetchMediaPath(uri, mimeType), suggestedName, mimeType);
}
