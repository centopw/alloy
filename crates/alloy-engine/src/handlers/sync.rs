//! Devices and sync: the configuration bundle, LAN pairing, the recovery phrase and the server
//! commands (ADR 0020).
//!
//! The bundle, pairing and recovery phrase are local: pairing is device-to-device over the LAN
//! and the bundle is read from and written to the files the rest of the engine already owns.
//! `ConfigureSync`, `SyncStatus` and `SyncNow` delegate to [`crate::sync`], which is the only
//! module that talks to the sync server; its secrets stay in the platform secret store and the
//! endpoint is pinned before a request carries the token.
//!
//! Three pieces of state live outside the files this handler writes:
//!
//! - the master key and this device's Ed25519 key are in the platform secret store, created on
//!   first use and never in `settings.json`;
//! - the device roster, its version and the local key epoch are in `settings.json` under `sync`;
//! - the recovery verifier (never the phrase and never a key) sits beside the roster.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::Write;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, UNIX_EPOCH};

use alloy_protocol::{
    ConfigBundleSummary, ConfigImportReport, DeviceDto, EngineCommand, EngineEvent, EngineResponse,
    PairingPhase, PairingRole, RoomTagDef, SyncLayerStatus,
};
use alloy_storage::StorageError;
use alloy_storage::config_bundle::{
    BUNDLE_SCHEMA, BackgroundAsset, BundleError, ChatKey, ConfigBundle, ConfigState, TagSet,
    ThemeSelection,
};
use alloy_storage::pairing::{
    self, CODE_TTL, DeviceProver, PairedBundle, PairedPeer, PairingError, PairingOffer,
    PairingTicket,
};
use alloy_store::ChatFlagsUpdate;
use alloy_sync::keys::MasterKey;
use argon2::{Algorithm, Argon2, Params, Version};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use bip39::{Language, Mnemonic};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::sync::oneshot;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::context::EngineContext;
use crate::error::{EngineError, map_store};
use crate::files;
use crate::handlers::settings::SettingsHandler;
use crate::router::{CommandGroup, GroupHandler, command_name};
use crate::sink::EventSink;
use crate::support::now_ms;

/// Version of the `sync` section of `settings.json`. A later slice bumps it and migrates.
const SYNC_VERSION: u32 = 1;

/// Secret-store keys. Both values are 32 random bytes, base64 in the store.
const MASTER_KEY_SECRET: &str = "alloy.sync.master_key.v1";
const DEVICE_KEY_SECRET: &str = "alloy.device.ed25519_key.v1";

/// `settings.json` keys that describe this installation, not the account. They never cross in a
/// bundle: an import must not move a window size or an updater channel. `sync` is here for a
/// harder reason: it holds this device's roster and epoch, so an incoming bundle that carried it
/// could rewrite the trust root.
const DEVICE_LOCAL_SETTINGS: &[&str] = &["window", "updater", "debug", "sync"];

/// Where user background files live, relative to the data directory.
const BACKGROUND_DIR: &str = "media/backgrounds";

/// Where user theme files live, relative to the data directory.
const THEME_DIR: &str = "themes";

/// Argon2id cost for the recovery wrapper. 19 MiB, two passes: the RFC 9106 second recommended
/// setting, which is priced for a desktop and still finishes in a fraction of a second.
const RECOVERY_M_COST: u32 = 19_456;
const RECOVERY_T_COST: u32 = 2;
const RECOVERY_P_COST: u32 = 1;
const RECOVERY_KEY_LEN: usize = 32;
const RECOVERY_SALT_LEN: usize = 16;
const RECOVERY_NONCE_LEN: usize = 12;
const ESCROW_VERSION: u32 = 1;
/// AAD of the escrow body: the blob is bound to its purpose, so it cannot be opened as anything
/// else alloy-sync seals.
const ESCROW_AAD: &[u8] = b"alloy-sync-v1/recovery/escrow/1";

/// Words in the recovery phrase. BIP39 with 256 bits of entropy: 24 words.
const RECOVERY_WORDS: usize = 24;

/// How long a pairing waits for the two people to compare the SAS before it gives up.
const DECISION_TIMEOUT: Duration = Duration::from_secs(120);

/// One entry of the local device roster, exactly as `settings.json` keeps it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeviceEntry {
    device_id: String,
    label: String,
    enrolled_ms: i64,
    epoch: u32,
}

/// The recovery verifier. It names the KDF inputs so a later verification can rebuild the escrow
/// blob byte for byte; the blob itself is not kept, and neither is the phrase.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RecoveryRecord {
    version: u32,
    m_cost: u32,
    t_cost: u32,
    p_cost: u32,
    salt_b64: String,
    nonce_b64: String,
    verifier_hex: String,
}

/// The escrow blob a later slice uploads. Serialised as canonical JSON and never stored here.
#[derive(Debug, Serialize)]
struct EscrowBlob<'a> {
    version: u32,
    kdf: &'a str,
    m_cost: u32,
    t_cost: u32,
    p_cost: u32,
    salt_b64: String,
    nonce_b64: String,
    ciphertext_b64: String,
}

/// A live pairing: its last phase, the SAS once the handshake produced one, and the channel that
/// carries the user's accept/decline decision to the driver task.
struct PairingEntry {
    phase: PairingPhase,
    sas: Option<String>,
    peer_label: Option<String>,
    transferred: u64,
    error: Option<String>,
    decision: Option<oneshot::Sender<bool>>,
}

impl PairingEntry {
    fn waiting() -> Self {
        Self {
            phase: PairingPhase::Waiting,
            sas: None,
            peer_label: None,
            transferred: 0,
            error: None,
            decision: None,
        }
    }
}

type Pairings = Arc<Mutex<HashMap<String, PairingEntry>>>;

/// Handler for devices and sync (`CommandGroup::Sync`).
pub struct SyncHandler {
    pairings: Pairings,
}

