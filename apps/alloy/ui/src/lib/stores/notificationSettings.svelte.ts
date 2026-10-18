import { client } from '../ipc/index.js';
import type { EngineResponse, NotificationLevel, NotificationSettingsDto } from '../generated/index.js';
import { errorMessage, expect } from './errors.js';

/** The engine refuses `Mute` as an account-wide default; rooms mute through their own override. */
export type DefaultLevel = Exclude<NotificationLevel, 'Mute'>;

export type MentionRule = 'room_mention' | 'user_mention' | 'invite';

/**
 * Account-wide notification defaults and mention rules, read from and written to the engine.
 * Writes apply optimistically and roll back to the last confirmed state when the engine rejects them.
 */
export class NotificationSettingsStore {
  settings = $state<NotificationSettingsDto | null>(null);
  loading = $state(false);
  /** Number of writes in flight. The page disables its controls while this is above zero. */
  saving = $state(0);
  error = $state<string | null>(null);

  async load(): Promise<void> {
    this.loading = true;
    this.error = null;
    try {
      const res = await client.invoke({ action: 'GetNotificationSettings' });
      this.settings = expect(res, 'NotificationSettings').payload.settings;
    } catch (err) {
      this.error = errorMessage(err);
    } finally {
      this.loading = false;
    }
  }

  /** Sets the default level for one-to-one rooms (`oneToOne`) or group rooms. */
  setDefault(oneToOne: boolean, level: DefaultLevel): Promise<void> {
    const prev = this.settings;
    if (!prev) return Promise.resolve();
    const next: NotificationSettingsDto = oneToOne
      ? { ...prev, one_to_one: level }
      : { ...prev, group: level };
    return this.apply(prev, next, () =>
      client.invoke({ action: 'SetDefaultNotificationLevel', params: { one_to_one: oneToOne, level } })
    );
  }

  /** Enables or disables one mention or invitation rule. The other rules are left unchanged. */
  setMentionRule(rule: MentionRule, value: boolean): Promise<void> {
    const prev = this.settings;
    if (!prev) return Promise.resolve();
    const next: NotificationSettingsDto = { ...prev, [rule]: value };
    const params = {
      room_mention: rule === 'room_mention' ? value : null,
      user_mention: rule === 'user_mention' ? value : null,
      invite: rule === 'invite' ? value : null,
    };
    return this.apply(prev, next, () => client.invoke({ action: 'SetMentionRules', params }));
  }

  private async apply(
    prev: NotificationSettingsDto,
    next: NotificationSettingsDto,
    command: () => Promise<EngineResponse>
  ): Promise<void> {
    this.settings = next;
    this.error = null;
    this.saving += 1;
    try {
      expect(await command(), 'Unit');
    } catch (err) {
      this.settings = prev;
      this.error = errorMessage(err);
    } finally {
      this.saving -= 1;
    }
  }
}

export const notificationSettings = new NotificationSettingsStore();
