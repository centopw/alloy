<script lang="ts">
  import { onMount } from 'svelte';
  import { client } from '$lib/ipc';
  import { errorCode, errorMessage, expect } from '$lib/stores/errors';
  import { copyText } from '$lib/platform/clipboard';
  import { t } from '$lib/i18n';
  import type { DeviceDto, EngineEvent, SyncLayerStatus, SyncStatusReport } from '$lib/generated';
  import DevicesSync from './DevicesSync.svelte';
  import { parsePinMismatch, syncErrorMessage } from './errors';
  import type {
    ConfigureSyncInput,
    DeviceView,
    ExportView,
    ImportView,
    LayerView,
    PairingRole,
    PairingState,
    PairingView,
    PinMismatchView,
    RecoverFromPhraseInput,
    RecoveryView,
    ServerView,
    SyncResultView,
    ThisDeviceView,
  } from './types';

  const EMPTY_SERVER: ServerView = {
    mode: 'off',
    url: '',
    pin: null,
    tokenStored: false,
    test: 'idle',
    testError: null,
    lastSyncMs: null,
    lastError: null,
    syncing: false,
  };

  const EMPTY_EXPORT_SUMMARY = {
    settings: false,
    catalogue: false,
    tags: 0,
    backgrounds: 0,
    theme: null,
  };

  const LIVE_STATES: ReadonlySet<PairingState> = new Set([
    'waiting',
    'peer_found',
    'sas_ready',
    'transferring',
  ]);

  let device = $state<ThisDeviceView>({ label: '', enrolledAtMs: null, epoch: null });
  let server = $state<ServerView>({ ...EMPTY_SERVER });
  let pairing = $state<PairingView | null>(null);
  let role = $state<PairingRole>('offer');
  let devices = $state<DeviceView[]>([]);
  let recovery = $state<RecoveryView>({ hasPhrase: false, words: null, verified: false });
  let exportState = $state<ExportView | null>(null);
  let importState = $state<ImportView | null>(null);
  let syncResult = $state<SyncResultView | null>(null);
  let pinMismatch = $state<PinMismatchView | null>(null);
  let error = $state<string | null>(null);

  /** The live pairing the engine knows; null while the accepting side is still entering a ticket. */
  let pairingId: string | null = null;
  /** Configuration that failed on a pin mismatch, kept to retry with `accept_new_pin: true`. */
  let pendingConfigure: ConfigureSyncInput | null = null;
  let poll: ReturnType<typeof setInterval> | null = null;
  let stopEvents: (() => void) | null = null;

  function mapServer(report: SyncStatusReport): ServerView {
    return {
      ...server,
      mode: report.mode,
      url: report.url ?? '',
      pin: report.pin,
      tokenStored: report.token_configured,
      lastSyncMs: report.last_sync_ms,
      lastError: report.last_error ? syncErrorMessage({ code: report.last_error }) : null,
      syncing: report.state === 'running',
    };
  }

  function mapDevice(dto: DeviceDto): DeviceView {
    return {
      id: dto.device_id,
      label: dto.label,
      enrolledAtMs: dto.enrolled_ms,
      lastSyncMs: null,
      epoch: dto.epoch,
    };
  }

  function mapLayers(layers: SyncLayerStatus[]): LayerView[] {
    return layers.map((layer) => ({
      key: layer.key,
      enabled: layer.enabled,
      updatedAtMs: layer.updated_at_ms,
    }));
  }

  async function loadDevices() {
    const res = expect(await client.invoke({ action: 'ListDevices' }), 'DevicesList');
    const roster = res.payload.devices;
    const current = roster.find((entry) => entry.current);
    devices = roster.filter((entry) => !entry.current).map(mapDevice);
    if (current) {
      device = {
        label: current.label,
        enrolledAtMs: current.enrolled_ms,
        epoch: current.epoch,
      };
    }
  }

  async function loadStatus() {
    const res = expect(await client.invoke({ action: 'SyncStatus' }), 'SyncStatus');
    server = mapServer(res.payload.status);
  }

  async function load() {
    error = null;
    const [roster, status] = await Promise.allSettled([loadDevices(), loadStatus()]);
    if (roster.status === 'rejected') error = syncErrorMessage(roster.reason);
    else if (status.status === 'rejected') {
      server = { ...server, lastError: syncErrorMessage(status.reason) };
    }
  }

  function stopPoll() {
    if (poll !== null) {
      clearInterval(poll);
      poll = null;
    }
  }

  function applyPairing(next: PairingState, sas: string | null, failure: string | null) {
    if (!pairing) return;
    pairing = { ...pairing, state: next, sas: sas ?? pairing.sas, error: failure };
    if (!LIVE_STATES.has(next)) stopPoll();
  }

  /** The event is the source of truth; the poll only covers a missed `PairingChanged`. */
  async function pollPairing() {
    if (!pairingId || !pairing) return;
    try {
      const res = expect(
        await client.invoke({ action: 'PairingStatus', params: { pairing_id: pairingId } }),
        'PairingState'
      );
      applyPairing(
        res.payload.state,
        res.payload.sas,
        res.payload.error ? syncErrorMessage({ message: res.payload.error }) : null
      );
    } catch (e) {
      // A transient poll failure is not a transition: keep the state the event last reported.
      if (errorCode(e) === 'not_found') stopPoll();
    }
  }

  function startPoll() {
    stopPoll();
    poll = setInterval(() => void pollPairing(), 1000);
  }

  function onEvent(event: EngineEvent) {
    if (event.type === 'PairingChanged') {
      if (!pairing || event.payload.pairing_id !== pairingId) return;
      applyPairing(event.payload.state, event.payload.sas, null);
      return;
    }
    if (event.type === 'SyncStateChanged') {
      const { state, detail } = event.payload;
      server = { ...server, syncing: state === 'running' };
      if (state === 'blocked') {
        // `detail` is the chain reason (fork, branch, signature, chain), not a sentence.
        server = { ...server, lastError: syncErrorMessage({ code: 'sync_blocked' }) };
      } else if (state === 'failed') {
        server = { ...server, lastError: detail ?? t('sync-error-sync-failed') };
      }
    }
  }

  async function beginPairing(nextRole: PairingRole) {
    role = nextRole;
    pairingId = null;
    stopPoll();
    pairing = null;

    // The accepting side has no engine pairing until it joins with a ticket; the form needs a
    // waiting view to render, and that placeholder never touches the engine.
    if (nextRole === 'accept') {
      pairing = {
        state: 'waiting',
        ticket: null,
        code: null,
        sas: null,
        peerLabel: null,
        expiresAtMs: null,
        error: null,
      };
      return;
    }

    try {
      const res = expect(
        await client.invoke({ action: 'BeginPairing', params: { role: 'offer' } }),
        'PairingStarted'
      );
      pairingId = res.payload.pairing_id;
      pairing = {
        state: 'waiting',
        ticket: res.payload.ticket,
        code: res.payload.code,
        sas: null,
        peerLabel: null,
        expiresAtMs: res.payload.expires_at_ms,
        error: null,
      };
      startPoll();
    } catch (e) {
      pairing = {
        state: 'failed',
        ticket: null,
        code: null,
        sas: null,
        peerLabel: null,
        expiresAtMs: null,
        error: syncErrorMessage(e),
      };
    }
  }

  async function submitPairing(ticket: string, code: string) {
    try {
      const res = expect(
        await client.invoke({ action: 'AcceptPairingTicket', params: { ticket, code } }),
        'PairingStarted'
      );
      pairingId = res.payload.pairing_id;
      pairing = {
        state: 'peer_found',
        ticket: res.payload.ticket,
        code: res.payload.code,
        sas: null,
        peerLabel: null,
        expiresAtMs: res.payload.expires_at_ms,
        error: null,
      };
      startPoll();
    } catch (e) {
      if (!pairing) return;
      pairing = { ...pairing, state: 'failed', error: syncErrorMessage(e) };
      stopPoll();
    }
  }

  async function confirmPairing(accept: boolean) {
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
      if (res.payload.state === 'done') await loadDevices();
    } catch (e) {
      if (!pairing) return;
      pairing = { ...pairing, state: 'failed', error: syncErrorMessage(e) };
      stopPoll();
    }
  }

  function restartPairing() {
    void beginPairing(role);
  }

  async function configure(input: ConfigureSyncInput) {
    pendingConfigure = input;
    pinMismatch = null;
    try {
      const res = expect(
        await client.invoke({
          action: 'ConfigureSync',
          params: {
            url: input.url,
            token: input.token ?? '',
            mode: input.mode,
            accept_new_pin: false,
          },
        }),
        'SyncConfigured'
      );
      pendingConfigure = null;
      server = { ...mapServer(res.payload.status), test: 'ok', testError: null };
      pinMismatch = null;
    } catch (e) {
      pinMismatch = pinMismatchFrom(e);
      server = { ...server, test: 'failed', testError: syncErrorMessage(e) };
    }
  }

  function pinMismatchFrom(e: unknown): PinMismatchView | null {
    if (errorCode(e) !== 'pin_mismatch') return null;
    const parsed = parsePinMismatch(errorMessage(e), server.pin ?? '');
    return { stored: parsed.stored, presented: parsed.presented, trusting: false };
  }

  async function trustNewPin() {
    const current = pinMismatch;
    if (!current) return;
    pinMismatch = { ...current, trusting: true };
    const input: ConfigureSyncInput = pendingConfigure ?? {
      mode: server.mode,
      url: server.url,
      token: null,
    };
    try {
      const res = expect(
        await client.invoke({
          action: 'ConfigureSync',
          params: {
            url: input.url,
            token: input.token ?? '',
            mode: input.mode,
            accept_new_pin: true,
          },
        }),
        'SyncConfigured'
      );
      pendingConfigure = null;
      pinMismatch = null;
      server = { ...mapServer(res.payload.status), test: 'ok', testError: null };
    } catch (e) {
      pinMismatch = { ...current, trusting: false };
      server = { ...server, test: 'failed', testError: syncErrorMessage(e) };
    }
  }

  async function testConnection() {
    server = { ...server, test: 'running', testError: null };
    try {
      const res = expect(await client.invoke({ action: 'SyncStatus' }), 'SyncStatus');
      const report = res.payload.status;
      server = { ...mapServer(report), test: report.last_error ? 'failed' : 'ok' };
      server = {
        ...server,
        testError: report.last_error
          ? syncErrorMessage({ code: report.last_error })
          : null,
      };
    } catch (e) {
      server = { ...server, test: 'failed', testError: syncErrorMessage(e) };
    }
  }

  async function syncNow() {
    server = { ...server, syncing: true, lastError: null };
    syncResult = null;
    try {
      const res = expect(await client.invoke({ action: 'SyncNow' }), 'SyncCompleted');
      syncResult = {
        uploaded: res.payload.uploaded,
        downloaded: res.payload.downloaded,
        headHex: res.payload.head_hex,
      };
      await loadStatus();
    } catch (e) {
      pinMismatch = pinMismatchFrom(e);
      server = { ...server, lastError: syncErrorMessage(e) };
    } finally {
      server = { ...server, syncing: false };
    }
  }

  async function revokeDevice(deviceId: string) {
    error = null;
    try {
      expect(await client.invoke({ action: 'RevokeDevice', params: { device_id: deviceId } }), 'DeviceRevoked');
      await loadDevices();
    } catch (e) {
      error = syncErrorMessage(e);
    }
  }

  async function createRecovery() {
    try {
      const res = expect(
        await client.invoke({ action: 'CreateRecoveryPhrase' }),
        'RecoveryPhraseCreated'
      );
      recovery = { hasPhrase: true, words: res.payload.words, verified: false, error: null };
    } catch (e) {
      recovery = { ...recovery, error: syncErrorMessage(e) };
    }
  }

  function recoveryWritten() {
    // The words exist only in the response that created them; forget them once written down.
    recovery = { ...recovery, words: null, hasPhrase: true };
  }

  async function verifyRecovery(words: string[]) {
    try {
      expect(
        await client.invoke({ action: 'VerifyRecoveryPhrase', params: { words } }),
        'RecoveryVerified'
      );
      recovery = { ...recovery, verified: true, error: null };
    } catch (e) {
      recovery = { ...recovery, verified: false, error: syncErrorMessage(e) };
    }
  }

  /**
   * The phrase is the only way back for a device with no configuration: no sibling to pair with,
   * no server endpoint yet, no master key. The pin is captured on the first `https` connection.
   */
  const fresh = $derived(
    server.mode === 'off' && devices.length === 0 && !recovery.hasPhrase
  );

  async function recoverFromPhrase(input: RecoverFromPhraseInput) {
    recovery = { ...recovery, recovering: true, error: null, recovered: null };
    try {
      const res = expect(
        await client.invoke({
          action: 'RecoverFromPhrase',
          params: {
            words: input.words,
            url: input.url,
            token: input.token,
            accept_new_pin: false,
          },
        }),
        'Recovered'
      );
      // The page must reflect the device the engine now is, not only what the response carried.
      await load();
      recovery = {
        hasPhrase: true,
        // The phrase opened the escrow; the engine now holds the wrapped blob.
        words: null,
        verified: false,
        recovering: false,
        error: null,
        recovered: {
          layers: mapLayers(res.payload.layers),
          roster: res.payload.devices.map(mapDevice),
          pin: server.pin,
        },
      };
    } catch (e) {
      // A rejected phrase writes nothing; keep the typed words and say only that it was wrong.
      recovery = { ...recovery, recovering: false, error: syncErrorMessage(e) };
    }
  }

  async function exportConfig() {
    exportState = null;
    try {
      const res = expect(
        await client.invoke({
          action: 'ExportConfigBundle',
          params: { include_backgrounds: false, include_themes: true },
        }),
        'ConfigBundleExported'
      );
      let copied = false;
      try {
        await copyText(res.payload.bundle_json);
        copied = true;
      } catch {
        copied = false;
      }
      exportState = {
        bytes: res.payload.bytes,
        summary: res.payload.summary,
        copied,
        error: null,
      };
    } catch (e) {
      exportState = {
        bytes: 0,
        summary: EMPTY_EXPORT_SUMMARY,
        copied: false,
        error: syncErrorMessage(e),
      };
    }
  }

  async function importConfig(bundleJson: string, apply: boolean) {
    const previous = importState;
    importState = {
      report: apply ? (previous?.report ?? null) : null,
      applied: false,
      busy: true,
      error: null,
    };
    try {
      const res = expect(
        await client.invoke({
          action: 'ImportConfigBundle',
          params: { bundle_json: bundleJson, apply },
        }),
        'ConfigBundleImported'
      );
      importState = {
        report: res.payload.report,
        applied: res.payload.applied,
        busy: false,
        error: null,
      };
      if (res.payload.applied) await loadStatus();
    } catch (e) {
      importState = {
        report: apply ? (previous?.report ?? null) : null,
        applied: false,
        busy: false,
        error: syncErrorMessage(e),
      };
    }
  }

  onMount(() => {
    stopEvents = client.subscribe(onEvent);
    void load();
    return () => {
      stopEvents?.();
      stopPoll();
      // Closing the section drops the recovery words, as the panel shows them exactly once.
      recovery = { ...recovery, words: null };
    };
  });
</script>

<DevicesSync
  {device}
  {server}
  {pairing}
  {devices}
  {recovery}
  {fresh}
  {role}
  {exportState}
  {importState}
  {syncResult}
  {pinMismatch}
  {error}
  onbeginpairing={(next) => void beginPairing(next)}
  onsubmitpairing={(ticket, code) => void submitPairing(ticket, code)}
  onconfirmpairing={(accept) => void confirmPairing(accept)}
  onrestartpairing={restartPairing}
  onconfigure={(input) => void configure(input)}
  ontestconnection={() => void testConnection()}
  onsyncnow={() => void syncNow()}
  onrevoke={(deviceId) => void revokeDevice(deviceId)}
  oncreaterecovery={() => void createRecovery()}
  onverifyrecovery={(words) => void verifyRecovery(words)}
  onrecoverywritten={recoveryWritten}
  onrecover={(input) => void recoverFromPhrase(input)}
  onexport={() => void exportConfig()}
  onimport={(bundle, apply) => void importConfig(bundle, apply)}
  ontrustnewpin={() => void trustNewPin()}
/>
