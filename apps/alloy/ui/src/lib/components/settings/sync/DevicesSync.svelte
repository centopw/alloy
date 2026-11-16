<script lang="ts">
  import { Button, SegmentedControl } from '$lib/components/common';
  import { SettingGroup, SettingRow } from '$lib/components/settings/controls';
  import DeviceList from './DeviceList.svelte';
  import PairingAccept from './PairingAccept.svelte';
  import PairingOffer from './PairingOffer.svelte';
  import RecoveryPanel from './RecoveryPanel.svelte';
  import SasConfirm from './SasConfirm.svelte';
  import ServerPanel from './ServerPanel.svelte';
  import { formatSyncTime } from './format';
  import type {
    ConfigureSyncInput,
    DeviceView,
    PairingRole,
    PairingState,
    PairingView,
    RecoveryView,
    ServerView,
    ThisDeviceView,
  } from './types';

  import { t } from '$lib/i18n';

  interface Props {
    device?: ThisDeviceView;
    server?: ServerView;
    pairing?: PairingView | null;
    devices?: DeviceView[];
    recovery?: RecoveryView;
    /** Which side the user picked before the engine minted a pairing. */
    role?: PairingRole;
    onbeginpairing?: (role: PairingRole) => void;
    onsubmitpairing?: (ticket: string, code: string) => void;
    onconfirmpairing?: (accept: boolean) => void;
    onrestartpairing?: () => void;
    onconfigure?: (input: ConfigureSyncInput) => void;
    ontestconnection?: () => void;
    onsyncnow?: () => void;
    onrevoke?: (deviceId: string) => void;
    oncreaterecovery?: () => void;
    onverifyrecovery?: (words: string[]) => void;
    onrecoverywritten?: () => void;
  }

  let {
    device = { label: '', enrolledAtMs: null, epoch: null },
    server = {
      mode: 'off',
      url: '',
      pin: null,
      tokenStored: false,
      test: 'idle',
      testError: null,
      lastSyncMs: null,
      lastError: null,
      syncing: false,
    },
    pairing = null,
    devices = [],
    recovery = { hasPhrase: false, words: null, verified: false },
    role = 'offer',
    onbeginpairing = () => {},
    onsubmitpairing = () => {},
    onconfirmpairing = () => {},
    onrestartpairing = () => {},
    onconfigure = () => {},
    ontestconnection = () => {},
    onsyncnow = () => {},
    onrevoke = () => {},
    oncreaterecovery = () => {},
    onverifyrecovery = () => {},
    onrecoverywritten = () => {},
  }: Props = $props();

  // Held locally because the segmented control writes it before the engine has a pairing; the
  // prop wins again whenever the caller changes it.
  let currentRole = $state<PairingRole>('offer');
  $effect(() => {
    currentRole = role;
  });

  const roleOptions = $derived([
    { value: 'offer', label: t('sync-pair-role-offer') },
    { value: 'accept', label: t('sync-pair-role-accept') },
  ]);

  /** One sentence per engine state; the panel under it carries the actions for that state. */
  function pairingStateText(state: PairingState): string {
    switch (state) {
      case 'waiting':
        return t('sync-pairing-state-waiting');
      case 'peer_found':
        return t('sync-pairing-state-peer-found');
      case 'sas_ready':
        return t('sync-pairing-state-sas-ready');
      case 'transferring':
        return t('sync-pairing-state-transferring');
      case 'done':
        return t('sync-pairing-state-done');
      case 'declined':
        return t('sync-pairing-state-declined');
      case 'expired':
        return t('sync-pairing-state-expired');
      default:
        return t('sync-pairing-state-failed');
    }
  }

  const pairingActive = $derived(
    pairing !== null &&
      (pairing.state === 'waiting' ||
        pairing.state === 'peer_found' ||
        pairing.state === 'sas_ready' ||
        pairing.state === 'transferring')
  );

  const enrolled = $derived(formatSyncTime(device.enrolledAtMs));
</script>

