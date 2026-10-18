/**
 * User preferences that drive real behaviour (message layout, notifications, room list, typing, ...).
 *
 * localStorage holds the local copy and is read synchronously at construction so the first paint is
 * correct. `load()` reconciles with the engine's `prefs` settings entry. Every change is written to
 * localStorage immediately and to the engine after a short debounce.
 */
import { client } from '../ipc/index.js';

export const PREFS_STORAGE_KEY = 'alloy.prefs.v1';
export const PREFS_ENGINE_KEY = 'prefs';
export const PREFS_ENGINE_DEBOUNCE_MS = 300;

export type MessageLayout = 'modern' | 'bubbles' | 'irc';
export type FontScale = 90 | 100 | 110 | 120 | 130;
export type TimeFormat = 'auto' | '12h' | '24h';

export interface Prefs {
  messageLayout: MessageLayout;
  fontScale: FontScale;
  timeFormat: TimeFormat;
  bigEmoji: boolean;
  notifyDesktop: boolean;
  notifySound: boolean;
  notifyPreview: boolean;
  notifyOnlyWhenUnfocused: boolean;
  roomListPreviews: boolean;
  roomListSections: boolean;
  roomListPeople: boolean;
  homeShowAllRooms: boolean;
  showTyping: boolean;
  sendTyping: boolean;
  showRemovedPlaceholder: boolean;
  showMembershipEvents: boolean;
  showProfileChanges: boolean;
  showRoomUpdates: boolean;
  jumpToBottomOnSend: boolean;
  autoplayMedia: boolean;
  enterToSend: boolean;
  launchAtLogin: boolean;
}

export type PrefKey = keyof Prefs;

/** Handle returned by `setTimeout` in this runtime. */
export type EngineTimer = ReturnType<typeof setTimeout>;

export const DEFAULT_PREFS: Readonly<Prefs> = Object.freeze({
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

const PREF_KEYS = Object.keys(DEFAULT_PREFS) as PrefKey[];

const ENUM_VALUES = {
  messageLayout: ['modern', 'bubbles', 'irc'],
  fontScale: [90, 100, 110, 120, 130],
  timeFormat: ['auto', '12h', '24h'],
} as const satisfies Partial<{ [K in PrefKey]: readonly Prefs[K][] }>;

type EnumKey = keyof typeof ENUM_VALUES;

function isEnumKey(key: PrefKey): key is EnumKey {
  return key in ENUM_VALUES;
}

/** Whether `value` is an allowed value for `key`. Booleans and the enum sets are the only accepted types. */
export function isValidPref(key: PrefKey, value: unknown): boolean {
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

export class PrefsStore {
  #storage: PrefsStorage | null;
  #values = $state<Prefs>({ ...DEFAULT_PREFS });
  #version = 0;
  #engineTimer: EngineTimer | undefined;

  /** True once `load()` has reconciled with the engine, or given up. */
  ready = $state(false);

  constructor(storage: PrefsStorage | null = defaultStorage()) {
    this.#storage = storage;
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
  get homeShowAllRooms(): boolean {
    return this.#values.homeShowAllRooms;
  }
  get showTyping(): boolean {
    return this.#values.showTyping;
  }
  get sendTyping(): boolean {
    return this.#values.sendTyping;
  }
  get showRemovedPlaceholder(): boolean {
    return this.#values.showRemovedPlaceholder;
  }
  get showMembershipEvents(): boolean {
    return this.#values.showMembershipEvents;
  }
  get showProfileChanges(): boolean {
    return this.#values.showProfileChanges;
  }
  get showRoomUpdates(): boolean {
    return this.#values.showRoomUpdates;
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

  get<K extends PrefKey>(key: K): Prefs[K] {
    return this.#values[key];
  }

  /** Ignores invalid values and no-op writes. */
  set<K extends PrefKey>(key: K, value: Prefs[K]): void {
    if (!isValidPref(key, value) || this.#values[key] === value) return;
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
   * Reconciles with the engine's `prefs` entry. The entry is merged over the defaults, unknown keys and
   * invalid values are dropped. If the engine has no entry, or the user changed a preference while the
   * request was in flight, the local values are kept and pushed to the engine. Never rejects.
   */
  async load(): Promise<void> {
    const startVersion = this.#version;
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
      this.#values = { ...DEFAULT_PREFS, ...sanitizePrefs(entry) };
      this.#writeLocal();
    } catch (err) {
      console.warn('[prefs] could not reconcile preferences with the engine; keeping the local copy', err);
    } finally {
      this.ready = true;
    }
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

  async #pushToEngine(): Promise<void> {
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
  return $effect.root(() => {
    $effect(() => {
      applyFontScale(store.fontScale);
    });
    $effect(() => {
      applyMessageLayout(store.messageLayout);
    });
  });
}
