import type { EngineResponse } from '../generated/index.js';

/** Shown instead of the transport's raw text, which names the request URL and an OS error. */
export const OFFLINE_MESSAGE = 'No connection to your server. Check your internet and try again.';

/** What a failed HTTP transport (no answer at all) looks like in the SDK, reqwest and browser messages. */
const TRANSPORT_FAILURE =
  /error sending request|dns error|failed to lookup|name or service not known|connection (refused|reset|closed|aborted)|tcp connect error|network is unreachable|no route to host|request timed out|operation timed out|failed to fetch|networkerror|load failed/i;

/** The text of a string or of an Error/EngineError-like value, else ''. */
function textOf(e: unknown): string {
  if (typeof e === 'string') return e;
  if (e && typeof e === 'object' && 'message' in e && typeof e.message === 'string') return e.message;
  return '';
}

/** True when the engine could not reach the server at all (as opposed to the server refusing a request). */
export function isNetworkError(e: unknown): boolean {
  return TRANSPORT_FAILURE.test(textOf(e));
}

/** Extract a readable message from a rejected engine call (EngineError {code,message} or Error). */
export function errorMessage(e: unknown): string {
  if (isNetworkError(e)) return OFFLINE_MESSAGE;
  const text = textOf(e);
  return text === '' ? 'Something went wrong' : text;
}

export function errorCode(e: unknown): string | null {
  if (e && typeof e === 'object' && typeof (e as { code?: unknown }).code === 'string') {
    return (e as { code: string }).code;
  }
  return null;
}

/** The engine's stable machine-readable detail for a failure, when it carries one. */
export function errorReason(e: unknown): string | null {
  if (e && typeof e === 'object' && typeof (e as { reason?: unknown }).reason === 'string') {
    return (e as { reason: string }).reason;
  }
  return null;
}

/**
 * What went wrong in an OIDC login, from the engine's stable `reason`. The engine reports the
 * failure on the attempt that started it, so the message always asks for a fresh attempt where
 * that is the only fix.
 */
export function oidcErrorMessage(e: unknown): string {
  switch (errorReason(e)) {
    case 'not_pending':
      return 'That sign-in attempt is no longer active. Start again.';
    case 'expired':
      return 'The sign-in attempt timed out. Start again.';
    case 'state_mismatch':
      return 'The sign-in response did not belong to this attempt. Start again.';
    case 'denied':
      return 'Sign-in was cancelled in the browser.';
    case 'exchange_failed':
      return 'The server did not finish the sign-in. Try again.';
    case 'unsupported':
      return 'This server does not support signing in through a browser. Use a password instead.';
    case 'registration_failed':
      return 'Alloy could not register with this server’s login service. Try again later.';
    case 'network':
      return OFFLINE_MESSAGE;
    default:
      return errorMessage(e);
  }
}

/** Narrow a response to one variant or throw (a protocol bug, not a user error). */
export function expect<T extends EngineResponse['type']>(
  res: EngineResponse,
  type: T
): Extract<EngineResponse, { type: T }> {
  if (res.type !== type) throw new Error(`Expected ${type}, got ${res.type}`);
  return res as Extract<EngineResponse, { type: T }>;
}
