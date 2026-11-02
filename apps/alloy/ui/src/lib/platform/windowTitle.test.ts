import { describe, expect, it } from 'vitest';
import { APP_NAME, applyWindowTitle, buildWindowTitle, unreadPrefix } from './windowTitle';

describe('buildWindowTitle', () => {
  it('names the app when no room is open', () => {
    expect(buildWindowTitle()).toBe('Alloy');
    expect(buildWindowTitle({ roomName: null })).toBe(APP_NAME);
    expect(buildWindowTitle({ roomName: '   ' })).toBe(APP_NAME);
  });

  it('puts the open room in front of the app name', () => {
    expect(buildWindowTitle({ roomName: 'Design' })).toBe('Design - Alloy');
    expect(buildWindowTitle({ roomName: '  Design  ' })).toBe('Design - Alloy');
  });

  it('hides unread counts while the window is focused', () => {
    expect(buildWindowTitle({ roomName: 'Design', unreadCount: 3, focused: true })).toBe(
      'Design - Alloy'
    );
    expect(buildWindowTitle({ roomName: 'Design', unreadCount: 3 })).toBe('Design - Alloy');
  });

  it('shows the unread count while the window is in the background', () => {
    expect(buildWindowTitle({ roomName: 'Design', unreadCount: 3, focused: false })).toBe(
      '(3) Design - Alloy'
    );
    expect(buildWindowTitle({ unreadCount: 2, focused: false })).toBe('(2) Alloy');
  });

  it('omits a zero, negative or non-numeric unread count', () => {
    for (const unreadCount of [0, -4, Number.NaN, Number.POSITIVE_INFINITY]) {
      expect(buildWindowTitle({ roomName: 'Design', unreadCount, focused: false })).toBe(
        'Design - Alloy'
      );
    }
  });

  it('counts whole messages', () => {
    expect(buildWindowTitle({ roomName: 'Design', unreadCount: 2.9, focused: false })).toBe(
      '(2) Design - Alloy'
    );
  });

  it('uses the passed app name', () => {
    expect(buildWindowTitle({ roomName: 'Design', appName: 'Alloy Test' })).toBe(
      'Design - Alloy Test'
    );
    expect(buildWindowTitle({ appName: '  ' })).toBe(APP_NAME);
  });
});

describe('unreadPrefix', () => {
  it('is empty while focused', () => {
    expect(unreadPrefix(3, true)).toBe('');
    expect(unreadPrefix(3, undefined)).toBe('');
  });

  it('is a parenthesised count while backgrounded', () => {
    expect(unreadPrefix(3, false)).toBe('(3) ');
    expect(unreadPrefix(0, false)).toBe('');
  });
});

describe('applyWindowTitle', () => {
  it('sets and returns the title on a document-like target', () => {
    const target = { title: 'Alloy' };
    expect(applyWindowTitle({ roomName: 'Design', unreadCount: 3, focused: false }, target)).toBe(
      '(3) Design - Alloy'
    );
    expect(target.title).toBe('(3) Design - Alloy');
  });
});
