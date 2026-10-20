/**
 * Native Android clipboard image access via the ClipboardBridge JS interface.
 * On desktop and in browsers, returns null/false; on Android the WebView exposes
 * window.AlloyClipboard with hasImage() and saveImage() methods.
 */

interface AlloyClipboardBridge {
  hasImage(): boolean;
  saveImage(): string;
}

declare global {
  interface Window {
    AlloyClipboard?: AlloyClipboardBridge;
  }
}

/** Checks whether the system clipboard contains an image. Desktop and browser always return false. */
export function nativeClipboardHasImage(): boolean {
  if (typeof window === 'undefined') return false;
  try {
    return Boolean(window.AlloyClipboard?.hasImage());
  } catch {
    return false;
  }
}

/** Copies the first image from the system clipboard into the app cache and returns its absolute path, or null when absent/unsupported. */
export function nativeClipboardImage(): string | null {
  if (typeof window === 'undefined') return null;
  try {
    const path = window.AlloyClipboard?.saveImage();
    return path && path.length > 0 ? path : null;
  } catch {
    return null;
  }
}
