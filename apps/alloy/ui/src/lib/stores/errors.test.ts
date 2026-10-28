import { describe, expect, it } from 'vitest';
import { OFFLINE_MESSAGE, errorReason, oidcErrorMessage } from './errors';

/** An engine failure as the transports deliver it: an object with code, message and reason. */
function failure(reason: string | null) {
  return { code: 'auth_failed', message: 'raw engine text', reason };
}

describe('oidcErrorMessage', () => {
  it('explains each engine reason instead of showing its raw text', () => {
    const messages = [
      'not_pending',
      'expired',
      'state_mismatch',
      'denied',
      'exchange_failed',
      'unsupported',
      'registration_failed',
    ].map((reason) => oidcErrorMessage(failure(reason)));
    for (const message of messages) {
      expect(message).not.toBe('raw engine text');
      expect(message).toMatch(/sign-in|browser|server/i);
    }
    expect(new Set(messages).size).toBe(messages.length);
  });

  it('tells the user how to recover when the attempt can no longer finish', () => {
    expect(oidcErrorMessage(failure('not_pending'))).toMatch(/start again/i);
    expect(oidcErrorMessage(failure('expired'))).toMatch(/start again/i);
    expect(oidcErrorMessage(failure('unsupported'))).toMatch(/password/i);
  });

  it('reports a network failure as offline rather than as a server refusal', () => {
    expect(oidcErrorMessage(failure('network'))).toBe(OFFLINE_MESSAGE);
  });

  it('falls back to the raw message for a reason it does not know', () => {
    expect(oidcErrorMessage(failure('some_future_reason'))).toBe('raw engine text');
    expect(oidcErrorMessage(failure(null))).toBe('raw engine text');
    expect(oidcErrorMessage('plain string')).toBe('plain string');
  });
});

describe('errorReason', () => {
  it('reads the reason from an engine failure and nothing from anything else', () => {
    expect(errorReason(failure('denied'))).toBe('denied');
    expect(errorReason(failure(null))).toBeNull();
    expect(errorReason(new Error('nope'))).toBeNull();
    expect(errorReason('nope')).toBeNull();
  });
});
