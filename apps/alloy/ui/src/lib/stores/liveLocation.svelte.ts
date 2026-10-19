import { client } from '../ipc/index.js';
import { toGeoUri } from '../content/geo.js';
import { watchFix } from '../platform/geolocation.js';
import { errorMessage, expect } from './errors.js';

/** A beacon is sent at most this often, however often the device reports a position. */
export const BEACON_INTERVAL_MS = 30_000;

/** Whether a new position should be sent now: the first one always, then once per interval. */
export function beaconDue(lastSentAt: number | null, now: number): boolean {
  return lastSentAt === null || now - lastSentAt >= BEACON_INTERVAL_MS;
}

/** The user's own live location share: one room at a time, until a chosen time or until stopped. */
class LiveLocationStore {
  active = $state<{ roomId: string; until: number } | null>(null);
  error = $state<string | null>(null);

  private stopWatching: (() => void) | null = null;
  private expiry: ReturnType<typeof setTimeout> | undefined;
  private lastSentAt: number | null = null;

  async start(roomId: string, durationMs: number): Promise<void> {
    await this.stop();
    this.error = null;
    try {
      expect(await client.startLiveLocation(roomId, durationMs), 'LiveLocationStarted');
    } catch (e) {
      this.error = errorMessage(e);
      return;
    }
    this.active = { roomId, until: Date.now() + durationMs };
    this.lastSentAt = null;
    this.expiry = setTimeout(() => void this.stop(), durationMs);
    this.stopWatching = watchFix(
      (fix) => {
        const now = Date.now();
        if (!beaconDue(this.lastSentAt, now)) return;
        this.lastSentAt = now;
        client.sendLocationBeacon(roomId, toGeoUri(fix.latitude, fix.longitude, fix.accuracy)).catch((e) => {
          this.error = errorMessage(e);
        });
      },
      (message) => {
        this.error = message;
        void this.stop();
      },
    );
  }

  async stop(): Promise<void> {
    const current = this.active;
    clearTimeout(this.expiry);
    this.stopWatching?.();
    this.stopWatching = null;
    this.active = null;
    if (!current) return;
    try {
      expect(await client.stopLiveLocation(current.roomId), 'LiveLocationStopped');
    } catch (e) {
      this.error = errorMessage(e);
    }
  }
}

export const liveLocation = new LiveLocationStore();
