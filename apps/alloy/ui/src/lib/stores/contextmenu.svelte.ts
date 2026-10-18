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
  separator?: false;
  onselect?: () => void;
}

export interface MenuSeparator {
  separator: true;
}

export type MenuItem = MenuAction | MenuSeparator;

export interface MenuPoint {
  x: number;
  y: number;
}

export function isSeparator(item: MenuItem): item is MenuSeparator {
  return item.separator === true;
}

class ContextMenuStore {
  items = $state.raw<MenuItem[]>([]);
  at = $state<MenuPoint>({ x: 0, y: 0 });
  isOpen = $state(false);
  /** Element to refocus when the menu is dismissed from the keyboard. Not reactive by design. */
  returnFocus: HTMLElement | null = null;

  /**
   * Open the menu with `items` at viewport coordinates `at`. Opening with no items closes the menu.
   * The focused element at the first open is remembered so keyboard dismissal can restore it.
   */
  open(items: MenuItem[], at: MenuPoint): void {
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
    this.isOpen = true;
  }

  /** Close without moving focus (pointer dismissal, blur, scroll, resize). */
  close(): void {
    this.items = [];
    this.isOpen = false;
    this.returnFocus = null;
  }

  /** Close and restore focus to the element that was focused before opening (keyboard dismissal). */
  dismiss(): void {
    const target = this.returnFocus;
    this.close();
    target?.focus?.();
  }
}

export const contextMenu = new ContextMenuStore();
