import { profileCard } from '$lib/stores/profileCard.svelte';
import { roomsStore } from '$lib/stores/rooms.svelte';
import { parseMatrixToLink } from './matrixTo';

/** Open an external URL: in the system browser under Tauri, a new tab on the web. */
export async function openExternal(url: string): Promise<void> {
  if (!/^(https?:|mailto:)/i.test(url)) return;
  if (typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window) {
    const { openUrl } = await import('@tauri-apps/plugin-opener');
    await openUrl(url);
  } else {
    window.open(url, '_blank', 'noopener,noreferrer');
  }
}

/**
 * Click/auxclick handler for rendered message HTML: never let the webview navigate or open a new window itself.
 * Matrix permalinks open the profile card (users) or select a joined room (rooms); other links open externally.
 */
export function interceptLinks(e: MouseEvent) {
  const a = (e.target as HTMLElement | null)?.closest?.('a');
  if (!a) return;
  // Middle-click (auxclick, button 1) would otherwise make the webview open the link in a new window.
  // Other buttons (e.g. right-click) keep their native behaviour.
  if (e.type === 'auxclick' && e.button !== 1) return;
  e.preventDefault();
  const href = a.getAttribute('href');
  if (!href) return;
  const link = parseMatrixToLink(href);
  if (link?.kind === 'user') {
    profileCard.open(link.id, roomsStore.activeRoomId, anchorPoint(e, a));
    return;
  }
  if (link?.kind === 'room' && roomsStore.rooms.some((r) => r.id === link.id)) {
    roomsStore.select(link.id);
    return;
  }
  void openExternal(href);
}

/** Pointer position for mouse clicks; the link's centre when activated from the keyboard (detail 0). */
function anchorPoint(e: MouseEvent, a: HTMLElement): { x: number; y: number } {
  if (e.detail > 0) return { x: e.clientX, y: e.clientY };
  const r = a.getBoundingClientRect();
  return { x: r.left + r.width / 2, y: r.top + r.height / 2 };
}
