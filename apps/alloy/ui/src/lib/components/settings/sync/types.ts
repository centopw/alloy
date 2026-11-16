/**
 * View models for the Devices & Sync settings surface.
 *
 * These are presentation types, not the IPC contract: the container maps `PairingState`,
 * `SyncStatusReport` and `DevicesList` from `docs/plan/11-sync-ipc.md` onto these fields and the
 * panels never import the IPC layer. Times are epoch milliseconds as the engine reports them.
 */

export type PairingState =
  | 'waiting'
  | 'peer_found'
  | 'sas_ready'
  | 'transferring'
  | 'done'
  | 'declined'
  | 'expired'
  | 'failed';

/** Which side of the pairing the section is driving. */
export type PairingRole = 'offer' | 'accept';

export interface PairingView {
  state: PairingState;
  /** QR payload string; null until the engine has minted one. */
  ticket: string | null;
  /** 8-character fallback code shown beside the QR. */
  code: string | null;
  /** Six-digit short authentication string, present once both sides met. */
  sas: string | null;
  peerLabel: string | null;
  expiresAtMs: number | null;
  error: string | null;
}

export interface ThisDeviceView {
  label: string;
  enrolledAtMs: number | null;
  epoch: number | null;
}

export type SyncMode = 'off' | 'prefs';

export type ConnectionTestState = 'idle' | 'running' | 'ok' | 'failed';

export interface ServerView {
  mode: SyncMode;
  url: string;
  /** TLS leaf SPKI fingerprint, derived by the engine on the last successful configure. */
  pin: string | null;
  /** The token is write-only: this is all the UI ever learns about it. */
  tokenStored: boolean;
  test: ConnectionTestState;
  testError: string | null;
  lastSyncMs: number | null;
  lastError: string | null;
  syncing: boolean;
}

export interface ConfigureSyncInput {
  mode: SyncMode;
  url: string;
  /** Null keeps the stored token; an empty string is the same as null. */
  token: string | null;
}

export interface DeviceView {
  id: string;
  label: string;
  enrolledAtMs: number | null;
  lastSyncMs: number | null;
  epoch: number | null;
}

export interface RecoveryView {
  hasPhrase: boolean;
  /** Present only in the response that created the phrase; never read back afterwards. */
  words: string[] | null;
  verified: boolean;
}
