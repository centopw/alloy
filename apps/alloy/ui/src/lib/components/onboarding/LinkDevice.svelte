<script lang="ts">
  import { onMount } from 'svelte';
  import { Button } from '../common';
  import PairingAccept from '../settings/sync/PairingAccept.svelte';
  import SasConfirm from '../settings/sync/SasConfirm.svelte';
  import { syncErrorMessage } from '../settings/sync/errors';
  import { client } from '$lib/ipc';
  import { errorCode, expect } from '$lib/stores/errors';
  import { session } from '$lib/stores/session.svelte';
  import type { EngineEvent, PairingPhase } from '$lib/generated';

  import { t } from '$lib/i18n';

  interface Props {
    /** Back to the start screen, leaving the pairing to expire on its own. */
    onback: () => void;
    /** The person is enrolled but has no account yet: the network list is the next step. */
    onaddaccount: () => void;
  }

  let { onback, onaddaccount }: Props = $props();

  /**
   * The accepting side of a device-to-device pairing. `entry` is before the engine knows a
   * pairing; the phases are the engine's, and `applying` covers the re-read that follows `done`.
   */
  type Stage = 'entry' | PairingPhase | 'applying' | 'linked';

  let stage = $state<Stage>('entry');
  let pairingId = $state<string | null>(null);
  let sas = $state<string | null>(null);
  let peerLabel = $state<string | null>(null);
  /** The engine's sentence for the failed command, or the state line when it carries none. */
  let failure = $state<string | null>(null);
  let poll: ReturnType<typeof setInterval> | null = null;
  let stopEvents: (() => void) | null = null;

  const LIVE: ReadonlySet<PairingPhase> = new Set([
    'waiting',
    'peer_found',
    'sas_ready',
    'transferring',
  ]);

  function stopPoll() {
    if (poll !== null) {
      clearInterval(poll);
      poll = null;
    }
  }

  function startPoll() {
    stopPoll();
    poll = setInterval(() => void pollPairing(), 1000);
  }

  function stateLine(phase: PairingPhase): string {
    switch (phase) {
      case 'expired':
        return t('sync-pairing-state-expired');
      case 'declined':
        return t('sync-pairing-state-declined');
      default:
        return t('sync-pairing-state-failed');
    }
  }

  function applyPairing(phase: PairingPhase, nextSas: string | null, failureText: string | null) {
    sas = nextSas ?? sas;
    if (phase === 'done') {
      void finish();
      return;
    }
    stage = phase;
    if (!LIVE.has(phase)) {
      stopPoll();
      failure = failureText ?? stateLine(phase);
    }
  }

  /** The event is the source of truth; the poll only covers a missed `PairingChanged`. */
  async function pollPairing() {
    if (!pairingId) return;
    try {
      const res = expect(
        await client.invoke({ action: 'PairingStatus', params: { pairing_id: pairingId } }),
        'PairingState'
      );
      peerLabel = res.payload.peer_label ?? peerLabel;
      applyPairing(
        res.payload.state,
        res.payload.sas,
        res.payload.error ? syncErrorMessage({ message: res.payload.error }) : null
      );
    } catch (e) {
      // A pairing the engine already dropped is not a transition; the event carried the ending.
      if (errorCode(e) === 'not_found') stopPoll();
    }
  }

  function onEvent(event: EngineEvent) {
    if (event.type !== 'PairingChanged') return;
    if (!pairingId || event.payload.pairing_id !== pairingId) return;
    applyPairing(event.payload.state, event.payload.sas, null);
  }

  async function submit(ticket: string, code: string) {
    failure = null;
    try {
      const res = expect(
        await client.invoke({ action: 'AcceptPairingTicket', params: { ticket, code } }),
        'PairingStarted'
      );
      pairingId = res.payload.pairing_id;
      stage = 'waiting';
      startPoll();
    } catch (e) {
      // The form stays as typed: a wrong ticket or a code outside the alphabet is fixed there.
      failure = syncErrorMessage(e);
    }
  }

  async function confirm(accept: boolean) {
    if (!pairingId) return;
    try {
      const res = expect(
        await client.invoke({
          action: 'ConfirmPairing',
          params: { pairing_id: pairingId, accept },
        }),
        'PairingState'
      );
      applyPairing(
        res.payload.state,
        res.payload.sas,
        res.payload.error ? syncErrorMessage({ message: res.payload.error }) : null
      );
    } catch (e) {
      stopPoll();
      stage = 'failed';
      failure = syncErrorMessage(e);
    }
  }

  /**
   * The bundle applied. Re-read the account list and the networks: with an account stored the
   * shell replaces onboarding; without one the person sees what arrived and the way in.
   */
  async function finish() {
    stopPoll();
    stage = 'applying';
    await Promise.allSettled([session.refreshAccounts(), session.loadNetworks()]);
    stage = 'linked';
  }

  onMount(() => {
    stopEvents = client.subscribe(onEvent);
    return () => {
      stopEvents?.();
      stopPoll();
    };
  });