impl Default for SyncHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl SyncHandler {
    pub fn new() -> Self {
        Self {
            pairings: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[async_trait::async_trait]
impl GroupHandler for SyncHandler {
    fn group(&self) -> CommandGroup {
        CommandGroup::Sync
    }

    async fn handle(
        &self,
        ctx: &EngineContext,
        cmd: EngineCommand,
    ) -> Result<EngineResponse, EngineError> {
        let name = command_name(&cmd);
        match cmd {
            EngineCommand::ExportConfigBundle {
                include_backgrounds,
                include_themes,
            } => {
                let bundle = build_bundle(ctx, include_backgrounds, include_themes).await?;
                let summary = ConfigBundleSummary {
                    settings: !bundle.settings.is_null(),
                    catalogue: !bundle.catalogue.is_null(),
                    tags: bundle.tags.len(),
                    backgrounds: bundle.backgrounds.len(),
                    theme: bundle.theme.as_ref().map(|theme| theme.id.clone()),
                };
                let bytes = bundle.to_json().map_err(map_bundle)?;
                let json = String::from_utf8(bytes.clone())
                    .map_err(|_| EngineError::internal_error("bundle is not UTF-8"))?;
                Ok(EngineResponse::ConfigBundleExported {
                    bundle_json: json,
                    bytes: bytes.len() as u64,
                    summary,
                })
            }
            EngineCommand::ImportConfigBundle { bundle_json, apply } => {
                let (report, applied) = import_bundle(ctx, &bundle_json, apply).await?;
                Ok(EngineResponse::ConfigBundleImported { report, applied })
            }
            EngineCommand::BeginPairing { role } => self.begin_pairing(ctx, role).await,
            EngineCommand::AcceptPairingTicket { ticket, code } => {
                self.accept_pairing_ticket(ctx, ticket, code)
            }
            EngineCommand::PairingStatus { pairing_id } => {
                let pairings = lock(&self.pairings);
                let entry = pairings
                    .get(&pairing_id)
                    .ok_or_else(|| EngineError::not_found(format!("pairing '{pairing_id}'")))?;
                Ok(pairing_state(entry))
            }
            EngineCommand::ConfirmPairing { pairing_id, accept } => {
                let mut pairings = lock(&self.pairings);
                let entry = pairings
                    .get_mut(&pairing_id)
                    .ok_or_else(|| EngineError::not_found(format!("pairing '{pairing_id}'")))?;
                let Some(decision) = entry.decision.take() else {
                    return Err(EngineError::invalid_argument(
                        "the pairing has not reached the SAS comparison yet",
                    ));
                };
                // The receiving task may already have gone; that is an expired pairing, and the
                // PairingChanged event says so.
                let _ = decision.send(accept);
                Ok(pairing_state(entry))
            }
            EngineCommand::CreateRecoveryPhrase => {
                let words = create_recovery_phrase(ctx).await?;
                Ok(EngineResponse::RecoveryPhraseCreated { words })
            }
            EngineCommand::VerifyRecoveryPhrase { words } => {
                verify_recovery_phrase(ctx, &words).await?;
                Ok(EngineResponse::RecoveryVerified)
            }
            EngineCommand::ListDevices => {
                let devices = list_devices(ctx).await?;
                Ok(EngineResponse::DevicesList { devices })
            }
            EngineCommand::RevokeDevice { device_id } => {
                let epoch = revoke_device(ctx, &device_id).await?;
                Ok(EngineResponse::DeviceRevoked { epoch })
            }
            EngineCommand::ConfigureSync {
                url,
                token,
                mode,
                accept_new_pin,
            } => {
                let (pin, status) =
                    crate::sync::configure(ctx, url, token, mode, accept_new_pin).await?;
                Ok(EngineResponse::SyncConfigured { pin, status })
            }
            EngineCommand::SyncStatus => {
                let status = crate::sync::status(ctx).await?;
                Ok(EngineResponse::SyncStatus { status })
            }
            EngineCommand::SyncNow => {
                let (uploaded, downloaded, head_hex, epoch) = crate::sync::sync_now(ctx).await?;
                Ok(EngineResponse::SyncCompleted {
                    uploaded,
                    downloaded,
                    head_hex,
                    epoch,
                })
            }
            _ => Err(EngineError::invalid_argument(format!(
                "Command '{}' does not belong to Sync group",
                name
            ))),
        }
    }
}

// === Pairing ===

impl SyncHandler {
    /// Starts the offering side: bind a LAN port, show ticket and code, and wait for one peer.
    async fn begin_pairing(
        &self,
        ctx: &EngineContext,
        role: PairingRole,
    ) -> Result<EngineResponse, EngineError> {
        if role == PairingRole::Accept {
            return Err(EngineError::invalid_argument(
                "BeginPairing only starts the offering side; the new device joins with AcceptPairingTicket",
            ));
        }
        // Backgrounds stay home: the ADR's enrolment payload is settings, tags, the catalogue and
        // the theme, and background images would dominate the transfer for no configuration gain.
        let bundle = build_bundle(ctx, false, true).await?;
        let bytes = bundle.to_json().map_err(map_bundle)?.len() as u64;
        let prover = EngineProver::load(ctx).await?;
        let offer = PairingOffer::bind(bundle, lan_bind_addr(), None, Arc::new(prover))
            .await
            .map_err(map_pairing)?;
        let ticket = offer.ticket().clone();
        let code = offer.code().to_string();
        let pairing_id = ticket.pairing_id.clone();
        let expires_at_ms = now_ms() + CODE_TTL.as_millis() as i64;

        {
            let mut pairings = lock(&self.pairings);
            pairings.insert(pairing_id.clone(), PairingEntry::waiting());
        }
        spawn_offer(
            ctx.clone(),
            self.pairings.clone(),
            pairing_id.clone(),
            offer,
            bytes,
        );

        Ok(EngineResponse::PairingStarted {
            pairing_id,
            ticket: ticket.to_qr_string(),
            code,
            endpoint: ticket.endpoint,
            expires_at_ms,
        })
    }

    /// Joins a pairing from the new device and starts the driver task in the background.
    fn accept_pairing_ticket(
        &self,
        ctx: &EngineContext,
        ticket: String,
        code: String,
    ) -> Result<EngineResponse, EngineError> {
        let parsed = PairingTicket::parse(&ticket).map_err(map_pairing)?;
        let pairing_id = parsed.pairing_id.clone();
        let expires_at_ms = now_ms() + CODE_TTL.as_millis() as i64;

        {
            let mut pairings = lock(&self.pairings);
            pairings.insert(pairing_id.clone(), PairingEntry::waiting());
        }
        ctx.sink.emit(EngineEvent::PairingChanged {
            pairing_id: pairing_id.clone(),
            state: PairingPhase::Waiting,
            sas: None,
        });
        spawn_accept(
            ctx.clone(),
            self.pairings.clone(),
            pairing_id.clone(),
            parsed.clone(),
            code.clone(),
        );

        Ok(EngineResponse::PairingStarted {
            pairing_id,
            ticket: parsed.to_qr_string(),
            code,
            endpoint: parsed.endpoint,
            expires_at_ms,
        })
    }
}

fn pairing_state(entry: &PairingEntry) -> EngineResponse {
    EngineResponse::PairingState {
        state: entry.phase,
        sas: entry.sas.clone(),
        peer_label: entry.peer_label.clone(),
        transferred: entry.transferred,
        error: entry.error.clone(),
    }
}

fn spawn_offer(
    ctx: EngineContext,
    pairings: Pairings,
    pairing_id: String,
    offer: PairingOffer,
    bundle_bytes: u64,
) {
    tokio::spawn(async move {
        let sink = ctx.sink.clone();
        let handshake = match offer.wait_for_peer().await {
            Ok(handshake) => handshake,
            Err(err) => {
                finish_pairing(
                    &pairings,
                    &sink,
                    &pairing_id,
                    phase_for(&err),
                    Some(err.to_string()),
                );
                return;
            }
        };

        let sas = handshake.sas().to_string();
        let Some(accepted) = begin_sas(&pairings, &sink, &pairing_id, sas).await else {
            finish_pairing(
                &pairings,
                &sink,
                &pairing_id,
                PairingPhase::Expired,
                Some("nobody confirmed the pairing in time".to_string()),
            );
            return;
        };
        if accepted {
            set_phase(&pairings, &sink, &pairing_id, PairingPhase::Transferring);
        }
        match handshake.finish(accepted).await {
            Ok(peer) => {
                let outcome = match verify_peer(&peer) {
                    Ok((device_id, label)) => enrol_verified_peer(&ctx, device_id, label.clone())
                        .await
                        .map(|()| label),
                    Err(err) => Err(err),
                };
                match outcome {
                    Ok(label) => {
                        set_peer_label(&pairings, &pairing_id, label);
                        set_transferred(&pairings, &pairing_id, bundle_bytes);
                        finish_pairing(&pairings, &sink, &pairing_id, PairingPhase::Done, None);
                    }
                    Err(err) => finish_pairing(
                        &pairings,
                        &sink,
                        &pairing_id,
                        PairingPhase::Failed,
                        Some(err.message),
                    ),
                }
            }
            Err(err) => finish_pairing(
                &pairings,
                &sink,
                &pairing_id,
                phase_for(&err),
                Some(err.to_string()),
            ),
        }
    });
}

fn spawn_accept(
    ctx: EngineContext,
    pairings: Pairings,
    pairing_id: String,
    ticket: PairingTicket,
    code: String,
) {
    tokio::spawn(async move {
        let sink = ctx.sink.clone();
        let prover = match EngineProver::load(&ctx).await {
            Ok(prover) => prover,
            Err(err) => {
                finish_pairing(
                    &pairings,
                    &sink,
                    &pairing_id,
                    PairingPhase::Failed,
                    Some(err.message),
                );
                return;
            }
        };
        let connected = tokio::time::timeout(
            CODE_TTL,
            pairing::connect(&ticket.endpoint, &ticket, &code, &prover),
        )
        .await;
        let pending = match connected {
            Err(_) => {
                finish_pairing(
                    &pairings,
                    &sink,
                    &pairing_id,
                    PairingPhase::Expired,
                    Some("the offering device stopped listening".to_string()),
                );
                return;
            }
            Ok(Err(err)) => {
                finish_pairing(
                    &pairings,
                    &sink,
                    &pairing_id,
                    phase_for(&err),
                    Some(err.to_string()),
                );
                return;
            }
            Ok(Ok(pending)) => pending,
        };

        let sas = pending.sas().to_string();
        let Some(accepted) = begin_sas(&pairings, &sink, &pairing_id, sas).await else {
            finish_pairing(
                &pairings,
                &sink,
                &pairing_id,
                PairingPhase::Expired,
                Some("nobody confirmed the pairing in time".to_string()),
            );
            return;
        };
        if accepted {
            set_phase(&pairings, &sink, &pairing_id, PairingPhase::Transferring);
        }
        match pending.finish(accepted).await {
            Ok(PairedBundle { bundle, peer }) => {
                let label = bundle.device_label.clone();
                set_peer_label(&pairings, &pairing_id, label);
                // The proof is checked before any layer is written: a peer that cannot prove
                // its identity gets nothing imported and no roster entry.
                let verified = verify_peer(&peer);
                let outcome = match verified {
                    Ok((device_id, peer_label)) => {
                        let imported = match bundle.to_json() {
                            Ok(bytes) => import_bundle_value(&ctx, &bundle, true)
                                .await
                                .map(|_| bytes.len() as u64),
                            Err(err) => Err(map_bundle(err)),
                        };
                        match imported {
                            Ok(bytes) => enrol_verified_peer(&ctx, device_id, peer_label)
                                .await
                                .map(|()| bytes),
                            Err(err) => Err(err),
                        }
                    }
                    Err(err) => Err(err),
                };
                match outcome {
                    Ok(bytes) => {
                        set_transferred(&pairings, &pairing_id, bytes);
                        finish_pairing(&pairings, &sink, &pairing_id, PairingPhase::Done, None);
                    }
                    Err(err) => finish_pairing(
                        &pairings,
                        &sink,
                        &pairing_id,
                        PairingPhase::Failed,
                        Some(err.message),
                    ),
                }
            }
            Err(err) => finish_pairing(
                &pairings,
                &sink,
                &pairing_id,
                phase_for(&err),
                Some(err.to_string()),
            ),
        }
    });
}

/// Registers the decision channel and announces the two transitions the handshake produced.
///
/// The storage handshake reports one completion step, so `peer_found` and `sas_ready` are emitted
/// together: a peer arrived, and the digits to compare are now on screen.
async fn begin_sas(
    pairings: &Pairings,
    sink: &EventSink,
    pairing_id: &str,
    sas: String,
) -> Option<bool> {
    let (tx, rx) = oneshot::channel();
    {
        let mut guard = lock(pairings);
        let entry = guard.get_mut(pairing_id)?;
        entry.phase = PairingPhase::SasReady;
        entry.sas = Some(sas.clone());
        entry.decision = Some(tx);
    }
    sink.emit(EngineEvent::PairingChanged {
        pairing_id: pairing_id.to_string(),
        state: PairingPhase::PeerFound,
        sas: None,
    });
    sink.emit(EngineEvent::PairingChanged {
        pairing_id: pairing_id.to_string(),
        state: PairingPhase::SasReady,
        sas: Some(sas),
    });
    match tokio::time::timeout(DECISION_TIMEOUT, rx).await {
        Ok(Ok(accepted)) => Some(accepted),
        _ => None,
    }
}

fn set_phase(pairings: &Pairings, sink: &EventSink, pairing_id: &str, phase: PairingPhase) {
    let sas = {
        let mut guard = lock(pairings);
        match guard.get_mut(pairing_id) {
            Some(entry) => {
                entry.phase = phase;
                entry.sas.clone()
            }
            None => None,
        }
    };
    sink.emit(EngineEvent::PairingChanged {
        pairing_id: pairing_id.to_string(),
        state: phase,
        sas,
    });
}

fn set_peer_label(pairings: &Pairings, pairing_id: &str, label: String) {
    if let Some(entry) = lock(pairings).get_mut(pairing_id) {
        entry.peer_label = Some(label);
    }
}

fn set_transferred(pairings: &Pairings, pairing_id: &str, bytes: u64) {
    if let Some(entry) = lock(pairings).get_mut(pairing_id) {
        entry.transferred = bytes;
    }
}

/// Ends a pairing: the terminal event carries the SAS, then the entry is dropped so the map holds
/// only live pairings.
fn finish_pairing(
    pairings: &Pairings,
    sink: &EventSink,
    pairing_id: &str,
    phase: PairingPhase,
    error: Option<String>,
) {
    let sas = {
        let mut guard = lock(pairings);
        match guard.remove(pairing_id) {
            Some(entry) => entry.sas,
            None => None,
        }
    };
    if let Some(error) = error {
        tracing::warn!(pairing_id, phase = ?phase, %error, "pairing ended");
    }
    sink.emit(EngineEvent::PairingChanged {
        pairing_id: pairing_id.to_string(),
        state: phase,
        sas,
    });
}

fn phase_for(err: &PairingError) -> PairingPhase {
    match err {
        PairingError::Declined(_) => PairingPhase::Declined,
        PairingError::CodeExpired(_) => PairingPhase::Expired,
        _ => PairingPhase::Failed,
    }
}

fn map_pairing(err: PairingError) -> EngineError {
    EngineError::pairing_failed(err.to_string())
}

/// This device's pairing identity: its Ed25519 public key followed by its roster label.
///
/// The key is the same one `ensure_this_device` hashes into the roster id, so the id the peer
/// derives from this identity is exactly the id this device reports for itself.
struct EngineProver {
    key: SigningKey,
    identity: Vec<u8>,
}

impl EngineProver {
    async fn load(ctx: &EngineContext) -> Result<Self, EngineError> {
        let key = device_key(ctx).await?;
        let mut identity = key.verifying_key().as_bytes().to_vec();
        identity.extend_from_slice(default_label().as_bytes());
        Ok(Self { key, identity })
    }
}

impl DeviceProver for EngineProver {
    fn identity(&self) -> &[u8] {
        &self.identity
    }

    fn sign(&self, transcript: &[u8; 32]) -> [u8; 64] {
        self.key.sign(transcript).to_bytes()
    }
}

/// Bytes of the Ed25519 key every pairing identity starts with.
const IDENTITY_KEY_LEN: usize = 32;

/// Checks the peer's proof over the transcript and reads its device id and label.
///
/// The identity shape is `Ed25519 public key (32 bytes) || UTF-8 label`, which is what
/// [`EngineProver`] sends. The returned id is the hex of the key: the same value the peer's own
/// roster uses for itself. Any failure here is a rejected pairing, never a warning.
fn verify_peer(peer: &PairedPeer) -> Result<(String, String), EngineError> {
    let key_bytes: [u8; IDENTITY_KEY_LEN] = peer
        .identity
        .get(..IDENTITY_KEY_LEN)
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| {
            EngineError::pairing_failed(
                "the other device sent an identity too short to hold an Ed25519 key",
            )
        })?;
    let key = VerifyingKey::from_bytes(&key_bytes).map_err(|_| {
        EngineError::pairing_failed("the other device's identity does not hold a valid Ed25519 key")
    })?;
    key.verify_strict(&peer.transcript, &Signature::from_bytes(&peer.proof))
        .map_err(|_| {
            EngineError::pairing_failed(format!(
                "the other device's identity proof over transcript {} did not verify",
                hex(&peer.transcript)
            ))
        })?;
    let label = std::str::from_utf8(&peer.identity[IDENTITY_KEY_LEN..]).map_err(|_| {
        EngineError::pairing_failed("the other device's pairing label is not UTF-8")
    })?;
    let label = if label.trim().is_empty() {
        "Paired device".to_string()
    } else {
        label.to_string()
    };
    Ok((hex(&key_bytes), label))
}

/// Writes an already-verified peer into this device's roster at the current epoch.
///
/// The caller must have checked `peer` with [`verify_peer`] first; nothing here re-checks, and
/// nothing here runs on a pairing whose proof failed.
async fn enrol_verified_peer(
    ctx: &EngineContext,
    device_id: String,
    label: String,
) -> Result<(), EngineError> {
    ensure_this_device(ctx).await?;
    SettingsHandler::with_settings_lock(ctx, || {
        let mut settings = settings_object(ctx)?;
        let mut sync = sync_section(&settings);
        let epoch = sync_epoch(&sync);
        let mut devices = roster(&settings)?;
        match devices
            .iter_mut()
            .find(|device| device.device_id == device_id)
        {
            Some(entry) => {
                entry.label = label.clone();
                entry.epoch = epoch;
            }
            None => devices.push(DeviceEntry {
                device_id: device_id.clone(),
                label: label.clone(),
                enrolled_ms: now_ms(),
                epoch,
            }),
        }
        sync["devices"] = serde_json::to_value(&devices).map_err(|err| {
            EngineError::internal_error(format!("cannot encode the device roster: {err}"))
        })?;
        settings["sync"] = sync;
        SettingsHandler::write_settings_file(ctx, settings)
    })
}

/// The address the offer binds: this machine's LAN IPv4 when there is a route, loopback when
/// there is not. Binding the concrete address (not `0.0.0.0`) makes the ticket's endpoint the
/// address a second device can dial.
fn lan_bind_addr() -> SocketAddr {
    let probe = std::net::UdpSocket::bind("0.0.0.0:0")
        .and_then(|socket| socket.connect("192.168.0.1:9").map(|()| socket))
        .and_then(|socket| socket.local_addr());
    match probe {
        Ok(addr) => SocketAddr::new(addr.ip(), 0),
        Err(_) => SocketAddr::from(([127, 0, 0, 1], 0)),
    }
}

fn lock(pairings: &Pairings) -> MutexGuard<'_, HashMap<String, PairingEntry>> {
    pairings.lock().unwrap_or_else(PoisonError::into_inner)
}

// === Configuration bundle ===

/// Export: read every layer this device owns and assemble the bundle.
pub(crate) async fn build_bundle(
    ctx: &EngineContext,
    include_backgrounds: bool,
    include_themes: bool,
) -> Result<ConfigBundle, EngineError> {
    let device = ensure_this_device(ctx).await?;
    let raw_settings = SettingsHandler::get_settings_file(ctx)?;
    let settings = layer_settings(&raw_settings);

    let catalogue = match read_catalogue(ctx)? {
        Some(tags) => serde_json::to_value(tags)
            .map_err(|err| EngineError::internal_error(format!("cannot encode tags: {err}")))?,
        None => Value::Null,
    };

    let backgrounds = if include_backgrounds {
        scan_backgrounds(ctx)?
    } else {
        Vec::new()
    };
    let theme = if include_themes {
        export_theme(ctx, &raw_settings)?
    } else {
        None
    };

    let bundle = ConfigBundle {
        schema: BUNDLE_SCHEMA,
        exported_at_ms: now_ms(),
        device_label: device.label,
        settings,
        tags: tag_map(ctx).await?,
        catalogue,
        backgrounds,
        theme,
    };
    bundle.validate().map_err(map_bundle)?;
    Ok(bundle)
}

async fn import_bundle(
    ctx: &EngineContext,
    bundle_json: &str,
    apply: bool,
) -> Result<(ConfigImportReport, bool), EngineError> {
    let bundle = ConfigBundle::from_json(bundle_json.as_bytes()).map_err(map_bundle)?;
    import_bundle_value(ctx, &bundle, apply).await
}

/// Validates every layer of an already-parsed bundle, then writes the layers that are newer.
///
/// Validation and every layer decision happen before the first write, so a rejected bundle
/// leaves the device untouched and a dry run returns exactly the report an apply produces. The
/// writes are ordered so `settings.json` (the layer that carries the theme selection) lands
/// last: a failure halfway leaves a state the next import retries.
pub(crate) async fn import_bundle_value(
    ctx: &EngineContext,
    bundle: &ConfigBundle,
    apply: bool,
) -> Result<(ConfigImportReport, bool), EngineError> {
    bundle.validate().map_err(map_bundle)?;

    // Untrusted input: sanitise the layers before they are merged, so a hand-written bundle
    // cannot smuggle device-local settings or a non-catalogue back in.
    let mut incoming = bundle.clone();
    if !bundle.settings.is_null() && !bundle.settings.is_object() {
        return Err(EngineError::invalid_argument(
            "the bundle's settings layer is not a JSON object",
        ));
    }
    incoming.settings = strip_device_local(&bundle.settings);
    let catalogue = match &bundle.catalogue {
        Value::Null => None,
        value => Some(
            serde_json::from_value::<Vec<RoomTagDef>>(value.clone()).map_err(|err| {
                EngineError::invalid_argument(format!(
                    "the bundle's catalogue is not a tag catalogue: {err}"
                ))
            })?,
        ),
    };
    if let Some(theme) = &bundle.theme {
        validate_theme(theme)?;
    }

    let local = local_state(ctx).await?;
    let report = incoming
        .import_into(&mut local.clone())
        .map_err(map_bundle)?;

    // Resolve the tagged chats now: the account may not exist on this device yet, and a chat the
    // roster cannot name is a skip, not an error.
    let mut tag_writes: Vec<(String, Vec<String>, i64)> = Vec::new();
    if report.tags > 0 {
        let labels = account_labels(ctx).await?;
        for (key, set) in &bundle.tags {
            if local
                .tags
                .get(key)
                .is_some_and(|current| set.updated_at_ms <= current.updated_at_ms)
            {
                continue;
            }
            let Some(account_id) = labels.get(&key.account) else {
                continue;
            };
            let Some(chat) = ctx
                .store
                .find_chat(account_id, &key.room)
                .await
                .map_err(map_store)?
            else {
                continue;
            };
            tag_writes.push((
                chat.chat_id,
                set.tags.iter().cloned().collect(),
                set.updated_at_ms,
            ));
        }
    }
    let backgrounds: Vec<&BackgroundAsset> = bundle
        .backgrounds
        .iter()
        .filter(|asset| {
            !local
                .backgrounds
                .get(&asset.relative_path)
                .is_some_and(|current| asset.updated_at_ms <= current.updated_at_ms)
        })
        .collect();

    let report = ConfigImportReport {
        settings: report.settings,
        catalogue: report.catalogue,
        tags: tag_writes.len(),
        backgrounds: backgrounds.len(),
        theme: report.theme,
    };
    if !apply {
        return Ok((report, false));
    }

    let mut wrote_theme = false;
    for asset in backgrounds {
        write_background(ctx, asset)?;
    }
    for (chat_id, tags, stamp) in tag_writes {
        ctx.store
            .update_chat_flags(
                &chat_id,
                ChatFlagsUpdate {
                    tags: Some(tags),
                    tags_updated_at: Some(stamp),
                    ..ChatFlagsUpdate::default()
                },
            )
            .await
            .map_err(map_store)?;
    }
    if report.tags > 0 {
        ctx.bus.chats();
    }
    if let (true, Some(tags)) = (report.catalogue, catalogue) {
        files::write_json(&catalogue_path(ctx), &tags)?;
    }
    if report.theme
        && let Some(theme) = &bundle.theme
    {
        if let Some(value) = &theme.theme {
            write_theme_file(ctx, &theme.id, value)?;
        }
        wrote_theme = true;
    }

    if report.settings || wrote_theme {
        SettingsHandler::with_settings_lock(ctx, || {
            let mut settings = SettingsHandler::get_settings_file(ctx)?;
            if report.settings {
                settings = merge_settings(settings, &incoming.settings);
                note_layer_change(&mut settings, bundle.exported_at_ms);
            }
            if wrote_theme && let Some(theme) = &bundle.theme {
                settings["active_theme"] = Value::String(theme.id.clone());
            }
            SettingsHandler::write_settings_file(ctx, settings)
        })?;
    }

    let applied = report != ConfigImportReport::default();
    Ok((report, applied))
}

/// Local state for the merge: the layers as they are now, stamped so `import_into` can compare.
async fn local_state(ctx: &EngineContext) -> Result<ConfigState, EngineError> {
    let raw_settings = SettingsHandler::get_settings_file(ctx)?;
    let settings = layer_settings(&raw_settings);
    let settings_updated_at_ms = layer_updated_at_ms(&raw_settings);

    let catalogue_path = catalogue_path(ctx);
    let catalogue = match files::read_json::<Value>(&catalogue_path)? {
        Some(value) => value,
        None => Value::Null,
    };
    let catalogue_updated_at_ms = if catalogue_path.exists() {
        mtime_ms(&catalogue_path)
    } else {
        0
    };

    let mut backgrounds = BTreeMap::new();
    for asset in scan_backgrounds_stamps(ctx)? {
        backgrounds.insert(asset.relative_path.clone(), asset);
    }

    let theme = local_theme(ctx, &raw_settings);

    Ok(ConfigState {
        settings,
        catalogue,
        tags: tag_map(ctx).await?,
        backgrounds,
        theme,
        settings_updated_at_ms,
        catalogue_updated_at_ms,
    })
}

fn strip_device_local(settings: &Value) -> Value {
    let Value::Object(map) = settings else {
        return Value::Null;
    };
    let mut out = map.clone();
    for key in DEVICE_LOCAL_SETTINGS {
        out.remove(*key);
    }
    Value::Object(out)
}

/// The settings layer as a bundle carries it, `null` when this device has none. A file holding
/// only device-local keys is an empty device: exporting it as `{}` would let it wipe a
/// configured one, because a whole-layer value replaces the receiving device's settings.
fn layer_settings(settings: &Value) -> Value {
    let stripped = strip_device_local(settings);
    match &stripped {
        Value::Object(map) if map.is_empty() => Value::Null,
        _ => stripped,
    }
}

/// When the UI-owned settings layer last changed. It is written by the settings commands and by
/// an import, never derived from the file's mtime: the engine also writes device-local
/// bookkeeping (the roster) into the same file, and that must not look like a user edit.
fn layer_updated_at_ms(settings: &Value) -> i64 {
    settings
        .get("sync")
        .and_then(|sync| sync.get("layer_updated_at_ms"))
        .and_then(Value::as_i64)
        .unwrap_or(0)
}

/// Stamps a change to the UI-owned settings layer.
pub(crate) fn note_layer_change(settings: &mut Value, at_ms: i64) {
    let mut sync = sync_section(settings);
    sync["layer_updated_at_ms"] = json!(at_ms);
    settings["sync"] = sync;
}

/// Replaces the exported layer's keys while every device-local key keeps its local value.
fn merge_settings(local: Value, layer: &Value) -> Value {
    let local = match local {
        Value::Object(map) => map,
        _ => serde_json::Map::new(),
    };
    let mut device_local = serde_json::Map::new();
    for key in DEVICE_LOCAL_SETTINGS {
        if let Some(value) = local.get(*key) {
            device_local.insert((*key).to_string(), value.clone());
        }
    }
    let mut out = serde_json::Map::new();
    if let Value::Object(layer) = layer {
        for (key, value) in layer {
            if !DEVICE_LOCAL_SETTINGS.contains(&key.as_str()) {
                out.insert(key.clone(), value.clone());
            }
        }
    }
    out.extend(device_local);
    Value::Object(out)
}

fn catalogue_path(ctx: &EngineContext) -> PathBuf {
    ctx.data_dir.join("tags.json")
}

fn background_root(ctx: &EngineContext) -> PathBuf {
    ctx.data_dir.join(BACKGROUND_DIR)
}

fn theme_dir(ctx: &EngineContext) -> PathBuf {
    ctx.data_dir.join(THEME_DIR)
}

/// The tag catalogue as the rest of the engine keeps it, `None` when it was never written.
fn read_catalogue(ctx: &EngineContext) -> Result<Option<Vec<RoomTagDef>>, EngineError> {
    files::read_json::<Vec<RoomTagDef>>(&catalogue_path(ctx))
}

/// Every tagged chat as bundle keys: the account's stable label and the network's own room id.
async fn tag_map(ctx: &EngineContext) -> Result<BTreeMap<ChatKey, TagSet>, EngineError> {
    let labels = account_labels(ctx).await?;
    let chats = ctx.store.list_tagged_chats().await.map_err(map_store)?;
    let mut out = BTreeMap::new();
    for chat in chats {
        let Some(account) = labels
            .iter()
            .find(|(_, id)| **id == chat.account_id)
            .map(|(label, _)| label.clone())
        else {
            continue;
        };
        out.insert(
            ChatKey::new(account, chat.remote_chat_id.clone()),
            TagSet {
                updated_at_ms: chat.tags_updated_at,
                deleted: false,
                tags: chat.local_tags.iter().cloned().collect::<BTreeSet<_>>(),
            },
        );
    }
    Ok(out)
}

/// Stable account label across devices: the network plus the network's own user id. The local
/// `account_id` is a UUID this device generated, so it cannot be the key a second device resolves.
async fn account_labels(ctx: &EngineContext) -> Result<HashMap<String, String>, EngineError> {
    let accounts = ctx.store.list_accounts().await.map_err(map_store)?;
    Ok(accounts
        .into_iter()
        .map(|account| {
            let network = serde_json::to_value(account.network)
                .ok()
                .and_then(|value| value.as_str().map(str::to_string))
                .unwrap_or_else(|| "unknown".to_string());
            let label = match &account.remote_user_id {
                Some(user_id) => format!("{network}:{user_id}"),
                None => format!("{network}:{}", account.account_id),
            };
            (label, account.account_id)
        })
        .collect())
}

fn scan_backgrounds(ctx: &EngineContext) -> Result<Vec<BackgroundAsset>, EngineError> {
    let root = background_root(ctx);
    let mut out = Vec::new();
    collect_assets(&root, &root, &mut out)?;
    Ok(out)
}

/// Background stamps only. A merge compares stamps, so the file bytes are read at write time,
/// never to decide whether to write.
fn scan_backgrounds_stamps(ctx: &EngineContext) -> Result<Vec<BackgroundAsset>, EngineError> {
    let mut assets = scan_backgrounds(ctx)?;
    for asset in &mut assets {
        asset.bytes = Vec::new();
    }
    Ok(assets)
}

fn collect_assets(
    root: &Path,
    dir: &Path,
    out: &mut Vec<BackgroundAsset>,
) -> Result<(), EngineError> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => {
            return Err(EngineError::internal_error(format!(
                "cannot read {}: {err}",
                dir.display()
            )));
        }
    };
    for entry in entries {
        let entry = entry.map_err(|err| {
            EngineError::internal_error(format!("cannot read {}: {err}", dir.display()))
        })?;
        let path = entry.path();
        let meta = entry.metadata().map_err(|err| {
            EngineError::internal_error(format!("cannot stat {}: {err}", path.display()))
        })?;
        if meta.is_dir() {
            collect_assets(root, &path, out)?;
            continue;
        }
        if !meta.is_file() {
            continue;
        }
        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        let relative_path = relative.to_string_lossy().replace('\\', "/");
        let bytes = std::fs::read(&path).map_err(|err| {
            EngineError::internal_error(format!("cannot read {}: {err}", path.display()))
        })?;
        out.push(BackgroundAsset {
            relative_path,
            updated_at_ms: mtime_ms(&path),
            deleted: false,
            bytes,
        });
    }
    Ok(())
}

