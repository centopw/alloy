import { describe, expect, it } from 'vitest';
import { errorReason, oidcErrorMessage } from './errors';

/** An engine failure as the transports deliver it: an object with code, message and reason. */
function failure(reason: string | null) {
  return { code: 'auth_failed', message: 'raw engine text', reason };
}

const SERVER = 'alloy.chat';
const REASONS = [
  'not_pending',
  'expired',
  'state_mismatch',
  'denied',
  'exchange_failed',
  'registration_failed',
  'unsupported',
  'launch_failed',
  'network',
];

describe('oidcErrorMessage', () => {
  it('explains every engine reason without showing its raw text or protocol jargon', () => {
    const messages = REASONS.map((reason) => oidcErrorMessage(failure(reason), SERVER));
    for (const [index, message] of messages.entries()) {
      expect(message, REASONS[index]).not.toBe('raw engine text');
      expect(message, REASONS[index]).toMatch(/sign-in|browser|reach|server/i);
      expect(message, REASONS[index]).not.toMatch(/oidc|oauth|pkce|callback|token/i);
    }
    expect(new Set(messages).size).toBe(REASONS.length);
  });

  it('names the server where the sentence is about that server', () => {
    expect(oidcErrorMessage(failure('denied'), SERVER)).toContain(SERVER);
    expect(oidcErrorMessage(failure('network'), SERVER)).toContain(SERVER);
  });

  it('offers a way forward in every sentence', () => {
    for (const reason of REASONS) {
      expect(oidcErrorMessage(failure(reason), SERVER)).toMatch(/try again|use a password|manually/i);
    }
  });

  it('falls back to a plain sentence for a reason it does not know', () => {
    expect(oidcErrorMessage(failure('some_future_reason'), SERVER)).toBe(
      'Sign-in didn’t complete. Try again.'
    );
    expect(oidcErrorMessage(failure(null), SERVER)).toBe('Sign-in didn’t complete. Try again.');
    expect(oidcErrorMessage('plain string', SERVER)).toBe('Sign-in didn’t complete. Try again.');
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
