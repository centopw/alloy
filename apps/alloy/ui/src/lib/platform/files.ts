import { convertFileSrc, invoke } from '@tauri-apps/api/core';

/** True inside the Tauri shell (desktop or mobile), false in a plain browser. */
export const isTauriRuntime = () => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

/** Pick a file to attach. Returns its absolute path (Tauri) or null when cancelled/unsupported. */
export async function pickFile(): Promise<string | null> {
  if (!isTauriRuntime()) return null;
  // The dialog runs in Rust so Android content:// picks are copied into the app cache.
  return invoke<string | null>('pick_attachment');
}

/** Pick several files to attach at once. Empty when cancelled or unsupported. */
export async function pickFiles(): Promise<string[]> {
  if (!isTauriRuntime()) return [];
  return invoke<string[]>('pick_attachments');
}

/** File extension for a recorded clip's MIME type (the engine and servers care about it). */
export function voiceExtension(mimeType: string): string {
  const type = mimeType.toLowerCase();
  if (type.includes('webm')) return 'webm';
  if (type.includes('ogg')) return 'ogg';
  if (type.includes('mp4') || type.includes('aac') || type.includes('m4a')) return 'm4a';
  return 'bin';
}

/** Base64 text of a blob, without the data-URL prefix. */
export function blobToBase64(blob: Blob): Promise<string> {
  const { promise, resolve, reject } = Promise.withResolvers<string>();
  const reader = new FileReader();
  reader.onload = () => resolve(String(reader.result).replace(/^data:[^,]*,/, ''));
  reader.onerror = () => reject(new Error('The recording could not be read.'));
  reader.readAsDataURL(blob);
  return promise;
}

/** Hands a recording to the shell, which writes it to a file the engine may send. Returns that path. */
export async function writeTempFile(blob: Blob, extension: string): Promise<string> {
  if (!isTauriRuntime()) throw new Error('Voice messages need the desktop or Android app.');
  return invoke<string>('write_temp_file', { data: await blobToBase64(blob), extension });
}

/** Whether attaching files is possible in this runtime. */
export const canPickFiles = () => isTauriRuntime();

/** Turn a local cache path into a URL the webview may load. */
export async function localFileUrl(path: string): Promise<string> {
  if (isTauriRuntime()) return convertFileSrc(path);
  return path;
}
