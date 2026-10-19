import { describe, expect, it } from 'vitest';
import type { RoomMemberDto } from '$lib/generated';
import { hasModeration, moderationOptions } from './moderation';

const admin = { my_power_level: 100, can_kick: true, can_ban: true, can_set_power_levels: true };
const mod = { my_power_level: 50, can_kick: true, can_ban: true, can_set_power_levels: false };
const plain = { my_power_level: 0, can_kick: false, can_ban: false, can_set_power_levels: false };

function member(power: number, membership: RoomMemberDto['membership'] = 'Joined', id = '@t:hs'): RoomMemberDto {
  return { user_id: id, display_name: null, avatar_url: null, power_level: power, membership };
}

describe('moderationOptions', () => {
  it('lets an admin remove, ban and re-rank a member', () => {
    const o = moderationOptions(admin, member(0), '@me:hs');
    expect(o.kick).toBe(true);
    expect(o.ban).toBe(true);
    expect(o.levels.map((l) => l.label)).toEqual(['Admin', 'Moderator']);
  });

  it('never offers actions on yourself, on equals or on people above you', () => {
    expect(moderationOptions(admin, member(0, 'Joined', '@me:hs'), '@me:hs')).toEqual({ kick: false, ban: false, unban: false, levels: [] });
    expect(hasModeration(moderationOptions(mod, member(50), '@me:hs'))).toBe(false);
    expect(hasModeration(moderationOptions(mod, member(100), '@me:hs'))).toBe(false);
  });

  it('caps the levels at your own and omits the current one', () => {
    const o = moderationOptions({ ...mod, can_set_power_levels: true }, member(0), '@me:hs');
    expect(o.levels.map((l) => l.level)).toEqual([50]);
  });

  it('offers unban for a banned member and nothing without permission', () => {
    expect(moderationOptions(admin, member(0, 'Banned'), '@me:hs')).toMatchObject({ unban: true, ban: false, kick: false });
    expect(hasModeration(moderationOptions(plain, member(0), '@me:hs'))).toBe(false);
    expect(hasModeration(moderationOptions(null, member(0), '@me:hs'))).toBe(false);
  });
});
