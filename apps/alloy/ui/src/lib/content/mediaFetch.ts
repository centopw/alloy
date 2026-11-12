import { localFileUrl } from '$lib/platform/files';
import { fetchMediaPath } from '$lib/platform/saveFile';
import type { MediaSource } from '$lib/generated';

/** Fetch a stored media object's full file into the local cache. Returns a URL the webview can load. */
export async function fetchMediaFile(source: MediaSource, mimeType: string): Promise<string> {
  return localFileUrl(await fetchMediaPath(source, mimeType));
}
