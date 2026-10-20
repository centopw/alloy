import { localFileUrl } from '$lib/platform/files';
import { fetchMediaPath } from '$lib/platform/saveFile';

/** Fetch the full file into the local media cache. Returns a URL the webview can load. */
export async function fetchMediaFile(uri: string, mimeType: string): Promise<string> {
  return localFileUrl(await fetchMediaPath(uri, mimeType));
}
