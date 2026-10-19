import { client } from '../ipc/index.js';
import type { UrlPreviewDto } from '../generated/index.js';
import { expect } from './errors.js';
import { prefs } from './prefs.svelte.js';
import { roomsStore } from './rooms.svelte.js';

export interface LinkPreviewView {
  preview: UrlPreviewDto | null;
  loading: boolean;
}

const IDLE: LinkPreviewView = { preview: null, loading: false };

/**
 * Link previews per URL. Fetches are lazy and once per URL for the session;
 * a missing preview and a failed fetch both settle to `preview: null`.
 */
export class LinkPreviewStore {
  private entries = $state<Record<string, LinkPreviewView>>({});
  /** Kept out of reactive state so the effect that calls `request` does not re-run on its own write. */
  private requested = new Set<string>();

  /** Current state for a URL. Reading never fetches. */
  view(url: string): LinkPreviewView {
    return this.entries[url] ?? IDLE;
  }

  /** Whether the synced `linkPreviews` setting allows previews in a room. Unknown rooms count as encrypted. */
  allowed(roomId: string): boolean {
    if (prefs.linkPreviews === 'off') return false;
    if (prefs.linkPreviews === 'always') return true;
    const room = roomsStore.rooms.find((r) => r.id === roomId);
    return room !== undefined && !room.encrypted;
  }

  /** Starts the fetch for a URL the first time it is requested. */
  request(url: string): void {
    if (this.requested.has(url)) return;
    this.requested.add(url);
    this.entries[url] = { preview: null, loading: true };
    void this.fetch(url);
  }

  private async fetch(url: string): Promise<void> {
    let preview: UrlPreviewDto | null = null;
    try {
      const res = await client.invoke({ action: 'GetUrlPreview', params: { url } });
      preview = expect(res, 'UrlPreview').payload.preview;
    } catch {
      preview = null;
    }
    this.entries[url] = { preview, loading: false };
  }
}

export const linkPreviews = new LinkPreviewStore();
