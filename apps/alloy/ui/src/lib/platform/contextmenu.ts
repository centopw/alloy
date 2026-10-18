/**
 * Context-menu triggers: right-click, Shift+F10 / ContextMenu key, and touch long-press.
 * Use as `<div use:contextmenu={() => buildItems()}>`; the factory runs on each trigger so the items
 * always reflect current state.
 */
import { contextMenu, type MenuItem, type MenuPoint } from '$lib/stores/contextmenu.svelte';

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

/** Svelte action: wire a node to the global context menu. The factory returns [] to opt out. */
export function contextmenu(node: HTMLElement, factory: () => MenuItem[]) {
  let getItems = factory;
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
    contextMenu.open(items, { x: e.clientX, y: e.clientY });
  }

  function onKeyDown(e: KeyboardEvent) {
    const isMenuKey = e.key === 'ContextMenu' || (e.shiftKey && e.key === 'F10');
    if (!isMenuKey) return;
    const items = getItems();
    if (items.length === 0) return;
    e.preventDefault();
    e.stopPropagation();
    const r = node.getBoundingClientRect();
    contextMenu.open(items, { x: r.left, y: r.bottom });
  }

  function onPointerDown(e: PointerEvent) {
    if (e.pointerType !== 'touch' || !e.isPrimary) return;
    cancelPress();
    suppressClick = false;
    pressStart = { x: e.clientX, y: e.clientY };
    pressTimer = window.setTimeout(() => {
      pressTimer = null;
      const items = getItems();
      if (items.length === 0) return;
      suppressClick = true;
      contextMenu.open(items, pressStart);
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
    update(next: () => MenuItem[]) {
      getItems = next;
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
