/**
 * Context-menu triggers: right-click, Shift+F10 / ContextMenu key, and touch long-press.
 * Use as `<div use:contextmenu={() => buildItems()}>`; the factory runs on each trigger so the items
 * always reflect current state.
 */
import { contextMenu, type MenuAnchor, type MenuItem, type MenuOptions, type MenuPoint } from '$lib/stores/contextmenu.svelte';

const LONG_PRESS_MS = 500;
const LONG_PRESS_SLOP_PX = 8;

export interface MenuSize {
  width: number;
  height: number;
}

export interface MenuViewport {
  width: number;
  height: number;
}

/**
 * Place a menu of `size` at the pointer `at`: open to the right and down by default, flip to the
 * left or up when it would overflow, then clamp so it stays inside the viewport with `margin`.
 */
export function placeMenu(
  at: MenuPoint,
  size: MenuSize,
  viewport: MenuViewport,
  margin = 8,
): MenuPoint {
  let x = at.x;
  let y = at.y;
  if (x + size.width > viewport.width - margin) x = at.x - size.width;
  if (y + size.height > viewport.height - margin) y = at.y - size.height;
  x = Math.max(margin, Math.min(x, viewport.width - size.width - margin));
  y = Math.max(margin, Math.min(y, viewport.height - size.height - margin));
  return { x, y };
}

/**
 * Place a menu of `size` relative to `anchor`: open directly below by default, matching its width
 * when requested, flip above when it overflows bottom, and clamp inside the viewport with `margin`.
 */
export function placeAnchor(
  anchor: MenuAnchor,
  size: MenuSize,
  viewport: MenuViewport,
  margin = 8,
  gap = 4,
): MenuPoint {
  let x = anchor.x;
  if (x + size.width > viewport.width - margin) {
    x = anchor.x + anchor.width - size.width;
  }
  x = Math.max(margin, Math.min(x, viewport.width - size.width - margin));

  let y = anchor.y + anchor.height + gap;
  if (y + size.height > viewport.height - margin) {
    const spaceAbove = anchor.y - gap - margin;
    const spaceBelow = viewport.height - margin - (anchor.y + anchor.height + gap);
    if (spaceAbove >= size.height || spaceAbove > spaceBelow) {
      y = Math.max(margin, anchor.y - size.height - gap);
    }
  }
  return { x, y };
}

/**
 * Place a flyout of `size` beside its parent `row`: to the right by default (the gap clears the parent
 * border), flipping left when it would overflow, and clamped to the viewport with `margin`. Vertically it
 * starts at the row's top edge.
 */
export function placeFlyout(
  row: MenuAnchor,
  size: MenuSize,
  viewport: MenuViewport,
  margin = 8,
  gap = 8,
): MenuPoint {
  let x = row.x + row.width + gap;
  if (x + size.width > viewport.width - margin) x = row.x - gap - size.width;
  x = Math.max(margin, Math.min(x, viewport.width - size.width - margin));
  const y = Math.max(margin, Math.min(row.y, viewport.height - size.height - margin));
  return { x, y };
}

/**
 * Place the floating phone reaction bar of `size` beside the pressed message `anchor`: centred above
 * it, flipping below when there is no room, and kept between `limits.top` and `limits.bottom` (the
 * top of the action sheet) so it never covers the sheet or the status bar.
 */
export function placeReactionBar(
  anchor: MenuAnchor,
  size: MenuSize,
  viewport: MenuViewport,
  limits: { top: number; bottom: number },
  margin = 8,
  gap = 8,
): MenuPoint {
  const maxX = viewport.width - size.width - margin;
  const x = maxX < margin ? margin : Math.max(margin, Math.min(anchor.x + anchor.width / 2 - size.width / 2, maxX));
  let y = anchor.y - size.height - gap;
  if (y < limits.top) y = anchor.y + anchor.height + gap;
  y = Math.max(limits.top, Math.min(y, limits.bottom - size.height - gap));
  return { x, y };
}

/** What a `use:contextmenu` element offers: rows, optionally with menu options (the reaction bar). */
export interface ContextMenuSource {
  items: () => MenuItem[];
  options?: () => MenuOptions | undefined;
}

/** Touch presses already claimed by an inner `use:contextmenu` node; outer nodes ignore them. */
const claimedPresses = new WeakSet<Event>();

/** Svelte action: wire a node to the global context menu. The factory returns [] to opt out. */
export function contextmenu(node: HTMLElement, factory: (() => MenuItem[]) | ContextMenuSource) {
  let source = factory;
  const getItems = () => (typeof source === 'function' ? source() : source.items());
  const getOptions = () => (typeof source === 'function' ? undefined : source.options?.());
  let pressTimer: number | null = null;
  let pressStart: MenuPoint = { x: 0, y: 0 };
  let suppressClick = false;

  function cancelPress() {
    if (pressTimer !== null) {
      window.clearTimeout(pressTimer);
      pressTimer = null;
    }
  }

  function onContextMenu(e: MouseEvent) {
    const items = getItems();
    if (items.length === 0) return;
    e.preventDefault();
    e.stopPropagation();
    contextMenu.open(items, { x: e.clientX, y: e.clientY }, getOptions());
  }

  function onKeyDown(e: KeyboardEvent) {
    const isMenuKey = e.key === 'ContextMenu' || (e.shiftKey && e.key === 'F10');
    if (!isMenuKey) return;
    const items = getItems();
    if (items.length === 0) return;
    e.preventDefault();
    e.stopPropagation();
    const r = node.getBoundingClientRect();
    contextMenu.open(items, { x: r.left, y: r.bottom }, getOptions());
  }

  function onPointerDown(e: PointerEvent) {
    if (e.pointerType !== 'touch' || !e.isPrimary || claimedPresses.has(e)) return;
    if (getItems().length === 0) return;
    claimedPresses.add(e);
    cancelPress();
    suppressClick = false;
    pressStart = { x: e.clientX, y: e.clientY };
    pressTimer = window.setTimeout(() => {
      pressTimer = null;
      const items = getItems();
      if (items.length === 0) return;
      suppressClick = true;
      contextMenu.open(items, pressStart, getOptions());
    }, LONG_PRESS_MS);
  }

  function onPointerMove(e: PointerEvent) {
    if (pressTimer === null) return;
    const dx = e.clientX - pressStart.x;
    const dy = e.clientY - pressStart.y;
    if (Math.hypot(dx, dy) > LONG_PRESS_SLOP_PX) cancelPress();
  }

  function onClickCapture(e: MouseEvent) {
    if (!suppressClick) return;
    suppressClick = false;
    e.preventDefault();
    e.stopPropagation();
  }

  node.addEventListener('contextmenu', onContextMenu);
  node.addEventListener('keydown', onKeyDown);
  node.addEventListener('pointerdown', onPointerDown);
  node.addEventListener('pointermove', onPointerMove);
  node.addEventListener('pointerup', cancelPress);
  node.addEventListener('pointercancel', cancelPress);
  node.addEventListener('click', onClickCapture, true);

  return {
    update(next: (() => MenuItem[]) | ContextMenuSource) {
      source = next;
    },
    destroy() {
      cancelPress();
      node.removeEventListener('contextmenu', onContextMenu);
      node.removeEventListener('keydown', onKeyDown);
      node.removeEventListener('pointerdown', onPointerDown);
      node.removeEventListener('pointermove', onPointerMove);
      node.removeEventListener('pointerup', cancelPress);
      node.removeEventListener('pointercancel', cancelPress);
      node.removeEventListener('click', onClickCapture, true);
    },
  };
}
