/** Pick a file to attach. Returns its absolute path (Tauri) or null when cancelled/unsupported. */
export async function pickFile(): Promise<string | null> {
  if (typeof window === 'undefined' || !('__TAURI_INTERNALS__' in window)) return null;
  const { open } = await import('@tauri-apps/plugin-dialog');
  const res = await open({ multiple: false, directory: false });
  return typeof res === 'string' ? res : null;
}

/** Whether attaching files is possible in this runtime. */
export const canPickFiles = () =>
  typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

/** Turn a local cache path into a URL the webview may load. */
export async function localFileUrl(path: string): Promise<string> {
  if (typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window) {
    const { convertFileSrc } = await import('@tauri-apps/api/core');
    return convertFileSrc(path);
  }
  return path;
}
