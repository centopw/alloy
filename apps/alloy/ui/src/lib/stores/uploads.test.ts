import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  UPLOAD_LINGER_MS,
  isUploadComplete,
  reduceUploadClear,
  reduceUploadProgress,
  uploadFraction,
  uploadsStore,
  type UploadProgressMap,
} from './uploads.svelte';

const payload = (txn_id: string, sent_bytes: number, total_bytes: number) => ({
  room_id: '!room:example.org',
  txn_id,
  sent_bytes,
  total_bytes,
});

describe('uploadFraction', () => {
  it('computes the ratio of sent to total', () => {
    expect(uploadFraction(50, 200)).toBe(0.25);
  });

  it('is 0 while the total is unknown or zero', () => {
    expect(uploadFraction(10, 0)).toBe(0);
    expect(uploadFraction(0, 0)).toBe(0);
  });

  it('clamps to the 0..1 range', () => {
    expect(uploadFraction(300, 200)).toBe(1);
    expect(uploadFraction(-5, 200)).toBe(0);
  });
});

describe('isUploadComplete', () => {
  it('is true once sent reaches a known total', () => {
    expect(isUploadComplete({ sent: 100, total: 100, fraction: 1 })).toBe(true);
    expect(isUploadComplete({ sent: 120, total: 100, fraction: 1 })).toBe(true);
  });

  it('is false while bytes remain or the total is unknown', () => {
    expect(isUploadComplete({ sent: 99, total: 100, fraction: 0.99 })).toBe(false);
    expect(isUploadComplete({ sent: 0, total: 0, fraction: 0 })).toBe(false);
  });
});

describe('reduceUploadProgress', () => {
  it('records sent, total and fraction under the txn_id', () => {
    const next = reduceUploadProgress({}, payload('txn_a', 25, 100));
    expect(next).toEqual({ txn_a: { sent: 25, total: 100, fraction: 0.25 } });
  });

  it('replaces the previous record for the same txn_id', () => {
    const first = reduceUploadProgress({}, payload('txn_a', 25, 100));
    const second = reduceUploadProgress(first, payload('txn_a', 80, 100));
    expect(second.txn_a).toEqual({ sent: 80, total: 100, fraction: 0.8 });
  });

  it('keeps other uploads untouched', () => {
    const first = reduceUploadProgress({}, payload('txn_a', 10, 100));
    const second = reduceUploadProgress(first, payload('txn_b', 50, 100));
    expect(second.txn_a).toEqual(first.txn_a);
    expect(second.txn_b.fraction).toBe(0.5);
  });

  it('does not mutate its input', () => {
    const state: UploadProgressMap = {};
    reduceUploadProgress(state, payload('txn_a', 1, 2));
    expect(state).toEqual({});
  });

  it('reports fraction 0 for an unknown total', () => {
    const next = reduceUploadProgress({}, payload('txn_a', 10, 0));
    expect(next.txn_a.fraction).toBe(0);
  });
});

describe('reduceUploadClear', () => {
  it('removes only the given txn_id', () => {
    const state = reduceUploadProgress(
      reduceUploadProgress({}, payload('txn_a', 1, 2)),
      payload('txn_b', 1, 2),
    );
    const next = reduceUploadClear(state, 'txn_a');
    expect(Object.keys(next)).toEqual(['txn_b']);
    expect(Object.keys(state)).toEqual(['txn_a', 'txn_b']);
  });

  it('returns the same object when the txn_id is absent', () => {
    const state = reduceUploadProgress({}, payload('txn_a', 1, 2));
    expect(reduceUploadClear(state, 'missing')).toBe(state);
  });

  it('ignores inherited property names', () => {
    expect(reduceUploadClear({}, 'constructor')).toEqual({});
  });
});

describe('uploadsStore.apply', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    uploadsStore.stop();
  });

  afterEach(() => {
    uploadsStore.stop();
    vi.useRealTimers();
  });

  it('exposes progress for a txn_id', () => {
    uploadsStore.apply(payload('txn_store_1', 30, 60));
    expect(uploadsStore.progressFor('txn_store_1')).toEqual({
      sent: 30,
      total: 60,
      fraction: 0.5,
    });
    expect(uploadsStore.progressFor('unknown')).toBeNull();
  });

  it('removes a completed upload shortly after sent reaches total', () => {
    uploadsStore.apply(payload('txn_store_2', 100, 100));
    expect(uploadsStore.progressFor('txn_store_2')).not.toBeNull();

    vi.advanceTimersByTime(UPLOAD_LINGER_MS - 1);
    expect(uploadsStore.progressFor('txn_store_2')).not.toBeNull();

    vi.advanceTimersByTime(1);
    expect(uploadsStore.progressFor('txn_store_2')).toBeNull();
  });

  it('keeps an in-flight upload until it completes', () => {
    uploadsStore.apply(payload('txn_store_3', 10, 100));
    vi.advanceTimersByTime(UPLOAD_LINGER_MS * 10);
    expect(uploadsStore.progressFor('txn_store_3')).not.toBeNull();
  });

  it('clear() removes progress and cancels the pending removal', () => {
    uploadsStore.apply(payload('txn_store_4', 5, 5));
    uploadsStore.clear('txn_store_4');
    expect(uploadsStore.progressFor('txn_store_4')).toBeNull();
    expect(vi.getTimerCount()).toBe(0);
  });
});
