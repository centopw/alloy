import { describe, expect, it, vi } from 'vitest';
import {
  clampLayoutWidth,
  LAYOUT_LIMITS,
  LAYOUT_STORAGE_KEY,
  LayoutStore,
  type LayoutStorage,
} from './layout.svelte';

function memoryStorage(initial: Record<string, string> = {}): LayoutStorage & {
  data: Map<string, string>;
} {
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

describe('clampLayoutWidth', () => {
  it('clamps to limits and rounds to whole pixels', () => {
    expect(clampLayoutWidth('sidebarWidth', 10)).toBe(LAYOUT_LIMITS.sidebarWidth.min);
    expect(clampLayoutWidth('sidebarWidth', 9999)).toBe(LAYOUT_LIMITS.sidebarWidth.max);
    expect(clampLayoutWidth('detailsWidth', 380.6)).toBe(381);
  });

  it('falls back to the default for non-finite values', () => {
    expect(clampLayoutWidth('detailsWidth', Number.NaN)).toBe(LAYOUT_LIMITS.detailsWidth.default);
    expect(clampLayoutWidth('detailsWidth', Number.POSITIVE_INFINITY)).toBe(
      LAYOUT_LIMITS.detailsWidth.default,
    );
  });
});

describe('LayoutStore', () => {
  it('starts from defaults when nothing is stored', () => {
    const store = new LayoutStore(memoryStorage());
    expect(store.sidebarWidth).toBe(LAYOUT_LIMITS.sidebarWidth.default);
    expect(store.detailsWidth).toBe(LAYOUT_LIMITS.detailsWidth.default);
    expect(store.sidebarCollapsed).toBe(false);
  });

  it('persists the collapsed sidebar and restores it', () => {
    const storage = memoryStorage();
    const store = new LayoutStore(storage);
    store.setSidebarCollapsed(true);
    expect(store.sidebarCollapsed).toBe(true);

    const reloaded = new LayoutStore(storage);
    expect(reloaded.sidebarCollapsed).toBe(true);
    expect(reloaded.sidebarWidth).toBe(LAYOUT_LIMITS.sidebarWidth.default);
  });

  it('toggles the sidebar', () => {
    const store = new LayoutStore(memoryStorage());
    store.toggleSidebar();
    expect(store.sidebarCollapsed).toBe(true);
    store.toggleSidebar();
    expect(store.sidebarCollapsed).toBe(false);
  });

  it('treats a non-boolean stored collapse value as expanded', () => {
    const storage = memoryStorage({
      [LAYOUT_STORAGE_KEY]: JSON.stringify({ sidebarCollapsed: 'yes' }),
    });
    expect(new LayoutStore(storage).sidebarCollapsed).toBe(false);
  });

  it('clamps and persists widths, and a new store reads them back', () => {
    const storage = memoryStorage();
    const store = new LayoutStore(storage);
    store.set('sidebarWidth', 9999);
    expect(store.sidebarWidth).toBe(LAYOUT_LIMITS.sidebarWidth.max);

    const reloaded = new LayoutStore(storage);
    expect(reloaded.sidebarWidth).toBe(LAYOUT_LIMITS.sidebarWidth.max);
    expect(reloaded.detailsWidth).toBe(LAYOUT_LIMITS.detailsWidth.default);
    expect(storage.data.has(LAYOUT_STORAGE_KEY)).toBe(true);
  });

  it('clamps out-of-range stored values on load', () => {
    const storage = memoryStorage({
      [LAYOUT_STORAGE_KEY]: JSON.stringify({ sidebarWidth: 1, detailsWidth: 99999 }),
    });
    const store = new LayoutStore(storage);
    expect(store.sidebarWidth).toBe(LAYOUT_LIMITS.sidebarWidth.min);
    expect(store.detailsWidth).toBe(LAYOUT_LIMITS.detailsWidth.max);
  });

  it('uses defaults for stored values that are not numbers', () => {
    const storage = memoryStorage({
      [LAYOUT_STORAGE_KEY]: JSON.stringify({ sidebarWidth: 'wide', detailsWidth: null }),
    });
    const store = new LayoutStore(storage);
    expect(store.sidebarWidth).toBe(LAYOUT_LIMITS.sidebarWidth.default);
    expect(store.detailsWidth).toBe(LAYOUT_LIMITS.detailsWidth.default);
  });

  it('ignores corrupt JSON and warns', () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    const storage = memoryStorage({ [LAYOUT_STORAGE_KEY]: '{not json' });
    const store = new LayoutStore(storage);
    expect(store.sidebarWidth).toBe(LAYOUT_LIMITS.sidebarWidth.default);
    expect(warn).toHaveBeenCalled();
    warn.mockRestore();
  });

  it('reset restores defaults and removes the stored layout', () => {
    const storage = memoryStorage();
    const store = new LayoutStore(storage);
    store.set('detailsWidth', 500);
    store.setSidebarCollapsed(true);
    store.reset();
    expect(store.detailsWidth).toBe(LAYOUT_LIMITS.detailsWidth.default);
    expect(store.sidebarCollapsed).toBe(false);
    expect(storage.data.has(LAYOUT_STORAGE_KEY)).toBe(false);
  });

  it('keeps working in memory when storage throws', () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    const broken: LayoutStorage = {
      getItem: () => {
        throw new Error('denied');
      },
      setItem: () => {
        throw new Error('quota');
      },
      removeItem: () => {
        throw new Error('denied');
      },
    };
    const store = new LayoutStore(broken);
    expect(store.sidebarWidth).toBe(LAYOUT_LIMITS.sidebarWidth.default);
    expect(() => store.set('sidebarWidth', 300)).not.toThrow();
    expect(store.sidebarWidth).toBe(300);
    expect(() => store.reset()).not.toThrow();
    warn.mockRestore();
  });

  it('works without storage at all', () => {
    const store = new LayoutStore(null);
    store.set('sidebarWidth', 260);
    expect(store.sidebarWidth).toBe(260);
  });
});
