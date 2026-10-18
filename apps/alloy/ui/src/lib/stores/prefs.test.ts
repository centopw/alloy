import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const invoke = vi.fn();

vi.mock('../ipc/index.js', () => ({
  client: { invoke: (...args: unknown[]) => invoke(...args) },
}));

import {
  DEFAULT_PREFS,
  PREFS_ENGINE_DEBOUNCE_MS,
  PREFS_ENGINE_KEY,
  PREFS_STORAGE_KEY,
  PrefsStore,
  isValidPref,
  sanitizePrefs,
  type PrefsStorage,
} from './prefs.svelte.js';

interface MemoryStorage extends PrefsStorage {
  data: Map<string, string>;
}

function memoryStorage(initial: Record<string, string> = {}): MemoryStorage {
  const data = new Map(Object.entries(initial));
  return {
    data,
    getItem: (key) => data.get(key) ?? null,
    setItem: (key, value) => {
      data.set(key, value);
    },
    removeItem: (key) => {
      data.delete(key);
    },
  };
}

function settingsResponse(settings: unknown) {
  return { type: 'SettingsRetrieved', payload: { settings_json: JSON.stringify(settings) } };
}

function storedPrefs(storage: MemoryStorage): unknown {
  const raw = storage.data.get(PREFS_STORAGE_KEY);
  return raw === undefined ? undefined : JSON.parse(raw);
}

function saveCalls(): unknown[][] {
  return invoke.mock.calls.filter(([req]: unknown[]) => {
    if (typeof req !== 'object' || req === null || !('action' in req)) return false;
    return req.action === 'SetSetting';
  });
}

beforeEach(() => {
  vi.useFakeTimers();
  invoke.mockReset();
  invoke.mockResolvedValue({ type: 'SettingSet', payload: { key: PREFS_ENGINE_KEY } });
});

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe('defaults', () => {
  it('matches the documented defaults for every key', () => {
    expect(DEFAULT_PREFS).toEqual({
      messageLayout: 'modern',
      fontScale: 100,
      timeFormat: 'auto',
      bigEmoji: true,
      notifyDesktop: true,
      notifySound: true,
      notifyPreview: true,
      notifyOnlyWhenUnfocused: true,
      roomListPreviews: true,
      roomListSections: true,
      roomListPeople: true,
      homeShowAllRooms: true,
      showTyping: true,
      sendTyping: true,
      showRemovedPlaceholder: true,
      showMembershipEvents: true,
      showProfileChanges: true,
      showRoomUpdates: true,
      jumpToBottomOnSend: true,
      autoplayMedia: false,
      enterToSend: true,
      launchAtLogin: false,
    });
  });

  it('is frozen', () => {
    expect(Object.isFrozen(DEFAULT_PREFS)).toBe(true);
  });

  it('exposes every key through its typed getter', () => {
    const store = new PrefsStore(memoryStorage());
    expect(store.messageLayout).toBe('modern');
    expect(store.fontScale).toBe(100);
    expect(store.timeFormat).toBe('auto');
    expect(store.autoplayMedia).toBe(false);
    expect(store.launchAtLogin).toBe(false);
    expect(store.enterToSend).toBe(true);
    expect(store.get('notifyDesktop')).toBe(true);
    expect(store.ready).toBe(false);
  });
});

describe('validation', () => {
  it('accepts allowed enum values and rejects others', () => {
    expect(isValidPref('messageLayout', 'irc')).toBe(true);
    expect(isValidPref('messageLayout', 'compact')).toBe(false);
    expect(isValidPref('fontScale', 130)).toBe(true);
    expect(isValidPref('fontScale', 105)).toBe(false);
    expect(isValidPref('fontScale', '100')).toBe(false);
    expect(isValidPref('timeFormat', '24h')).toBe(true);
    expect(isValidPref('timeFormat', 'iso')).toBe(false);
  });

  it('accepts only booleans for boolean keys', () => {
    expect(isValidPref('bigEmoji', false)).toBe(true);
    expect(isValidPref('bigEmoji', 'false')).toBe(false);
    expect(isValidPref('bigEmoji', 1)).toBe(false);
  });

  it('drops unknown keys and invalid values', () => {
    expect(
      sanitizePrefs({ fontScale: 110, bigEmoji: 'yes', nope: true, timeFormat: '12h' }),
    ).toEqual({ fontScale: 110, timeFormat: '12h' });
  });

  it('returns an empty object for non-object input', () => {
    expect(sanitizePrefs(null)).toEqual({});
    expect(sanitizePrefs('prefs')).toEqual({});
    expect(sanitizePrefs(42)).toEqual({});
  });

  it('ignores invalid set calls and keeps the previous value', () => {
    const storage = memoryStorage();
    const store = new PrefsStore(storage);
    // Force invalid values through the untyped boundary.
    store.set('fontScale', 999 as never);
    store.set('bigEmoji', 'no' as never);
    expect(store.fontScale).toBe(100);
    expect(store.bigEmoji).toBe(true);
    expect(storage.data.has(PREFS_STORAGE_KEY)).toBe(false);
  });
});

