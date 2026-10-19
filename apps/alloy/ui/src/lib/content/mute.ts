/** Rooms muted for a limited time, as room id -> epoch milliseconds when the mute ends. */
export type MutedUntil = Readonly<Record<string, number>>;

export const NO_MUTES: MutedUntil = Object.freeze({});

const HOUR_MS = 3_600_000;

/** Mute choices offered in the room menu; `ms: null` mutes until the user turns notifications back on. */
export const MUTE_DURATIONS: readonly { label: string; ms: number | null }[] = [
  { label: 'For 1 hour', ms: HOUR_MS },
  { label: 'For 8 hours', ms: 8 * HOUR_MS },
  { label: 'For 24 hours', ms: 24 * HOUR_MS },
  { label: 'Until I turn it back on', ms: null },
];

export function isMutedUntil(value: unknown): value is MutedUntil {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) return false;
  return Object.values(value).every((v) => typeof v === 'number' && Number.isFinite(v));
}

export function sameMutedUntil(a: MutedUntil, b: MutedUntil): boolean {
  const ak = Object.keys(a);
  return ak.length === Object.keys(b).length && ak.every((k) => a[k] === b[k]);
}

/** Room ids whose timed mute has ended at `now`. */
export function expiredMutes(map: MutedUntil, now: number): string[] {
  return Object.entries(map)
    .filter(([, until]) => until <= now)
    .map(([id]) => id);
}

/** The map with `roomId` muted until `until`, or with its entry removed when `until` is null. */
export function withMute(map: MutedUntil, roomId: string, until: number | null): MutedUntil {
  const next = { ...map };
  if (until === null) delete next[roomId];
  else next[roomId] = until;
  return next;
}

/** "Muted until 3:40 PM" / "Muted until tomorrow 3:40 PM" for a room's remaining mute, or null when it has no timer. */
export function muteEndLabel(until: number | undefined, now: number, format: (ms: number) => string): string | null {
  if (until === undefined || until <= now) return null;
  const sameDay = new Date(until).toDateString() === new Date(now).toDateString();
  return `Muted until ${sameDay ? '' : 'tomorrow '}${format(until)}`;
}
