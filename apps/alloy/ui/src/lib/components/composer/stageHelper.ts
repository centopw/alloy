import { importPastedImagePath, localFileUrl } from '$lib/platform/files';

/** How the composer should render a staged file's thumbnail. */
export type StagedKind = 'image' | 'video' | 'file';

export interface StagedPicture {
  path: string;
  name: string;
  kind: StagedKind;
  preview: string | null;
}

const IMAGE_EXTENSION = /\.(png|jpe?g|gif|webp|bmp|avif|heic)$/i;
const VIDEO_EXTENSION = /\.(mp4|m4v|mov|webm|mkv|ogv)$/i;

/** Which element draws this file's thumbnail, decided from its extension. */
export function stagedKind(path: string): StagedKind {
  if (IMAGE_EXTENSION.test(path)) return 'image';
  if (VIDEO_EXTENSION.test(path)) return 'video';
  return 'file';
}

/**
 * A URL the webview may render for a picture or a video, or null for a file that has no thumbnail.
 * Videos use the same asset URL as pictures: the webview draws its own poster frame from it.
 */
export async function stagedPreview(path: string): Promise<string | null> {
  if (stagedKind(path) === 'file') return null;
  try {
    return await localFileUrl(path);
  } catch {
    return null;
  }
}

/** Builds the staged item for a file already copied into the app's pick cache. */
export async function stageFile(path: string): Promise<StagedPicture> {
  return {
    path,
    name: path.split(/[\\/]/).pop() ?? path,
    kind: stagedKind(path),
    preview: await stagedPreview(path),
  };
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
    return await stageFile(path);
  } catch {
    return null;
  }
}
