import { afterEach, describe, it, expect, vi } from 'vitest';
import {
  parsePushState,
  extractOpenRoomId,
  setPushPreview,
  UNSUPPORTED_PUSH_STATE,
  type PushState,
} from './push';

describe('setPushPreview', () => {
  afterEach(() => {
    delete window.AlloyPush;
  });

  it('hands the preference to the native bridge', () => {
    const setPreview = vi.fn();
    window.AlloyPush = { state: () => '{}', enable() {}, disable() {}, pick() {}, setPreview };
    setPushPreview(false);
    expect(setPreview).toHaveBeenCalledExactlyOnceWith(false);
  });

  it('does nothing where there is no bridge', () => {
    expect(() => setPushPreview(true)).not.toThrow();
  });
});

describe('parsePushState', () => {
  it('parses a valid push state JSON string', () => {
    const json = JSON.stringify({
      supported: true,
      enabled: true,
      registered: true,
      distributor: 'ntfy',
      distributors: ['ntfy', 'fcm'],
      endpoint: 'https://push.example.com/test',
      gateway: 'https://gateway.example.com',
      permission: 'granted',
      error: null,
    });

    const state = parsePushState(json);

    expect(state.supported).toBe(true);
    expect(state.enabled).toBe(true);
    expect(state.registered).toBe(true);
    expect(state.distributor).toBe('ntfy');
    expect(state.distributors).toEqual(['ntfy', 'fcm']);
    expect(state.endpoint).toBe('https://push.example.com/test');
    expect(state.gateway).toBe('https://gateway.example.com');
    expect(state.permission).toBe('granted');
    expect(state.error).toBe(null);
  });

  it('parses a raw object (not JSON string)', () => {
    const obj: Partial<PushState> = {
      supported: true,
      enabled: false,
      registered: false,
      distributor: null,
      distributors: [],
      endpoint: null,
      gateway: null,
      permission: 'unknown',
      error: 'test error',
    };

    const state = parsePushState(obj);

    expect(state.enabled).toBe(false);
    expect(state.error).toBe('test error');
  });

  it('returns unsupported state for invalid JSON', () => {
    const state = parsePushState('{invalid json}');
    expect(state.supported).toBe(false);
    expect(state.error).toBe('Invalid push state payload');
  });

  it('returns unsupported state for null input', () => {
    const state = parsePushState(null);
    expect(state).toEqual(UNSUPPORTED_PUSH_STATE);
  });

  it('filters out empty distributors', () => {
    const obj = {
      supported: true,
      distributors: ['ntfy', '', 'fcm', null, undefined],
    };

    const state = parsePushState(obj);
    expect(state.distributors).toEqual(['ntfy', 'fcm']);
  });

  it('normalizes permission values', () => {
    expect(parsePushState({ permission: 'granted' }).permission).toBe('granted');
    expect(parsePushState({ permission: 'denied' }).permission).toBe('denied');
    expect(parsePushState({ permission: 'unknown' }).permission).toBe('unknown');
    expect(parsePushState({ permission: 'invalid' }).permission).toBe('unknown');
  });

  it('treats empty strings as null for string fields', () => {
    const state = parsePushState({
      distributor: '',
      endpoint: '',
      gateway: '',
      error: '',
    });

    expect(state.distributor).toBe(null);
    expect(state.endpoint).toBe(null);
    expect(state.gateway).toBe(null);
    expect(state.error).toBe(null);
  });
});

describe('extractOpenRoomId', () => {
  it('extracts room ID from string payload', () => {
    expect(extractOpenRoomId('!abc123:example.com')).toBe('!abc123:example.com');
  });

  it('extracts roomId property from object', () => {
    const payload = { roomId: '!test:example.com', other: 'data' };
    expect(extractOpenRoomId(payload)).toBe('!test:example.com');
  });

  it('extracts room_id property from object (snake_case fallback)', () => {
    const payload = { room_id: '!snake:example.com' };
    expect(extractOpenRoomId(payload)).toBe('!snake:example.com');
  });

  it('extracts id property from object (generic fallback)', () => {
    const payload = { id: '!generic:example.com' };
    expect(extractOpenRoomId(payload)).toBe('!generic:example.com');
  });

  it('prefers roomId over room_id', () => {
    const payload = { roomId: '!first:example.com', room_id: '!second:example.com' };
    expect(extractOpenRoomId(payload)).toBe('!first:example.com');
  });

  it('returns null for empty string', () => {
    expect(extractOpenRoomId('')).toBe(null);
    expect(extractOpenRoomId('   ')).toBe(null);
  });

  it('returns null for null/undefined', () => {
    expect(extractOpenRoomId(null)).toBe(null);
    expect(extractOpenRoomId(undefined)).toBe(null);
  });

  it('returns null when object has no valid room ID fields', () => {
    expect(extractOpenRoomId({ other: 'data' })).toBe(null);
  });

  it('returns null when object has empty room ID string', () => {
    expect(extractOpenRoomId({ roomId: '' })).toBe(null);
  });

  it('trims whitespace from extracted IDs', () => {
    expect(extractOpenRoomId('  !room:example.com  ')).toBe('!room:example.com');
    expect(extractOpenRoomId({ roomId: '  !room:example.com  ' })).toBe('!room:example.com');
  });
});
