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

/** Click/auxclick handler for rendered message HTML: never let the webview navigate or open a new window itself. */
export function interceptLinks(e: MouseEvent) {
  const a = (e.target as HTMLElement | null)?.closest?.('a');
  if (!a) return;
  // Middle-click (auxclick, button 1) would otherwise make the webview open the link in a new window.
  // Other buttons (e.g. right-click) keep their native behaviour.
  if (e.type === 'auxclick' && e.button !== 1) return;
  e.preventDefault();
  const href = a.getAttribute('href');
  if (href) void openExternal(href);
}
