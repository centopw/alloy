/**
 * User preferences that drive real behaviour (message layout, notifications, room list, typing, ...).
 *
 * localStorage holds the local copy and is read synchronously at construction so the first paint is
 * correct. `load()` reconciles with the engine's device copy and the account's synced document. Synced
 * preferences (see `SYNCED_PREF_KEYS`) travel with the Matrix account; device preferences stay local.
 * Every change is written to localStorage immediately and to the engine and account after a short debounce.
 */
import { client } from '../ipc/index.js';
import { viewport } from './viewport.svelte.js';
import { ROOM_SORTS, type RoomSort } from './roomSort.js';

export const PREFS_STORAGE_KEY = 'alloy.prefs.v1';
export const PREFS_ENGINE_KEY = 'prefs';
export const PREFS_ENGINE_DEBOUNCE_MS = 300;
export const PREFS_SYNCED_AT_STORAGE_KEY = 'alloy.prefs.syncedAt.v1';

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
  autoCheckUpdates: boolean;
  autoDownloadUpdates: boolean;
  roomSort: RoomSort;
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
  autoCheckUpdates: true,
  autoDownloadUpdates: false,
  roomSort: 'activity',
});

/**
 * Defaults for a device with no stored preference. Phones start in the bubble layout, which fits a
 * narrow screen; desktops keep the modern layout. The stored value always wins over these defaults.
 */
export function defaultPrefsFor(isPhone: boolean): Readonly<Prefs> {
  return isPhone ? { ...DEFAULT_PREFS, messageLayout: 'bubbles' } : DEFAULT_PREFS;
}

const PREF_KEYS = Object.keys(DEFAULT_PREFS) as PrefKey[];

type PrefScope = 'device' | 'synced';

/**
 * Where each preference lives. Device preferences depend on this install (screen, input method, OS
 * notification permission, login item, updater), so they stay local. Synced preferences describe what the
 * user sees and sends in the account's conversations, so they travel with the Matrix account.
 */
const PREF_SCOPE: Record<PrefKey, PrefScope> = {
  messageLayout: 'device',
  fontScale: 'device',
  enterToSend: 'device',
  jumpToBottomOnSend: 'device',
  notifyDesktop: 'device',
  notifySound: 'device',
  notifyPreview: 'device',
  notifyOnlyWhenUnfocused: 'device',
  launchAtLogin: 'device',
  autoCheckUpdates: 'device',
  autoDownloadUpdates: 'device',
  timeFormat: 'synced',
  bigEmoji: 'synced',
  roomListPreviews: 'synced',
  roomListSections: 'synced',
  roomListPeople: 'synced',
  homeShowAllRooms: 'synced',
  showTyping: 'synced',
  sendTyping: 'synced',
  showRemovedPlaceholder: 'synced',
  showMembershipEvents: 'synced',
  showProfileChanges: 'synced',
  showRoomUpdates: 'synced',
  autoplayMedia: 'synced',
  roomSort: 'synced',
};

/** Preferences that stay on this install. */
export const DEVICE_PREF_KEYS: readonly PrefKey[] = PREF_KEYS.filter((key) => PREF_SCOPE[key] === 'device');
/** Preferences stored in the account's `chat.alloy.prefs` document, shared by every device. */
export const SYNCED_PREF_KEYS: readonly PrefKey[] = PREF_KEYS.filter((key) => PREF_SCOPE[key] === 'synced');

/** Keeps only the synced keys of `values`. */
function syncedSubset(values: Partial<Prefs>): Partial<Prefs> {
  const out: Partial<Record<PrefKey, unknown>> = {};
  for (const key of SYNCED_PREF_KEYS) {
    if (values[key] !== undefined) out[key] = values[key];
  }
  return out as Partial<Prefs>;
}

