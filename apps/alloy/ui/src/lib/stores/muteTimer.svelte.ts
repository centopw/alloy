import { expiredMutes, withMute } from '../content/mute.js';
import { prefs } from './prefs.svelte.js';
import { roomNotify } from './roomNotify.svelte.js';

const SWEEP_MS = 60_000;

/** Mutes a room, for `ms` milliseconds or (null) until the user turns notifications back on. */
export async function muteRoom(roomId: string, ms: number | null): Promise<void> {
  prefs.set('mutedUntil', withMute(prefs.mutedUntil, roomId, ms === null ? null : Date.now() + ms));
  await roomNotify.set(roomId, 'Mute');
}

/** Sets a level other than mute (or the default with `null`) and forgets any mute timer of the room. */
export async function setRoomLevel(roomId: string, level: 'All' | 'Mentions' | null): Promise<void> {
  if (roomId in prefs.mutedUntil) prefs.set('mutedUntil', withMute(prefs.mutedUntil, roomId, null));
  await roomNotify.set(roomId, level);
}

/** Restores rooms whose timed mute has ended to the account default. */
export async function sweepMutes(now = Date.now()): Promise<void> {
  const ended = expiredMutes(prefs.mutedUntil, now);
  if (ended.length === 0) return;
  let next = prefs.mutedUntil;
  for (const id of ended) next = withMute(next, id, null);
  prefs.set('mutedUntil', next);
  await Promise.all(ended.map((id) => roomNotify.set(id, null)));
}

/** Runs `sweepMutes` now and every minute. Returns a function that stops it. */
export function startMuteTimer(): () => void {
  void sweepMutes();
  const timer = setInterval(() => void sweepMutes(), SWEEP_MS);
  return () => clearInterval(timer);
}
