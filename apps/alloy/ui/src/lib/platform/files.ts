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

/**
 * Pick pictures or videos, opening the system media chooser on Android and iOS. Empty when
 * cancelled or unsupported.
 */
export async function pickMediaFiles(): Promise<string[]> {
  if (!isTauriRuntime()) return [];
  return invoke<string[]>('pick_media_files');
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

/** The pictures on the clipboard as files: a screenshot copied as an image, or a picture copied from a browser. */
export function pastedImageFiles(data: DataTransfer | null): File[] {
  if (!data) return [];
  return [...data.files].filter((file) => file.type.startsWith('image/'));
}

/** A file name for a pasted picture, with an extension the engine can derive its type from. */
export function pastedImageName(file: Pick<File, 'name' | 'type'>): string {
  if (/\.[A-Za-z0-9]{2,5}$/.test(file.name)) return file.name;
  const extension = file.type.split('/')[1]?.replace('jpeg', 'jpg').replace(/[^a-z0-9].*$/, '') || 'png';
  return `${file.name || 'pasted-image'}.${extension}`;
}

/**
 * Whether pasted text could be the path of a file: one line starting with a slash, a drive letter or file://.
 * Only a cheap pre-check; the shell decides whether it really is a picture that exists.
 */
export function looksLikeFilePath(text: string): boolean {
  const line = text.trim().replace(/^["']|["']$/g, '');
  if (line === '' || /[\r\n]/.test(line)) return false;
  return /^(file:\/\/|\/|[A-Za-z]:[\\/])/.test(line);
}

/** Stores a pasted picture where the engine may send it. Returns its path. Desktop and Android only. */
export async function savePastedImage(file: File): Promise<string> {
  if (!isTauriRuntime()) throw new Error('Sending pictures needs the desktop or Android app.');
  return invoke<string>('save_pasted_image', { data: await blobToBase64(file), fileName: pastedImageName(file) });
}

/** A sendable copy of the picture file whose path was pasted as text, or null when the text is not such a path. */
export async function importPastedImagePath(text: string): Promise<string | null> {
  if (!isTauriRuntime()) return null;
  return invoke<string | null>('import_pasted_image_path', { text });
}

/** Whether attaching files is possible in this runtime. */
export const canPickFiles = () => isTauriRuntime();

/** Turn a local cache path into a URL the webview may load. */
export async function localFileUrl(path: string): Promise<string> {
  if (isTauriRuntime()) return convertFileSrc(path);
  return path;
}
