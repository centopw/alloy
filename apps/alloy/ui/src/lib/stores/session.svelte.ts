import { client } from '../ipc/index.js';
import type {
  Account,
  AccountId,
  ConnectionStatus,
  LoginId,
  LoginInputDto,
  LoginStepDto,
  Network
} from '../generated/index.js';
import { errorCode, errorMessage, expect } from './errors.js';

import { t } from '$lib/i18n';

export type Phase = 'booting' | 'onboarding' | 'ready' | 'no-engine';

/** One in-progress login attempt: the id the engine assigned and the step the UI must show. */
export interface LoginAttempt {
  id: LoginId;
  network: Network;
  step: LoginStepDto;
}

/** The network's name as chrome; the wire ids never reach the user. */
export function networkLabel(network: Network): string {
  switch (network) {
    case 'telegram':
      return t('network-telegram');
    case 'messenger':
      return t('network-messenger');
    case 'fake':
      return t('network-fake');
  }
}

/**
 * Account and login lifecycle on protocol v2: the connected accounts, their live connection
 * status, the onboarding overlay, and the login attempt being answered.
 */
class SessionStore {
  phase = $state<Phase>('booting');
  /** Every locally stored account, newest last, as `ListAccounts`/`AccountsChanged` give it. */
  accounts = $state<Account[]>([]);
  /** Live connector status per account; an account without an event has no entry yet. */
  statuses = $state<Record<AccountId, ConnectionStatus>>({});
  /** The networks this build can connect, from `ListNetworks`. */
  networks = $state<Network[]>([]);
  /** Engine-level failure of the last account or login call (not a `LoginStepDto::Failed`). */
  error = $state<string | null>(null);
  /** True while an account or login command is in flight. */
  busy = $state(false);
  /** Onboarding shown over the signed-in app for "Add account" and "Reconnect". */
  addingAccount = $state(false);
  /** Network a reconnect starts with, so its first login step opens without picking it again. */
  preferredNetwork = $state<Network | null>(null);
  /** The attempt in progress, null when the user is choosing a network or no login runs. */
  login = $state<LoginAttempt | null>(null);
  /** Failure of `BeginLogin`/`SubmitLogin`/`CancelLogin`, shown above the step. */
  loginError = $state<string | null>(null);
  loginBusy = $state(false);

  /** At least one stored account lost its session; the app offers Reconnect instead of a login wall. */
  needsRelogin = $derived(this.accounts.some((a) => a.state === 'needs_relogin'));

  private started = false;

  /** Load the stored accounts once and keep them current. Idempotent. */
  async boot() {
    if (this.started) return;
    this.started = true;
    client.subscribe((event) => {
      if (event.type === 'AccountsChanged') {
        this.applyAccounts(event.payload.accounts);
        this.syncPhase();
        return;
      }
      if (event.type === 'ConnectionStatus') {
        this.statuses[event.payload.account_id] = event.payload.status;
      }
    });
    await this.refreshAccounts();
  }

  /** Re-reads the account list; also the retry path after an engine failure. */
  async refreshAccounts() {
    try {
      const res = expect(await client.invoke({ action: 'ListAccounts' }), 'AccountsList');
      this.applyAccounts(res.payload.accounts);
      this.error = null;
      this.syncPhase();
    } catch (e) {
      // A browser build with no engine cannot hold accounts: say so instead of showing a picker
      // that can only fail, and never substitute demo data.
      if (errorCode(e) === 'no_engine') {
        this.phase = 'no-engine';
        return;
      }
      this.error = errorMessage(e);
      if (!this.addingAccount) this.phase = 'onboarding';
    }
  }

  /** The networks to offer, loaded once by onboarding. */
  async loadNetworks(): Promise<Network[]> {
    const res = expect(await client.invoke({ action: 'ListNetworks' }), 'Networks');
    this.networks = res.payload.networks;
    return this.networks;
  }

  /** Opens onboarding over the app, keeping the current accounts on screen behind it. */
  openAddAccount() {
    this.preferredNetwork = null;
    this.loginError = null;
    this.addingAccount = true;
  }

  /** Opens onboarding over the app with the account's network already selected. */
  openReconnect(network: Network) {
    this.preferredNetwork = network;
    this.loginError = null;
    this.addingAccount = true;
  }

  /** Closes the overlay, abandoning any attempt it started; the signed-in app stays as it was. */
  async closeAddAccount() {
    const attempt = this.login;
    if (attempt && attempt.step.type !== 'failed') await this.cancelLogin();
    this.preferredNetwork = null;
    this.login = null;
    this.loginError = null;
    this.addingAccount = false;
    this.syncPhase();
  }