fn write_background(ctx: &EngineContext, asset: &BackgroundAsset) -> Result<(), EngineError> {
    let root = background_root(ctx);
    let path = root.join(&asset.relative_path);
    // `bundle.validate` already refused unsafe relative paths; this is the belt to that braces.
    if !path.starts_with(&root) {
        return Err(EngineError::invalid_argument(format!(
            "background {:?} escapes the background directory",
            asset.relative_path
        )));
    }
    write_atomic(&path, &asset.bytes)
}

fn export_theme(
    ctx: &EngineContext,
    settings: &Value,
) -> Result<Option<ThemeSelection>, EngineError> {
    let Some(id) = active_theme(settings) else {
        return Ok(None);
    };
    // The directory the running app resolves themes from in this build, then the release
    // location an import writes to: a debug build reads `packages/themes`, and a round trip
    // through it must still carry the file it just wrote.
    let mut value = match SettingsHandler::themes_dir(ctx) {
        Some(dir) => find_theme_file(&dir, &id)?,
        None => None,
    };
    if value.is_none() {
        value = find_theme_file(&theme_dir(ctx), &id)?;
    }
    let Some(value) = value else {
        return Ok(None);
    };
    Ok(Some(ThemeSelection {
        id,
        updated_at_ms: layer_updated_at_ms(settings),
        deleted: false,
        theme: Some(value),
    }))
}

