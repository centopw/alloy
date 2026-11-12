import { client } from '../ipc/index.js';
import type { UserProfileDto } from '../generated/index.js';
import { expect } from './errors.js';

/** How long a fetched profile is served from cache. */
export const PROFILE_TTL_MS = 60_000;

interface CachedProfile {
  profile: UserProfileDto;
  expires: number;
}

/** Profile lookups cached for `PROFILE_TTL_MS`. */
export class UserProfileStore {
  /** Keys are `accountId NUL userId`, so one user's entries share a prefix. */
  private cache = new Map<string, CachedProfile>();
  private inflight = new Map<string, Promise<UserProfileDto>>();

  /** Profile of a user inside the account that knows them. Concurrent calls share one request. */
  get(accountId: string, userId: string): Promise<UserProfileDto> {
    const key = profileKey(accountId, userId);
    const hit = this.cache.get(key);
    if (hit && hit.expires > Date.now()) return Promise.resolve(hit.profile);

    const pending = this.inflight.get(key);
    if (pending) return pending;

    const p: Promise<UserProfileDto> = client
      .invoke({ action: 'GetUserProfile', params: { account_id: accountId, user_id: userId } })
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

  /** Drop every cached profile of one account (logout, account switch). */
  forgetAccount(accountId: string): void {
    const prefix = `${accountId}\u0000`;
    for (const key of [...this.cache.keys()]) {
      if (key.startsWith(prefix)) this.cache.delete(key);
    }
  }
}

const profileKey = (accountId: string, userId: string) => `${accountId}\u0000${userId}`;

export const userProfile = new UserProfileStore();
