import { client } from '../ipc/index.js';
import type {
  RecoveryState,
  SasEmoji,
  UserId,
} from '../generated/index.js';
import { errorMessage, expect } from './errors.js';

export type VerificationView =
  | { step: 'idle' }
  | { step: 'incoming'; userId: UserId; deviceId: string }
  | { step: 'waiting' }
  | { step: 'compare'; emojis: SasEmoji[]; decimals: [number, number, number] }
  | { step: 'done' }
  | { step: 'cancelled'; reason: string };

/** Device verification (emoji) and recovery-key state. */
class VerificationStore {
  view = $state<VerificationView>({ step: 'idle' });
  recovery = $state<RecoveryState>('Unknown');
  recoveryKey = $state<string | null>(null);
  busy = $state(false);
  error = $state<string | null>(null);

  private userId: UserId = '';
  private deviceId = '';
  private listening = false;

  /** Subscribe to crypto events once. */
  listen() {
    if (this.listening) return;
    this.listening = true;
    client.subscribe((e) => {
      if (e.type === 'RecoveryState') this.recovery = e.payload.state;
      if (e.type !== 'VerificationState') return;
      const { user_id, device_id, state } = e.payload;
      this.userId = user_id;
      if (device_id) this.deviceId = device_id;
      switch (state) {
        case 'Ready':
          // An unprompted Ready means another device asked us; after we accept it is a no-op.
          if (this.view.step === 'idle' || this.view.step === 'cancelled' || this.view.step === 'done') {
            this.view = { step: 'incoming', userId: user_id, deviceId: device_id };
          }
          break;
        case 'WaitingForPartner':
          if (this.view.step !== 'compare') this.view = { step: 'waiting' };
          break;
        case 'Verified':
          this.view = { step: 'done' };
          break;
        default:
          if (typeof state === 'object' && 'CompareEmojis' in state && state.CompareEmojis) {
            const c = state.CompareEmojis;
            this.view = {
              step: 'compare',
              emojis: c.emojis,
              decimals: [c.decimal_1, c.decimal_2, c.decimal_3],
            };
          } else if (typeof state === 'object' && 'Cancelled' in state && state.Cancelled) {
            this.view = { step: 'cancelled', reason: state.Cancelled.reason };
          }
      }
    });
  }

  async refreshRecovery() {
    try {
      this.recovery = expect(await client.getRecoveryStatus(), 'RecoveryStatusRetrieved').payload.state;
    } catch {
      /* not logged in yet */
    }
  }

  async requestVerification() {
    this.error = null;
    try {
      this.view = { step: 'waiting' };
      await client.requestSelfVerification();
    } catch (e) {
      this.view = { step: 'idle' };
      this.error = errorMessage(e);
    }
  }

  async accept() {
    await this.guard(() => client.acceptVerification(this.userId, this.deviceId));
    this.view = { step: 'waiting' };
  }

  async confirm() {
    await this.guard(() => client.confirmSas(this.userId, this.deviceId));
    this.view = { step: 'waiting' };
  }

  /** "They don't match" or dismiss. */
  async cancel() {
    await this.guard(() => client.cancelVerification(this.userId, this.deviceId));
  }

  dismiss() {
    this.view = { step: 'idle' };
  }

  async enableRecovery(password: string | null) {
    return this.guard(async () => {
      const res = expect(await client.enableRecovery(password), 'RecoveryEnabled');
      this.recoveryKey = res.payload.recovery_key;
    });
  }

  async recoverWithKey(key: string) {
    return this.guard(async () => {
      expect(await client.recoverWithKey(key), 'RecoveredFromKey');
      await this.refreshRecovery();
    });
  }

  clearKey() {
    this.recoveryKey = null;
  }

  private async guard(fn: () => Promise<unknown>) {
    this.busy = true;
    this.error = null;
    try {
      await fn();
      return true;
    } catch (e) {
      this.error = errorMessage(e);
      return false;
    } finally {
      this.busy = false;
    }
  }
}

export const verification = new VerificationStore();