fn local_theme(ctx: &EngineContext, settings: &Value) -> Option<ThemeSelection> {
    let id = active_theme(settings)?;
    let dir = theme_dir(ctx);
    let value = find_theme_file(&dir, &id).ok().flatten()?;
    Some(ThemeSelection {
        id,
        updated_at_ms: layer_updated_at_ms(settings),
        deleted: false,
        theme: Some(value),
    })
}

fn active_theme(settings: &Value) -> Option<String> {
    settings
        .get("active_theme")
        .and_then(Value::as_str)
        .filter(|id| SettingsHandler::is_valid_theme_id(id))
        .map(str::to_string)
}

/// Reads the theme file whose parsed id is `id`, so a renamed file still resolves.
fn find_theme_file(dir: &Path, id: &str) -> Result<Option<Value>, EngineError> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => {
            return Err(EngineError::internal_error(format!(
                "cannot read {}: {err}",
                dir.display()
            )));
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(_) => continue,
        };
        let Ok(value) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        if value.get("id").and_then(Value::as_str) == Some(id) {
            return Ok(Some(value));
        }
    }
    Ok(None)
}

fn validate_theme(theme: &ThemeSelection) -> Result<(), EngineError> {
    if !SettingsHandler::is_valid_theme_id(&theme.id) {
        return Err(EngineError::invalid_argument(format!(
            "theme id {:?} is not a plain slug",
            theme.id
        )));
    }
    if let Some(value) = &theme.theme {
        let text = serde_json::to_string(value)
            .map_err(|err| EngineError::internal_error(format!("cannot read theme: {err}")))?;
        let parsed = alloy_theme::ThemeLoader::load_from_str(&text).map_err(|err| {
            EngineError::invalid_argument(format!("the bundle's theme does not parse: {err}"))
        })?;
        if parsed.id != theme.id {
            return Err(EngineError::invalid_argument(format!(
                "the bundle's theme is {:?} but is selected as {:?}",
                parsed.id, theme.id
            )));
        }
    }
    Ok(())
}

