/**
 * Persisted widths for the resizable columns of the main layout. Widths are clamped to fixed
 * limits on every write and on every load, so a stale or hand-edited value cannot break the layout.
 */

export const LAYOUT_STORAGE_KEY = 'alloy.layout.v1';

export const LAYOUT_LIMITS = {
  sidebarWidth: { min: 200, max: 320, default: 240 },
  detailsWidth: { min: 320, max: 420, default: 380 },
} as const;

export type LayoutKey = keyof typeof LAYOUT_LIMITS;

export interface LayoutStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

/** Clamp a width to its limits and round it to whole pixels. Non-finite input yields the default. */
export function clampLayoutWidth(name: LayoutKey, value: number): number {
  const { min, max, default: fallback } = LAYOUT_LIMITS[name];
  if (!Number.isFinite(value)) return fallback;
  return Math.min(max, Math.max(min, Math.round(value)));
}

function defaultStorage(): LayoutStorage | null {
  try {
    return typeof localStorage === 'undefined' ? null : localStorage;
  } catch {
    return null;
  }
}

export class LayoutStore {
  #storage: LayoutStorage | null;

  sidebarWidth = $state<number>(LAYOUT_LIMITS.sidebarWidth.default);
  detailsWidth = $state<number>(LAYOUT_LIMITS.detailsWidth.default);
  /** The room list is hidden but stays mounted, so its scroll position and state survive. */
  sidebarCollapsed = $state(false);

  constructor(storage: LayoutStorage | null = defaultStorage()) {
    this.#storage = storage;
    this.#load();
  }

  /** Set one width, clamped to its limits, and persist the layout. */
  set(name: LayoutKey, value: number): void {
    this[name] = clampLayoutWidth(name, value);
    this.#write();
  }

  /** Show or hide the room list, keeping its width for when it comes back. */
  setSidebarCollapsed(collapsed: boolean): void {
    this.sidebarCollapsed = collapsed;
    this.#write();
  }

  toggleSidebar(): void {
    this.setSidebarCollapsed(!this.sidebarCollapsed);
  }

  /** Restore the default widths and show the room list again; forget the stored layout. */
  reset(): void {
    this.sidebarWidth = LAYOUT_LIMITS.sidebarWidth.default;
    this.detailsWidth = LAYOUT_LIMITS.detailsWidth.default;
    this.sidebarCollapsed = false;
    try {
      this.#storage?.removeItem(LAYOUT_STORAGE_KEY);
    } catch (err) {
      console.warn('[layout] could not clear stored layout', err);
    }
  }

  #load(): void {
    const raw = this.#read();
    if (raw === null) return;
    let parsed: unknown;
    try {
      parsed = JSON.parse(raw);
    } catch {
      console.warn('[layout] ignoring corrupt stored layout');
      return;
    }
    if (typeof parsed !== 'object' || parsed === null) return;
    const data = parsed as Partial<Record<LayoutKey | 'sidebarCollapsed', unknown>>;
    this.sidebarWidth = clampLayoutWidth(
      'sidebarWidth',
      typeof data.sidebarWidth === 'number' ? data.sidebarWidth : Number.NaN,
    );
    this.detailsWidth = clampLayoutWidth(
      'detailsWidth',
      typeof data.detailsWidth === 'number' ? data.detailsWidth : Number.NaN,
    );
    this.sidebarCollapsed = data.sidebarCollapsed === true;
  }

  #read(): string | null {
    try {
      return this.#storage?.getItem(LAYOUT_STORAGE_KEY) ?? null;
    } catch (err) {
      console.warn('[layout] could not read stored layout', err);
      return null;
    }
  }

  #write(): void {
    const data = {
      sidebarWidth: this.sidebarWidth,
      detailsWidth: this.detailsWidth,
      sidebarCollapsed: this.sidebarCollapsed,
    };
    try {
      this.#storage?.setItem(LAYOUT_STORAGE_KEY, JSON.stringify(data));
    } catch (err) {
      console.warn('[layout] could not persist layout', err);
    }
  }
}

export const layout = new LayoutStore();
