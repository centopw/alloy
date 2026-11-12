/**
 * User preferences that drive real behaviour (message layout, notifications, chat list, typing, ...).
 *
 * Every preference is device-local: it describes this install (screen, input method, OS
 * notification permission, login item, updater) or how the user wants to read their own chats, and
 * none of it is sent to Telegram or Messenger. localStorage is read synchronously at construction
 * so the first paint is correct; `load()` reconciles with the copy the engine keeps in its local
 * settings file, so a webview that lost its storage still shows the user's choices. Every change is
 * written to localStorage immediately and to the engine after a short debounce.
 */
import { client } from '../ipc/index.js';
import { ROOM_SORTS, type RoomSort } from './roomSort.js';
import { DEFAULT_QUICK_REACTIONS, isQuickReactions } from '../content/quickReactions.js';

export const PREFS_STORAGE_KEY = 'alloy.prefs.v1';
export const PREFS_ENGINE_KEY = 'prefs';
export const PREFS_ENGINE_DEBOUNCE_MS = 300;

export type MessageLayout = 'modern' | 'bubbles' | 'irc';
export type FontScale = 90 | 100 | 110 | 120 | 130;
export type TimeFormat = 'auto' | '12h' | '24h';
export type VoiceSpeed = '1' | '1.5' | '2';

export interface Prefs {
  messageLayout: MessageLayout;
  fontScale: FontScale;
  timeFormat: TimeFormat;
  bigEmoji: boolean;
  /**
   * Show the read markers other participants send. Alloy always reports its own reads: the
   * networks' terms require it and the toggle never suppresses them.
   */
  showReadReceipts: boolean;
  notifyDesktop: boolean;
  notifySound: boolean;
  notifyPreview: boolean;
  notifyOnlyWhenUnfocused: boolean;
  roomListPreviews: boolean;
  roomListSections: boolean;
  roomListPeople: boolean;
  showTyping: boolean;
  sendTyping: boolean;
  /** Announces incoming messages through the conversation's polite live region (mute for chatty chats). */
  announceMessages: boolean;
  showRemovedPlaceholder: boolean;
  showMembershipEvents: boolean;
  jumpToBottomOnSend: boolean;
  autoplayMedia: boolean;
  enterToSend: boolean;
  launchAtLogin: boolean;
  autoCheckUpdates: boolean;
  autoDownloadUpdates: boolean;
  roomSort: RoomSort;
  /** Emoji shown in the quick-reaction bar, in order. */
  quickReactions: readonly string[];
  voiceSpeed: VoiceSpeed;
  /** The first-run permissions prompt was dismissed on this device. */
  permissionsPromptDismissed: boolean;
}

export type PrefKey = keyof Prefs;

export const DEFAULT_PREFS: Readonly<Prefs> = Object.freeze({
  messageLayout: 'bubbles',
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
  showTyping: true,
  sendTyping: true,
  announceMessages: true,
  showRemovedPlaceholder: true,
  showMembershipEvents: true,
  jumpToBottomOnSend: true,
  autoplayMedia: false,
  enterToSend: true,
  launchAtLogin: false,
  autoCheckUpdates: true,
  autoDownloadUpdates: false,
  roomSort: 'activity',
  quickReactions: DEFAULT_QUICK_REACTIONS,
  voiceSpeed: '1',
  permissionsPromptDismissed: false,
});

/** Equality for preference values: lists compare by content, everything else by identity. */
function sameValue(a: unknown, b: unknown): boolean {
  if (Array.isArray(a) && Array.isArray(b)) return a.length === b.length && a.every((v, i) => v === b[i]);
  return a === b;
}

const PREF_KEYS = Object.keys(DEFAULT_PREFS) as PrefKey[];

const ENUM_VALUES = {
  messageLayout: ['modern', 'bubbles', 'irc'],
  fontScale: [90, 100, 110, 120, 130],
  timeFormat: ['auto', '12h', '24h'],
  roomSort: ROOM_SORTS,
  voiceSpeed: ['1', '1.5', '2'],
} as const satisfies Partial<{ [K in PrefKey]: readonly Prefs[K][] }>;

type EnumKey = keyof typeof ENUM_VALUES;

function isEnumKey(key: PrefKey): key is EnumKey {
  return key in ENUM_VALUES;
}

/** Whether `value` is an allowed value for `key`. Booleans and the enum sets are the only accepted types. */
export function isValidPref(key: PrefKey, value: unknown): boolean {
  if (key === 'quickReactions') return isQuickReactions(value);
  if (typeof DEFAULT_PREFS[key] === 'boolean') return typeof value === 'boolean';
  if (isEnumKey(key)) return (ENUM_VALUES[key] as readonly unknown[]).includes(value);
  return false;
}

/** Keeps only known keys with valid values. Anything else is dropped silently. */
export function sanitizePrefs(raw: unknown): Partial<Prefs> {
  if (typeof raw !== 'object' || raw === null) return {};
  const source = raw as Partial<Record<PrefKey, unknown>>;
  const out: Partial<Record<PrefKey, unknown>> = {};
  for (const key of PREF_KEYS) {
    if (isValidPref(key, source[key])) out[key] = source[key];
  }
  return out as Partial<Prefs>;
}