fn write_theme_file(ctx: &EngineContext, id: &str, value: &Value) -> Result<(), EngineError> {
    let path = theme_dir(ctx).join(format!("{id}.json"));
    let text = serde_json::to_vec_pretty(value)
        .map_err(|err| EngineError::internal_error(format!("cannot encode theme: {err}")))?;
    write_atomic(&path, &text)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), EngineError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| {
            EngineError::internal_error(format!("cannot create {}: {err}", parent.display()))
        })?;
    }
    let temp = path.with_file_name(format!(".{}.tmp", Uuid::new_v4()));
    let write = || -> std::io::Result<()> {
        let mut file = std::fs::File::create(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::rename(&temp, path)
    };
    if let Err(err) = write() {
        let _ = std::fs::remove_file(&temp);
        return Err(EngineError::internal_error(format!(
            "cannot write {}: {err}",
            path.display()
        )));
    }
    Ok(())
}

fn mtime_ms(path: &Path) -> i64 {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|elapsed| i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or_default()
}

fn map_bundle(err: BundleError) -> EngineError {
    match err {
        BundleError::UnsupportedSchema { found, supported } => {
            EngineError::bundle_rejected(found, supported)
        }
        other => EngineError::invalid_argument(other.to_string()),
    }
}

// === Device roster ===

