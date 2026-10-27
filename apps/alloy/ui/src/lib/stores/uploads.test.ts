import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  UPLOAD_LINGER_MS,
  hasKnownSize,
  isUploadComplete,
  reduceUploadClear,
  reduceUploadProgress,
  uploadFraction,
  uploadPercent,
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

describe('hasKnownSize', () => {
  it('is false with no progress record yet', () => {
    expect(hasKnownSize(null)).toBe(false);
  });

  it('is false for the 0-of-0 event the engine emits when an upload starts', () => {
    expect(hasKnownSize({ sent: 0, total: 0, fraction: 0 })).toBe(false);
  });

  it('is true once a non-zero total arrives', () => {
    expect(hasKnownSize({ sent: 0, total: 2048, fraction: 0 })).toBe(true);
    expect(hasKnownSize({ sent: 1024, total: 2048, fraction: 0.5 })).toBe(true);
  });
});

describe('uploadPercent', () => {
  it('is 0 with no progress record yet', () => {
    expect(uploadPercent(null)).toBe(0);
  });

  it('rounds the fraction to whole percent', () => {
    expect(uploadPercent({ sent: 1, total: 3, fraction: 1 / 3 })).toBe(33);
    expect(uploadPercent({ sent: 2, total: 3, fraction: 2 / 3 })).toBe(67);
  });

  it('reads 100 for a finished upload', () => {
    expect(uploadPercent({ sent: 2048, total: 2048, fraction: 1 })).toBe(100);
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
