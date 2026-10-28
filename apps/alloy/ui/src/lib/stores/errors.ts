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
 * What went wrong in a browser sign-in, from the engine's stable `reason` and the server the user
 * chose. Copy follows the sign-in design spec: no raw engine text, no protocol jargon, and every
 * sentence ends at something the user can do.
 */
export function oidcErrorMessage(e: unknown, server: string): string {
  switch (errorReason(e)) {
    case 'not_pending':
      return 'Sign-in wasn’t completed. You can try again.';
    case 'expired':
      return 'This sign-in took too long and expired. Try again to get a fresh prompt.';
    case 'state_mismatch':
      return 'Sign-in didn’t complete. Try again.';
    case 'denied':
      return `${server} didn’t approve the sign-in. Try again or use a different account.`;
    case 'exchange_failed':
      return 'We couldn’t finish signing you in. Try again. If it keeps happening, check that the server is reachable.';
    case 'registration_failed':
      return 'We couldn’t start the browser sign-in. Try again.';
    case 'unsupported':
      return 'This server doesn’t offer browser sign-in. Use a password if your server allows it.';
    case 'launch_failed':
      return 'We couldn’t open your browser. Open it manually and try again.';
    case 'network':
      return `We couldn’t reach ${server}. Check your connection and try again.`;
    default:
      return 'Sign-in didn’t complete. Try again.';
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