pub(crate) async fn list_devices(ctx: &EngineContext) -> Result<Vec<DeviceDto>, EngineError> {
    let this = ensure_this_device(ctx).await?;
    let settings = SettingsHandler::get_settings_file(ctx)?;
    let mut devices = roster(&settings)?;
    if !devices
        .iter()
        .any(|device| device.device_id == this.device_id)
    {
        devices.push(this.clone());
    }
    devices.sort_by_key(|device| device.enrolled_ms);
    Ok(devices
        .into_iter()
        .map(|device| DeviceDto {
            current: device.device_id == this.device_id,
            device_id: device.device_id,
            label: device.label,
            enrolled_ms: device.enrolled_ms,
            epoch: device.epoch,
        })
        .collect())
}

/// The five configuration layers and when each last changed on this device. `enabled` is the
/// caller's decision (mode `prefs`), not a property of local data.
pub(crate) async fn sync_layer_statuses(
    ctx: &EngineContext,
) -> Result<Vec<SyncLayerStatus>, EngineError> {
    let settings = SettingsHandler::get_settings_file(ctx)?;
    let tags_updated_at_ms = tag_map(ctx)
        .await?
        .values()
        .map(|set| set.updated_at_ms)
        .max()
        .unwrap_or(0);
    let catalogue = catalogue_path(ctx);
    let catalogue_updated_at_ms = if catalogue.exists() {
        mtime_ms(&catalogue)
    } else {
        0
    };
    let backgrounds_updated_at_ms = scan_backgrounds_stamps(ctx)?
        .iter()
        .map(|asset| asset.updated_at_ms)
        .max()
        .unwrap_or(0);
    let theme_updated_at_ms = local_theme(ctx, &settings)
        .map(|theme| theme.updated_at_ms)
        .unwrap_or(0);
    Ok(vec![
        SyncLayerStatus {
            key: "settings".to_string(),
            enabled: true,
            updated_at_ms: layer_updated_at_ms(&settings),
        },
        SyncLayerStatus {
            key: "tags".to_string(),
            enabled: true,
            updated_at_ms: tags_updated_at_ms,
        },
        SyncLayerStatus {
            key: "catalogue".to_string(),
            enabled: true,
            updated_at_ms: catalogue_updated_at_ms,
        },
        SyncLayerStatus {
            key: "backgrounds".to_string(),
            enabled: true,
            updated_at_ms: backgrounds_updated_at_ms,
        },
        SyncLayerStatus {
            key: "theme".to_string(),
            enabled: true,
            updated_at_ms: theme_updated_at_ms,
        },
        SyncLayerStatus {
            key: "messages".to_string(),
            enabled: false,
            updated_at_ms: 0,
        },
    ])
}

