import { getCurrentWindow } from '@tauri-apps/api/window';
import { isTauriShell } from './notify';
import { currentPlatform } from './shortcuts';

/**
 * macOS overlay title bar. The window is configured with `titleBarStyle: "Overlay"` (tauri.conf.json),
 * so the native title strip is gone and the app's own header is the top edge. The traffic lights are
 * drawn over the webview at `trafficLightPosition` (x: 18, y: 20): each light is 12px tall and the
 * sidebar header is 52px (`--alloy-layout-header-height`), so y = (52 - 12) / 2 = 20 centres them.
 * `--alloy-titlebar-lights-inset` (layout.css) reserves 84px on the left: three 12px lights, two 8px
 * gaps, the 18px left offset and 6px of air.
 */

export type TitlebarMode = 'overlay' | 'native';

/** `overlay` only in the Tauri shell on macOS; the browser build and other platforms keep the native bar. */
export function titlebarMode(isTauri: boolean, platform: string): TitlebarMode {
  return isTauri && /mac/i.test(platform) ? 'overlay' : 'native';
}

/** True when the overlay title bar applies to the running shell. */
export function isMacTauri(): boolean {
  return titlebarMode(isTauriShell(), currentPlatform()) === 'overlay';
}

/**
 * Marks the document for the overlay title bar and tracks fullscreen. In fullscreen macOS hides the
 * traffic lights, so `data-fullscreen` collapses the inset. Every other platform is a no-op.
 * Returns the cleanup that removes the resize listener and the data attributes.
 */
export function installTitlebar(doc: Document): () => void {
  if (!isMacTauri()) return () => {};

  const root = doc.documentElement;
  root.dataset.titlebar = 'overlay';
  root.dataset.fullscreen = 'false';

  let disposed = false;
  let unlisten: (() => void) | null = null;

  const syncFullscreen = async (): Promise<void> => {
    try {
      const full = await getCurrentWindow().isFullscreen();
      if (!disposed) root.dataset.fullscreen = full ? 'true' : 'false';
    } catch (err) {
      console.warn('titlebar: could not read fullscreen state', err);
      if (!disposed) root.dataset.fullscreen = 'false';
    }
  };

  void syncFullscreen();
  void getCurrentWindow()
    .onResized(() => void syncFullscreen())
    .then((stop) => {
      if (disposed) stop();
      else unlisten = stop;
    })
    .catch((err: unknown) => {
      console.warn('titlebar: could not listen for window resizes', err);
    });

  return () => {
    disposed = true;
    unlisten?.();
    delete root.dataset.titlebar;
    delete root.dataset.fullscreen;
  };
}
