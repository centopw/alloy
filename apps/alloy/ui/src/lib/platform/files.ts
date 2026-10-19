import { convertFileSrc, invoke } from '@tauri-apps/api/core';

/** True inside the Tauri shell (desktop or mobile), false in a plain browser. */
export const isTauriRuntime = () => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

/** Pick a file to attach. Returns its absolute path (Tauri) or null when cancelled/unsupported. */
export async function pickFile(): Promise<string | null> {
  if (!isTauriRuntime()) return null;
  // The dialog runs in Rust so Android content:// picks are copied into the app cache.
  return invoke<string | null>('pick_attachment');
}

/** Whether attaching files is possible in this runtime. */
export const canPickFiles = () => isTauriRuntime();

/** Turn a local cache path into a URL the webview may load. */
export async function localFileUrl(path: string): Promise<string> {
  if (isTauriRuntime()) return convertFileSrc(path);
  return path;
}