async fn revoke_device(ctx: &EngineContext, device_id: &str) -> Result<u32, EngineError> {
    let this = ensure_this_device(ctx).await?;
    if device_id == this.device_id {
        return Err(EngineError::invalid_argument(
            "this device cannot revoke itself; erase its data instead",
        ));
    }
    let device_id = device_id.to_string();
    SettingsHandler::with_settings_lock(ctx, || {
        let mut settings = settings_object(ctx)?;
        let mut sync = sync_section(&settings);
        let mut devices = roster(&settings)?;
        let before = devices.len();
        devices.retain(|device| device.device_id != device_id);
        if devices.len() == before {
            return Err(EngineError::not_found(format!("device '{device_id}'")));
        }
        let epoch = sync_epoch(&sync).saturating_add(1);
        // The remaining devices move to the new epoch; the removed one keeps the old key in its
        // own store, which is exactly why the epoch advances.
        for device in &mut devices {
            device.epoch = epoch;
        }
        sync["epoch"] = json!(epoch);
        sync["devices"] = serde_json::to_value(&devices).map_err(|err| {
            EngineError::internal_error(format!("cannot encode the device roster: {err}"))
        })?;
        settings["sync"] = sync;
        SettingsHandler::write_settings_file(ctx, settings)?;
        Ok(epoch)
    })
}

/// Reads this device's roster entry, creating the Ed25519 key and the entry on first use.
async fn ensure_this_device(ctx: &EngineContext) -> Result<DeviceEntry, EngineError> {
    let key = device_key(ctx).await?;
    let device_id = hex(key.verifying_key().as_bytes());
    let label = default_label();
    SettingsHandler::with_settings_lock(ctx, || {
        let mut settings = settings_object(ctx)?;
        let mut sync = sync_section(&settings);
        let mut devices = roster(&settings)?;
        if let Some(device) = devices
            .iter()
            .find(|device| device.device_id == device_id)
            .cloned()
        {
            return Ok(device);
        }
        let device = DeviceEntry {
            device_id,
            label,
            enrolled_ms: now_ms(),
            epoch: sync_epoch(&sync),
        };
        devices.push(device.clone());
        sync["devices"] = serde_json::to_value(&devices).map_err(|err| {
            EngineError::internal_error(format!("cannot encode the device roster: {err}"))
        })?;
        settings["sync"] = sync;
        SettingsHandler::write_settings_file(ctx, settings)?;
        Ok(device)
    })
}

/// The versioned `sync` section. A device that never synced starts at version 1, epoch 0.
pub(crate) fn sync_section(settings: &Value) -> Value {
    match settings.get("sync") {
        Some(Value::Object(_)) => {
            let mut sync = settings["sync"].clone();
            if sync.get("version").is_none() {
                sync["version"] = json!(SYNC_VERSION);
            }
            sync
        }
        _ => json!({ "version": SYNC_VERSION, "epoch": 0, "devices": [] }),
    }
}

/// Reads `settings.json` for a read-modify-write that indexes into it. A file holding a JSON
/// value that is not an object is corruption, not a settings layer, and must not panic.
pub(crate) fn settings_object(ctx: &EngineContext) -> Result<Value, EngineError> {
    let settings = SettingsHandler::get_settings_file(ctx)?;
    if settings.is_object() {
        Ok(settings)
    } else {
        Err(EngineError::internal_error(
            "settings.json does not hold a JSON object",
        ))
    }
}

pub(crate) fn sync_epoch(sync: &Value) -> u32 {
    sync.get("epoch")
        .and_then(Value::as_u64)
        .and_then(|epoch| u32::try_from(epoch).ok())
        .unwrap_or(0)
}

fn roster(settings: &Value) -> Result<Vec<DeviceEntry>, EngineError> {
    let Some(devices) = settings.get("sync").and_then(|sync| sync.get("devices")) else {
        return Ok(Vec::new());
    };
    serde_json::from_value(devices.clone()).map_err(|err| {
        EngineError::internal_error(format!(
            "the device roster in settings.json is corrupt: {err}"
        ))
    })
}

fn default_label() -> String {
    std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .ok()
        .map(|label| label.trim().to_string())
        .filter(|label| !label.is_empty())
        .unwrap_or_else(|| "This device".to_string())
}

pub(crate) async fn device_key(ctx: &EngineContext) -> Result<SigningKey, EngineError> {
    match ctx.secrets.get_secret(DEVICE_KEY_SECRET).await {
        Ok(text) => Ok(SigningKey::from_bytes(&decode_32(
            &text,
            DEVICE_KEY_SECRET,
        )?)),
        Err(StorageError::SecretNotFound(_)) => {
            let mut seed = Zeroizing::new([0u8; 32]);
            getrandom::fill(&mut seed[..]).map_err(|err| {
                EngineError::internal_error(format!("cannot generate a device key: {err}"))
            })?;
            ctx.secrets
                .set_secret(DEVICE_KEY_SECRET, &BASE64.encode(seed.as_slice()))
                .await
                .map_err(map_secret)?;
            Ok(SigningKey::from_bytes(&seed))
        }
        Err(err) => Err(map_secret(err)),
    }
}

