<script lang="ts">
  import { Button, SegmentedControl, TextArea } from '$lib/components/common';
  import { SettingGroup, SettingRow } from '$lib/components/settings/controls';
  import DeviceList from './DeviceList.svelte';
  import PairingAccept from './PairingAccept.svelte';
  import PairingOffer from './PairingOffer.svelte';
  import RecoveryPanel from './RecoveryPanel.svelte';
  import SasConfirm from './SasConfirm.svelte';
  import ServerPanel from './ServerPanel.svelte';
  import { formatSyncTime, groupPin } from './format';
  import type {
    ConfigureSyncInput,
    DeviceView,
    ExportView,
    ImportReportView,
    ImportView,
    PairingRole,
    PairingState,
    PairingView,
    PinMismatchView,
    RecoverFromPhraseInput,
    RecoveryView,
    RevokedView,
    ServerView,
    SyncResultView,
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
    /** A fresh device that has nothing but its phrase: offer the recovery branch. */
    fresh?: boolean;
    onrecover?: (input: RecoverFromPhraseInput) => void;
    /** Export result and import form, both rendered under This device. */
    exportState?: ExportView | null;
    importState?: ImportView | null;
    onexport?: () => void;
    onimport?: (bundleJson: string, apply: boolean) => void;
    /** The last `SyncNow` answer, rendered under the server panel. */
    syncResult?: SyncResultView | null;
    /** Set when a configured endpoint presented a different certificate. */
    pinMismatch?: PinMismatchView | null;
    ontrustnewpin?: () => void;
    /** The last `RevokeDevice` answer, rendered under the roster it changed. */
    revoked?: RevokedView | null;
    /** Failure that is not tied to one panel (roster load, revoke). */
    error?: string | null;
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
    fresh = false,
    onrecover = () => {},
    exportState = null,
    importState = null,
    onexport = () => {},
    onimport = () => {},
    syncResult = null,
    pinMismatch = null,
    ontrustnewpin = () => {},
    revoked = null,
    error = null,
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

  // Only the response's own fields are rendered. A zero is a server confirmation; null is silence.
  const leftoverCount = $derived(revoked?.blobsRemaining ?? 0);
  const hasLeftover = $derived(leftoverCount > 0);
  const partialRevoke = $derived(hasLeftover || revoked?.error != null);

  // The import field keeps its text locally; the dry-run result comes from the container. Apply is
  // offered only for the exact bundle that was checked, so editing the field hides it again.
  let bundleText = $state('');
  let checkedBundle = $state<string | null>(null);

  const importChanges = $derived.by(() => {
    const report: ImportReportView | null = importState?.report ?? null;
    if (!report) return '';
    const parts: string[] = [];
    if (report.settings) parts.push(t('sync-import-part-settings'));
    if (report.catalogue) parts.push(t('sync-import-part-catalogue'));
    if (report.tags > 0) parts.push(t('sync-import-part-tags', { count: report.tags }));
    if (report.backgrounds > 0) {
      parts.push(t('sync-import-part-backgrounds', { count: report.backgrounds }));
    }
    if (report.theme) parts.push(t('sync-import-part-theme'));
    return parts.join(', ');
  });

  const canApplyImport = $derived(
    importState?.report !== null &&
      importState?.report !== undefined &&
      checkedBundle === bundleText.trim() &&
      !importState.applied &&
      !importState.busy
  );

  function checkBundle() {
    const value = bundleText.trim();
    if (!value) return;
    checkedBundle = value;
    onimport(value, false);
  }

  function applyBundle() {
    const value = bundleText.trim();
    if (!value) return;
    onimport(value, true);
  }
</script>

<section class="devices-sync">
  {#if error}
    <p class="error section-error" role="alert">{error}</p>
  {/if}
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

    <div class="bundle">
      <p class="hint">{t('sync-export-description')}</p>
      <div class="bundle-actions">
        <Button variant="secondary" size="sm" onclick={onexport}>{t('sync-export')}</Button>
      </div>
      {#if exportState}
        {#if exportState.error}
          <p class="error" role="alert">{exportState.error}</p>
        {:else}
          <p class="status" role="status">
            {exportState.copied
              ? t('sync-export-ready', { bytes: exportState.bytes })
              : t('sync-export-copy-failed')}
          </p>
          <ul class="summary">
            {#if exportState.summary.settings}
              <li>{t('sync-export-summary-settings')}</li>
            {/if}
            {#if exportState.summary.catalogue}
              <li>{t('sync-export-summary-catalogue')}</li>
            {/if}
            <li>{t('sync-export-summary-tags', { count: exportState.summary.tags })}</li>
            {#if exportState.summary.backgrounds > 0}
              <li>
                {t('sync-export-summary-backgrounds', { count: exportState.summary.backgrounds })}
              </li>
            {/if}
            <li>
              {exportState.summary.theme
                ? t('sync-export-summary-theme', { id: exportState.summary.theme })
                : t('sync-export-summary-theme-none')}
            </li>
          </ul>
        {/if}
      {/if}
    </div>

    <div class="bundle">
      <p class="hint">{t('sync-import-description')}</p>
      <TextArea
        bind:value={bundleText}
        rows={3}
        placeholder={t('sync-import-placeholder')}
        aria-label={t('sync-import')}
      />
      <div class="bundle-actions">
        <Button
          variant="secondary"
          size="sm"
          disabled={importState?.busy || bundleText.trim() === ''}
          onclick={checkBundle}
        >
          {importState?.busy ? t('sync-import-checking') : t('sync-import-check')}
        </Button>
        {#if canApplyImport}
          <Button variant="primary" size="sm" disabled={importState?.busy} onclick={applyBundle}>
            {t('sync-import-apply')}
          </Button>
        {/if}
      </div>
      {#if importState?.error}
        <p class="error" role="alert">{importState.error}</p>
      {/if}
      {#if importState?.report}
        <p class="status" role="status">
          {importChanges
            ? t('sync-import-report', { changes: importChanges })
            : t('sync-import-no-changes')}
        </p>
      {/if}
      {#if importState?.applied}
        <p class="status" role="status">{t('sync-import-applied')}</p>
      {/if}
    </div>
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
    {#if syncResult}
      <p class="status" role="status">
        {t('sync-server-sync-result', {
          uploaded: syncResult.uploaded,
          downloaded: syncResult.downloaded,
          head: syncResult.headHex,
        })}
      </p>
    {/if}
    {#if pinMismatch}
      <div class="pin-mismatch" role="alert">
        <h4 class="pin-mismatch-title">{t('sync-pin-mismatch-title')}</h4>
        <p class="hint">{t('sync-pin-mismatch-description')}</p>
        <div class="fingerprints">
          <div class="fingerprint">
            <span class="fingerprint-label">{t('sync-pin-mismatch-stored')}</span>
            <code>{groupPin(pinMismatch.stored) || t('sync-pin-mismatch-unknown')}</code>
          </div>
          <div class="fingerprint">
            <span class="fingerprint-label">{t('sync-pin-mismatch-presented')}</span>
            <code>{groupPin(pinMismatch.presented) || t('sync-pin-mismatch-unknown')}</code>
          </div>
        </div>
        <Button
          variant="danger"
          size="sm"
          disabled={pinMismatch.trusting}
          onclick={ontrustnewpin}
        >
          {pinMismatch.trusting
            ? t('sync-pin-mismatch-trusting')
            : t('sync-pin-mismatch-trust')}
        </Button>
      </div>
    {/if}
  </SettingGroup>

  <SettingGroup title={t('sync-devices')} description={t('sync-devices-description')}>
    <p class="hint">{t('sync-devices-survivors')}</p>
    <DeviceList {devices} onrevoke={onrevoke} />

    {#if revoked}
      <div class="revoke-result" role="status">
        <h4 class="revoke-title">
          {t('sync-revoke-result-title', { device: revoked.label, epoch: revoked.epoch })}
        </h4>
        {#if revoked.resealed === true}
          <p>{t('sync-revoke-resealed')}</p>
        {:else if revoked.resealed === false}
          <p class="warning">{t('sync-revoke-not-resealed')}</p>
        {/if}
        {#if revoked.blobsDeleted !== null}
          <p>{t('sync-revoke-blobs-deleted', { count: revoked.blobsDeleted })}</p>
        {/if}
        {#if hasLeftover}
          <p class="warning">{t('sync-revoke-leftover', { count: leftoverCount })}</p>
        {/if}
        {#if revoked.error}
          <p class="warning">{t('sync-revoke-error', { message: revoked.error })}</p>
        {/if}
        {#if partialRevoke}
          <p class="warning">{t('sync-revoke-retroactive')}</p>
        {:else if revoked.blobsRemaining === 0}
          <p>{t('sync-revoke-clean')}</p>
        {/if}
      </div>
    {/if}
  </SettingGroup>

  <SettingGroup title={t('sync-recovery')} description={t('sync-recovery-description')}>
    <RecoveryPanel
      {recovery}
      {fresh}
      oncreate={oncreaterecovery}
      onverify={onverifyrecovery}
      onwritten={onrecoverywritten}
      onrecover={onrecover}
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

  .section-error {
    padding: 0;
  }

  .bundle {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--alloy-spacing-scale-2);
    padding: var(--alloy-spacing-scale-3) 0;
    width: 100%;
  }

  .bundle-actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--alloy-spacing-scale-2);
  }

  .status {
    margin: 0;
    font-size: var(--alloy-font-size-preview);
    color: var(--alloy-color-text-secondary);
  }

  .revoke-result {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-1);
    padding: var(--alloy-spacing-scale-3) 0;
    font-size: var(--alloy-font-size-preview);
    color: var(--alloy-color-text-secondary);
  }

  .revoke-result p {
    margin: 0;
  }

  .revoke-title {
    margin: 0;
    font-size: var(--alloy-font-size-row);
    font-weight: var(--alloy-typography-weight-medium);
    color: var(--alloy-color-text-primary);
  }

  .revoke-result .warning {
    color: var(--alloy-color-status-error);
  }

  .summary {
    display: flex;
    flex-wrap: wrap;
    gap: var(--alloy-spacing-scale-1) var(--alloy-spacing-scale-4);
    margin: 0;
    padding: 0;
    list-style: none;
    font-size: var(--alloy-font-size-meta);
    color: var(--alloy-color-text-muted);
  }

  .pin-mismatch {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--alloy-spacing-scale-3);
    margin-top: var(--alloy-spacing-scale-3);
    padding: var(--alloy-spacing-scale-4);
    border: 1px solid var(--alloy-color-status-error);
    border-radius: var(--alloy-radii-md);
    background: var(--alloy-color-surface-subtle);
  }

  .pin-mismatch-title {
    margin: 0;
    font-size: var(--alloy-font-size-title);
    font-weight: var(--alloy-typography-weight-semibold);
    color: var(--alloy-color-status-error);
  }

  .fingerprints {
    display: flex;
    flex-wrap: wrap;
    gap: var(--alloy-spacing-scale-5);
    width: 100%;
  }

  .fingerprint {
    display: flex;
    flex-direction: column;
    gap: var(--alloy-spacing-scale-1);
    min-width: 0;
  }

  .fingerprint-label {
    font-size: var(--alloy-font-size-meta);
    color: var(--alloy-color-text-muted);
  }

  .fingerprint code {
    font-family: var(--alloy-typography-font-family-mono);
    font-size: var(--alloy-font-size-preview);
    color: var(--alloy-color-text-primary);
    overflow-wrap: anywhere;
    user-select: all;
  }
</style>
