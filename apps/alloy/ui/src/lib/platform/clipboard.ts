/**
 * Clipboard utilities for copying plain text and images to the system clipboard.
 */

export function canCopyImage(): boolean {
  return typeof ClipboardItem !== 'undefined' && typeof navigator !== 'undefined' && !!navigator.clipboard?.write;
}

/**
 * Convert an image Blob to image/png through an HTMLCanvasElement when it is not already PNG.
 */
export async function convertBlobToPng(blob: Blob): Promise<Blob> {
  if (blob.type === 'image/png') {
    return blob;
  }
  return new Promise<Blob>((resolve, reject) => {
    const img = new Image();
    const url = URL.createObjectURL(blob);
    img.onload = () => {
      URL.revokeObjectURL(url);
      try {
        const canvas = document.createElement('canvas');
        canvas.width = img.naturalWidth || img.width;
        canvas.height = img.naturalHeight || img.height;
        const ctx = canvas.getContext('2d');
        if (!ctx) {
          reject(new Error('Canvas 2D context unavailable'));
          return;
        }
        ctx.drawImage(img, 0, 0);
        canvas.toBlob((pngBlob) => {
          if (pngBlob) {
            resolve(pngBlob);
          } else {
            reject(new Error('Failed to convert image to PNG'));
          }
        }, 'image/png');
      } catch (err) {
        reject(err);
      }
    };
    img.onerror = () => {
      URL.revokeObjectURL(url);
      reject(new Error('Failed to decode image'));
    };
    img.src = url;
  });
}

/**
 * Copy an image Blob to the system clipboard as image/png.
 */
export async function copyImageBlob(blob: Blob): Promise<void> {
  if (typeof ClipboardItem === 'undefined' || typeof navigator === 'undefined' || !navigator.clipboard?.write) {
    throw new Error('Clipboard image copy is not supported in this environment');
  }
  const pngBlob = await convertBlobToPng(blob);
  await navigator.clipboard.write([new ClipboardItem({ 'image/png': pngBlob })]);
}

/**
 * Fetch an image URL and write it to the system clipboard as image/png.
 */
export async function copyImageFromUrl(url: string): Promise<void> {
  const res = await fetch(url);
  if (!res.ok) {
    throw new Error(`Failed to fetch media: ${res.statusText || res.status}`);
  }
  const blob = await res.blob();
  await copyImageBlob(blob);
}

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
