import { client } from '../ipc/index.js';
import type { UserProfileDto } from '../generated/index.js';
import { expect } from './errors.js';

/** How long a fetched profile is served from cache. */
export const PROFILE_TTL_MS = 60_000;

interface CachedProfile {
  profile: UserProfileDto;
  expires: number;
}

/** Profile lookups cached for `PROFILE_TTL_MS`, plus direct-message room opening. */
export class UserProfileStore {
  /** Keys are `userId NUL roomId`, so one user's entries share a prefix. */
  private cache = new Map<string, CachedProfile>();
  private inflight = new Map<string, Promise<UserProfileDto>>();
  private dmInflight = new Map<string, Promise<string>>();

  /** Profile of a user, as seen in `roomId` when given. Concurrent calls share one request. */
  get(userId: string, roomId: string | null): Promise<UserProfileDto> {
    const key = profileKey(userId, roomId);
    const hit = this.cache.get(key);
    if (hit && hit.expires > Date.now()) return Promise.resolve(hit.profile);

    const pending = this.inflight.get(key);
    if (pending) return pending;

    const p: Promise<UserProfileDto> = client
      .invoke({ action: 'GetUserProfile', params: { user_id: userId, room_id: roomId } })
      .then((res) => {
        const profile = expect(res, 'UserProfile').payload.profile;
        this.cache.set(key, { profile, expires: Date.now() + PROFILE_TTL_MS });
        return profile;
      })
      .finally(() => {
        if (this.inflight.get(key) === p) this.inflight.delete(key);
      });
    this.inflight.set(key, p);
    return p;
  }

  /** Opens (or reuses) the direct-message room with a user and returns its room id. */
  startDirectMessage(userId: string): Promise<string> {
    const pending = this.dmInflight.get(userId);
    if (pending) return pending;

    const p: Promise<string> = client
      .invoke({ action: 'StartDirectMessage', params: { user_id: userId } })
      .then((res) => {
        const roomId = expect(res, 'DirectMessageRoom').payload.room_id;
        // Cached profiles carry `dm_room_id`, which is stale now that the room exists.
        this.forget(userId);
        return roomId;
      })
      .finally(() => {
        if (this.dmInflight.get(userId) === p) this.dmInflight.delete(userId);
      });
    this.dmInflight.set(userId, p);
    return p;
  }

  private forget(userId: string): void {
    const prefix = `${userId}\u0000`;
    for (const key of [...this.cache.keys()]) {
      if (key.startsWith(prefix)) this.cache.delete(key);
    }
  }
}

const profileKey = (userId: string, roomId: string | null) => `${userId}\u0000${roomId ?? ''}`;

export const userProfile = new UserProfileStore();
