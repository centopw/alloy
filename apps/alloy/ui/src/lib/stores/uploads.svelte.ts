import { client } from '../ipc/index.js';
import type { EngineEvent } from '../generated/index.js';

export type UploadProgressPayload = Extract<EngineEvent, { type: 'UploadProgress' }>['payload'];

/** Progress of one attachment upload, keyed by its txn_id. */
export interface UploadProgress {
  sent: number;
  total: number;
  /** 0..1. Stays 0 while the total is unknown. */
  fraction: number;
}

/** Progress records keyed by txn_id. Treated as immutable: reducers return new objects. */
export type UploadProgressMap = Readonly<Record<string, UploadProgress>>;

/** How long a finished upload stays visible before it is removed. */
export const UPLOAD_LINGER_MS = 600;

export function uploadFraction(sent: number, total: number): number {
  if (!(total > 0)) return 0;
  return Math.min(1, Math.max(0, sent / total));
}

/** An upload is complete once sent reaches a known, non-zero total. */
export function isUploadComplete(progress: UploadProgress): boolean {
  return progress.total > 0 && progress.sent >= progress.total;
}

/*
 * The engine emits a `0 of 0` event the moment an upload starts. A progress bar that treats that as
 * real progress freezes at 0% with no sweep — the "looks stuck" report. Until a total arrives the
 * bar must stay indeterminate, so the strip asks these two questions rather than reading the
 * fraction directly.
 */

/**
 * Whether the engine has reported a size yet. Drives indeterminate vs determinate.
 *
 * A type guard so the template's `hasSize ?` branch narrows `progress` to non-null and can read
 * `sent`/`total` without a redundant null check.
 */
export function hasKnownSize(progress: UploadProgress | null): progress is UploadProgress {
  return progress !== null && progress.total > 0;
}

/** Percent 0..100, rounded. Only meaningful when `hasKnownSize` is true. */
export function uploadPercent(progress: UploadProgress | null): number {
  return progress === null ? 0 : Math.round(progress.fraction * 100);
}

/** Pure: returns a new map with the event's progress recorded under its txn_id. */
export function reduceUploadProgress(
  state: UploadProgressMap,
  payload: UploadProgressPayload,
): UploadProgressMap {
  return {
    ...state,
    [payload.txn_id]: {
      sent: payload.sent_bytes,
      total: payload.total_bytes,
      fraction: uploadFraction(payload.sent_bytes, payload.total_bytes),
    },
  };
}

/** Pure: returns a map without txn_id. Returns the same object when txn_id is absent. */
export function reduceUploadClear(state: UploadProgressMap, txnId: string): UploadProgressMap {
  if (!Object.hasOwn(state, txnId)) return state;
  const next: Record<string, UploadProgress> = { ...state };
  delete next[txnId];
  return next;
}

/** An attachment the user started uploading from this client. */
export interface PendingUpload {
  txnId: string;
  roomId: string;
  name: string;
}

class UploadsStore {
  /** Live progress by txn_id. */
  entries = $state<UploadProgressMap>({});
  /** Uploads in flight, oldest first. The send call blocks until the file is on the server. */
  pending = $state<PendingUpload[]>([]);

  private unlisten: (() => void) | null = null;
  private timers = new Map<string, ReturnType<typeof setTimeout>>();

  /** Subscribe to engine UploadProgress events. Idempotent. */
  start() {
    if (this.unlisten) return;
    this.unlisten = client.subscribe((event) => {
      if (event.type === 'UploadProgress') this.apply(event.payload);
    });
  }

  /** Stop listening and drop all progress. */
  stop() {
    this.unlisten?.();
    this.unlisten = null;
    for (const timer of this.timers.values()) clearTimeout(timer);
    this.timers.clear();
    this.entries = {};
    this.pending = [];
  }

  begin(upload: PendingUpload) {
    this.start();
    this.pending = [...this.pending, upload];
  }

  /** The upload finished (or failed): drop it and its progress. */
  end(txnId: string) {
    this.pending = this.pending.filter((u) => u.txnId !== txnId);
    this.clear(txnId);
  }

  inRoom(roomId: string | null): PendingUpload[] {
    return roomId ? this.pending.filter((u) => u.roomId === roomId) : [];
  }

  progressFor(txnId: string): UploadProgress | null {
    return Object.hasOwn(this.entries, txnId) ? this.entries[txnId] : null;
  }

  /** Forget an upload's progress and cancel any pending removal. */
  clear(txnId: string) {
    const timer = this.timers.get(txnId);
    if (timer !== undefined) {
      clearTimeout(timer);
      this.timers.delete(txnId);
    }
    this.entries = reduceUploadClear(this.entries, txnId);
  }

  apply(payload: UploadProgressPayload) {
    this.entries = reduceUploadProgress(this.entries, payload);
    const txnId = payload.txn_id;
    if (isUploadComplete(this.entries[txnId]) && !this.timers.has(txnId)) {
      this.timers.set(
        txnId,
        setTimeout(() => this.clear(txnId), UPLOAD_LINGER_MS),
      );
    }
  }
}

export const uploadsStore = new UploadsStore();