describe('set and reset', () => {
  it('updates the value and persists it locally', () => {
    const storage = memoryStorage();
    const store = new PrefsStore(storage);
    store.set('messageLayout', 'bubbles');
    expect(store.messageLayout).toBe('bubbles');
    expect(storedPrefs(storage)).toEqual({ ...DEFAULT_PREFS, messageLayout: 'bubbles' });
  });

  it('does not write when the value is unchanged', () => {
    const storage = memoryStorage();
    const store = new PrefsStore(storage);
    store.set('enterToSend', true);
    expect(storage.data.has(PREFS_STORAGE_KEY)).toBe(false);
  });

  it('resets one key to its default', () => {
    const store = new PrefsStore(memoryStorage());
    store.set('fontScale', 120);
    store.set('timeFormat', '24h');
    store.reset('fontScale');
    expect(store.fontScale).toBe(100);
    expect(store.timeFormat).toBe('24h');
  });

  it('resets every key when called without arguments', () => {
    const store = new PrefsStore(memoryStorage());
    store.set('fontScale', 120);
    store.set('autoplayMedia', true);
    store.reset();
    expect(store.fontScale).toBe(100);
    expect(store.autoplayMedia).toBe(false);
  });
});

describe('local persistence', () => {
  it('restores values written by a previous instance', () => {
    const storage = memoryStorage();
    new PrefsStore(storage).set('timeFormat', '12h');
    const reopened = new PrefsStore(storage);
    expect(reopened.timeFormat).toBe('12h');
  });

  it('drops unknown keys and invalid values from storage', () => {
    const storage = memoryStorage({
      [PREFS_STORAGE_KEY]: JSON.stringify({ fontScale: 200, bigEmoji: false, extra: 1 }),
    });
    const store = new PrefsStore(storage);
    expect(store.fontScale).toBe(100);
    expect(store.bigEmoji).toBe(false);
  });

  it('warns and falls back to defaults on corrupt JSON', () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const store = new PrefsStore(memoryStorage({ [PREFS_STORAGE_KEY]: '{not json' }));
    expect(store.fontScale).toBe(100);
    expect(warn).toHaveBeenCalled();
  });

  it('warns and falls back to defaults when storage read throws', () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const storage: PrefsStorage = {
      getItem: () => {
        throw new Error('denied');
      },
      setItem: () => {},
      removeItem: () => {},
    };
    const store = new PrefsStore(storage);
    expect(store.fontScale).toBe(100);
    expect(warn).toHaveBeenCalled();
  });

  it('keeps working without storage', () => {
    const store = new PrefsStore(null);
    store.set('fontScale', 110);
    expect(store.fontScale).toBe(110);
  });
});

describe('engine sync', () => {
  it('debounces engine saves and sends the last value once', async () => {
    const store = new PrefsStore(memoryStorage());
    store.set('fontScale', 110);
    store.set('fontScale', 120);
    store.set('timeFormat', '24h');

    await vi.advanceTimersByTimeAsync(PREFS_ENGINE_DEBOUNCE_MS - 1);
    expect(saveCalls()).toHaveLength(0);

    await vi.advanceTimersByTimeAsync(1);
    expect(saveCalls()).toHaveLength(1);
    expect(invoke).toHaveBeenLastCalledWith({
      action: 'SetSetting',
      params: {
        key: PREFS_ENGINE_KEY,
        value_json: JSON.stringify({ ...DEFAULT_PREFS, fontScale: 120, timeFormat: '24h' }),
      },
    });
  });

  it('swallows engine save errors with a warning', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    invoke.mockRejectedValue(new Error('offline'));
    const store = new PrefsStore(memoryStorage());
    store.set('bigEmoji', false);
    await vi.advanceTimersByTimeAsync(PREFS_ENGINE_DEBOUNCE_MS);
    expect(warn).toHaveBeenCalled();
    expect(store.bigEmoji).toBe(false);
  });

  it('merges the engine entry over defaults and writes it locally without saving back', async () => {
    invoke.mockResolvedValueOnce(
      settingsResponse({ [PREFS_ENGINE_KEY]: { fontScale: 130, notifySound: false, bogus: 1 } }),
    );
    const storage = memoryStorage();
    const store = new PrefsStore(storage);
    await store.load();
    await vi.advanceTimersByTimeAsync(PREFS_ENGINE_DEBOUNCE_MS);

    expect(store.fontScale).toBe(130);
    expect(store.notifySound).toBe(false);
    expect(store.messageLayout).toBe('modern');
    expect(storedPrefs(storage)).toEqual({ ...DEFAULT_PREFS, fontScale: 130, notifySound: false });
    expect(saveCalls()).toHaveLength(0);
    expect(store.ready).toBe(true);
  });

  it('pushes local values when the engine has no prefs entry', async () => {
    invoke.mockResolvedValueOnce(settingsResponse({ other: 1 }));
    const store = new PrefsStore(memoryStorage());
    store.set('timeFormat', '12h');
    await store.load();
    await vi.advanceTimersByTimeAsync(PREFS_ENGINE_DEBOUNCE_MS);

    expect(store.timeFormat).toBe('12h');
    expect(saveCalls()).toHaveLength(1);
  });

  it('keeps local values and becomes ready when the load fails', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    invoke.mockRejectedValueOnce(new Error('no engine'));
    const store = new PrefsStore(memoryStorage());
    store.set('fontScale', 90);
    await store.load();
    expect(store.fontScale).toBe(90);
    expect(store.ready).toBe(true);
    expect(warn).toHaveBeenCalled();
  });

  it('keeps a change made while the load is in flight', async () => {
    const response = Promise.withResolvers<unknown>();
    invoke.mockReturnValueOnce(response.promise);
    const store = new PrefsStore(memoryStorage());
    const pending = store.load();
    store.set('fontScale', 120);
    response.resolve(settingsResponse({ [PREFS_ENGINE_KEY]: { fontScale: 90 } }));
    await pending;

    expect(store.fontScale).toBe(120);
    await vi.advanceTimersByTimeAsync(PREFS_ENGINE_DEBOUNCE_MS);
    expect(saveCalls()).toHaveLength(1);
  });
});
