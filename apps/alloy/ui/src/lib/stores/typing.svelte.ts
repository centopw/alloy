import { SvelteMap } from 'svelte/reactivity';
import { client } from '../ipc/index.js';
import type { UserId } from '../generated/index.js';
import { session } from './session.svelte.js';

/**
 * Matrix sends the full set of users typing in a room with each notice, so a notice replaces
 * the previous one. Matrix expires typing after at most 30 s; this fallback drops a room whose
 * final empty notice never arrived (for example across a reconnect).
 */
export const TYPING_EXPIRY_MS = 30_000;

/** Who is typing in each room, as last reported by the engine. */
class TypingStore {
  /** Typing users by room id. A room with nobody typing has no entry. */
  readonly rooms = new SvelteMap<string, UserId[]>();

  private listening = false;
  private expiry = new Map<string, ReturnType<typeof setTimeout>>();

  /** Subscribe to engine TypingNotice events. Idempotent. */
  start() {
    if (this.listening) return;
    this.listening = true;
    client.subscribe((event) => {
      if (event.type === 'TypingNotice') this.apply(event.payload.room_id, event.payload.users);
    });
  }

  /** Replace a room's typing users. An empty list removes the room's entry. */
  apply(roomId: string, users: UserId[]) {
    this.cancelExpiry(roomId);
    if (users.length === 0) {
      this.rooms.delete(roomId);
      return;
    }
    this.rooms.set(roomId, [...users]);
    this.expiry.set(
      roomId,
      setTimeout(() => this.apply(roomId, []), TYPING_EXPIRY_MS),
    );
  }

  /** Users typing in a room, excluding the signed-in user. */
  usersIn(roomId: string | null): UserId[] {
    if (!roomId) return [];
    const me = session.account?.user_id;
    return (this.rooms.get(roomId) ?? []).filter((id) => id !== me);
  }

  private cancelExpiry(roomId: string) {
    const timer = this.expiry.get(roomId);
    if (timer !== undefined) {
      clearTimeout(timer);
      this.expiry.delete(roomId);
    }
  }
}

export const typingStore = new TypingStore();