const ENUM_VALUES = {
  messageLayout: ['modern', 'bubbles', 'irc'],
  fontScale: [90, 100, 110, 120, 130],
  timeFormat: ['auto', '12h', '24h'],
  roomSort: ROOM_SORTS,
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

/** Epoch milliseconds of the latest synced change this device knows of; 0 when none. */
function readSyncedAt(storage: PrefsStorage | null): number {
  if (!storage) return 0;
  try {
    const at = Number(storage.getItem(PREFS_SYNCED_AT_STORAGE_KEY));
    return Number.isSafeInteger(at) && at > 0 ? at : 0;
  } catch (err) {
    console.warn('[prefs] could not read the synced-preferences timestamp', err);
    return 0;
  }
}

/** A parsed account document: the synced values it carries and the time they were changed. */
interface AccountDoc {
  updatedAt: number;
  values: Partial<Prefs>;
}

/** Reads the account's `chat.alloy.prefs` document. Null when absent or not a valid document. */
function parseAccountDoc(json: string | null): AccountDoc | null {
  if (json === null) return null;
  let raw: unknown;
  try {
    raw = JSON.parse(json);
  } catch {
    return null;
  }
  if (typeof raw !== 'object' || raw === null || !('updated_at' in raw)) return null;
  const updatedAt = raw.updated_at;
  if (typeof updatedAt !== 'number' || !Number.isSafeInteger(updatedAt) || updatedAt < 0) return null;
  return { updatedAt, values: syncedSubset(sanitizePrefs(raw)) };
}

export class PrefsStore {
  #storage: PrefsStorage | null;
  #values = $state<Prefs>({ ...DEFAULT_PREFS });
  #version = 0;
  #engineTimer: EngineTimer | undefined;
  /** Epoch ms of the latest synced change this device knows of: local, or the adopted account document. */
  #syncedAt = 0;
  /** Synced values changed here and not yet stored on the account. */
  #accountDirty = false;
  #listening = false;

  /** True once `load()` has reconciled with the engine, or given up. */
  ready = $state(false);

  constructor(storage: PrefsStorage | null = defaultStorage()) {
    this.#storage = storage;
    this.#values = { ...defaultPrefsFor(viewport.isPhone), ...readStored(storage) };
    this.#syncedAt = readSyncedAt(storage);
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
  get autoCheckUpdates(): boolean {
    return this.#values.autoCheckUpdates;
  }
  get autoDownloadUpdates(): boolean {
    return this.#values.autoDownloadUpdates;
  }

  get roomSort(): RoomSort {
    return this.#values.roomSort;
  }

  get<K extends PrefKey>(key: K): Prefs[K] {
    return this.#values[key];
  }

  /** Ignores invalid values and no-op writes. */
  set<K extends PrefKey>(key: K, value: Prefs[K]): void {
    if (!isValidPref(key, value) || this.#values[key] === value) return;
    this.#values[key] = value;
    this.#changed(PREF_SCOPE[key] === 'synced');
  }

  /** Restores one key to its default, or every key when called without arguments. */
  reset<K extends PrefKey>(key?: K): void {
    if (key === undefined) {
      this.#values = { ...defaultPrefsFor(viewport.isPhone) };
    } else {
      this.#values[key] = defaultPrefsFor(viewport.isPhone)[key];
    }
    this.#changed(key === undefined || PREF_SCOPE[key] === 'synced');
  }

  /**
   * Reconciles with the engine's device copy, then with the account's synced document. Each step keeps
   * the local values when it fails and never rejects. The first call also starts listening for synced
   * changes from other devices.
   */
  async load(): Promise<void> {
    this.#listen();
    await this.#loadEngineCopy();
    await this.#loadAccount();
    this.ready = true;
  }

  /**
   * Merges the engine's `prefs` entry over the defaults. If the engine has no entry, or the user changed a
   * preference while the request was in flight, the local values are kept and pushed to the engine.
   */
  async #loadEngineCopy(): Promise<void> {
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
      this.#values = { ...defaultPrefsFor(viewport.isPhone), ...sanitizePrefs(entry) };
      this.#writeLocal();
    } catch (err) {
      console.warn('[prefs] could not reconcile preferences with the engine; keeping the local copy', err);
    }
  }

  /**
   * Adopts the account document when it is newer than this device's last synced change. Otherwise pushes
   * this device's synced values, when they are newer or the account has none yet.
   */
  async #loadAccount(): Promise<void> {
    const startVersion = this.#version;
    try {
      const res = await client.invoke({ action: 'GetAccountPrefs' });
      if (res.type !== 'AccountPrefs') {
        throw new Error(`unexpected response: ${res.type}`);
      }
      // A newer user change is pending; it is pushed with its own timestamp.
      if (this.#version !== startVersion) return;
      const remote = parseAccountDoc(res.payload.json);
      if (this.#adoptIfNewer(remote)) return;
      if (!remote && this.#syncedAt === 0) this.#syncedAt = Date.now();
      if (!remote || this.#syncedAt > remote.updatedAt) {
        this.#accountDirty = true;
        this.#writeLocal();
        this.#scheduleEngineSave();
      }
    } catch (err) {
      console.warn('[prefs] could not reconcile preferences with the account; keeping the local copy', err);
    }
  }

  /** Starts the listener for synced changes from other devices. Idempotent. */
  #listen(): void {
    if (this.#listening) return;
    this.#listening = true;
    client.subscribe((event) => {
      if (event.type === 'AccountPrefsChanged') this.#adoptIfNewer(parseAccountDoc(event.payload.json));
    });
  }

  /** Takes the synced values of a newer account document. A local change not yet pushed is newer and wins. */
  #adoptIfNewer(remote: AccountDoc | null): boolean {
    if (!remote || remote.updatedAt <= this.#syncedAt) return false;
    this.#values = { ...this.#values, ...remote.values };
    this.#syncedAt = remote.updatedAt;
    this.#accountDirty = false;
    this.#writeLocal();
    return true;
  }

  #changed(synced: boolean): void {
    this.#version++;
    if (synced) {
      this.#accountDirty = true;
      // Strictly increasing, so a change never ties with the last synced one.
      this.#syncedAt = Math.max(Date.now(), this.#syncedAt + 1);
    }
    this.#writeLocal();
    this.#scheduleEngineSave();
  }

  #writeLocal(): void {
    if (!this.#storage) return;
    try {
      this.#storage.setItem(PREFS_STORAGE_KEY, JSON.stringify(this.#values));
      this.#storage.setItem(PREFS_SYNCED_AT_STORAGE_KEY, String(this.#syncedAt));
    } catch (err) {
      console.warn('[prefs] could not persist preferences locally', err);
    }
  }

  #scheduleEngineSave(): void {
    clearTimeout(this.#engineTimer);
    this.#engineTimer = setTimeout(() => {
      this.#engineTimer = undefined;
      void this.#pushToEngine();
      if (this.#accountDirty) void this.#pushToAccount();
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

  /**
   * Stores the synced values with this device's change time. The dirty flag is cleared before the request so
   * a change made meanwhile is not lost; on failure it is set again and the next change or load retries.
   */
  async #pushToAccount(): Promise<void> {
    this.#accountDirty = false;
    const json = JSON.stringify({ ...syncedSubset(this.#values), updated_at: this.#syncedAt });
    try {
      const res = await client.invoke({ action: 'SetAccountPrefs', params: { json } });
      if (res.type !== 'Unit') {
        console.warn('[prefs] unexpected response while saving synced preferences', res.type);
      }
    } catch (err) {
      this.#accountDirty = true;
      console.warn('[prefs] could not save synced preferences to the account', err);
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
