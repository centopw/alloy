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
//! - the recovery verifier and the wrapped escrow blob (never the phrase) sit beside the roster.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::Write;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, UNIX_EPOCH};

use alloy_protocol::{
    ConfigBundleSummary, ConfigImportReport, DeviceDto, EngineCommand, EngineEvent, EngineResponse,
    PairingPhase, PairingRole, RoomTagDef, SyncLayerStatus, SyncMode,
};
use alloy_storage::StorageError;
use alloy_storage::config_bundle::{
    BUNDLE_SCHEMA, BackgroundAsset, BundleError, ChatKey, ConfigBundle, ConfigState, TagSet,
    ThemeSelection,
};
use alloy_storage::pairing::{
    self, CODE_TTL, DeviceProver, MasterEnvelope, MasterEnvelopeProvider, PairedBundle, PairedPeer,
    PairingError, PairingOffer, PairingTicket,
};
use alloy_store::{ChatFlagsUpdate, ChatKind, ChatUpsert, MessageKind, NewMessage};
use alloy_sync::device::{DeviceSecretKey, open_master, wrap_master};
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
/// The private half of this device's X25519 key. Stored exactly like the master key (ADR 0021):
/// the same secret store, the same wrapping tier, never in `settings.json`.
const DEVICE_X25519_SECRET: &str = "alloy.device.x25519_key.v1";

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
    /// Hex X25519 public key the device publishes at enrolment, so a revocation can wrap the
    /// rotated master to it. Empty for an entry written before device keys existed: such a device
    /// cannot adopt a rotation and has to enrol again.
    #[serde(default)]
    x25519_pub: String,
}

/// The recovery verifier plus the wrapped blob it describes. It names the KDF inputs so a later
/// verification can rebuild the escrow byte for byte; the phrase itself is never kept.
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
    /// The exact bytes published to a sync server. A record written before the escrow was
    /// published lacks it, and verification only needs the parameters above.
    #[serde(default)]
    escrow_b64: String,
}

/// The KDF every escrow blob declares.
const ESCROW_KDF: &str = "argon2id";

/// Bounds on the Argon2id parameters a server-supplied blob may ask for. The server is untrusted,
/// so a hostile blob cannot make the client allocate or busy-run beyond these.
const RECOVERY_M_COST_MIN: u32 = 8 * 1024;
const RECOVERY_M_COST_MAX: u32 = 256 * 1024;
const RECOVERY_T_COST_MAX: u32 = 16;
const RECOVERY_P_COST_MAX: u32 = 8;

