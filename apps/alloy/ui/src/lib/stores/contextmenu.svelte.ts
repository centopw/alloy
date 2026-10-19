/**
 * Single app-wide context menu. Mounted once (ContextMenu.svelte) in the root layout; anything
 * opens it through `contextMenu.open(items, at)` or the `contextmenu` action in platform/contextmenu.
 */
import type { Component } from 'svelte';

export type IconComponent = Component<{ size?: number; strokeWidth?: number; class?: string }>;

export interface MenuAction {
  id: string;
  label: string;
  icon?: IconComponent;
  /** Display-only hint, e.g. "Ctrl+C". */
  shortcut?: string;
  danger?: boolean;
  disabled?: boolean;
  /** Checkmark state. When defined the row is a menuitemradio with aria-checked and a Check icon. */
  selected?: boolean;
  separator?: false;
  onselect?: () => void;
  /** Opens a flyout from this row instead of selecting. A row with a submenu has no onselect. */
  submenu?: MenuSubmenu;
  /** Selecting runs onselect but keeps the menu open (toggles). Function-based submenu items are re-resolved. */
  keepOpen?: boolean;
}

export interface MenuAnchor {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** A bar of quick reactions shown above the rows of a message menu. */
export interface MenuReactions {
  emojis: readonly string[];
  /** Emoji the user already reacted with. */
  active?: readonly string[];
  onpick: (emoji: string) => void;
  /** Open the full emoji picker. */
  onmore: () => void;
  /** Rect of the pressed message; the phone bar floats beside it. */
  anchor?: MenuAnchor;
}

export interface MenuOptions {
  reactions?: MenuReactions;
  anchor?: MenuAnchor;
  matchWidth?: boolean;
  minWidth?: number;
  ariaLabel?: string;
  onclose?: () => void;
}
export interface MenuSeparator {
  separator: true;
}

export type MenuItem = MenuAction | MenuSeparator;

export interface MenuSearchCreate {
  label: (query: string) => string;
  onCreate: (query: string) => void;
}

export interface MenuSubmenuSearch {
  placeholder: string;
  create?: MenuSearchCreate;
}

export interface MenuSubmenu {
  items: MenuItem[] | (() => MenuItem[]);
  /** Adds a search field at the top of the flyout; typing filters rows by label (case-insensitive). */
  search?: MenuSubmenuSearch;
  /** Max flyout height in px before it scrolls (default 320). */
  maxHeight?: number;
}

export interface MenuPoint {
  x: number;
  y: number;
}

export function isSeparator(item: MenuItem): item is MenuSeparator {
  return item.separator === true;
}

export type SubmenuAction = MenuAction & { submenu: MenuSubmenu };

export function isSubmenuAction(item: MenuItem): item is SubmenuAction {
  return !isSeparator(item) && item.submenu !== undefined;
}

export function resolveSubmenuItems(submenu: MenuSubmenu): MenuItem[] {
  return typeof submenu.items === 'function' ? submenu.items() : submenu.items;
}

export interface SubmenuView {
  /** Rows after filtering. Separators are kept only when the query is empty. */
  items: MenuItem[];
  /** Trimmed query the rows were filtered with. */
  query: string;
  /** Whether the create row is offered: a non-empty query, no exact label match, and a create action. */
  showCreate: boolean;
}

/**
 * Filter submenu `items` by `query` (trimmed, case-insensitive substring of the label). The create row
 * is offered only when `allowCreate` is true, the trimmed query is non-empty, and no label equals it.
 */
export function submenuView(items: MenuItem[], query: string, allowCreate: boolean): SubmenuView {
  const q = query.trim();
  if (q === '') return { items, query: q, showCreate: false };
  const needle = q.toLowerCase();
  const filtered = items.filter(
    (item): item is MenuAction => !isSeparator(item) && item.label.toLowerCase().includes(needle),
  );
  const exact = items.some((item) => !isSeparator(item) && item.label.trim().toLowerCase() === needle);
  return { items: filtered, query: q, showCreate: allowCreate && !exact };
}

class ContextMenuStore {
  items = $state.raw<MenuItem[]>([]);
  at = $state<MenuPoint>({ x: 0, y: 0 });
  options = $state.raw<MenuOptions | undefined>(undefined);
  isOpen = $state(false);
  /** Element to refocus when the menu is dismissed from the keyboard. Not reactive by design. */
  returnFocus: HTMLElement | null = null;

  /**
   * Open the menu with `items` at viewport coordinates `at`. Opening with no items closes the menu.
   * The focused element at the first open is remembered so keyboard dismissal can restore it.
   */
  open(items: MenuItem[], at: MenuPoint, options?: MenuOptions): void {
    if (items.length === 0) {
      this.close();
      return;
    }
    if (!this.isOpen) {
      const active = document.activeElement;
      this.returnFocus = active instanceof HTMLElement && active !== document.body ? active : null;
    }
    this.items = items;
    this.at = { x: at.x, y: at.y };
    this.options = options;
    this.isOpen = true;
  }

  /** Close without moving focus (pointer dismissal, blur, scroll, resize). */
  close(): void {
    const onclose = this.options?.onclose;
    this.items = [];
    this.options = undefined;
    this.isOpen = false;
    this.returnFocus = null;
    onclose?.();
  }

  /** Close and restore focus to the element that was focused before opening (keyboard dismissal). */
  dismiss(): void {
    const target = this.returnFocus;
    this.close();
    target?.focus?.();
  }
}

export const contextMenu = new ContextMenuStore();
