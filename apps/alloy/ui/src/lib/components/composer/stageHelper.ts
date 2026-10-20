import { importPastedImagePath } from '$lib/platform/files';

export interface StagedPicture {
  path: string;
  name: string;
  preview: string | null;
}

/**
 * Converts a cached or copied image path into a staged picture ready for sending.
 * The path should be the result of nativeClipboardImage() or similar; it will be
 * imported into the pick cache and prepared for display.
 */
export async function stagePictureFromPath(cachedPath: string): Promise<StagedPicture | null> {
  try {
    const path = await importPastedImagePath(cachedPath);
    if (!path) return null;
    return {
      path,
      name: path.split(/[\\/]/).pop() ?? path,
      preview: null,
    };
  } catch {
    return null;
  }
}