export interface PrefsStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

function defaultStorage(): PrefsStorage | null {
  try {
    return typeof localStorage === 'undefined' ? null : localStorage;
  } catch {
    return null;
  }
}

function readStored(storage: PrefsStorage | null): Partial<Prefs> {
  if (!storage) return {};
  let raw: string | null;
  try {
    raw = storage.getItem(PREFS_STORAGE_KEY);
  } catch (err) {
    console.warn('[prefs] could not read stored preferences', err);
    return {};
  }
  if (raw === null) return {};
  try {
    return sanitizePrefs(JSON.parse(raw));
  } catch (err) {
    console.warn('[prefs] ignoring corrupt stored preferences', err);
    return {};
  }
}

const flushHandlers = new Set<() => Promise<void> | void>();

/** Registers another store's pending persistence writes with the app lifecycle flush. */
export function registerFlushHandler(handler: () => Promise<void> | void): () => void {
  flushHandlers.add(handler);
  return () => flushHandlers.delete(handler);
}

export class PrefsStore {
  #storage: PrefsStorage | null;
  #values = $state<Prefs>({ ...DEFAULT_PREFS });
  #version = 0;
  #engineTimer: ReturnType<typeof setTimeout> | undefined;
  /** Delays between retry attempts for initial GetSettings load. */
  #retryDelays: readonly number[];
  /** Pending engine save promise, or undefined if none in flight. */
  #inFlightEngine: Promise<void> | undefined;

  /** True once `load()` has reconciled with the engine, or given up. */
  ready = $state(false);

  constructor(storage: PrefsStorage | null = defaultStorage(), retryDelays: readonly number[] = [100, 200]) {
    this.#storage = storage;
    this.#retryDelays = retryDelays;
    this.#values = { ...DEFAULT_PREFS, ...readStored(storage) };
  }

  get messageLayout(): MessageLayout {
    return this.#values.messageLayout;
  }
  get fontScale(): FontScale {
    return this.#values.fontScale;
  }
  get timeFormat(): TimeFormat {
    return this.#values.timeFormat;
  }
  get bigEmoji(): boolean {
    return this.#values.bigEmoji;
  }
  get showReadReceipts(): boolean {
    return this.#values.showReadReceipts;
  }
  get notifyDesktop(): boolean {
    return this.#values.notifyDesktop;
  }
  get notifySound(): boolean {
    return this.#values.notifySound;
  }
  get notifyPreview(): boolean {
    return this.#values.notifyPreview;
  }
  get notifyOnlyWhenUnfocused(): boolean {
    return this.#values.notifyOnlyWhenUnfocused;
  }
  get roomListPreviews(): boolean {
    return this.#values.roomListPreviews;
  }
  get roomListSections(): boolean {
    return this.#values.roomListSections;
  }
  get roomListPeople(): boolean {
    return this.#values.roomListPeople;
  }
  get showTyping(): boolean {
    return this.#values.showTyping;
  }
  get sendTyping(): boolean {
    return this.#values.sendTyping;
  }
  get announceMessages(): boolean {
    return this.#values.announceMessages;
  }
  get showRemovedPlaceholder(): boolean {
    return this.#values.showRemovedPlaceholder;
  }
  get showMembershipEvents(): boolean {
    return this.#values.showMembershipEvents;
  }
  get jumpToBottomOnSend(): boolean {
    return this.#values.jumpToBottomOnSend;
  }
  get autoplayMedia(): boolean {
    return this.#values.autoplayMedia;
  }
  get enterToSend(): boolean {
    return this.#values.enterToSend;
  }
  get launchAtLogin(): boolean {
    return this.#values.launchAtLogin;
  }
  get autoCheckUpdates(): boolean {
    return this.#values.autoCheckUpdates;
  }
  get autoDownloadUpdates(): boolean {
    return this.#values.autoDownloadUpdates;
  }

  get roomSort(): RoomSort {
    return this.#values.roomSort;
  }

  get quickReactions(): readonly string[] {
    return this.#values.quickReactions;
  }
  get voiceSpeed(): VoiceSpeed {
    return this.#values.voiceSpeed;
  }
  get permissionsPromptDismissed(): boolean {
    return this.#values.permissionsPromptDismissed;
  }

  get<K extends PrefKey>(key: K): Prefs[K] {
    return this.#values[key];
  }

