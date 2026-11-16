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

/** One store layer as `SyncStatus` and `Recovered` report it. */
export interface LayerView {
  /** `settings | tags | catalogue | backgrounds | theme | messages`, as the engine names it. */
  key: string;
  enabled: boolean;
  /** Epoch milliseconds of the last local write to the layer; 0 when it never arrived. */
  updatedAtMs: number;
}

/** What a successful `RecoverFromPhrase` rebuilt, and the roster it followed. */
export interface RecoveredView {
  layers: LayerView[];
  /** Every device in the roster the recovery returned, this one included. */
  roster: DeviceView[];
  /** Pin adopted on the first `https` connection, once `SyncStatus` reports it. */
  pin: string | null;
}

export interface RecoveryView {
  hasPhrase: boolean;
  /** Present only in the response that created the phrase; never read back afterwards. */
  words: string[] | null;
  verified: boolean;
  /** Latest rejection or failure, already localized by the container. */
  error?: string | null;
  /** A phrase recovery is on the wire: the server is being opened and pulled. */
  recovering?: boolean;
  /** Set by a successful recovery; cleared when a new attempt starts. */
  recovered?: RecoveredView | null;
  /**
   * The phrase on record predates a key rotation, so it opens only the retired key.
   * `CreateRecoveryPhrase` is the remedy and the next `SyncStatus` clears the flag.
   */
  stale?: boolean;
}

/** What `RevokeDevice` answered: the new epoch and how much of the old ciphertext went. */
export interface RevokedView {
  /** The device that was removed, as the roster named it before the revoke. */
  label: string;
  /** The epoch every layer is sealed under now. */
  epoch: number;
  /** True when the response confirmed every enabled layer was re-published. Null means unstated. */
  resealed: boolean | null;
  /** Previous-epoch blobs the server confirmed deleted, including ones already gone. */
  blobsDeleted: number | null;
  /** Previous-epoch blobs the server still holds; the revocation is valid regardless. */
  blobsRemaining: number | null;
  /** The server half was incomplete, as the engine reported it. */
  error: string | null;
}

/** What the fresh-device branch of the recovery panel submits. */
export interface RecoverFromPhraseInput {
  words: string[];
  url: string;
  /** Write-only: handed to the engine and dropped, never read back. */
  token: string;
}

/** What an exported bundle carries, as the engine's `ConfigBundleSummary` reports it. */
export interface BundleSummaryView {
  settings: boolean;
  catalogue: boolean;
  tags: number;
  backgrounds: number;
  theme: string | null;
}

/** The last export: the bundle was copied when `copied` is true. */
export interface ExportView {
  bytes: number;
  summary: BundleSummaryView;
  copied: boolean;
  error: string | null;
}

/** What applying a bundle would change; all-false means local state already covers it. */
export interface ImportReportView {
  settings: boolean;
  catalogue: boolean;
  tags: number;
  backgrounds: number;
  theme: boolean;
}

/** The import field's answer: a dry-run report, then the applied result on confirm. */
export interface ImportView {
  report: ImportReportView | null;
  applied: boolean;
  busy: boolean;
  error: string | null;
}

/** One completed `SyncNow`. */
export interface SyncResultView {
  uploaded: number;
  downloaded: number;
  headHex: string;
}

/** A configured endpoint presented a different TLS leaf; both pins are public. */
export interface PinMismatchView {
  stored: string;
  presented: string;
  trusting: boolean;
}