  /** Starts a fresh attempt on `network`; an attempt already running is abandoned first. */
  async beginLogin(network: Network) {
    if (this.login) await this.cancelLogin();
    this.loginBusy = true;
    this.loginError = null;
    try {
      const res = expect(
        await client.invoke({ action: 'BeginLogin', params: { network } }),
        'LoginStep'
      );
      // The overlay may have closed while the engine was starting the attempt: nothing on screen
      // can answer it any more, so drop it instead of keeping a step no one sees.
      if (!this.addingAccount && this.phase === 'ready') {
        await client
          .invoke({ action: 'CancelLogin', params: { login_id: res.payload.login_id } })
          .catch(() => {});
        return;
      }
      this.login = { id: res.payload.login_id, network, step: res.payload.step };
      this.reachStep(res.payload.step);
    } catch (e) {
      this.loginError = errorMessage(e);
      throw e;
    } finally {
      this.loginBusy = false;
    }
  }

  /** Answers the current step; a `Done` step stores the account and leaves onboarding. */
  async submitLogin(input: LoginInputDto) {
    const attempt = this.login;
    if (!attempt) return;
    this.loginBusy = true;
    this.loginError = null;
    try {
      const res = expect(
        await client.invoke({
          action: 'SubmitLogin',
          params: { login_id: attempt.id, input }
        }),
        'LoginStep'
      );
      this.login = { ...attempt, step: res.payload.step };
      this.reachStep(res.payload.step);
    } catch (e) {
      this.loginError = errorMessage(e);
      throw e;
    } finally {
      this.loginBusy = false;
    }
  }

  /** Abandons the attempt in the engine and returns to the network list. */
  async cancelLogin() {
    const attempt = this.login;
    this.login = null;
    if (!attempt) return;
    this.loginBusy = true;
    this.loginError = null;
    try {
      expect(
        await client.invoke({ action: 'CancelLogin', params: { login_id: attempt.id } }),
        'LoginCancelled'
      );
    } catch (e) {
      this.loginError = errorMessage(e);
    } finally {
      this.loginBusy = false;
    }
  }

  /**
   * Drops a terminal `Failed` attempt locally. The engine already ended it, so no `CancelLogin`
   * is sent: the protocol says retrying starts with `BeginLogin`.
   */
  dismissLogin() {
    this.login = null;
    this.loginError = null;
  }

  /** Starts the attempt over on the same network after a `Failed` step. */
  async retryLogin() {
    const network = this.login?.network;
    this.dismissLogin();
    if (network) await this.beginLogin(network);
  }

  /** Moves login out of the UI: the engine emits `AccountsChanged`, this keeps the list honest. */
  enterAccount(account: Account) {
    this.upsert(account);
    this.login = null;
    this.loginError = null;
    this.addingAccount = false;
    this.preferredNetwork = null;
    this.syncPhase();
  }

  /** Logs out one account, or every account when no id is given (the settings-wide sign-out). */
  async logout(accountId?: AccountId) {
    const ids = accountId ? [accountId] : this.accounts.map((a) => a.id);
    if (ids.length === 0) return;
    this.busy = true;
    this.error = null;
    try {
      for (const id of ids) {
        expect(await client.invoke({ action: 'Logout', params: { account_id: id } }), 'LoggedOut');
        this.dropAccount(id);
      }
      this.syncPhase();
    } catch (e) {
      this.error = errorMessage(e);
      throw e;
    } finally {
      this.busy = false;
    }
  }

  private reachStep(step: LoginStepDto) {
    if (step.type === 'done') this.enterAccount(step.account);
  }

  private applyAccounts(accounts: Account[]) {
    this.accounts = accounts;
    const present = new Set(accounts.map((a) => a.id));
    for (const id of Object.keys(this.statuses)) {
      if (!present.has(id)) delete this.statuses[id];
    }
  }

  private upsert(account: Account) {
    const index = this.accounts.findIndex((a) => a.id === account.id);
    if (index === -1) this.accounts.push(account);
    else this.accounts[index] = account;
  }

  private dropAccount(accountId: AccountId) {
    this.accounts = this.accounts.filter((a) => a.id !== accountId);
    delete this.statuses[accountId];
  }

  /** First sign-in is onboarding; with an account stored the app is the screen. */
  private syncPhase() {
    if (this.addingAccount) return;
    this.phase = this.accounts.length > 0 ? 'ready' : 'onboarding';
  }
}

export const session = new SessionStore();
