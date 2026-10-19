/**
 * Suppresses the OS-native context menu everywhere. One bubble-phase `contextmenu` listener runs
 * after every target handler, so `defaultPrevented` tells us whether a component already opened
 * its own menu through `use:contextmenu`. Unhandled targets get a text-edit, link, or selection
 * menu when applicable, and are otherwise silently suppressed.
 */
import { contextMenu, type MenuItem } from '$lib/stores/contextmenu.svelte';
import { openExternal } from './links';

export type NativeMenuId =
  | 'undo'
  | 'redo'
  | 'cut'
  | 'copy'
  | 'paste'
  | 'selectAll'
  | 'openLink'
  | 'copyLink';

export type NativeMenuEntry = { id: NativeMenuId; enabled: boolean } | { separator: true };

/** What the pointer landed on, reduced to the facts the menu decision needs. */
export interface NativeMenuTarget {
  /** Editable control under the pointer, or null. */
  editable: { readonly: boolean; hasSelection: boolean } | null;
  /** Link under the pointer, or null. */
  link: { href: string } | null;
  /** True when the document has a non-empty text selection. */
  hasTextSelection: boolean;
}

/** Pure menu decision: editable > link > selection > nothing (empty list means no menu). */
export function decideMenu(target: NativeMenuTarget): NativeMenuEntry[] {
  if (target.editable) {
    const { readonly, hasSelection } = target.editable;
    return [
      { id: 'undo', enabled: !readonly },
      { id: 'redo', enabled: !readonly },
      { separator: true },
      { id: 'cut', enabled: !readonly && hasSelection },
      { id: 'copy', enabled: hasSelection },
      { id: 'paste', enabled: !readonly },
      { separator: true },
      { id: 'selectAll', enabled: true },
    ];
  }
  const entries: NativeMenuEntry[] = [];
  if (target.link) {
    entries.push({ id: 'openLink', enabled: true }, { id: 'copyLink', enabled: true });
  }
  if (target.hasTextSelection) {
    if (entries.length > 0) entries.push({ separator: true });
    entries.push({ id: 'copy', enabled: true });
  }
  return entries;
}

const LABELS: Record<NativeMenuId, string> = {
  undo: 'Undo',
  redo: 'Redo',
  cut: 'Cut',
  copy: 'Copy',
  paste: 'Paste',
  selectAll: 'Select all',
  openLink: 'Open link',
  copyLink: 'Copy link address',
};

type TextControl = HTMLInputElement | HTMLTextAreaElement;

/** Input types that hold no editable text, so they get no text-edit menu. */
const NON_TEXT_INPUT_TYPES: Record<string, true> = {
  checkbox: true,
  radio: true,
  range: true,
  file: true,
  button: true,
  submit: true,
  reset: true,
  image: true,
  color: true,
  hidden: true,
};

function isTextControl(el: Element): el is TextControl {
  if (el instanceof HTMLTextAreaElement) return true;
  return el instanceof HTMLInputElement && NON_TEXT_INPUT_TYPES[el.type] !== true;
}

/** The editable element owning `el`, or null. Covers text controls and contenteditable hosts. */
function editableOf(el: Element): TextControl | HTMLElement | null {
  const host = el.closest('input, textarea, [contenteditable]');
  if (!host) return null;
  if (host instanceof HTMLInputElement || host instanceof HTMLTextAreaElement) {
    return isTextControl(host) ? host : null;
  }
  return host.getAttribute('contenteditable') === 'false' ? null : (host as HTMLElement);
}

function elementOf(target: EventTarget | null): Element | null {
  if (target instanceof Element) return target;
  if (target instanceof Node) return target.parentElement;
  return null;
}

/** Selection captured at open time. Menu clicks move focus, so it is restored before acting. */
interface EditSnapshot {
  el: TextControl | HTMLElement;
  start: number;
  end: number;
  range: Range | null;
  text: string;
}

interface Captured {
  target: NativeMenuTarget;
  edit: EditSnapshot | null;
  href: string | null;
  selectedText: string;
}

function snapshotEditable(el: TextControl | HTMLElement): EditSnapshot {
  if (isTextControl(el)) {
    const start = el.selectionStart ?? el.value.length;
    const end = el.selectionEnd ?? start;
    return { el, start, end, range: null, text: el.value.slice(start, end) };
  }
  const sel = window.getSelection();
  const live = sel && sel.rangeCount > 0 ? sel.getRangeAt(0) : null;
  const range = live && el.contains(live.commonAncestorContainer) ? live.cloneRange() : null;
  return { el, start: 0, end: 0, range, text: range?.toString() ?? '' };
}

