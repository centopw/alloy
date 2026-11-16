/**
 * Error copy for the Devices & Sync surface.
 *
 * Every `EngineError` code the sync commands can produce has its own sentence; the engine's
 * `message` is a diagnostic and names endpoints and paths, so it is never shown when a code is
 * known. Unknown failures fall back to the shared message helper.
 */

import { errorCode, errorMessage } from '$lib/stores/errors';
import { t } from '$lib/i18n';

/** Engine error code → Fluent message id. */
const KEYS: Record<string, string> = {
  bundle_rejected: 'sync-error-bundle-rejected',
  pairing_failed: 'sync-error-pairing-failed',
  recovery_rejected: 'sync-error-recovery-rejected',
  pin_mismatch: 'sync-error-pin-mismatch',
  sync_blocked: 'sync-error-sync-blocked',
  payload_too_large: 'sync-error-payload-too-large',
  rate_limited: 'sync-error-rate-limited',
  auth_failed: 'sync-error-auth-failed',
  invalid_argument: 'sync-error-invalid-argument',
};

/** The localized sentence for a failed sync command. */
export function syncErrorMessage(error: unknown): string {
  const code = errorCode(error);
  const key = code === null ? undefined : KEYS[code];
  if (key) return t(key);
  return errorMessage(error);
}

/**
 * Splits a `pin_mismatch` message into the two fingerprints.
 *
 * `EngineError::pin_mismatch` carries both in its text (`pinned <hex>, presented <hex>`) because
 * the protocol flattens the error to `code`/`message`/`reason`; no structured field crosses IPC.
 * The stored pin is already known from `SyncStatus`, so it is the fallback when the text does not
 * match the engine's format (for example a `SyncNow` that only names the reason code).
 */
export function parsePinMismatch(
  message: string,
  fallbackStored: string
): { stored: string; presented: string } {
  const match = /pinned\s+([0-9a-f]+)\s*,\s*presented\s+([0-9a-f]+)/i.exec(message);
  if (match) return { stored: match[1], presented: match[2] };
  return { stored: fallbackStored, presented: '' };
}
