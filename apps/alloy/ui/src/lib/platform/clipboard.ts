/**
 * Copy plain text to the system clipboard. Uses the async Clipboard API when available and falls
 * back to a hidden textarea with `execCommand('copy')` for webviews that deny the async API.
 * Rejects when neither path succeeds so callers can surface a failure if they choose.
 */
export async function copyText(text: string): Promise<void> {
  if (typeof navigator !== 'undefined' && navigator.clipboard?.writeText) {
    try {
      await navigator.clipboard.writeText(text);
      return;
    } catch {
      // Fall through to the legacy path below.
    }
  }

  const area = document.createElement('textarea');
  area.value = text;
  area.setAttribute('readonly', '');
  area.style.position = 'fixed';
  area.style.opacity = '0';
  area.style.pointerEvents = 'none';
  document.body.appendChild(area);
  area.select();
  try {
    if (!document.execCommand('copy')) throw new Error('Clipboard copy was rejected');
  } finally {
    area.remove();
  }
}