/// The escrow blob: the wrapped master key and the parameters needed to open it. Serialised as
/// canonical JSON; the same bytes are published to the sync server.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EscrowBlob {
    version: u32,
    kdf: String,
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
            EngineCommand::RecoverFromPhrase {
                words,
                url,
                token,
                accept_new_pin,
            } => {
                let report = crate::sync::recover(ctx, &words, url, token, accept_new_pin).await?;
                Ok(EngineResponse::Recovered {
                    layers: report.layers,
                    devices: report.devices,
                })
            }
            EngineCommand::ListDevices => {
                let devices = list_devices(ctx).await?;
                Ok(EngineResponse::DevicesList { devices })
            }
            EngineCommand::RevokeDevice { device_id } => {
                let epoch = revoke_device(ctx, &device_id).await?;
                // The roster and the epoch already moved; everything below is best effort and is
                // reported rather than turned into a failed command (invariant 14).
                let outcome = crate::sync::reseal_after_revoke(ctx, epoch).await;
                Ok(EngineResponse::DeviceRevoked {
                    epoch,
                    re_sealed: Some(outcome.re_sealed),
                    blobs_deleted: Some(outcome.blobs_deleted),
                    blobs_remaining: Some(outcome.blobs_remaining),
                    recovery_stale: Some(outcome.recovery_stale),
                    error: outcome.error,
                })
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
        // The master key and the epoch it belongs to travel with the settings: a device that
        // enrolled without them could open no server blob the offering device published.
        let master = master_key(ctx).await?;
        let key = device_key(ctx).await?;
        let sender_id = hex(key.verifying_key().as_bytes());
        let epoch = sync_epoch(&sync_section(&settings_object(ctx)?));
        let delivery = Arc::new(EngineMasterEnvelope {
            master,
            signer: key,
            sender_id,
            epoch,
        });
        let offer = PairingOffer::bind(
            bundle,
            lan_bind_addr(),
            None,
            Arc::new(prover),
            Some(delivery),
        )
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
                    Ok((device_id, label, x25519_pub)) => {
                        enrol_verified_peer(&ctx, device_id, label.clone(), x25519_pub)
                            .await
                            .map(|()| label)
                    }
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
            Ok(PairedBundle {
                bundle,
                peer,
                master,
            }) => {
                let label = bundle.device_label.clone();
                set_peer_label(&pairings, &pairing_id, label);
                // The proof is checked before any layer or key is written: a peer that cannot
                // prove its identity gets nothing imported and no roster entry.
                let verified = verify_peer(&peer);
                let outcome = match verified {
                    Ok((device_id, peer_label, x25519_pub)) => {
                        // The master key is unwrapped and stored first. A failure here leaves the
                        // bundle untouched: no half-adopted account with settings but no key.
                        let adopted =
                            adopt_paired_master(&ctx, &peer, master.as_ref(), &device_id).await;
                        match adopted {
                            Ok(()) => {
                                let imported = match bundle.to_json() {
                                    Ok(bytes) => import_bundle_value(&ctx, &bundle, true)
                                        .await
                                        .map(|_| bytes.len() as u64),
                                    Err(err) => Err(map_bundle(err)),
                                };
                                match imported {
                                    Ok(bytes) => {
                                        enrol_verified_peer(&ctx, device_id, peer_label, x25519_pub)
                                            .await
                                            .map(|()| bytes)
                                    }
                                    Err(err) => Err(err),
                                }
                            }
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
///
/// A pairing that expires, is declined or completes did what it was told: those are debug lines.
/// Only `Failed` describes something actionable and reaches `warn`; the reason rides on it as
/// `error`. Nothing here ever logs the pairing code.
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
    match (phase, error) {
        (PairingPhase::Failed, Some(error)) => {
            tracing::warn!(pairing_id, %error, "pairing failed");
        }
        (PairingPhase::Failed, None) => tracing::warn!(pairing_id, "pairing failed"),
        (_, Some(error)) => tracing::debug!(pairing_id, phase = ?phase, %error, "pairing ended"),
        (_, None) => tracing::debug!(pairing_id, phase = ?phase, "pairing ended"),
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

/// This device's pairing identity: its Ed25519 public key, its X25519 public key, then the roster
/// label.
///
/// The Ed25519 key is the same one `ensure_this_device` hashes into the roster id, so the id the
/// peer derives from this identity is exactly the id this device reports for itself. The X25519
/// half travels on the same authenticated channel, so each side records the other's wrap target
/// without a second exchange (ADR 0020 invariant 14).
struct EngineProver {
    key: SigningKey,
    identity: Vec<u8>,
}

impl EngineProver {
    async fn load(ctx: &EngineContext) -> Result<Self, EngineError> {
        let key = device_key(ctx).await?;
        let exchange = device_exchange_key(ctx).await?;
        let mut identity = key.verifying_key().as_bytes().to_vec();
        identity.extend_from_slice(&exchange.public_key());
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

/// Seals this device's master key to the peer that just accepted the SAS.
///
/// The envelope is built only after the peer's identity arrived. It verifies the peer's proof
/// first, so a device that cannot prove the Ed25519 key it claims never receives a wrap, and
/// then uses the wrap helper from `alloy-sync` with the epoch this device is on. A failure aborts
/// the transfer before the bundle is sent.
struct EngineMasterEnvelope {
    master: MasterKey,
    signer: SigningKey,
    sender_id: String,
    epoch: u32,
}

impl MasterEnvelopeProvider for EngineMasterEnvelope {
    fn envelope(&self, peer: &PairedPeer) -> Result<Option<MasterEnvelope>, PairingError> {
        let (device_id, _label, x25519_pub) =
            verify_peer(peer).map_err(|err| PairingError::Protocol(err.message))?;
        let recipient_key =
            decode_device_key(&x25519_pub).map_err(|err| PairingError::Protocol(err.message))?;
        let bytes = wrap_master(
            &self.master,
            &device_id,
            &recipient_key,
            self.epoch,
            &self.sender_id,
            &self.signer,
        )
        .map_err(|err| {
            PairingError::Protocol(format!("cannot wrap the master key for the peer: {err}"))
        })?;
        Ok(Some(MasterEnvelope {
            epoch: self.epoch,
            bytes,
        }))
    }
}

/// Bytes of the Ed25519 key every pairing identity starts with.
const IDENTITY_KEY_LEN: usize = 32;
/// Bytes of the X25519 key that follows it.
const IDENTITY_X25519_LEN: usize = 32;

/// Checks the peer's proof over the transcript and reads its device id, wrap target and label.
///
/// The identity shape is
/// `Ed25519 public key (32 bytes) || X25519 public key (32 bytes) || UTF-8 label`, which is what
/// [`EngineProver`] sends. The returned id is the hex of the Ed25519 key: the same value the
/// peer's own roster uses for itself. Any failure here is a rejected pairing, never a warning.
fn verify_peer(peer: &PairedPeer) -> Result<(String, String, String), EngineError> {
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
    let x25519: [u8; IDENTITY_X25519_LEN] = peer
        .identity
        .get(IDENTITY_KEY_LEN..IDENTITY_KEY_LEN + IDENTITY_X25519_LEN)
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| {
            EngineError::pairing_failed(
                "the other device's identity does not carry an X25519 device key",
            )
        })?;
    let label = std::str::from_utf8(&peer.identity[IDENTITY_KEY_LEN + IDENTITY_X25519_LEN..])
        .map_err(|_| {
            EngineError::pairing_failed("the other device's pairing label is not UTF-8")
        })?;
    let label = if label.trim().is_empty() {
        "Paired device".to_string()
    } else {
        label.to_string()
    };
    Ok((hex(&key_bytes), label, hex(&x25519)))
}

/// Opens the pairing envelope and installs the master key it carries, then moves onto its epoch.
///
/// The peer's proof must already be verified (the caller does that with [`verify_peer`]): the
/// envelope's signature is checked against that proven Ed25519 key, and the wrap is addressed to
/// this device's X25519 key and id. A wrong recipient, a replay on another device or a tampered
/// envelope fails here, and the caller applies nothing. `None` is a version-1 offer, which never
/// carried a master key; a fresh device then stays settings-only exactly as before.
async fn adopt_paired_master(
    ctx: &EngineContext,
    peer: &PairedPeer,
    envelope: Option<&MasterEnvelope>,
    peer_id: &str,
) -> Result<(), EngineError> {
    let Some(envelope) = envelope else {
        return Ok(());
    };
    let this = ensure_this_device(ctx).await?;
    let sender_bytes: [u8; IDENTITY_KEY_LEN] = peer
        .identity
        .get(..IDENTITY_KEY_LEN)
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| EngineError::pairing_failed("the peer identity holds no Ed25519 key"))?;
    let sender = VerifyingKey::from_bytes(&sender_bytes)
        .map_err(|_| EngineError::pairing_failed("the peer identity holds no valid Ed25519 key"))?;
    let secret = device_exchange_key(ctx).await?;
    let (master, sender_id) = open_master(
        &secret,
        &this.device_id,
        envelope.epoch,
        &sender,
        &envelope.bytes,
    )
    .map_err(|err| {
        EngineError::pairing_failed(format!(
            "the pairing envelope did not open for this device: {err}"
        ))
    })?;
    if sender_id != peer_id {
        return Err(EngineError::pairing_failed(
            "the pairing envelope was signed by a device other than the one that proved itself",
        ));
    }
    // Never replace a different master: that would orphan every blob this device sealed under
    // the key it already holds. Recovery refuses a second master for the same reason.
    match ctx.secrets.get_secret(MASTER_KEY_SECRET).await {
        Ok(text) => {
            let existing = decode_32(&text, MASTER_KEY_SECRET)?;
            if existing != *master.expose_secret() {
                return Err(EngineError::pairing_failed(
                    "this device already holds a different master key; pairing will not replace it",
                ));
            }
        }
        Err(StorageError::SecretNotFound(_)) => {}
        Err(err) => return Err(map_secret(err)),
    }
    store_master_key(ctx, &master).await?;
    adopt_epoch(ctx, envelope.epoch)?;
    Ok(())
}

/// Writes an already-verified peer into this device's roster at the current epoch.
///
/// The caller must have checked `peer` with [`verify_peer`] first; nothing here re-checks, and
/// nothing here runs on a pairing whose proof failed.
async fn enrol_verified_peer(
    ctx: &EngineContext,
    device_id: String,
    label: String,
    x25519_pub: String,
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
                entry.x25519_pub = x25519_pub.clone();
            }
            None => devices.push(DeviceEntry {
                device_id: device_id.clone(),
                label: label.clone(),
                enrolled_ms: now_ms(),
                epoch,
                x25519_pub: x25519_pub.clone(),
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

// === Opt-in message mirror (`full` mode) ===

/// Mirror payload version. A payload this build does not know is refused, never guessed at.
pub(crate) const MIRROR_SCHEMA: u32 = 1;

/// Blob logical id of the one message-layer blob per batch.
pub(crate) const MIRROR_LAYER_ID: &str = "messages";

/// Largest plaintext that seals into one server-accepted body.
///
/// The server caps a body at 1 MiB (`MAX_BLOB_BYTES`), and the blob codec pads to 4 KiB. The
/// documented window therefore stops below that; it is not the pairing bundle's 48 MiB bound,
/// which never travels to the server. Bodies above 1 MiB would be refused with `413`.
pub(crate) const MIRROR_PLAINTEXT_LIMIT: usize = 1000 * 1024;

/// How many newest messages are considered before the byte budget stops the window. A bound on
/// the local read, not on what is published.
const MIRROR_MESSAGE_SCAN: u32 = 50_000;

/// One chat's metadata in the mirror. Identity is the network identity, never a local UUID.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct MirrorChat {
    pub network: String,
    pub account_remote_id: String,
    pub remote_chat_id: String,
    pub kind: ChatKind,
    pub title: Option<String>,
    pub last_activity_at: Option<i64>,
    pub unread_count: i64,
    pub mention_count: i64,
    #[serde(default)]
    pub read_markers: Vec<MirrorReadMarker>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct MirrorReadMarker {
    pub remote_user_id: String,
    pub up_to_message_id: String,
    pub read_at: i64,
}

/// One message body in the mirror. Media is out of scope: `has_media` records that an attachment
/// existed, and the bytes stay on the publishing device.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct MirrorMessage {
    /// The publishing device's stable local id. Reused on import when the row is unknown, and the
    /// identity a re-pull resolves to.
    pub message_id: String,
    pub remote_msg_id: Option<String>,
    pub chat_remote_id: String,
    pub sender_remote_id: String,
    pub is_own: bool,
    pub sent_at: i64,
    pub edited_at: Option<i64>,
    pub deleted_at: Option<i64>,
    pub kind: MessageKind,
    pub body: Option<String>,
    pub formatted: Option<String>,
    pub reply_to_message_id: Option<String>,
    pub has_media: bool,
}

/// The message layer as one blob travels: chat metadata, read markers and message bodies,
/// newest first, bounded to [`MIRROR_PLAINTEXT_LIMIT`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct MessageMirror {
    pub schema: u32,
    /// Always true: the window keeps the newest rows and drops older ones first.
    pub newest_first: bool,
    /// True when the payload budget omitted chats or messages; older history is not mirrored.
    pub truncated: bool,
    pub chats: Vec<MirrorChat>,
    pub messages: Vec<MirrorMessage>,
}

impl MessageMirror {
    /// Newest timestamp the window carries, for `SyncLayerStatus.updated_at_ms`.
    pub(crate) fn newest_at_ms(&self) -> i64 {
        let chats = self
            .chats
            .iter()
            .filter_map(|chat| chat.last_activity_at)
            .max()
            .unwrap_or(0);
        let messages = self
            .messages
            .iter()
            .map(|message| message.sent_at)
            .max()
            .unwrap_or(0);
        chats.max(messages)
    }

    /// Refuse a payload this build does not understand. Sizes are bounded by the blob codec.
    pub(crate) fn validate(&self) -> Result<(), EngineError> {
        if self.schema != MIRROR_SCHEMA {
            return Err(EngineError::sync_blocked("content"));
        }
        if !self.newest_first {
            return Err(EngineError::sync_blocked("content"));
        }
        Ok(())
    }
}

/// Read the store into a newest-first mirror window.
///
/// Chats are considered in newest-activity order, then messages newest first. A row that does not
/// fit once serialised ends the window and sets `truncated`; nothing is silently half-written.
pub(crate) async fn build_message_mirror(
    ctx: &EngineContext,
) -> Result<MessageMirror, EngineError> {
    let accounts = ctx.store.list_accounts().await.map_err(map_store)?;
    let mut account_keys = BTreeMap::new();
    for account in accounts {
        if let Some(remote_id) = account.remote_user_id {
            account_keys.insert(
                account.account_id.clone(),
                (account.network.as_str().to_string(), remote_id),
            );
        }
    }

    let mut markers_by_chat: BTreeMap<String, Vec<MirrorReadMarker>> = BTreeMap::new();
    for marker in ctx.store.list_all_read_markers().await.map_err(map_store)? {
        markers_by_chat
            .entry(marker.chat_id)
            .or_default()
            .push(MirrorReadMarker {
                remote_user_id: marker.remote_user_id,
                up_to_message_id: marker.up_to_message_id,
                read_at: marker.read_at,
            });
    }

    let mut mirror = MessageMirror {
        schema: MIRROR_SCHEMA,
        newest_first: true,
        truncated: false,
        chats: Vec::new(),
        messages: Vec::new(),
    };
    let mut budget = serde_json::to_vec(&mirror)
        .map_err(|err| EngineError::internal_error(format!("cannot encode the mirror: {err}")))?
        .len();

    let mut chat_remote_ids: BTreeMap<String, String> = BTreeMap::new();
    for chat in ctx.store.list_mirror_chats().await.map_err(map_store)? {
        let Some((network, account_remote_id)) = account_keys.get(&chat.account_id) else {
            // An account with no remote identity cannot be named on another device.
            continue;
        };
        let entry = MirrorChat {
            network: network.clone(),
            account_remote_id: account_remote_id.clone(),
            remote_chat_id: chat.remote_chat_id.clone(),
            kind: chat.kind,
            title: chat.title,
            last_activity_at: chat.last_activity_at,
            unread_count: chat.unread_count,
            mention_count: chat.mention_count,
            read_markers: markers_by_chat.remove(&chat.chat_id).unwrap_or_default(),
        };
        let size = serde_json::to_vec(&entry)
            .map_err(|err| EngineError::internal_error(format!("cannot encode a chat: {err}")))?
            .len();
        if budget.saturating_add(size).saturating_add(1) > MIRROR_PLAINTEXT_LIMIT {
            mirror.truncated = true;
            break;
        }
        budget += size + 1;
        chat_remote_ids.insert(chat.chat_id, chat.remote_chat_id);
        mirror.chats.push(entry);
    }

    let candidates = ctx
        .store
        .list_mirror_messages(MIRROR_MESSAGE_SCAN)
        .await
        .map_err(map_store)?;
    let scanned = candidates.len();
    for message in candidates {
        let Some(chat_remote_id) = chat_remote_ids.get(&message.chat_id) else {
            continue;
        };
        let entry = MirrorMessage {
            message_id: message.message_id,
            remote_msg_id: message.remote_msg_id,
            chat_remote_id: chat_remote_id.clone(),
            sender_remote_id: message.sender_remote_id,
            is_own: message.is_own,
            sent_at: message.sent_at,
            edited_at: message.edited_at,
            deleted_at: message.deleted_at,
            kind: message.kind,
            body: message.body,
            formatted: message.formatted,
            reply_to_message_id: message.reply_to_message_id,
            has_media: message.media_id.is_some(),
        };
        let size = serde_json::to_vec(&entry)
            .map_err(|err| EngineError::internal_error(format!("cannot encode a message: {err}")))?
            .len();
        if budget.saturating_add(size).saturating_add(1) > MIRROR_PLAINTEXT_LIMIT {
            mirror.truncated = true;
            break;
        }
        budget += size + 1;
        mirror.messages.push(entry);
    }
    // The scan bound is a second, independent window: hitting it means older rows exist and are
    // not in the payload, which the layer must say rather than hide.
    if scanned as u32 >= MIRROR_MESSAGE_SCAN {
        mirror.truncated = true;
    }

    // Keep the promise to the codec even after array separators and closing brackets.
    while serde_json::to_vec(&mirror)
        .map_err(|err| EngineError::internal_error(format!("cannot encode the mirror: {err}")))?
        .len()
        > MIRROR_PLAINTEXT_LIMIT
    {
        if mirror.messages.pop().is_none() && mirror.chats.pop().is_none() {
            return Err(EngineError::internal_error(
                "the message mirror cannot fit in one blob",
            ));
        }
        mirror.truncated = true;
    }
    Ok(mirror)
}

/// Apply a verified mirror to the local store, idempotently.
///
/// Chats resolve by `(network, account remote id)` then `remote_chat_id`; messages resolve by
/// their recorded id and fall back to the network message id. A re-pull therefore updates the
/// same rows instead of duplicating them. Unknown accounts are skipped: the mirror never creates
/// an account.
pub(crate) async fn import_message_mirror(
    ctx: &EngineContext,
    mirror: &MessageMirror,
) -> Result<(), EngineError> {
    mirror.validate()?;
    let mut accounts: BTreeMap<(String, String), String> = BTreeMap::new();
    for account in ctx.store.list_accounts().await.map_err(map_store)? {
        if let Some(remote_id) = account.remote_user_id {
            accounts.insert(
                (account.network.as_str().to_string(), remote_id),
                account.account_id,
            );
        }
    }

    let mut local_chats: BTreeMap<String, String> = BTreeMap::new();
    for chat in &mirror.chats {
        let Some(account_id) =
            accounts.get(&(chat.network.clone(), chat.account_remote_id.clone()))
        else {
            continue;
        };
        let chat_id = match ctx
            .store
            .find_chat(account_id, &chat.remote_chat_id)
            .await
            .map_err(map_store)?
        {
            Some(existing) => existing.chat_id,
            None => ctx
                .store
                .upsert_chat(ChatUpsert {
                    account_id: account_id.clone(),
                    remote_chat_id: chat.remote_chat_id.clone(),
                    kind: chat.kind,
                    title: chat.title.clone(),
                    avatar_media_id: None,
                    last_message_id: None,
                    last_activity_at: chat.last_activity_at,
                    parent_remote_chat_id: None,
                    is_forum: false,
                })
                .await
                .map_err(map_store)?,
        };
        let stored = ctx
            .store
            .get_chat(&chat_id)
            .await
            .map_err(map_store)?
            .ok_or_else(|| {
                EngineError::internal_error("the mirrored chat vanished after upsert")
            })?;
        if stored.unread_count != chat.unread_count || stored.mention_count != chat.mention_count {
            ctx.store
                .set_unread(&chat_id, chat.unread_count, chat.mention_count)
                .await
                .map_err(map_store)?;
        }
        local_chats.insert(chat.remote_chat_id.clone(), chat_id);
    }

    for message in &mirror.messages {
        let Some(chat_id) = local_chats.get(&message.chat_remote_id) else {
            continue;
        };
        let record = ctx
            .store
            .upsert_message(NewMessage {
                chat_id: chat_id.clone(),
                message_id: Some(message.message_id.clone()),
                remote_msg_id: message.remote_msg_id.clone(),
                txn_id: None,
                sender_remote_id: message.sender_remote_id.clone(),
                is_own: message.is_own,
                sent_at: message.sent_at,
                edited_at: message.edited_at,
                kind: message.kind,
                body: message.body.clone(),
                formatted: message.formatted.clone(),
                reply_to_message_id: message.reply_to_message_id.clone(),
                media_id: None,
                send_state: None,
                extra: None,
            })
            .await
            .map_err(map_store)?;
        if let Some(deleted_at) = message.deleted_at
            && record.deleted_at.is_none()
        {
            ctx.store
                .mark_deleted(&record.message_id, deleted_at)
                .await
                .map_err(map_store)?;
        }
    }

    for chat in &mirror.chats {
        let Some(chat_id) = local_chats.get(&chat.remote_chat_id) else {
            continue;
        };
        for marker in &chat.read_markers {
            ctx.store
                .set_read_marker(
                    chat_id,
                    &marker.remote_user_id,
                    &marker.up_to_message_id,
                    marker.read_at,
                )
                .await
                .map_err(map_store)?;
        }
    }
    if !mirror.chats.is_empty() || !mirror.messages.is_empty() {
        ctx.bus.chats();
    }
    Ok(())
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

/// Records the device whose signed head a recovery followed. The id comes from the batch that
/// this device verified, not from a server assertion, so the entry is evidence of the chain.
pub(crate) fn enroll_recovered_device(
    ctx: &EngineContext,
    device_id: &str,
    epoch: u32,
) -> Result<(), EngineError> {
    let short = device_id.get(..8).unwrap_or(device_id);
    SettingsHandler::with_settings_lock(ctx, || {
        let mut settings = settings_object(ctx)?;
        let mut sync = sync_section(&settings);
        let mut devices = roster(&settings)?;
        if devices.iter().any(|device| device.device_id == device_id) {
            return Ok(());
        }
        devices.push(DeviceEntry {
            device_id: device_id.to_string(),
            label: format!("Device {short}"),
            enrolled_ms: now_ms(),
            epoch,
            // Recovery follows a signed head, not an enrolment, so this device never received the
            // signer's X25519 key. The entry stays a chain witness until the two enrol.
            x25519_pub: String::new(),
        });
        sync["devices"] = serde_json::to_value(&devices).map_err(|err| {
            EngineError::internal_error(format!("cannot encode the device roster: {err}"))
        })?;
        settings["sync"] = sync;
        SettingsHandler::write_settings_file(ctx, settings)
    })
}

/// Moves this device onto an epoch a verified batch established. A no-op when it is already
/// there or ahead. The roster entries follow, exactly as a local revocation moves them.
pub(crate) fn adopt_epoch(ctx: &EngineContext, epoch: u32) -> Result<(), EngineError> {
    SettingsHandler::with_settings_lock(ctx, || {
        let mut settings = settings_object(ctx)?;
        let mut sync = sync_section(&settings);
        if sync_epoch(&sync) >= epoch {
            return Ok(());
        }
        let mut devices = roster(&settings)?;
        for device in &mut devices {
            device.epoch = epoch;
        }
        sync["epoch"] = json!(epoch);
        sync["devices"] = serde_json::to_value(&devices).map_err(|err| {
            EngineError::internal_error(format!("cannot encode the device roster: {err}"))
        })?;
        settings["sync"] = sync;
        SettingsHandler::write_settings_file(ctx, settings)
    })
}

/// The five configuration layers plus the message mirror, and when each last changed on this
/// device. `enabled` follows the configured mode, never a property of local data.
pub(crate) async fn sync_layer_statuses(
    ctx: &EngineContext,
    mode: SyncMode,
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
    // What the last successful publish actually put in the message layer. Absent until a `full`
    // device publishes once; the status never guesses from an unsynced local store.
    let window = settings
        .get("sync")
        .and_then(|sync| sync.get("server"))
        .and_then(|server| server.get("message_window"))
        .cloned()
        .unwrap_or(Value::Null);
    let messages_updated_at_ms = window
        .get("updated_at_ms")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let messages_windowed = window.get("windowed").and_then(Value::as_bool);
    let config_enabled = mode != SyncMode::Off;
    Ok(vec![
        SyncLayerStatus {
            key: "settings".to_string(),
            enabled: config_enabled,
            updated_at_ms: layer_updated_at_ms(&settings),
            windowed: None,
        },
        SyncLayerStatus {
            key: "tags".to_string(),
            enabled: config_enabled,
            updated_at_ms: tags_updated_at_ms,
            windowed: None,
        },
        SyncLayerStatus {
            key: "catalogue".to_string(),
            enabled: config_enabled,
            updated_at_ms: catalogue_updated_at_ms,
            windowed: None,
        },
        SyncLayerStatus {
            key: "backgrounds".to_string(),
            enabled: config_enabled,
            updated_at_ms: backgrounds_updated_at_ms,
            windowed: None,
        },
        SyncLayerStatus {
            key: "theme".to_string(),
            enabled: config_enabled,
            updated_at_ms: theme_updated_at_ms,
            windowed: None,
        },
        SyncLayerStatus {
            key: "messages".to_string(),
            enabled: mode == SyncMode::Full,
            updated_at_ms: messages_updated_at_ms,
            windowed: messages_windowed,
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

/// Reads this device's roster entry, creating the Ed25519 and X25519 keys and the entry on first
/// use. An entry written before device keys existed gains its public half here.
async fn ensure_this_device(ctx: &EngineContext) -> Result<DeviceEntry, EngineError> {
    let key = device_key(ctx).await?;
    let device_id = hex(key.verifying_key().as_bytes());
    let exchange = device_exchange_key(ctx).await?;
    let x25519_pub = hex(&exchange.public_key());
    let label = default_label();
    SettingsHandler::with_settings_lock(ctx, || {
        let mut settings = settings_object(ctx)?;
        let mut sync = sync_section(&settings);
        let mut devices = roster(&settings)?;
        let position = devices
            .iter()
            .position(|device| device.device_id == device_id);
        if let Some(position) = position {
            if devices[position].x25519_pub != x25519_pub {
                devices[position].x25519_pub = x25519_pub;
                sync["devices"] = serde_json::to_value(&devices).map_err(|err| {
                    EngineError::internal_error(format!("cannot encode the device roster: {err}"))
                })?;
                settings["sync"] = sync;
                SettingsHandler::write_settings_file(ctx, settings)?;
            }
            return Ok(devices[position].clone());
        }
        let device = DeviceEntry {
            device_id,
            label,
            enrolled_ms: now_ms(),
            epoch: sync_epoch(&sync),
            x25519_pub,
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

/// This device's X25519 key: created on first use and stored exactly where the master key lives
/// (ADR 0021's tier), so its private half never appears in `settings.json`. It is only a wrap
/// target; it derives no sync key on its own.
pub(crate) async fn device_exchange_key(
    ctx: &EngineContext,
) -> Result<DeviceSecretKey, EngineError> {
    match ctx.secrets.get_secret(DEVICE_X25519_SECRET).await {
        Ok(text) => Ok(DeviceSecretKey::from_bytes(decode_32(
            &text,
            DEVICE_X25519_SECRET,
        )?)),
        Err(StorageError::SecretNotFound(_)) => {
            let key = DeviceSecretKey::generate();
            ctx.secrets
                .set_secret(
                    DEVICE_X25519_SECRET,
                    &BASE64.encode(key.expose_secret().as_slice()),
                )
                .await
                .map_err(map_secret)?;
            Ok(key)
        }
        Err(err) => Err(map_secret(err)),
    }
}

/// Every roster device other than this one, with the X25519 key it published at enrolment. An
/// empty key marks a legacy entry that has to enrol again before it can follow a rotation.
pub(crate) async fn key_wrap_targets(
    ctx: &EngineContext,
) -> Result<Vec<(String, String)>, EngineError> {
    let this = ensure_this_device(ctx).await?;
    let settings = SettingsHandler::get_settings_file(ctx)?;
    Ok(roster(&settings)?
        .into_iter()
        .filter(|device| device.device_id != this.device_id)
        .map(|device| (device.device_id, device.x25519_pub))
        .collect())
}

/// Decodes a hex X25519 public key from a roster entry.
pub(crate) fn decode_device_key(text: &str) -> Result<[u8; 32], EngineError> {
    if text.len() != 64 {
        return Err(EngineError::internal_error(
            "the roster holds a device key that is not 64 hex characters",
        ));
    }
    let mut bytes = [0u8; 32];
    for (index, pair) in text.as_bytes().as_chunks::<2>().0.iter().enumerate() {
        let pair = std::str::from_utf8(pair)
            .map_err(|_| EngineError::internal_error("a roster device key is not ASCII hex"))?;
        bytes[index] = u8::from_str_radix(pair, 16)
            .map_err(|_| EngineError::internal_error("a roster device key is not hex"))?;
    }
    Ok(bytes)
}

/// Records that a recovery phrase created earlier no longer opens the account's current master.
/// Only a rotation calls this, and only when a phrase record exists: nothing is written otherwise.
pub(crate) fn mark_recovery_stale(ctx: &EngineContext) -> Result<(), EngineError> {
    SettingsHandler::with_settings_lock(ctx, || {
        let mut settings = settings_object(ctx)?;
        let mut sync = sync_section(&settings);
        if sync.get("recovery").is_none() {
            return Ok(());
        }
        sync["recovery_stale"] = json!(true);
        settings["sync"] = sync;
        SettingsHandler::write_settings_file(ctx, settings)
    })
}

/// Whether the stored recovery phrase predates a rotation. False on a device that never created
/// one; `CreateRecoveryPhrase` is the only way back to a usable phrase.
pub(crate) fn recovery_stale(settings: &Value) -> bool {
    settings
        .get("sync")
        .and_then(|sync| sync.get("recovery_stale"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
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

/// Whether this device already holds the account's master key. Recovery only targets a device
/// that has none: adopting a second master would orphan every blob sealed under the first.
pub(crate) async fn has_master_key(ctx: &EngineContext) -> Result<bool, EngineError> {
    match ctx.secrets.get_secret(MASTER_KEY_SECRET).await {
        Ok(_) => Ok(true),
        Err(StorageError::SecretNotFound(_)) => Ok(false),
        Err(err) => Err(map_secret(err)),
    }
}

/// Stores a master key recovered from the escrow, exactly where first use would have put it.
pub(crate) async fn store_master_key(
    ctx: &EngineContext,
    key: &MasterKey,
) -> Result<(), EngineError> {
    ctx.secrets
        .set_secret(MASTER_KEY_SECRET, &BASE64.encode(key.expose_secret()))
        .await
        .map_err(map_secret)
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
        escrow_b64: BASE64.encode(&escrow),
    };

    // Publish first: the words are returned only when the server actually holds what opens them,
    // so the user is never shown a phrase that no fresh device could use.
    crate::sync::publish_escrow(ctx, &escrow).await?;

    let words: Vec<String> = mnemonic.words().map(str::to_string).collect();
    SettingsHandler::with_settings_lock(ctx, || {
        let mut settings = settings_object(ctx)?;
        let mut sync = sync_section(&settings);
        sync["recovery"] = serde_json::to_value(&record).map_err(|err| {
            EngineError::internal_error(format!("cannot encode the recovery record: {err}"))
        })?;
        // A phrase created now wraps the current master, so it is never stale.
        sync["recovery_stale"] = json!(false);
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

/// The phrase wraps the master key into the blob published to the sync server. The wrap is
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
    let key = derive_wrapper_key(mnemonic, salt, m_cost, t_cost, p_cost).map_err(|err| {
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
        kdf: ESCROW_KDF.to_string(),
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

/// `argon2id(phrase, salt)`, the one key that opens an escrow blob.
fn derive_wrapper_key(
    mnemonic: &Mnemonic,
    salt: &[u8],
    m_cost: u32,
    t_cost: u32,
    p_cost: u32,
) -> Result<Zeroizing<[u8; RECOVERY_KEY_LEN]>, argon2::Error> {
    let params = Params::new(m_cost, t_cost, p_cost, Some(RECOVERY_KEY_LEN))?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = Zeroizing::new([0u8; RECOVERY_KEY_LEN]);
    argon.hash_password_into(mnemonic.to_string().as_bytes(), salt, &mut key[..])?;
    Ok(key)
}

/// Opens a server-held escrow blob with `words`. The parameters beside the blob come from an
/// untrusted server, so they are validated before the KDF runs. Every failure after a well-formed
/// blob is one indistinguishable `recovery_rejected`: a wrong word and a phrase from another
/// account must look the same, and the message never names a word.
pub(crate) fn unwrap_escrow(words: &[String], body: &[u8]) -> Result<MasterKey, EngineError> {
    if words.len() != RECOVERY_WORDS {
        return Err(EngineError::recovery_rejected(format!(
            "a recovery phrase has {RECOVERY_WORDS} words, not {}",
            words.len()
        )));
    }
    let mnemonic =
        Mnemonic::parse_in_normalized(Language::English, &words.join(" ")).map_err(|_| {
            EngineError::recovery_rejected("the phrase does not open the recovery escrow")
        })?;

    let blob: EscrowBlob = serde_json::from_slice(body).map_err(|_| {
        EngineError::unavailable("the sync server's recovery escrow is not a valid blob")
    })?;
    if blob.version != ESCROW_VERSION || blob.kdf != ESCROW_KDF {
        return Err(EngineError::unavailable(
            "the sync server's recovery escrow uses a format this build does not open",
        ));
    }
    if !(RECOVERY_M_COST_MIN..=RECOVERY_M_COST_MAX).contains(&blob.m_cost)
        || !(1..=RECOVERY_T_COST_MAX).contains(&blob.t_cost)
        || !(1..=RECOVERY_P_COST_MAX).contains(&blob.p_cost)
    {
        return Err(EngineError::unavailable(
            "the sync server's recovery escrow asks for KDF parameters outside the accepted range",
        ));
    }
    let salt = decode_b64(&blob.salt_b64, "recovery salt").map_err(|_| {
        EngineError::unavailable("the sync server's recovery escrow has a malformed salt")
    })?;
    let nonce = decode_b64(&blob.nonce_b64, "recovery nonce").map_err(|_| {
        EngineError::unavailable("the sync server's recovery escrow has a malformed nonce")
    })?;
    let ciphertext = decode_b64(&blob.ciphertext_b64, "recovery ciphertext").map_err(|_| {
        EngineError::unavailable("the sync server's recovery escrow has a malformed body")
    })?;
    if salt.len() != RECOVERY_SALT_LEN
        || nonce.len() != RECOVERY_NONCE_LEN
        || ciphertext.len() != RECOVERY_KEY_LEN + 16
    {
        return Err(EngineError::unavailable(
            "the sync server's recovery escrow has the wrong shape",
        ));
    }

    let key = derive_wrapper_key(&mnemonic, &salt, blob.m_cost, blob.t_cost, blob.p_cost).map_err(
        |err| EngineError::internal_error(format!("cannot derive the recovery key: {err}")),
    )?;
    let cipher = ChaCha20Poly1305::new(
        &Key::try_from(&key[..])
            .map_err(|_| EngineError::internal_error("the recovery key is not 32 bytes"))?,
    );
    let plaintext = Zeroizing::new(
        cipher
            .decrypt(
                &Nonce::try_from(&nonce[..]).map_err(|_| {
                    EngineError::internal_error("the recovery nonce is not 12 bytes")
                })?,
                Payload {
                    msg: &ciphertext,
                    aad: ESCROW_AAD,
                },
            )
            .map_err(|_| {
                EngineError::recovery_rejected("the phrase does not open the recovery escrow")
            })?,
    );
    let bytes: [u8; RECOVERY_KEY_LEN] = plaintext
        .as_slice()
        .try_into()
        .map_err(|_| EngineError::recovery_rejected("the recovery escrow holds no master key"))?;
    Ok(MasterKey::from_bytes(bytes))
}

fn decode_b64(text: &str, name: &str) -> Result<Vec<u8>, EngineError> {
    BASE64
        .decode(text)
        .map_err(|err| EngineError::internal_error(format!("{name} is not base64: {err}")))
}

fn random_error(err: getrandom::Error) -> EngineError {
    EngineError::internal_error(format!("the OS random source failed: {err}"))
}

#[cfg(test)]
mod tests {
    //! Pins the end-of-pairing log policy: routine ends are debug, only a broken pairing warns.
    //!
    //! `finish_pairing` is the one place a pairing ends, so its level choice is the whole policy.
    //! Log levels are observed with a hand-written subscriber installed as the thread-local
    //! default, the same way `tests/backfill_warn.rs` does: `alloy-engine` has no
    //! `tracing-subscriber` dependency to build a writer from.

    use std::fmt::Write as _;
    use std::sync::{Arc, Mutex};

    use tracing::field::{Field, Visit};
    use tracing::span;
    use tracing::{Event, Level, Metadata, Subscriber};

    use super::*;

    #[derive(Default)]
    struct LogCapture {
        events: Mutex<Vec<(Level, String)>>,
    }

    impl LogCapture {
        fn new() -> Self {
            Self::default()
        }

        fn snapshot(&self) -> Vec<(Level, String)> {
            self.events.lock().expect("log capture lock").clone()
        }
    }

    /// Renders an event's fields as `name=value` pairs.
    #[derive(Default)]
    struct RenderedFields(String);

    impl Visit for RenderedFields {
        fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
            if !self.0.is_empty() {
                self.0.push(' ');
            }
            let _ = write!(self.0, "{}={value:?}", field.name());
        }
    }

    impl Subscriber for LogCapture {
        fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
            // Capture every level, so a test proves `debug` replaced `warn` rather than both
            // being filtered out.
            true
        }

        fn new_span(&self, _span: &span::Attributes<'_>) -> span::Id {
            span::Id::from_u64(1)
        }

        fn record(&self, _span: &span::Id, _values: &span::Record<'_>) {}

        fn record_follows_from(&self, _span: &span::Id, _follows: &span::Id) {}

        fn event(&self, event: &Event<'_>) {
            let mut fields = RenderedFields::default();
            event.record(&mut fields);
            self.events
                .lock()
                .expect("log capture lock")
                .push((*event.metadata().level(), fields.0));
        }

        fn enter(&self, _span: &span::Id) {}

        fn exit(&self, _span: &span::Id) {}
    }

    fn live_pairing() -> (Pairings, EventSink) {
        let pairings: Pairings = Arc::new(Mutex::new(HashMap::new()));
        lock(&pairings).insert("p1".to_string(), PairingEntry::waiting());
        (pairings, EventSink::new())
    }

    fn terminal_states(
        events: &mut tokio::sync::broadcast::Receiver<EngineEvent>,
    ) -> Vec<PairingPhase> {
        let mut phases = Vec::new();
        while let Ok(event) = events.try_recv() {
            if let EngineEvent::PairingChanged { state, .. } = event {
                phases.push(state);
            }
        }
        phases
    }

    /// The report that produced this test: a pairing expiring after its 90 seconds logged
    /// `WARN ... error=pairing code expired` and read as a failure. It is routine.
    #[test]
    fn an_expired_pairing_logs_no_warning() {
        let capture = Arc::new(LogCapture::new());
        let _guard = tracing::subscriber::set_default(capture.clone());
        let (pairings, sink) = live_pairing();
        let mut events = sink.subscribe();

        finish_pairing(
            &pairings,
            &sink,
            "p1",
            PairingPhase::Expired,
            Some("pairing code expired after 90s".to_string()),
        );

        let logged = capture.snapshot();
        assert!(
            logged.iter().all(|(level, _)| *level != Level::WARN),
            "expiry must not warn: {logged:?}"
        );
        let debug: Vec<_> = logged
            .iter()
            .filter(|(level, _)| *level == Level::DEBUG)
            .collect();
        assert_eq!(
            debug.len(),
            1,
            "expiry is still traceable at debug: {logged:?}"
        );
        assert_eq!(
            terminal_states(&mut events),
            vec![PairingPhase::Expired],
            "the UI is told the code is no longer offered"
        );
        assert!(
            !lock(&pairings).contains_key("p1"),
            "the ended pairing is dropped, so its code and ticket do not outlive it"
        );
    }

    #[test]
    fn a_declined_pairing_logs_no_warning() {
        let capture = Arc::new(LogCapture::new());
        let _guard = tracing::subscriber::set_default(capture.clone());
        let (pairings, sink) = live_pairing();

        finish_pairing(
            &pairings,
            &sink,
            "p1",
            PairingPhase::Declined,
            Some("the other device declined".to_string()),
        );

        let logged = capture.snapshot();
        assert!(
            logged.iter().all(|(level, _)| *level != Level::WARN),
            "a decline is a decision, not an error: {logged:?}"
        );
    }

    #[test]
    fn a_failed_pairing_still_warns_with_its_reason() {
        let capture = Arc::new(LogCapture::new());
        let _guard = tracing::subscriber::set_default(capture.clone());
        let (pairings, sink) = live_pairing();

        finish_pairing(
            &pairings,
            &sink,
            "p1",
            PairingPhase::Failed,
            Some("the peer proof did not verify".to_string()),
        );

        let logged = capture.snapshot();
        let warnings: Vec<_> = logged
            .iter()
            .filter(|(level, _)| *level == Level::WARN)
            .collect();
        assert_eq!(
            warnings.len(),
            1,
            "a broken pairing is one actionable warning: {logged:?}"
        );
        assert!(
            warnings[0].1.contains("the peer proof did not verify"),
            "the warning keeps the reason: {:?}",
            warnings[0].1
        );
    }
}