/// The master key, created on first use. It is the root of every sync key (ADR 0020); nothing
/// here derives from it yet, but the recovery phrase wraps it.
pub(crate) async fn master_key(ctx: &EngineContext) -> Result<MasterKey, EngineError> {
    match ctx.secrets.get_secret(MASTER_KEY_SECRET).await {
        Ok(text) => Ok(MasterKey::from_bytes(decode_32(&text, MASTER_KEY_SECRET)?)),
        Err(StorageError::SecretNotFound(_)) => {
            let key = MasterKey::generate();
            let text = BASE64.encode(key.expose_secret());
            ctx.secrets
                .set_secret(MASTER_KEY_SECRET, &text)
                .await
                .map_err(map_secret)?;
            Ok(key)
        }
        Err(err) => Err(map_secret(err)),
    }
}

fn map_secret(err: StorageError) -> EngineError {
    EngineError::internal_error(format!("secret store: {err}"))
}

fn decode_32(text: &str, name: &str) -> Result<[u8; 32], EngineError> {
    let bytes = BASE64
        .decode(text.trim())
        .map_err(|err| EngineError::internal_error(format!("{name} is not base64: {err}")))?;
    bytes
        .try_into()
        .map_err(|_| EngineError::internal_error(format!("{name} does not hold 32 bytes")))
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

// === Recovery phrase ===

async fn create_recovery_phrase(ctx: &EngineContext) -> Result<Vec<String>, EngineError> {
    let mnemonic = Mnemonic::generate_in(Language::English, RECOVERY_WORDS).map_err(|err| {
        EngineError::internal_error(format!("cannot generate a recovery phrase: {err}"))
    })?;
    let master = master_key(ctx).await?;

    let mut salt = [0u8; RECOVERY_SALT_LEN];
    let mut nonce = [0u8; RECOVERY_NONCE_LEN];
    getrandom::fill(&mut salt).map_err(random_error)?;
    getrandom::fill(&mut nonce).map_err(random_error)?;

    let escrow = escrow_blob(&mnemonic, &master, &salt, &nonce)?;
    let verifier_hex = hex(&Sha256::digest(&escrow));
    let record = RecoveryRecord {
        version: ESCROW_VERSION,
        m_cost: RECOVERY_M_COST,
        t_cost: RECOVERY_T_COST,
        p_cost: RECOVERY_P_COST,
        salt_b64: BASE64.encode(salt),
        nonce_b64: BASE64.encode(nonce),
        verifier_hex,
    };

    let words: Vec<String> = mnemonic.words().map(str::to_string).collect();
    SettingsHandler::with_settings_lock(ctx, || {
        let mut settings = settings_object(ctx)?;
        let mut sync = sync_section(&settings);
        sync["recovery"] = serde_json::to_value(&record).map_err(|err| {
            EngineError::internal_error(format!("cannot encode the recovery record: {err}"))
        })?;
        settings["sync"] = sync;
        SettingsHandler::write_settings_file(ctx, settings)
    })?;
    Ok(words)
}

async fn verify_recovery_phrase(ctx: &EngineContext, words: &[String]) -> Result<(), EngineError> {
    let settings = SettingsHandler::get_settings_file(ctx)?;
    let Some(record) = settings
        .get("sync")
        .and_then(|sync| sync.get("recovery"))
        .cloned()
    else {
        return Err(EngineError::unavailable(
            "no recovery phrase has been created on this device",
        ));
    };
    let record: RecoveryRecord = serde_json::from_value(record).map_err(|err| {
        EngineError::internal_error(format!("the recovery record is corrupt: {err}"))
    })?;
    if words.len() != RECOVERY_WORDS {
        return Err(EngineError::recovery_rejected(format!(
            "a recovery phrase has {RECOVERY_WORDS} words, not {}",
            words.len()
        )));
    }
    // One message for every failure past this point: an unknown word, a wrong word and a phrase
    // from another device are indistinguishable to the caller. Saying which word was wrong would
    // hand a guesser the position.
    let mnemonic =
        Mnemonic::parse_in_normalized(Language::English, &words.join(" ")).map_err(|_| {
            EngineError::recovery_rejected(
                "the phrase does not match the recovery phrase this device holds",
            )
        })?;
    let master = master_key(ctx).await?;
    let salt = decode_b64(&record.salt_b64, "recovery salt")?;
    let nonce = decode_b64(&record.nonce_b64, "recovery nonce")?;
    let escrow = escrow_blob_with(
        &mnemonic,
        &master,
        &salt,
        &nonce,
        record.m_cost,
        record.t_cost,
        record.p_cost,
    )?;
    if hex(&Sha256::digest(&escrow)) != record.verifier_hex {
        return Err(EngineError::recovery_rejected(
            "the phrase does not match the recovery phrase this device holds",
        ));
    }
    Ok(())
}

fn escrow_blob(
    mnemonic: &Mnemonic,
    master: &MasterKey,
    salt: &[u8],
    nonce: &[u8],
) -> Result<Vec<u8>, EngineError> {
    escrow_blob_with(
        mnemonic,
        master,
        salt,
        nonce,
        RECOVERY_M_COST,
        RECOVERY_T_COST,
        RECOVERY_P_COST,
    )
}

/// The phrase wraps the master key into the blob a future server upload will carry. The wrap is
/// `argon2id(phrase) -> ChaCha20-Poly1305(master)`; the phrase is the only input to the key, so
/// the server never holds anything that opens it.
fn escrow_blob_with(
    mnemonic: &Mnemonic,
    master: &MasterKey,
    salt: &[u8],
    nonce: &[u8],
    m_cost: u32,
    t_cost: u32,
    p_cost: u32,
) -> Result<Vec<u8>, EngineError> {
    if salt.len() != RECOVERY_SALT_LEN || nonce.len() != RECOVERY_NONCE_LEN {
        return Err(EngineError::internal_error(
            "the recovery record's salt or nonce has the wrong length",
        ));
    }
    let params = Params::new(m_cost, t_cost, p_cost, Some(RECOVERY_KEY_LEN)).map_err(|err| {
        EngineError::internal_error(format!("invalid recovery KDF parameters: {err}"))
    })?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = Zeroizing::new([0u8; RECOVERY_KEY_LEN]);
    argon
        .hash_password_into(mnemonic.to_string().as_bytes(), salt, &mut key[..])
        .map_err(|err| {
            EngineError::internal_error(format!("cannot derive the recovery key: {err}"))
        })?;

    let cipher = ChaCha20Poly1305::new(
        &Key::try_from(&key[..])
            .map_err(|_| EngineError::internal_error("the recovery key is not 32 bytes"))?,
    );
    let ciphertext = cipher
        .encrypt(
            &Nonce::try_from(nonce)
                .map_err(|_| EngineError::internal_error("the recovery nonce is not 12 bytes"))?,
            Payload {
                msg: master.expose_secret(),
                aad: ESCROW_AAD,
            },
        )
        .map_err(|_| EngineError::internal_error("cannot seal the recovery escrow"))?;

    let blob = EscrowBlob {
        version: ESCROW_VERSION,
        kdf: "argon2id",
        m_cost,
        t_cost,
        p_cost,
        salt_b64: BASE64.encode(salt),
        nonce_b64: BASE64.encode(nonce),
        ciphertext_b64: BASE64.encode(ciphertext),
    };
    serde_json::to_vec(&blob)
        .map_err(|err| EngineError::internal_error(format!("cannot encode the escrow: {err}")))
}

fn decode_b64(text: &str, name: &str) -> Result<Vec<u8>, EngineError> {
    BASE64
        .decode(text)
        .map_err(|err| EngineError::internal_error(format!("{name} is not base64: {err}")))
}

fn random_error(err: getrandom::Error) -> EngineError {
    EngineError::internal_error(format!("the OS random source failed: {err}"))
}
