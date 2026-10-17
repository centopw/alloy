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

/** Click handler for rendered message HTML: never let the webview navigate away. */
export function interceptLinks(e: MouseEvent) {
  const a = (e.target as HTMLElement | null)?.closest?.('a');
  if (!a) return;
  e.preventDefault();
  const href = a.getAttribute('href');
  if (href) void openExternal(href);
}
