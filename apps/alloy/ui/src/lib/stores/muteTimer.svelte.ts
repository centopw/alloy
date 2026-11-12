import { MUTE_FOREVER, roomsStore } from './rooms.svelte.js';

const SWEEP_MS = 60_000;

/** Mutes a chat for `ms` milliseconds, or (null) until the user turns notifications back on. */
export async function muteRoom(roomId: string, ms: number | null): Promise<void> {
  await roomsStore.setMuted(roomId, ms === null ? MUTE_FOREVER : Date.now() + ms);
}

/** Turns notifications for a chat back on. */
export async function unmuteRoom(roomId: string): Promise<void> {
  await roomsStore.setMuted(roomId, null);
}

/**
 * Restores chats whose timed mute has ended to the network default. A mute set to "forever" is left
 * alone; the engine reports the end of a timed mute as a past timestamp.
 */
export async function sweepMutes(now = Date.now()): Promise<void> {
  const ended = roomsStore.rooms.filter(
    (room) => room.muted_until !== null && room.muted_until < MUTE_FOREVER && room.muted_until <= now
  );
  await Promise.all(ended.map((room) => roomsStore.setMuted(room.id, null)));
}

/** Runs `sweepMutes` now and every minute. Returns a function that stops it. */
export function startMuteTimer(): () => void {
  void sweepMutes();
  const timer = setInterval(() => void sweepMutes(), SWEEP_MS);
  return () => clearInterval(timer);
}
