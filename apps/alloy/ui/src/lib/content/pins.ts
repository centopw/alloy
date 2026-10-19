/** Index of the next pin to show after jumping to `current`: one older, wrapping to the newest. */
export function nextPinIndex(current: number, count: number): number {
  if (count <= 1) return 0;
  return current <= 0 ? count - 1 : current - 1;
}

/** One-line preview of a pinned message: whitespace collapsed, shortened with an ellipsis. */
export function pinPreview(text: string, max = 80): string {
  const flat = text.replace(/\s+/g, ' ').trim();
  if (flat === '') return 'Message';
  return flat.length > max ? `${flat.slice(0, max - 1)}\u2026` : flat;
}
