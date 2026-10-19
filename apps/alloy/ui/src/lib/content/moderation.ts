import type { RoomDetails, RoomMemberDto } from '$lib/generated';

export const POWER_ADMIN = 100;
export const POWER_MODERATOR = 50;
export const POWER_MEMBER = 0;

/** What the signed-in user may do to one member of a room. */
export interface ModerationOptions {
  kick: boolean;
  ban: boolean;
  unban: boolean;
  /** Power levels the member can be moved to (never above the user's own, never the current one). */
  levels: { level: number; label: string }[];
}

const LEVELS = [
  { level: POWER_ADMIN, label: 'Admin' },
  { level: POWER_MODERATOR, label: 'Moderator' },
  { level: POWER_MEMBER, label: 'Member' },
];

const NONE: ModerationOptions = { kick: false, ban: false, unban: false, levels: [] };

/** Matrix only lets you act on people ranked below you, so every action needs a strictly higher level. */
export function moderationOptions(
  details: Pick<RoomDetails, 'my_power_level' | 'can_kick' | 'can_ban' | 'can_set_power_levels'> | null,
  target: RoomMemberDto,
  selfId: string | null,
): ModerationOptions {
  if (!details || target.user_id === selfId) return NONE;
  const mine = details.my_power_level ?? 0;
  const outranks = mine > target.power_level;
  const present = target.membership === 'Joined' || target.membership === 'Invited';
  return {
    kick: !!details.can_kick && outranks && present,
    ban: !!details.can_ban && outranks && target.membership !== 'Banned',
    unban: !!details.can_ban && target.membership === 'Banned',
    levels:
      details.can_set_power_levels && outranks && target.membership === 'Joined'
        ? LEVELS.filter((l) => l.level <= mine && l.level !== target.power_level)
        : [],
  };
}

/** Whether any moderation action is available for the member. */
export function hasModeration(options: ModerationOptions): boolean {
  return options.kick || options.ban || options.unban || options.levels.length > 0;
}