</script>

<div class="link">
  <div class="step-head">
    <p class="eyebrow">{t('onboarding-link-eyebrow')}</p>
    <h3>{t('onboarding-link-title')}</h3>
    <p class="hint">{t('onboarding-link-other-device')}</p>
  </div>

  {#if stage === 'entry'}
    <p class="hint">{t('onboarding-link-what-arrives')}</p>
    <PairingAccept focusCode onsubmit={(ticket, code) => void submit(ticket, code)} />
    {#if failure}
      <p class="error" role="alert">{failure}</p>
    {/if}
    <div class="actions">
      <Button variant="ghost" type="button" onclick={onback}>{t('common-back')}</Button>
    </div>
  {:else if stage === 'waiting' || stage === 'peer_found' || stage === 'transferring'}
    <p class="status" role="status">
      {stage === 'transferring'
        ? t('sync-pairing-state-transferring')
        : t('sync-pairing-state-waiting')}
    </p>
    <div class="actions">
      <Button variant="ghost" type="button" onclick={onback}>{t('common-back')}</Button>
    </div>
  {:else if stage === 'sas_ready'}
    <SasConfirm
      sas={sas ?? ''}
      {peerLabel}
      onmatch={() => void confirm(true)}
      onnomatch={() => void confirm(false)}
    />
  {:else if stage === 'applying'}
    <p class="status" role="status">{t('onboarding-link-applying')}</p>
  {:else if stage === 'linked'}
    <p class="status" role="status">{t('onboarding-link-done')}</p>
    <p class="hint">{t('onboarding-link-accounts-here')}</p>
    <div class="actions">
      <Button type="button" onclick={onaddaccount}>{t('onboarding-link-add-account')}</Button>
    </div>
  {:else}
    <p class="error" role="alert">{failure ?? stateLine(stage)}</p>
    <div class="actions">
      <Button variant="ghost" type="button" onclick={() => (stage = 'entry')}>
        {t('sync-pairing-retry')}
      </Button>
      <Button variant="ghost" type="button" onclick={onback}>{t('common-back')}</Button>
    </div>
  {/if}
</div>

<style>
  .link {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--alloy-spacing-scale-4);
  }
  .link > :global(form) {
    width: 100%;
  }
  .step-head {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-2);
  }
  .eyebrow {
    margin: 0;
    font-size: var(--alloy-typography-size-xs);
    font-weight: var(--alloy-typography-weight-semibold);
    letter-spacing: var(--alloy-letter-spacing-caps, 0.08em);
    text-transform: uppercase;
    color: var(--alloy-color-accent-primary);
  }
  h3 {
    margin: 0;
    font-size: var(--alloy-typography-size-xl);
    font-weight: var(--alloy-typography-weight-semibold);
    letter-spacing: -0.01em;
    color: var(--alloy-color-text-primary);
  }
  .hint,
  .status,
  .error {
    margin: 0;
    font-size: var(--alloy-font-size-preview);
    line-height: var(--alloy-typography-line-height-normal, 1.45);
  }
  .hint {
    color: var(--alloy-color-text-muted);
  }
  .status {
    color: var(--alloy-color-text-secondary);
  }
  .error {
    color: var(--alloy-color-status-error);
  }
  .actions {
    display: flex;
    align-items: center;
    gap: var(--alloy-spacing-scale-2);
  }
</style>