<section class="devices-sync">
  <SettingGroup title={t('sync-this-device')} description={t('sync-this-device-description')}>
    <SettingRow label={t('sync-this-device-label')}>
      <span class="value">{device.label || t('sync-unknown')}</span>
    </SettingRow>
    <SettingRow label={t('sync-this-device-enrolled')}>
      <span class="value">{enrolled ?? t('sync-unknown')}</span>
    </SettingRow>
    <SettingRow label={t('sync-this-device-epoch')}>
      <span class="value">{device.epoch ?? t('sync-unknown')}</span>
    </SettingRow>
  </SettingGroup>

  <SettingGroup title={t('sync-pair-a-device')} description={t('sync-pair-description')}>
    <div class="pair-controls">
      <SegmentedControl
        options={roleOptions}
        value={currentRole}
        label={t('sync-pair-role')}
        onchange={(value) => (currentRole = value === 'accept' ? 'accept' : 'offer')}
      />
      <Button
        variant="secondary"
        size="sm"
        disabled={pairingActive}
        onclick={() => onbeginpairing(currentRole)}
      >
        {t('sync-pair-start')}
      </Button>
    </div>

    <!-- A fixed-height body: the QR, the code form and every status line share one box, so the
         panels below never move when the state machine advances. -->
    <div class="pair-body" aria-live="polite">
      {#if pairing === null}
        <p class="hint">{t('sync-pair-idle')}</p>
      {:else}
        <p class="pair-status" data-pairing-state={pairing.state} role="status">
          {pairingStateText(pairing.state)}
        </p>

        {#if pairing.state === 'failed' && pairing.error}
          <p class="error" role="alert">{pairing.error}</p>
        {/if}

        {#if pairing.state === 'sas_ready'}
          <SasConfirm
            sas={pairing.sas ?? ''}
            peerLabel={pairing.peerLabel}
            onmatch={() => onconfirmpairing(true)}
            onnomatch={() => onconfirmpairing(false)}
          />
        {:else if pairing.state === 'waiting' || pairing.state === 'peer_found'}
          {#if currentRole === 'offer'}
            <PairingOffer
              ticket={pairing.ticket}
              code={pairing.code}
              expiresAtMs={pairing.expiresAtMs}
            />
          {:else}
            <PairingAccept
              disabled={pairing.state !== 'waiting'}
              onsubmit={(ticket, code) => onsubmitpairing(ticket, code)}
            />
          {/if}
        {:else}
          <div class="terminal">
            <Button variant="secondary" size="sm" onclick={onrestartpairing}>
              {t('sync-pairing-retry')}
            </Button>
          </div>
        {/if}
      {/if}
    </div>
  </SettingGroup>

  <SettingGroup title={t('sync-server')} description={t('sync-server-description')}>
    <ServerPanel
      {server}
      onconfigure={onconfigure}
      ontestconnection={ontestconnection}
      onsyncnow={onsyncnow}
    />
  </SettingGroup>

  <SettingGroup title={t('sync-devices')} description={t('sync-devices-description')}>
    <DeviceList {devices} onrevoke={onrevoke} />
  </SettingGroup>

  <SettingGroup title={t('sync-recovery')} description={t('sync-recovery-description')}>
    <RecoveryPanel
      {recovery}
      oncreate={oncreaterecovery}
      onverify={onverifyrecovery}
      onwritten={onrecoverywritten}
    />
  </SettingGroup>
</section>

<style>
  .devices-sync {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-6);
  }

  .pair-controls {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--alloy-spacing-scale-3);
    padding: var(--alloy-spacing-scale-2) 0;
  }

  .pair-body {
    box-sizing: border-box;
    min-height: 26rem;
    padding-top: var(--alloy-spacing-scale-2);
  }

  .pair-status {
    margin: 0;
    padding: var(--alloy-spacing-scale-1) 0;
    font-size: var(--alloy-font-size-row);
    font-weight: var(--alloy-typography-weight-medium);
    color: var(--alloy-color-text-primary);
  }

  .hint,
  .error {
    margin: 0;
    font-size: var(--alloy-font-size-preview);
  }

  .hint {
    color: var(--alloy-color-text-muted);
  }

  .error {
    padding: var(--alloy-spacing-scale-1) 0;
    color: var(--alloy-color-status-error);
    overflow-wrap: anywhere;
  }

  .terminal {
    display: flex;
    gap: var(--alloy-spacing-scale-2);
    padding-top: var(--alloy-spacing-scale-3);
  }

  .value {
    font-size: var(--alloy-font-size-preview);
    color: var(--alloy-color-text-secondary);
  }
</style>
