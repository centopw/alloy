import { afterEach, describe, expect, it } from 'vitest';
import {
  CHAT_ENTRY_ATTR,
  escapeAttributeValue,
  focusChatEntry,
  focusRoomList,
  focusRoomRow,
  focusSkipTarget,
  roomRowSelector,
} from './shellFocus';

function host(html: string): HTMLElement {
  const el = document.createElement('div');
  el.innerHTML = html;
  document.body.appendChild(el);
  return el;
}

afterEach(() => {
  document.body.innerHTML = '';
});

describe('roomRowSelector', () => {
  it('quotes the room id so Matrix ids stay selector-safe', () => {
    expect(roomRowSelector('!room:matrix.org')).toBe('[data-room-id="!room:matrix.org"]');
  });

  it('escapes quotes and backslashes', () => {
    expect(escapeAttributeValue('a"b')).toBe('a\\"b');
    expect(escapeAttributeValue('a\\b')).toBe('a\\\\b');
    expect(roomRowSelector('!x"y')).toBe('[data-room-id="!x\\"y"]');
  });
});

describe('focusChatEntry', () => {
  it('focuses the room heading', () => {
    host(`<h1 ${CHAT_ENTRY_ATTR} tabindex="-1">Design</h1>`);
    expect(focusChatEntry()).toBe(true);
    expect(document.activeElement?.tagName).toBe('H1');
  });

  it('reports false when no heading is rendered', () => {
    host('<p>Nothing here</p>');
    expect(focusChatEntry()).toBe(false);
  });
});

describe('focusRoomRow', () => {
  it('focuses the row with that room id, not the row with a similar id', () => {
    host(`
      <div class="groups">
        <button data-room-id="!a:example.org">A</button>
        <button data-room-id="!a:example.org.old">old A</button>
      </div>
    `);
    expect(focusRoomRow('!a:example.org')).toBe(true);
    expect(document.activeElement?.getAttribute('data-room-id')).toBe('!a:example.org');
  });

  it('reports false when the room is no longer listed', () => {
    host('<div class="groups"><button data-room-id="!b:example.org">B</button></div>');
    expect(focusRoomRow('!a:example.org')).toBe(false);
  });
});

describe('focusRoomList', () => {
  it('focuses the open room row inside the list', () => {
    host(`
      <div id="room-list">
        <button data-room-id="!a:example.org">A</button>
        <button data-room-id="!b:example.org" aria-current="true" tabindex="0">B</button>
      </div>
    `);
    expect(focusRoomList()).toBe(true);
    expect(document.activeElement?.getAttribute('data-room-id')).toBe('!b:example.org');
  });

  it('falls back to the list container when the list has no rooms', () => {
    host('<div id="room-list"><p>Nothing here yet</p></div>');
    expect(focusRoomList()).toBe(true);
    expect(document.activeElement?.id).toBe('room-list');
    expect(document.getElementById('room-list')?.getAttribute('tabindex')).toBe('-1');
  });

  it('reports false without a room list', () => {
    expect(focusRoomList()).toBe(false);
  });
});

describe('focusSkipTarget', () => {
  it('makes a landmark programmatically focusable and focuses it', () => {
    host('<main id="conversation"><p>messages</p></main>');
    expect(focusSkipTarget('conversation')).toBe(true);
    expect(document.activeElement?.id).toBe('conversation');
    expect(document.getElementById('conversation')?.getAttribute('tabindex')).toBe('-1');
  });

  it('keeps an existing tabindex', () => {
    host('<div id="target" tabindex="0"></div>');
    expect(focusSkipTarget('target')).toBe(true);
    expect(document.getElementById('target')?.getAttribute('tabindex')).toBe('0');
  });

  it('reports false for a missing target', () => {
    expect(focusSkipTarget('missing')).toBe(false);
  });
});
