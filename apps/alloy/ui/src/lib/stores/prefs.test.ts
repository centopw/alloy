import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const invoke = vi.fn();
const subscribe = vi.fn();

vi.mock('../ipc/index.js', () => ({
  client: {
    invoke: (...args: unknown[]) => invoke(...args),
    subscribe: (...args: unknown[]) => subscribe(...args),
  },
}));

import {
  DEFAULT_PREFS,
  DEVICE_PREF_KEYS,
  PREFS_ENGINE_DEBOUNCE_MS,
  PREFS_ENGINE_KEY,
  PREFS_STORAGE_KEY,
  PREFS_SYNCED_AT_STORAGE_KEY,
  SYNCED_PREF_KEYS,
  type PrefKey,
  PrefsStore,
  isValidPref,
  defaultPrefsFor,
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

/** A request the store sent to the engine through `client.invoke`. */
interface SentRequest {
  action: string;
  params?: { json?: string; key?: string; value_json?: string };
}

function sentRequests(action: string): SentRequest[] {
  return (invoke.mock.calls as [SentRequest][]).map(([req]) => req).filter((req) => req.action === action);
}

function saveCalls(): SentRequest[] {
  return sentRequests('SetSetting');
}

/** Documents pushed to the account, in order, including attempts that failed. */
function pushedAccountDocs(): unknown[] {
  return sentRequests('SetAccountPrefs').map((req) => JSON.parse(req.params?.json ?? 'null'));
}

let remoteDoc: unknown = null;
let accountSaveFails = false;

beforeEach(() => {
  vi.useFakeTimers();
  invoke.mockReset();
  subscribe.mockReset();
  remoteDoc = null;
  accountSaveFails = false;
  invoke.mockImplementation(async (req: { action: string }) => {
    switch (req.action) {
      case 'GetSettings':
        return settingsResponse({});
      case 'GetAccountPrefs':
        return { type: 'AccountPrefs', payload: { json: remoteDoc === null ? null : JSON.stringify(remoteDoc) } };
      case 'SetAccountPrefs':
        if (accountSaveFails) throw new Error('offline');
        return { type: 'Unit' };
      default:
        return { type: 'SettingSet', payload: { key: PREFS_ENGINE_KEY } };
    }
  });
});

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe('defaultPrefsFor', () => {
  it('starts phones in the bubble layout and keeps the desktop default otherwise', () => {
    expect(defaultPrefsFor(true).messageLayout).toBe('bubbles');
    expect(defaultPrefsFor(false)).toEqual(DEFAULT_PREFS);
  });

  it('differs from the desktop default only in the message layout', () => {
    for (const key of Object.keys(DEFAULT_PREFS) as PrefKey[]) {
      if (key === 'messageLayout') continue;
      expect(defaultPrefsFor(true)[key]).toEqual(DEFAULT_PREFS[key]);
    }
  });
});

describe('defaults', () => {
  it('matches the documented defaults for every key', () => {
    expect(DEFAULT_PREFS).toEqual({
      messageLayout: 'modern',
      fontScale: 100,
      timeFormat: 'auto',
      bigEmoji: true,
      showReadReceipts: true,
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
      linkPreviews: 'unencrypted',
      enterToSend: true,
      launchAtLogin: false,
      autoCheckUpdates: true,
      autoDownloadUpdates: false,
      roomSort: 'activity',
      quickReactions: ['\u{1F44D}', '\u2764\uFE0F', '\u{1F602}', '\u{1F62E}', '\u{1F622}', '\u{1F64F}'],
      voiceSpeed: '1',
      permissionsPromptDismissed: false,
      mutedUntil: {},
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
    expect(store.linkPreviews).toBe('unencrypted');
    expect(store.launchAtLogin).toBe(false);
    expect(store.autoCheckUpdates).toBe(true);
    expect(store.autoDownloadUpdates).toBe(false);
    expect(store.roomSort).toBe('activity');
    expect(store.enterToSend).toBe(true);
    expect(store.get('notifyDesktop')).toBe(true);
    expect(store.voiceSpeed).toBe('1');
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
    expect(isValidPref('linkPreviews', 'always')).toBe(true);
    expect(isValidPref('linkPreviews', 'yes')).toBe(false);
    expect(isValidPref('voiceSpeed', '1.5')).toBe(true);
    expect(isValidPref('voiceSpeed', '3')).toBe(false);
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
    expect(saveCalls()[0]).toEqual({
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

  it('merges the engine entry over defaults and writes it locally', async () => {
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
    invoke.mockRejectedValue(new Error('no engine'));
    const store = new PrefsStore(memoryStorage(), [10, 20]);
    store.set('fontScale', 90);
    const pending = store.load();
    await vi.runAllTimersAsync();
    await pending;
    expect(store.fontScale).toBe(90);
    expect(store.ready).toBe(true);
    expect(warn).toHaveBeenCalled();
  });

  it('retries GetSettings on initial failure and recovers engine settings', async () => {
    invoke
      .mockRejectedValueOnce(new Error('IPC not ready'))
      .mockResolvedValueOnce(settingsResponse({ [PREFS_ENGINE_KEY]: { fontScale: 130 } }));
    const storage = memoryStorage();
    const store = new PrefsStore(storage, [10, 20]);
    const pending = store.load();
    await vi.runAllTimersAsync();
    await pending;

    expect(store.fontScale).toBe(130);
    expect(storedPrefs(storage)).toEqual({ ...DEFAULT_PREFS, fontScale: 130 });
    expect(store.ready).toBe(true);
  });

  it('does not overwrite engine settings with defaults when engine load fails', async () => {
    vi.spyOn(console, 'warn').mockImplementation(() => {});
    invoke.mockRejectedValue(new Error('no engine'));
    const store = new PrefsStore(memoryStorage(), [10, 20]);
    const pending = store.load();
    await vi.runAllTimersAsync();
    await pending;
    await vi.advanceTimersByTimeAsync(PREFS_ENGINE_DEBOUNCE_MS);

    expect(saveCalls()).toHaveLength(0);
  });

  it('recovers settings from the engine even when localStorage is empty', async () => {
    invoke.mockResolvedValueOnce(
      settingsResponse({ [PREFS_ENGINE_KEY]: { fontScale: 120, timeFormat: '24h' } }),
    );
    const storage = memoryStorage();
    const store = new PrefsStore(storage);
    expect(store.fontScale).toBe(DEFAULT_PREFS.fontScale);

    await store.load();
    expect(store.fontScale).toBe(120);
    expect(store.timeFormat).toBe('24h');
    expect(storedPrefs(storage)).toEqual({ ...DEFAULT_PREFS, fontScale: 120, timeFormat: '24h' });
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

describe('sign-out and flush', () => {
  it('drops synced values and the sync stamp on sign-out but keeps device settings', () => {
    const storage = memoryStorage();
    const store = new PrefsStore(storage);
    store.set('timeFormat', '24h');
    store.set('fontScale', 120);
    store.signOut();

    expect(store.timeFormat).toBe(DEFAULT_PREFS.timeFormat);
    expect(store.fontScale).toBe(120);
    expect(storage.getItem(PREFS_SYNCED_AT_STORAGE_KEY)).toBe('0');
  });

  it('clears synced values from the engine on sign-out', async () => {
    const storage = memoryStorage();
    const store = new PrefsStore(storage);
    store.set('timeFormat', '24h');
    store.set('fontScale', 120);

    await store.signOut();
    await vi.advanceTimersByTimeAsync(PREFS_ENGINE_DEBOUNCE_MS);

    expect(store.fontScale).toBe(120);
    expect(store.timeFormat).toBe(DEFAULT_PREFS.timeFormat);
    expect(saveCalls()).toHaveLength(1);
    const [savedPrefs] = saveCalls();
    const prefs = JSON.parse(savedPrefs.params?.value_json ?? '{}');
    expect(prefs).toHaveProperty('fontScale', 120);
    expect(prefs).not.toHaveProperty('timeFormat');
  });

  it('awaits flush before returning', async () => {
    const store = new PrefsStore(memoryStorage());
    store.set('fontScale', 110);
    expect(saveCalls()).toHaveLength(0);

    await store.flush();

    expect(saveCalls()).toHaveLength(1);
  });

  it('awaits signOut persistence before returning', async () => {
    const storage = memoryStorage();
    const store = new PrefsStore(storage);
    store.set('timeFormat', '24h');
    store.set('fontScale', 120);

    await store.signOut();

    expect(store.fontScale).toBe(120);
    expect(store.timeFormat).toBe(DEFAULT_PREFS.timeFormat);
    expect(saveCalls()).toHaveLength(1);
    const [savedPrefs] = saveCalls();
    const prefs = JSON.parse(savedPrefs.params?.value_json ?? '{}');
    expect(prefs).not.toHaveProperty('timeFormat');
  });
});


describe('account sync', () => {
  const REMOTE_STAMP = 1_700_000_000_000;

  it('keeps the device and synced scopes disjoint and complete', () => {
    expect([...DEVICE_PREF_KEYS, ...SYNCED_PREF_KEYS].sort()).toEqual(Object.keys(DEFAULT_PREFS).sort());
  });

  it('adopts a newer account document on load without pushing it back', async () => {
    remoteDoc = { updated_at: REMOTE_STAMP, timeFormat: '24h', bigEmoji: false };
    const storage = memoryStorage();
    const store = new PrefsStore(storage);
    await store.load();
    await vi.advanceTimersByTimeAsync(PREFS_ENGINE_DEBOUNCE_MS);

    expect(store.timeFormat).toBe('24h');
    expect(store.bigEmoji).toBe(false);
    expect(storage.data.get(PREFS_SYNCED_AT_STORAGE_KEY)).toBe(String(REMOTE_STAMP));
    expect(pushedAccountDocs()).toEqual([]);
  });

  it('pushes local synced values, not device values, when the account has no document', async () => {
    const store = new PrefsStore(memoryStorage());
    store.set('bigEmoji', false);
    store.set('fontScale', 120);
    await store.load();
    await vi.advanceTimersByTimeAsync(PREFS_ENGINE_DEBOUNCE_MS);

    const [doc] = pushedAccountDocs();
    expect(doc).toMatchObject({ bigEmoji: false });
    expect(doc).not.toHaveProperty('fontScale');
    expect(doc).toHaveProperty('updated_at', expect.any(Number));
  });

  it('does not push to the account when only device preferences change', async () => {
    const store = new PrefsStore(memoryStorage());
    store.set('fontScale', 110);
    store.set('notifySound', false);
    await vi.advanceTimersByTimeAsync(PREFS_ENGINE_DEBOUNCE_MS);

    expect(sentRequests('SetAccountPrefs')).toEqual([]);
    expect(saveCalls()).toHaveLength(1);
  });

  it('pushes one document per debounced change and ignores the echo of its own write', async () => {
    const store = new PrefsStore(memoryStorage());
    await store.load();
    store.set('showTyping', false);
    await vi.advanceTimersByTimeAsync(PREFS_ENGINE_DEBOUNCE_MS);

    const pushed = pushedAccountDocs();
    expect(pushed).toHaveLength(1);
    const [onEvent] = subscribe.mock.calls[0];
    onEvent({ type: 'AccountPrefsChanged', payload: { json: JSON.stringify(pushed[0]) } });
    await vi.advanceTimersByTimeAsync(PREFS_ENGINE_DEBOUNCE_MS);

    expect(pushedAccountDocs()).toHaveLength(1);
    expect(store.showTyping).toBe(false);
  });

  it('drops invalid values from a remote document', async () => {
    remoteDoc = { updated_at: REMOTE_STAMP, timeFormat: 'bogus', bigEmoji: 'no', showTyping: false };
    const store = new PrefsStore(memoryStorage());
    await store.load();

    expect(store.timeFormat).toBe(DEFAULT_PREFS.timeFormat);
    expect(store.bigEmoji).toBe(DEFAULT_PREFS.bigEmoji);
    expect(store.showTyping).toBe(false);
  });

  it('keeps the local values and retries on the next change when the account save fails', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const store = new PrefsStore(memoryStorage());
    await store.load();
    accountSaveFails = true;
    await vi.advanceTimersByTimeAsync(PREFS_ENGINE_DEBOUNCE_MS);
    expect(warn).toHaveBeenCalled();

    accountSaveFails = false;
    store.set('bigEmoji', false);
    await vi.advanceTimersByTimeAsync(PREFS_ENGINE_DEBOUNCE_MS);

    expect(pushedAccountDocs()).toHaveLength(2);
    expect(pushedAccountDocs()[1]).toMatchObject({ bigEmoji: false });
  });

  it('adopts a newer change from another device while running', async () => {
    const store = new PrefsStore(memoryStorage());
    await store.load();
    const [onEvent] = subscribe.mock.calls[0];
    onEvent({
      type: 'AccountPrefsChanged',
      payload: { json: JSON.stringify({ updated_at: Date.now() + 60_000, timeFormat: '24h' }) },
    });

    expect(store.timeFormat).toBe('24h');
  });

  it('ignores an older change from another device while a local change is pending', async () => {
    const store = new PrefsStore(memoryStorage());
    await store.load();
    store.set('timeFormat', '12h');
    const [onEvent] = subscribe.mock.calls[0];
    onEvent({
      type: 'AccountPrefsChanged',
      payload: { json: JSON.stringify({ updated_at: REMOTE_STAMP, timeFormat: '24h' }) },
    });

    expect(store.timeFormat).toBe('12h');
  });

  it('stamps a reset of every key as a synced change', async () => {
    remoteDoc = { updated_at: REMOTE_STAMP, bigEmoji: false };
    const store = new PrefsStore(memoryStorage());
    await store.load();
    store.reset();
    await vi.advanceTimersByTimeAsync(PREFS_ENGINE_DEBOUNCE_MS);

    expect(pushedAccountDocs()).toHaveLength(1);
    expect(pushedAccountDocs()[0]).toMatchObject({ bigEmoji: store.bigEmoji });
  });
});

describe('quickReactions', () => {
  it('accepts a distinct non-empty list and rejects an empty or malformed one', () => {
    expect(isValidPref('quickReactions', ['\u{1F525}', '\u{1F44D}'])).toBe(true);
    expect(isValidPref('quickReactions', [])).toBe(false);
    expect(isValidPref('quickReactions', ['\u{1F525}', '\u{1F525}'])).toBe(false);
    expect(isValidPref('quickReactions', 'x')).toBe(false);
  });

  it('syncs with the account and ignores a set with the same content', () => {
    expect(SYNCED_PREF_KEYS).toContain('quickReactions');
    const store = new PrefsStore(memoryStorage());
    const before = store.quickReactions;
    store.set('quickReactions', [...before]);
    expect(store.quickReactions).toBe(before);
    store.set('quickReactions', ['\u{1F525}']);
    expect(store.quickReactions).toEqual(['\u{1F525}']);
  });
});
