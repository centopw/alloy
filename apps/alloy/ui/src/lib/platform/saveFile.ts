import { invoke } from '@tauri-apps/api/core';
import { client } from '$lib/ipc';
import { isAndroid } from './capabilities';
import { isTauriRuntime, localFileUrl } from './files';

/**
 * Whether the Download action is offered. Android's native save path (SAF create-document plus a
 * content:// write) has not been verified on a device, so the action stays hidden there.
 */
export function canDownloadMedia(): boolean {
  return !isTauriRuntime() || !isAndroid();
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
 * Save a cached file. Tauri shows the native save dialog and resolves false when cancelled.
 * The browser starts a download and always resolves true.
 */
export async function saveMediaFile(localPath: string, suggestedName: string): Promise<boolean> {
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
  return saveMediaFile(await fetchMediaPath(uri, mimeType), suggestedName);
}
