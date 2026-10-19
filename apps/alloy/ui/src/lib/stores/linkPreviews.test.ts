import { beforeEach, describe, expect, it, vi } from 'vitest';
import { LinkPreviewStore } from './linkPreviews.svelte';

const shared = vi.hoisted(() => ({
  invoke: vi.fn(),
  prefs: { linkPreviews: 'unencrypted' as 'off' | 'unencrypted' | 'always' },
  rooms: [] as Array<{ id: string; encrypted: boolean }>,
}));

vi.mock('../ipc/index.js', () => ({
  client: { invoke: (...args: unknown[]) => shared.invoke(...args) },
}));
vi.mock('./prefs.svelte.js', () => ({ prefs: shared.prefs }));
vi.mock('./rooms.svelte.js', () => ({ roomsStore: { rooms: shared.rooms } }));

const URL = 'https://example.com/post';
const PREVIEW = {
  url: URL,
  title: 'Post',
  description: 'About the post',
  site_name: 'Example',
  image: null,
  image_width: null,
  image_height: null,
};

let store: LinkPreviewStore;

beforeEach(() => {
  shared.invoke.mockReset();
  shared.prefs.linkPreviews = 'unencrypted';
  shared.rooms.splice(0, shared.rooms.length, { id: '!plain:x', encrypted: false });
  shared.rooms.push({ id: '!secret:x', encrypted: true });
  store = new LinkPreviewStore();
});

describe('LinkPreviewStore.request', () => {
  it('fetches once per URL and exposes the preview', async () => {
    shared.invoke.mockResolvedValue({ type: 'UrlPreview', payload: { preview: PREVIEW } });

    store.request(URL);
    store.request(URL);
    expect(store.view(URL).loading).toBe(true);

    await vi.waitFor(() => expect(store.view(URL).loading).toBe(false));
    expect(shared.invoke).toHaveBeenCalledTimes(1);
    expect(shared.invoke).toHaveBeenCalledWith({
      action: 'GetUrlPreview',
      params: { url: URL },
    });
    expect(store.view(URL).preview).toEqual(PREVIEW);
  });

  it('settles to null when the homeserver has no preview', async () => {
    shared.invoke.mockResolvedValue({ type: 'UrlPreview', payload: { preview: null } });

    store.request(URL);
    await vi.waitFor(() => expect(store.view(URL).loading).toBe(false));
    expect(store.view(URL).preview).toBeNull();
  });

  it('settles to null without throwing when the engine call fails', async () => {
    shared.invoke.mockRejectedValue({ code: 'network_error', message: 'offline' });

    store.request(URL);
    await vi.waitFor(() => expect(store.view(URL).loading).toBe(false));
    expect(store.view(URL).preview).toBeNull();
  });

  it('reads of an unrequested URL never fetch', () => {
    expect(store.view(URL)).toEqual({ preview: null, loading: false });
    expect(shared.invoke).not.toHaveBeenCalled();
  });
});

describe('LinkPreviewStore.allowed', () => {
  it('never previews when off', () => {
    shared.prefs.linkPreviews = 'off';
    expect(store.allowed('!plain:x')).toBe(false);
  });

  it('previews everywhere when always', () => {
    shared.prefs.linkPreviews = 'always';
    expect(store.allowed('!secret:x')).toBe(true);
  });

  it('previews only unencrypted rooms by default', () => {
    expect(store.allowed('!plain:x')).toBe(true);
    expect(store.allowed('!secret:x')).toBe(false);
  });

  it('treats unknown rooms as encrypted', () => {
    expect(store.allowed('!unknown:x')).toBe(false);
  });
});