function capture(target: EventTarget | null): Captured {
  const el = elementOf(target);
  const control = el ? editableOf(el) : null;
  const edit = control ? snapshotEditable(control) : null;
  const anchor = el?.closest('a[href]') ?? null;
  const href = anchor instanceof HTMLAnchorElement ? anchor.href : null;
  const selectedText = window.getSelection()?.toString() ?? '';
  return {
    target: {
      editable: control && edit ? { readonly: isTextControl(control) && (control.readOnly || control.disabled), hasSelection: edit.text.length > 0 } : null,
      link: href ? { href } : null,
      hasTextSelection: selectedText.length > 0,
    },
    edit,
    href,
    selectedText,
  };
}

async function writeClipboard(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
    return;
  } catch {
    // Clipboard API unavailable or denied: fall back to execCommand below.
  }
  const area = document.createElement('textarea');
  area.value = text;
  area.setAttribute('readonly', '');
  area.style.position = 'fixed';
  area.style.opacity = '0';
  document.body.append(area);
  area.select();
  document.execCommand('copy');
  area.remove();
}

async function readClipboard(): Promise<string | null> {
  try {
    return await navigator.clipboard.readText();
  } catch {
    return null;
  }
}

function restoreSelection(s: EditSnapshot): void {
  const el = s.el;
  el.focus();
  if (isTextControl(el)) {
    try {
      el.setSelectionRange(s.start, s.end);
    } catch {
      // Some input types do not support selection; the value edit below still applies.
    }
    return;
  }
  const sel = window.getSelection();
  if (sel && s.range) {
    sel.removeAllRanges();
    sel.addRange(s.range);
  }
}

/** Replace the snapshot's selection with `text`, notifying Svelte `bind:value` listeners. */
function replaceSelection(s: EditSnapshot, text: string): void {
  restoreSelection(s);
  const el = s.el;
  if (isTextControl(el)) {
    el.value = el.value.slice(0, s.start) + text + el.value.slice(s.end);
    el.dispatchEvent(new Event('input', { bubbles: true }));
    const caret = s.start + text.length;
    try {
      el.setSelectionRange(caret, caret);
    } catch {
      // Selection is not supported for this input type; caret placement is best-effort.
    }
    return;
  }
  document.execCommand('insertText', false, text);
}

function selectAll(s: EditSnapshot): void {
  const el = s.el;
  el.focus();
  if (isTextControl(el)) {
    el.select();
    return;
  }
  const range = document.createRange();
  range.selectNodeContents(el);
  const sel = window.getSelection();
  sel?.removeAllRanges();
  sel?.addRange(range);
}

async function paste(s: EditSnapshot): Promise<void> {
  const text = await readClipboard();
  if (text === null) {
    restoreSelection(s);
    document.execCommand('paste');
    return;
  }
  if (text) replaceSelection(s, text);
}

function run(id: NativeMenuId, c: Captured): void {
  switch (id) {
    case 'openLink':
      if (c.href) void openExternal(c.href);
      return;
    case 'copyLink':
      if (c.href) void writeClipboard(c.href);
      return;
    case 'copy':
      void writeClipboard(c.edit ? c.edit.text : c.selectedText);
      return;
    case 'cut':
      if (c.edit && c.edit.text) {
        void writeClipboard(c.edit.text);
        replaceSelection(c.edit, '');
      }
      return;
    case 'paste':
      if (c.edit) void paste(c.edit);
      return;
    case 'undo':
    case 'redo':
      if (c.edit) {
        restoreSelection(c.edit);
        document.execCommand(id);
      }
      return;
    case 'selectAll':
      if (c.edit) selectAll(c.edit);
      return;
  }
}

function itemsFor(entries: NativeMenuEntry[], c: Captured): MenuItem[] {
  return entries.map((entry): MenuItem => {
    if ('separator' in entry) return { separator: true };
    return {
      id: entry.id,
      label: LABELS[entry.id],
      disabled: !entry.enabled,
      onselect: () => run(entry.id, c),
    };
  });
}

/**
 * Install the single app-wide guard. Returns an uninstall function.
 * Targets that already opened a menu (`defaultPrevented`) are left alone.
 */
export function installNativeMenuGuard(root: EventTarget = document): () => void {
  const onContextMenu = (event: Event): void => {
    if (event.defaultPrevented) return;
    event.preventDefault();
    const captured = capture(event.target);
    const items = itemsFor(decideMenu(captured.target), captured);
    if (items.length === 0) return;
    const pointer = event as MouseEvent;
    contextMenu.open(items, { x: pointer.clientX, y: pointer.clientY });
  };
  root.addEventListener('contextmenu', onContextMenu);
  return () => root.removeEventListener('contextmenu', onContextMenu);
}
