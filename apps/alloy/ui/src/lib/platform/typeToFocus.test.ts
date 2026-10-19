import { describe, expect, it } from 'vitest';
import { focusComposer, shouldFocusComposer } from './typeToFocus';

function press(key: string, init: KeyboardEventInit = {}, target?: Element): KeyboardEvent {
  const e = new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true, ...init });
  if (target) Object.defineProperty(e, 'target', { value: target });
  return e;
}

function el(html: string): Element {
  const host = document.createElement('div');
  host.innerHTML = html;
  document.body.appendChild(host);
  return host.firstElementChild!;
}

describe('shouldFocusComposer', () => {
  it('redirects a printable key pressed on the page background', () => {
    expect(shouldFocusComposer(press('h', {}, document.body))).toBe(true);
  });

  it('leaves shortcuts, navigation keys and IME input alone', () => {
    expect(shouldFocusComposer(press('k', { metaKey: true }, document.body))).toBe(false);
    expect(shouldFocusComposer(press('c', { ctrlKey: true }, document.body))).toBe(false);
    expect(shouldFocusComposer(press('ArrowDown', {}, document.body))).toBe(false);
    expect(shouldFocusComposer(press('Enter', {}, document.body))).toBe(false);
    expect(shouldFocusComposer(press('a', { isComposing: true }, document.body))).toBe(false);
  });

  it('does not steal typing from another field or an open dialog or menu', () => {
    expect(shouldFocusComposer(press('a', {}, el('<input />')))).toBe(false);
    expect(shouldFocusComposer(press('a', {}, el('<div contenteditable="true"></div>')))).toBe(false);
    expect(shouldFocusComposer(press('a', {}, el('<div role="dialog"><p id="x">x</p></div>').querySelector('#x')!))).toBe(false);
    expect(shouldFocusComposer(press('a', {}, el('<div role="menu"><button>one</button></div>').querySelector('button')!))).toBe(false);
  });

  it('keeps Space for buttons but lets letters through', () => {
    const button = el('<button>Go</button>');
    expect(shouldFocusComposer(press(' ', {}, button))).toBe(false);
    expect(shouldFocusComposer(press('g', {}, button))).toBe(true);
  });
});

describe('focusComposer', () => {
  it('focuses the composer field and reports it', () => {
    const field = el('<textarea data-composer-field></textarea>') as HTMLTextAreaElement;
    expect(focusComposer()).toBe(true);
    expect(document.activeElement).toBe(field);
  });
});
