import { describe, expect, it } from 'vitest';
import { errorReason } from './errors';

/** An engine failure as the transports deliver it: an object with code, message and reason. */
function failure(reason: string | null) {
  return { code: 'auth_failed', message: 'raw engine text', reason };
}

describe('errorReason', () => {
  it('reads the reason from an engine failure and nothing from anything else', () => {
    expect(errorReason(failure('denied'))).toBe('denied');
    expect(errorReason(failure(null))).toBeNull();
    expect(errorReason(new Error('nope'))).toBeNull();
    expect(errorReason('nope')).toBeNull();
  });
});