  /** Ignores invalid values and no-op writes. */
  set<K extends PrefKey>(key: K, value: Prefs[K]): void {
    if (!isValidPref(key, value) || sameValue(this.#values[key], value)) return;
    this.#values[key] = value;
    this.#changed();
  }

  /** Restores one key to its default, or every key when called without arguments. */
  reset<K extends PrefKey>(key?: K): void {
    if (key === undefined) {
      this.#values = { ...DEFAULT_PREFS };
    } else {
      this.#values[key] = DEFAULT_PREFS[key];
    }
    this.#changed();
  }

  /**
   * Writes pending changes immediately. Callers can await persistence before teardown where possible.
   */
  async flush(): Promise<void> {
    const pending: Promise<unknown>[] = [];
    if (this.#inFlightEngine) pending.push(this.#inFlightEngine);
    if (this.#engineTimer !== undefined) {
      clearTimeout(this.#engineTimer);
      this.#engineTimer = undefined;
      pending.push(this.#pushToEngine());
    }
    for (const handler of flushHandlers) {
      try {
        pending.push(Promise.resolve(handler()));
      } catch (err) {
        console.warn('[prefs] could not flush another store', err);
      }
    }
    await Promise.allSettled(pending);
  }

  /**
   * Merges the engine's copy over the defaults. Each step keeps the local values when it fails and
   * never rejects.
   */
  async load(): Promise<void> {
    await this.#loadEngineCopy();
    this.ready = true;
  }

  /**
   * Merges the engine's `prefs` entry over the defaults. Retries with exponential backoff (3 attempts)
   * to handle IPC not ready on startup. If the engine has no entry, or the user changed a
   * preference while the request was in flight, the local values are kept and pushed to the engine.
   * Failed load keeps the local (localStorage + defaults) values without overwriting them.
   */
  async #loadEngineCopy(): Promise<void> {
    const startVersion = this.#version;
    let lastErr: unknown;
    for (let attempt = 0; attempt <= this.#retryDelays.length; attempt++) {
      try {
        const res = await client.invoke({ action: 'GetSettings' });
        if (res.type !== 'SettingsRetrieved') {
          throw new Error(`unexpected response: ${res.type}`);
        }
        const settings = JSON.parse(res.payload.settings_json) as { [PREFS_ENGINE_KEY]?: unknown } | null;
        const entry = settings?.[PREFS_ENGINE_KEY];
        // A newer user change is already scheduled to reach the engine; do not overwrite it.
        if (this.#version !== startVersion) return;
        if (entry === undefined) {
          this.#scheduleEngineSave();
          return;
        }
        this.#values = { ...this.#values, ...sanitizePrefs(entry) };
        this.#writeLocal();
        return;
      } catch (err) {
        lastErr = err;
        const delayMs = this.#retryDelays[attempt];
        if (delayMs !== undefined) {
          const { promise, resolve } = Promise.withResolvers<void>();
          setTimeout(resolve, delayMs);
          await promise;
        }
      }
    }
    console.warn('[prefs] could not reconcile preferences with the engine after retries; keeping the local copy', lastErr);
  }

  #changed(): void {
    this.#version++;
    this.#writeLocal();
    this.#scheduleEngineSave();
  }

  #writeLocal(): void {
    if (!this.#storage) return;
    try {
      this.#storage.setItem(PREFS_STORAGE_KEY, JSON.stringify(this.#values));
    } catch (err) {
      console.warn('[prefs] could not persist preferences locally', err);
    }
  }

  #scheduleEngineSave(): void {
    clearTimeout(this.#engineTimer);
    this.#engineTimer = setTimeout(() => {
      this.#engineTimer = undefined;
      void this.#pushToEngine();
    }, PREFS_ENGINE_DEBOUNCE_MS);
  }

  /** Saves the preferences to the engine's local settings file. Errors are logged, never thrown. */
  async #pushToEngine(): Promise<void> {
    const promise = (async () => {
      try {
        const res = await client.invoke({
          action: 'SetSetting',
          params: { key: PREFS_ENGINE_KEY, value_json: JSON.stringify(this.#values) },
        });
        if (res.type !== 'SettingSet') {
          console.warn('[prefs] unexpected response while saving preferences', res.type);
        }
      } catch (err) {
        console.warn('[prefs] could not save preferences to the engine', err);
      }
    })();
    this.#inFlightEngine = promise;
    try {
      await promise;
    } finally {
      if (this.#inFlightEngine === promise) {
        this.#inFlightEngine = undefined;
      }
    }
  }
}

export const prefs = new PrefsStore();

/** Applies the font scale as the root font size. No-op without a DOM. */
export function applyFontScale(scale: FontScale): void {
  if (typeof document === 'undefined') return;
  document.documentElement.style.fontSize = `${scale}%`;
}

/** Exposes the message layout as `data-message-layout` on the root element. No-op without a DOM. */
export function applyMessageLayout(layout: MessageLayout): void {
  if (typeof document === 'undefined') return;
  document.documentElement.setAttribute('data-message-layout', layout);
}

/**
 * Keeps the document in sync with the given store. Call once from the app shell. Returns a teardown.
 */
export function installPrefsEffects(store: PrefsStore = prefs): () => void {
  const flush = () => void store.flush();
  const onVisibility = () => {
    if (document.visibilityState === 'hidden') void store.flush();
  };
  window.addEventListener('pagehide', flush);
  document.addEventListener('visibilitychange', onVisibility);
  const stopEffects = $effect.root(() => {
    $effect(() => {
      applyFontScale(store.fontScale);
    });
    $effect(() => {
      applyMessageLayout(store.messageLayout);
    });
  });
  return () => {
    window.removeEventListener('pagehide', flush);
    document.removeEventListener('visibilitychange', onVisibility);
    stopEffects();
  };
}
