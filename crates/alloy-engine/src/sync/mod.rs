//! The optional sync server (ADR 0020): configure, publish, pull, and the pin gate.
//!
//! The server is a byte store with one compare-and-swap `head`. This module is the only place
//! that talks to it:
//!
//! - every layer is sealed with [`alloy_sync::seal`] under the master key's epoch key, and named
//!   with `HMAC(addr_key, slot)` so the server cannot enumerate or correlate blobs;
//! - the published `head` is a signed [`SignedBatch`]; a `Fork`, `Branch` or signature failure
//!   stops with `sync_blocked` and leaves local state exactly as it was;
//! - a competing `PUT /v1/head` is re-read and rebased, never overwritten;
//! - the endpoint is pinned to its leaf's SPKI on first success, and plain `http` is loopback
//!   only.
//!
//! Pulled layers go through the existing bundle import (`handlers::sync`), so the server path
//! merges with the same timestamp rules as a LAN pairing. The recovery escrow is published
//! beside the head, and [`recover`] turns a phrase into the master key that opens the head.

mod http;
mod tls;

use std::collections::BTreeMap;
use std::path::PathBuf;

use alloy_protocol::{EngineError, SyncMode, SyncState, SyncStatusReport};
use alloy_storage::SecretStore;
use alloy_storage::StorageError;
use alloy_storage::config_bundle::{BUNDLE_SCHEMA, ChatKey, ConfigBundle, TagSet};
use alloy_sync::chain::{Head, HeadStore};
use alloy_sync::device::{DeviceWrapError, open_master, wrap_master};
use alloy_sync::{
    BatchHeader, BlobAddr, BlobKeyRing, ChainError, FileHeadStore, Layer, MasterKey, Pin,
    SignedBatch, open, seal, verify_batch,
};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ed25519_dalek::{SigningKey, VerifyingKey};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::context::EngineContext;
use crate::handlers::settings::SettingsHandler;
use crate::handlers::sync::{
    MIRROR_LAYER_ID, MessageMirror, adopt_epoch, build_bundle, build_message_mirror,
    decode_device_key, device_exchange_key, device_key, enroll_recovered_device, has_master_key,
    import_bundle_value, import_message_mirror, key_wrap_targets, list_devices,
    mark_recovery_stale, master_key, recovery_stale, settings_object, store_master_key, sync_epoch,
    sync_layer_statuses, sync_section, unwrap_escrow,
};
use crate::support::now_ms;

use http::{Endpoint, Precondition, WriteOutcome, parse_target};

/// The bearer token. It lives in the platform secret store (ADR 0019), never in `settings.json`.
const TOKEN_SECRET: &str = "alloy.sync.server_token.v1";

/// Verified heads, persisted between runs so a replay is detectable across restarts.
const HEAD_STORE_FILE: &str = "sync-heads.json";

/// Blobs this device published, per epoch, persisted next to the head store. Revocation is the
/// only consumer: the server offers no enumeration, so the client must know every id it wrote.
const BLOB_INDEX_FILE: &str = "sync-blobs.json";

/// Index file version. A file this build does not understand is refused, never guessed at.
const BLOB_INDEX_VERSION: u32 = 1;

/// Domain separator for the batch content root.
const ROOT_DOMAIN: &[u8] = b"alloy-sync-v1/root/1";

/// Domain separator for blob ids derived from [`BlobAddr`].
const ADDR_DOMAIN: &[u8] = b"alloy-sync-v1/blob/id/1";

/// Domain separator for the ids of rotation key wraps. Distinct from [`ADDR_DOMAIN`] so a wrap id
/// can never collide with a layer blob id under the same epoch key.
const WRAP_ID_DOMAIN: &[u8] = b"alloy-sync-v1/device-key/id/1";

/// Layer name the blob index records beside a rotation wrap, for a human reading the file.
const WRAP_LAYER: &str = "device-key";

/// At most this many head CAS retries before the run reports the server is churning.
const MAX_REBASE_ATTEMPTS: usize = 4;

/// The configuration layers the `prefs` mode mirrors, in a fixed order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum LayerKey {
    Settings,
    Tags,
    Catalogue,
    Backgrounds,
    Theme,
}

impl LayerKey {
    const ALL: [Self; 5] = [
        Self::Settings,
        Self::Tags,
        Self::Catalogue,
        Self::Backgrounds,
        Self::Theme,
    ];

    fn id(self) -> &'static str {
        match self {
            Self::Settings => "settings",
            Self::Tags => "tags",
            Self::Catalogue => "catalogue",
            Self::Backgrounds => "backgrounds",
            Self::Theme => "theme",
        }
    }

    fn layer(self) -> Layer {
        match self {
            Self::Tags => Layer::Tags,
            _ => Layer::Config,
        }
    }
}

/// The persisted server section of `settings.json`, plus what the last run reported.
#[derive(Debug, Clone)]
struct ServerState {
    mode: SyncMode,
    url: Option<String>,
    pin: Option<Pin>,
    state: SyncState,
    last_sync_ms: Option<i64>,
    last_error: Option<String>,
}

impl Default for ServerState {
    fn default() -> Self {
        Self {
            mode: SyncMode::Off,
            url: None,
            pin: None,
            state: SyncState::Idle,
            last_sync_ms: None,
            last_error: None,
        }
    }
}

impl ServerState {
    fn from_settings(settings: &Value) -> Self {
        let server = settings
            .get("sync")
            .and_then(|sync| sync.get("server"))
            .cloned()
            .unwrap_or_else(|| json!({}));
        let mode = server
            .get("mode")
            .cloned()
            .and_then(|value| serde_json::from_value::<SyncMode>(value).ok())
            .unwrap_or(SyncMode::Off);
        let state = server
            .get("state")
            .cloned()
            .and_then(|value| serde_json::from_value::<SyncState>(value).ok())
            .unwrap_or(SyncState::Idle);
        Self {
            mode,
            url: server
                .get("url")
                .and_then(Value::as_str)
                .map(str::to_string),
            pin: server
                .get("pin")
                .and_then(Value::as_str)
                .and_then(|text| Pin::parse(text).ok()),
            state,
            last_sync_ms: server.get("last_sync_ms").and_then(Value::as_i64),
            last_error: server
                .get("last_error")
                .and_then(Value::as_str)
                .map(str::to_string),
        }
    }
}

/// One blob this device published: enough to name it for deletion, nothing that names content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct IndexedBlob {
    id: String,
    /// The layer key (`settings`, `tags`, …), for a human reading the file.
    layer: String,
    seq: u64,
}

/// The on-disk shape, versioned so a future change is refused rather than misread.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct BlobIndexFile {
    version: u32,
    #[serde(default)]
    epochs: BTreeMap<u32, Vec<IndexedBlob>>,
}

/// Every blob this device published, grouped by the epoch it was sealed under.
///
/// The server cannot enumerate what it stores, so this local index is the only list of ids a
/// revocation may delete. It is written after each successful `PUT`: a crash between the upload
/// and the write would leave ciphertext the client could never name again.
struct BlobIndex {
    path: PathBuf,
    epochs: BTreeMap<u32, Vec<IndexedBlob>>,
}

impl BlobIndex {
    fn open(path: PathBuf) -> Result<Self, EngineError> {
        let epochs = match std::fs::read(&path) {
            Ok(bytes) => {
                let file: BlobIndexFile = serde_json::from_slice(&bytes).map_err(|error| {
                    EngineError::internal_error(format!(
                        "the sync blob index {} is corrupt: {error}",
                        path.display()
                    ))
                })?;
                if file.version != BLOB_INDEX_VERSION {
                    return Err(EngineError::internal_error(format!(
                        "the sync blob index has version {}, not {BLOB_INDEX_VERSION}",
                        file.version
                    )));
                }
                file.epochs
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
            Err(error) => {
                return Err(EngineError::internal_error(format!(
                    "cannot read the sync blob index {}: {error}",
                    path.display()
                )));
            }
        };
        Ok(Self { path, epochs })
    }

    /// Records one blob as published under `epoch` and persists the index atomically.
    fn record(
        &mut self,
        epoch: u32,
        layer: &'static str,
        seq: u64,
        id: &str,
    ) -> Result<(), EngineError> {
        let entries = self.epochs.entry(epoch).or_default();
        entries.retain(|blob| blob.id != id);
        entries.push(IndexedBlob {
            id: id.to_string(),
            layer: layer.to_string(),
            seq,
        });
        self.persist()
    }

    /// Epochs older than `epoch`, ascending. Their blobs are the ones a rotation invalidates.
    fn older_epochs(&self, epoch: u32) -> Vec<u32> {
        self.epochs
            .keys()
            .copied()
            .filter(|old| *old < epoch)
            .collect()
    }

    fn blobs(&self, epoch: u32) -> Vec<IndexedBlob> {
        self.epochs.get(&epoch).cloned().unwrap_or_default()
    }

    /// Drops one id, so a later sweep does not retry what the server already removed.
    fn forget(&mut self, epoch: u32, id: &str) {
        if let Some(entries) = self.epochs.get_mut(&epoch) {
            entries.retain(|blob| blob.id != id);
            if entries.is_empty() {
                self.epochs.remove(&epoch);
            }
        }
    }

    fn persist(&self) -> Result<(), EngineError> {
        let file = BlobIndexFile {
            version: BLOB_INDEX_VERSION,
            epochs: self.epochs.clone(),
        };
        let bytes = serde_json::to_vec_pretty(&file).map_err(|error| {
            EngineError::internal_error(format!("cannot encode the sync blob index: {error}"))
        })?;
        let temp = self.path.with_extension("json.tmp");
        std::fs::write(&temp, bytes).map_err(|error| {
            EngineError::internal_error(format!(
                "cannot write the sync blob index {}: {error}",
                temp.display()
            ))
        })?;
        std::fs::rename(&temp, &self.path).map_err(|error| {
            EngineError::internal_error(format!(
                "cannot replace the sync blob index {}: {error}",
                self.path.display()
            ))
        })
    }
}

fn blob_index_path(ctx: &EngineContext) -> PathBuf {
    ctx.data_dir.join(BLOB_INDEX_FILE)
}

/// `ConfigureSync`: validate the endpoint, probe it, pin it on first success, then sync once.
pub async fn configure(
    ctx: &EngineContext,
    url: String,
    token: String,
    mode: SyncMode,
    accept_new_pin: bool,
) -> Result<(Option<String>, SyncStatusReport), EngineError> {
    let target = parse_target(&url)?;
    let settings = SettingsHandler::get_settings_file(ctx)?;
    let stored = ServerState::from_settings(&settings);
    let token = if token.trim().is_empty() {
        read_token(ctx).await?.ok_or_else(|| {
            EngineError::invalid_argument("no sync token is stored; provide one to configure sync")
        })?
    } else {
        token
    };

    if mode == SyncMode::Off {
        store_token(ctx, &token).await?;
        update_server(ctx, |server| {
            server["mode"] = json!("off");
            server["url"] = json!(url);
            server["pin"] = Value::Null;
            server["state"] = json!("idle");
            server["last_error"] = Value::Null;
        })?;
        return Ok((None, status(ctx).await?));
    }

    // With a stored pin the handshake must match it before any request (and the token) is sent.
    // Without one, or when the caller accepts a rotation, the verifier captures the leaf instead.
    let enforce = if target.secure && !accept_new_pin {
        stored.pin
    } else {
        None
    };
    let endpoint = Endpoint::new(&target, token.clone(), enforce)?;
    let probe = match endpoint.get_head().await {
        Ok(probe) => probe,
        Err(error) if error.code == "pin_mismatch" => {
            update_server(ctx, |server| {
                server["state"] = json!("blocked");
                server["last_error"] = json!("pin_mismatch");
            })?;
            ctx.sink
                .emit(alloy_protocol::EngineEvent::SyncStateChanged {
                    state: SyncState::Blocked,
                    detail: Some("pin_mismatch".to_string()),
                });
            return Err(error);
        }
        Err(error) => return Err(error),
    };
    let pin = if target.secure {
        let observed = endpoint.observed_pin().ok_or_else(|| {
            EngineError::internal_error("the sync endpoint presented no certificate")
        })?;
        if let Some(expected) = enforce
            && expected != observed
        {
            return Err(EngineError::pin_mismatch(
                &expected.to_hex(),
                &observed.to_hex(),
            ));
        }
        Some(observed)
    } else {
        None
    };
    drop(probe);

    store_token(ctx, &token).await?;
    update_server(ctx, |server| {
        server["mode"] = json!(mode);
        server["url"] = json!(url);
        server["pin"] = match pin {
            Some(pin) => json!(pin.to_hex()),
            None => Value::Null,
        };
        server["state"] = json!("idle");
        server["last_error"] = Value::Null;
    })?;

    sync_now(ctx).await?;
    let status = status(ctx).await?;
    Ok((pin.map(Pin::to_hex), status))
}

/// `CreateRecoveryPhrase` publishes the wrapper here, under the same compare-and-swap as the
/// head. Nothing to do while sync is off; a configured server is the only place a fresh device
/// can find the escrow.
pub(crate) async fn publish_escrow(ctx: &EngineContext, blob: &[u8]) -> Result<(), EngineError> {
    let settings = SettingsHandler::get_settings_file(ctx)?;
    let server = ServerState::from_settings(&settings);
    if server.mode == SyncMode::Off {
        return Ok(());
    }
    let Some(url) = server.url.clone() else {
        return Ok(());
    };
    let target = parse_target(&url)?;
    if target.secure && server.pin.is_none() {
        return Err(EngineError::unavailable(
            "the sync endpoint has no stored pin; configure it again",
        ));
    }
    let token = read_token(ctx)
        .await?
        .ok_or_else(|| EngineError::unavailable("no sync token is stored for this endpoint"))?;
    let endpoint = Endpoint::new(&target, token, server.pin.filter(|_| target.secure))?;

    // One re-read on a loss: the loser of the CAS adopts the winner's body and tries again. A
    // second loss is reported rather than looped, because another device is actively writing.
    for attempt in 0..2 {
        let precondition = match endpoint.get_escrow().await? {
            Some((bytes, _etag)) if bytes == blob => return Ok(()),
            Some((_bytes, etag)) => Precondition::Match(etag),
            None => Precondition::CreateOnly,
        };
        match endpoint.put_escrow(blob, precondition).await? {
            WriteOutcome::Written => return Ok(()),
            WriteOutcome::Conflict { .. } if attempt == 0 => continue,
            WriteOutcome::Conflict { .. } => break,
        }
    }
    Err(EngineError::network_error(
        "another device kept writing the sync escrow; try again",
    ))
}

/// `RecoverFromPhrase`: a device with no master key adopts the one in the server's escrow and
/// pulls the head its phrase-protected key can open.
///
/// Nothing is written until the phrase has opened the escrow: a wrong phrase leaves no master
/// key, no token, no settings and no head cursor behind. The KDF parameters arrive with the blob
/// and are validated before they are used.
pub async fn recover(
    ctx: &EngineContext,
    words: &[String],
    url: String,
    token: String,
    accept_new_pin: bool,
) -> Result<SyncStatusReport, EngineError> {
    if has_master_key(ctx).await? {
        return Err(EngineError::invalid_argument(
            "this device already has a sync master key; recovery targets a fresh device",
        ));
    }
    let target = parse_target(&url)?;
    let settings = SettingsHandler::get_settings_file(ctx)?;
    let stored = ServerState::from_settings(&settings);
    let token = if token.trim().is_empty() {
        read_token(ctx).await?.ok_or_else(|| {
            EngineError::invalid_argument("no sync token is stored; provide one to recover")
        })?
    } else {
        token
    };

    // Same pin rule as `ConfigureSync`: a stored pin is enforced before the token is sent; a
    // first connection captures the leaf, and `accept_new_pin` is the only way past a stored one.
    let enforce = if target.secure && !accept_new_pin {
        stored.pin
    } else {
        None
    };
    let endpoint = Endpoint::new(&target, token.clone(), enforce)?;

    let Some((escrow, _etag)) = endpoint.get_escrow().await? else {
        return Err(EngineError::unavailable(
            "the sync server holds no recovery escrow; create a phrase on a device that has one",
        ));
    };
    // Everything above is a read. The phrase is checked here, before the first write.
    let master = unwrap_escrow(words, &escrow)?;

    let store_path = ctx.data_dir.join(HEAD_STORE_FILE);
    let mut store = FileHeadStore::open(&store_path)
        .map_err(|error| EngineError::sync_blocked(reason_for_store(&error)))?;
    let mut signer: Option<(String, u32)> = None;
    let mut recovered_mirror = false;
    if let Some((bytes, _etag)) = endpoint.get_head().await? {
        let (_, batch) = observe_head(&mut store, &bytes)?;
        // A recovered device starts at epoch 0; the head names the epoch the account is on.
        adopt_epoch(ctx, batch.header.epoch)?;
        pull_layers(ctx, &endpoint, &master, &batch, true).await?;
        recovered_mirror = batch.header.mirror;
        signer = Some((batch.header.device_id.clone(), batch.header.epoch));
    }

    store_master_key(ctx, &master).await?;
    store_token(ctx, &token).await?;
    let pin = if target.secure {
        let observed = endpoint.observed_pin().ok_or_else(|| {
            EngineError::internal_error("the sync endpoint presented no certificate")
        })?;
        if let Some(expected) = enforce
            && expected != observed
        {
            return Err(EngineError::pin_mismatch(
                &expected.to_hex(),
                &observed.to_hex(),
            ));
        }
        Some(observed)
    } else {
        None
    };
    let pin_hex = pin.map(Pin::to_hex);
    update_server(ctx, |server| {
        server["mode"] = json!(if recovered_mirror {
            SyncMode::Full
        } else {
            SyncMode::Prefs
        });
        server["url"] = json!(url);
        server["pin"] = match &pin_hex {
            Some(pin) => json!(pin),
            None => Value::Null,
        };
        server["state"] = json!("done");
        server["last_sync_ms"] = json!(now_ms());
        server["last_error"] = Value::Null;
    })?;
    if let Some((device_id, epoch)) = signer {
        enroll_recovered_device(ctx, &device_id, epoch)?;
    }
    status(ctx).await
}

/// `SyncStatus`: what is configured and when each layer last changed. Never the token.
pub async fn status(ctx: &EngineContext) -> Result<SyncStatusReport, EngineError> {
    let settings = SettingsHandler::get_settings_file(ctx)?;
    let server = ServerState::from_settings(&settings);
    let token_configured = read_token(ctx).await?.is_some();
    let layers = sync_layer_statuses(ctx, server.mode).await?;
    Ok(SyncStatusReport {
        mode: server.mode,
        url: server.url,
        pin: server.pin.map(Pin::to_hex),
        token_configured,
        state: server.state,
        last_sync_ms: server.last_sync_ms,
        last_error: server.last_error,
        recovery_stale: settings
            .get("sync")
            .and_then(|sync| sync.get("recovery"))
            .is_some()
            .then(|| recovery_stale(&settings)),
        layers,
        devices: list_devices(ctx).await?,
    })
}

/// `SyncNow`: one publish/pull pass. A chain violation is `sync_blocked`, never a retry loop.
pub async fn sync_now(ctx: &EngineContext) -> Result<(u64, u64, String, u32), EngineError> {
    ctx.sink
        .emit(alloy_protocol::EngineEvent::SyncStateChanged {
            state: SyncState::Running,
            detail: None,
        });
    match run_once(ctx).await {
        Ok((uploaded, downloaded, head_hex, epoch)) => {
            update_server(ctx, |server| {
                server["state"] = json!("done");
                server["last_sync_ms"] = json!(now_ms());
                server["last_error"] = Value::Null;
            })?;
            ctx.sink
                .emit(alloy_protocol::EngineEvent::SyncStateChanged {
                    state: SyncState::Done,
                    detail: None,
                });
            Ok((uploaded, downloaded, head_hex, epoch))
        }
        Err(error) if error.code == "sync_blocked" || error.code == "pin_mismatch" => {
            let reason = error.reason.clone().unwrap_or_else(|| "chain".to_string());
            update_server(ctx, |server| {
                server["state"] = json!("blocked");
                server["last_sync_ms"] = json!(now_ms());
                server["last_error"] = json!(reason);
            })?;
            ctx.sink
                .emit(alloy_protocol::EngineEvent::SyncStateChanged {
                    state: SyncState::Blocked,
                    detail: Some(reason),
                });
            Err(error)
        }
        Err(error) => {
            update_server(ctx, |server| {
                server["state"] = json!("failed");
                server["last_sync_ms"] = json!(now_ms());
                server["last_error"] = json!(error.code);
            })?;
            ctx.sink
                .emit(alloy_protocol::EngineEvent::SyncStateChanged {
                    state: SyncState::Failed,
                    detail: Some(error.code.clone()),
                });
            Err(error)
        }
    }
}

/// What a revocation managed to do on the server. The roster change and the epoch bump are the
/// security boundary and stand on their own; this says how much of the ciphertext followed.
#[derive(Debug, Clone, Default)]
pub(crate) struct RevokeOutcome {
    /// The layers were re-published under the new epoch; the revoked device cannot open them.
    pub re_sealed: bool,
    /// Blobs from an older epoch the server confirmed gone (including already-absent ones).
    pub blobs_deleted: u32,
    /// Blobs from an older epoch the server still holds.
    pub blobs_remaining: u32,
    /// The master key rotated, so an existing recovery phrase no longer opens new data.
    pub recovery_stale: bool,
    /// Why the sweep was incomplete. The revocation itself is valid regardless.
    pub error: Option<String>,
}

/// Re-seals the account after a device was revoked (ADR 0020 invariant 14).
///
/// Order is the whole design: rotate the master key, publish every enabled layer under the new
/// epoch, and only then delete the older epoch's blobs. A failure before the publish deletes
/// nothing, because losing data is worse than leaving ciphertext that is already unreadable to
/// the revoked device. A failed delete leaves the rotation valid and is reported, never hidden.
pub(crate) async fn reseal_after_revoke(ctx: &EngineContext, epoch: u32) -> RevokeOutcome {
    let mut outcome = RevokeOutcome::default();

    // The new epoch key must not be derivable from the key the revoked device still holds, so the
    // root itself rotates. The old master is not kept: this device already holds its own data.
    let had_master = match has_master_key(ctx).await {
        Ok(had) => had,
        Err(error) => {
            outcome.error = Some(format!(
                "the master key state could not be read: {}",
                error.message
            ));
            return outcome;
        }
    };
    // The old master is needed for one thing: naming the wraps under the epoch key the survivors
    // still hold. It is dropped as soon as the wraps are uploaded.
    let old_master = if had_master {
        match master_key(ctx).await {
            Ok(key) => Some(key),
            Err(error) => {
                outcome.error = Some(format!(
                    "the previous master key could not be read, so nothing was rotated: {}",
                    error.message
                ));
                return outcome;
            }
        }
    } else {
        None
    };
    let master = MasterKey::generate();
    if let Err(error) = store_master_key(ctx, &master).await {
        outcome.error = Some(format!(
            "the master key could not be rotated: {}",
            error.message
        ));
        return outcome;
    }
    outcome.recovery_stale = had_master;
    // The old escrow wraps a master that will never seal new data again. The phrase is not kept,
    // so the escrow cannot be re-sealed here; it is reported and `CreateRecoveryPhrase` replaces
    // it. A failure to record the flag is reported, never hidden.
    if had_master && let Err(error) = mark_recovery_stale(ctx) {
        outcome.error = Some(format!(
            "the stale recovery phrase could not be marked as stale: {}",
            error.message
        ));
    }

    let settings = match SettingsHandler::get_settings_file(ctx) {
        Ok(settings) => settings,
        Err(error) => {
            outcome.error = Some(format!(
                "the sync settings could not be read: {}",
                error.message
            ));
            return outcome;
        }
    };
    let server = ServerState::from_settings(&settings);
    let Some(url) = server.url.clone().filter(|_| server.mode != SyncMode::Off) else {
        outcome.error = Some(
            "sync is off, so only the master key rotated: no layers were re-sealed and no server \
             blobs were deleted"
                .to_string(),
        );
        return outcome;
    };

    let endpoint = match configured_endpoint(&server, &url, ctx).await {
        Ok(endpoint) => endpoint,
        Err(error) => {
            outcome.error = Some(format!(
                "the sync endpoint is not usable: {}",
                error.message
            ));
            return outcome;
        }
    };
    let bundle = match build_bundle(ctx, true, true).await {
        Ok(bundle) => bundle,
        Err(error) => {
            outcome.error = Some(format!("the layers could not be read: {}", error.message));
            return outcome;
        }
    };
    // A rotated key re-seals the message mirror exactly like every configuration layer. The old
    // epoch's message blobs are in the index and are deleted by the sweep below.
    let mirror = if server.mode == SyncMode::Full {
        match build_message_mirror(ctx).await {
            Ok(mirror) => Some(mirror),
            Err(error) => {
                outcome.error = Some(format!(
                    "the message mirror could not be read: {}",
                    error.message
                ));
                return outcome;
            }
        }
    } else {
        None
    };
    let mirror_bytes = match mirror
        .as_ref()
        .map(serde_json::to_vec)
        .transpose()
        .map_err(|error| {
            EngineError::internal_error(format!("cannot encode the message mirror: {error}"))
        }) {
        Ok(bytes) => bytes,
        Err(error) => {
            outcome.error = Some(format!(
                "the message mirror could not be encoded: {}",
                error.message
            ));
            return outcome;
        }
    };
    let local_root = match content_root(&bundle, mirror_bytes.as_deref()) {
        Ok(root) => root,
        Err(error) => {
            outcome.error = Some(format!("the layers could not be sealed: {}", error.message));
            return outcome;
        }
    };
    let signing = match device_key(ctx).await {
        Ok(key) => key,
        Err(error) => {
            outcome.error = Some(format!("the device key is not usable: {}", error.message));
            return outcome;
        }
    };
    let device_id = hex(signing.verifying_key().as_bytes());
    let mut store = match FileHeadStore::open(ctx.data_dir.join(HEAD_STORE_FILE)) {
        Ok(store) => store,
        Err(error) => {
            outcome.error = Some(format!("the head store is not usable: {error}"));
            return outcome;
        }
    };
    let mut index = match BlobIndex::open(blob_index_path(ctx)) {
        Ok(index) => index,
        Err(error) => {
            outcome.error = Some(format!(
                "the published blob index is not usable, so nothing was deleted: {}",
                error.message
            ));
            return outcome;
        }
    };

    // Distribute the rotated key before the head that demands it. Each surviving device gets its
    // own sealed box, named under the *previous* epoch's address key: a survivor still holding
    // that master can find its copy, while the revoked device has no copy addressed to it and
    // cannot derive the new epoch's names without the new master. The old master is dropped after
    // this point.
    if let Some(old_master) = old_master {
        match key_wrap_targets(ctx).await {
            Ok(targets) => {
                for (target_id, target_pub) in targets {
                    let short = target_id.get(..8).unwrap_or(&target_id).to_string();
                    if target_pub.is_empty() {
                        outcome.error = Some(format!(
                            "device {short} published no device key at enrolment, so it cannot \
                             adopt the rotated key without enrolling again"
                        ));
                        continue;
                    }
                    let target_key = match decode_device_key(&target_pub) {
                        Ok(key) => key,
                        Err(error) => {
                            outcome.error = Some(format!(
                                "the roster key for device {short} is unusable: {}",
                                error.message
                            ));
                            continue;
                        }
                    };
                    let wrap = match wrap_master(
                        &master,
                        &target_id,
                        &target_key,
                        epoch,
                        &device_id,
                        &signing,
                    ) {
                        Ok(wrap) => wrap,
                        Err(error) => {
                            outcome.error = Some(format!(
                                "the rotated key could not be wrapped for device {short}: {error}"
                            ));
                            continue;
                        }
                    };
                    let id = wrap_blob_id(&old_master, epoch.saturating_sub(1), &target_id, epoch);
                    if let Err(error) = endpoint.put_blob(&id, wrap).await {
                        outcome.error = Some(format!(
                            "the rotated key could not be published for device {short}: {}",
                            error.message
                        ));
                        continue;
                    }
                    // Booked under the *new* epoch so this rotation's sweep leaves it alone; the
                    // next rotation deletes it, because the copy is stale by then.
                    if let Err(error) = index.record(epoch, WRAP_LAYER, u64::from(epoch), &id) {
                        outcome.error = Some(error.message);
                    }
                }
            }
            Err(error) => {
                outcome.error = Some(format!(
                    "the roster could not be read, so no device received the rotated key: {}",
                    error.message
                ));
            }
        }
    }

    // Re-publish first. Every enabled layer goes out under the rotated key; only a head that
    // names our new epoch proves the server holds what the revoked device cannot open.
    if let Err(error) = publish(
        &endpoint,
        &mut store,
        &mut index,
        &master,
        &signing,
        &device_id,
        epoch,
        &bundle,
        mirror_bytes.as_deref(),
        local_root,
    )
    .await
    {
        outcome.error = Some(format!(
            "the layers were not re-published, so no old blobs were deleted: {}",
            error.message
        ));
        return outcome;
    }
    outcome.re_sealed = true;
    if let Some(mirror) = &mirror
        && let Err(error) = record_mirror_window(ctx, mirror)
    {
        outcome.error = Some(format!(
            "the revocation stands, but the message window could not be recorded: {}",
            error.message
        ));
    }

    // The new epoch is live; now the old ciphertext leaves. Leftovers are retried by the next
    // revocation, because the index keeps every id the server did not confirm gone.
    let mut first_error: Option<String> = None;
    for old in index.older_epochs(epoch) {
        for blob in index.blobs(old) {
            match endpoint.delete_blob(&blob.id).await {
                Ok(_deleted) => {
                    outcome.blobs_deleted = outcome.blobs_deleted.saturating_add(1);
                    index.forget(old, &blob.id);
                }
                Err(error) => {
                    outcome.blobs_remaining = outcome.blobs_remaining.saturating_add(1);
                    first_error.get_or_insert(error.message);
                }
            }
        }
    }
    if let Err(error) = index.persist() {
        first_error.get_or_insert(error.message);
    }
    if let Some(message) = first_error {
        outcome.error = Some(format!(
            "the revocation stands, but {} previous-epoch blob(s) were left on the server: {message}",
            outcome.blobs_remaining
        ));
    }
    outcome
}

/// Records what the last successful `full` publish put in the message layer, so `SyncStatus` can
/// say a window was published without re-reading the whole local store.
fn record_mirror_window(ctx: &EngineContext, mirror: &MessageMirror) -> Result<(), EngineError> {
    update_server(ctx, |server| {
        server["message_window"] = json!({
            "windowed": mirror.truncated,
            "updated_at_ms": mirror.newest_at_ms(),
            "chats": mirror.chats.len(),
            "messages": mirror.messages.len(),
        });
    })
}

/// Builds the endpoint exactly as a sync run would, without deciding whether sync is on.
async fn configured_endpoint(
    server: &ServerState,
    url: &str,
    ctx: &EngineContext,
) -> Result<Endpoint, EngineError> {
    let target = parse_target(url)?;
    if target.secure && server.pin.is_none() {
        return Err(EngineError::unavailable(
            "the sync endpoint has no stored pin; configure it again",
        ));
    }
    let token = read_token(ctx)
        .await?
        .ok_or_else(|| EngineError::unavailable("no sync token is stored for this endpoint"))?;
    Endpoint::new(&target, token, server.pin.filter(|_| target.secure))
}

/// One pass: verify and pull the server head, then publish our layers on top of it.
async fn run_once(ctx: &EngineContext) -> Result<(u64, u64, String, u32), EngineError> {
    let settings = SettingsHandler::get_settings_file(ctx)?;
    let server = ServerState::from_settings(&settings);
    if server.mode == SyncMode::Off {
        return Err(EngineError::unavailable(
            "sync is off; configure a server and mode 'prefs' or 'full' first",
        ));
    }
    let url = server
        .url
        .clone()
        .ok_or_else(|| EngineError::unavailable("no sync endpoint is configured"))?;
    let endpoint = configured_endpoint(&server, &url, ctx).await?;

    let settings = SettingsHandler::get_settings_file(ctx)?;
    let sync = sync_section(&settings);
    let mut epoch = sync_epoch(&sync);
    let mut master = master_key(ctx).await?;
    let signing = device_key(ctx).await?;
    let device_id = hex(signing.verifying_key().as_bytes());

    let store_path = ctx.data_dir.join(HEAD_STORE_FILE);
    let mut store = FileHeadStore::open(&store_path)
        .map_err(|error| EngineError::sync_blocked(reason_for_store(&error)))?;

    let mut uploaded = 0u64;
    let mut downloaded = 0u64;
    let mut remote_root: Option<[u8; 32]> = None;

    match endpoint.get_head().await? {
        None => {
            if store.current().is_some() {
                // The server lost a head this device already verified: a rollback, not a fresh
                // start. Keep every local byte and stop.
                return Err(EngineError::sync_blocked("branch"));
            }
        }
        Some((bytes, _etag)) => {
            downloaded += bytes.len() as u64;
            let before = store.current().map(|record| record.head);
            let (_, batch) = observe_head(&mut store, &bytes)?;
            // A rotation is visible only in the signed batch. Without adopting it, a surviving
            // device would keep sealing under the old epoch after it got the new key. The wrap
            // carries the new root; when there is none addressed to this device (it was revoked,
            // or it missed two rotations), the pull below blocks on content instead.
            if batch.header.epoch > epoch {
                if let Some(rotated) = adopt_rotation(ctx, &endpoint, &master, &batch).await? {
                    store_master_key(ctx, &rotated).await?;
                    master = rotated;
                }
                adopt_epoch(ctx, batch.header.epoch)?;
                epoch = batch.header.epoch;
            }
            remote_root = Some(batch.header.root_hash);
            let changed = store.current().map(|record| record.head) != before;
            if changed {
                downloaded += pull_layers(
                    ctx,
                    &endpoint,
                    &master,
                    &batch,
                    server.mode == SyncMode::Full,
                )
                .await?;
            }
        }
    }

    // Build what we have *after* the pull: importing the remote layers is what makes the two
    // sides converge, and publishing a pre-import bundle would overwrite the winner's content.
    let bundle = build_bundle(ctx, true, true).await?;
    // The message mirror is opt-in per account. `prefs` never reads the local message store
    // at all, so no message data can leak through this path by accident.
    let mirror = if server.mode == SyncMode::Full {
        Some(build_message_mirror(ctx).await?)
    } else {
        None
    };
    let mirror_bytes = mirror
        .as_ref()
        .map(serde_json::to_vec)
        .transpose()
        .map_err(|error| {
            EngineError::internal_error(format!("cannot encode the message mirror: {error}"))
        })?;
    let local_root = content_root(&bundle, mirror_bytes.as_deref())?;
    let mut index = BlobIndex::open(blob_index_path(ctx))?;
    if remote_root != Some(local_root) {
        let (sent, _) = publish(
            &endpoint,
            &mut store,
            &mut index,
            &master,
            &signing,
            &device_id,
            epoch,
            &bundle,
            mirror_bytes.as_deref(),
            local_root,
        )
        .await?;
        uploaded += sent;
    }
    if let Some(mirror) = &mirror {
        record_mirror_window(ctx, mirror)?;
    }

    let head_hex = store
        .current()
        .map(|record| record.head.to_hex())
        .unwrap_or_default();
    Ok((uploaded, downloaded, head_hex, epoch))
}

/// Verifies one head against the persisted chain and appends it only on success.
///
/// A head the store already recorded but does not currently trust is a replay or a rollback, and
/// is refused as `branch`; a parent it never saw is `fork`; a bad signature names itself.
fn observe_head(
    store: &mut FileHeadStore,
    bytes: &[u8],
) -> Result<(Head, SignedBatch), EngineError> {
    let batch = SignedBatch::from_bytes(bytes).map_err(|_| EngineError::sync_blocked("chain"))?;
    let verifying = verifying_key(&batch.header.device_id)?;
    let head = batch.head().map_err(blocked_chain)?;
    if store.current().map(|record| record.head) == Some(head) {
        return Ok((head, batch));
    }
    if store.get(&head).is_some() {
        return Err(EngineError::sync_blocked("branch"));
    }
    if store.current().is_none() {
        // A fresh device (the master key and endpoint pin came from enrolment) has no history to
        // check the parent against. Trust this one signed head as the starting cursor; every
        // later read is verified against it, so a rollback after this point is still detected.
        batch.verify_signature(&verifying).map_err(blocked_chain)?;
        store
            .append(alloy_sync::chain::HeadRecord {
                head,
                device_id: batch.header.device_id.clone(),
                seq: batch.header.seq,
                epoch: batch.header.epoch,
            })
            .map_err(blocked_chain)?;
        return Ok((head, batch));
    }
    verify_batch(&batch, &verifying, store).map_err(blocked_chain)?;
    Ok((head, batch))
}

/// Publishes the local layers, re-reading and rebasing on every competing head.
#[allow(clippy::too_many_arguments)]
async fn publish(
    endpoint: &Endpoint,
    store: &mut FileHeadStore,
    index: &mut BlobIndex,
    master: &MasterKey,
    signing: &SigningKey,
    device_id: &str,
    epoch: u32,
    bundle: &ConfigBundle,
    mirror: Option<&[u8]>,
    local_root: [u8; 32],
) -> Result<(u64, Head), EngineError> {
    let mut uploaded = 0u64;
    for _ in 0..MAX_REBASE_ATTEMPTS {
        let (prev, seq, precondition) = match endpoint.get_head().await? {
            None => {
                if store.current().is_some() {
                    // The server lost a head this device already verified.
                    return Err(EngineError::sync_blocked("branch"));
                }
                (Head::GENESIS, 1u64, Precondition::CreateOnly)
            }
            Some((bytes, etag)) => {
                let (head, batch) = observe_head(store, &bytes)?;
                // Identical content at the same epoch is already published. The epoch matters:
                // after a rotation the same layers must go out again under the new key even
                // though their content root did not change.
                if batch.header.root_hash == local_root && batch.header.epoch == epoch {
                    return Ok((uploaded, head));
                }
                (
                    head,
                    batch.header.seq.saturating_add(1),
                    Precondition::Match(etag),
                )
            }
        };

        let (blobs, root) = prepare_layers(master, epoch, seq, bundle, mirror)?;
        for blob in blobs {
            uploaded += blob.bytes.len() as u64;
            endpoint.put_blob(&blob.id, blob.bytes).await?;
            // Persist after the upload, not before: an id the server never accepted must not be
            // in the list a later revocation tries to delete.
            index.record(epoch, blob.layer, seq, &blob.id)?;
        }

        let header = BatchHeader {
            device_id: device_id.to_string(),
            seq,
            prev_head: prev,
            root_hash: root,
            epoch,
            mirror: mirror.is_some(),
            created_ms: now_ms() as u64,
        };
        let batch = SignedBatch::sign(header, signing).map_err(blocked_chain)?;
        let bytes = batch.to_bytes().map_err(blocked_chain)?;
        uploaded += bytes.len() as u64;
        match endpoint.put_head(&bytes, precondition).await? {
            WriteOutcome::Written => {
                // Our own batch is the new head; record it in the same chain the pulls use.
                let verifying = signing.verifying_key();
                let head = verify_batch(&batch, &verifying, store).map_err(blocked_chain)?;
                return Ok((uploaded, head));
            }
            WriteOutcome::Conflict { bytes, etag } => {
                // Another writer won between our read and our write: verify the head it returned
                // (the next loop re-reads it anyway) and publish on top instead of overwriting.
                if let (Some(bytes), Some(_etag)) = (bytes.as_deref(), etag.as_deref()) {
                    let _ = observe_head(store, bytes)?;
                }
                continue;
            }
        }
    }
    Err(EngineError::network_error(
        "another device kept writing the sync head; try again",
    ))
}

/// One sealed layer ready to upload, with the layer name the index records beside its id.
struct PreparedBlob {
    id: String,
    layer: &'static str,
    bytes: Vec<u8>,
}

/// Seals every layer for slot `(seq, epoch)`, returning them and the content root.
type PreparedLayers = (Vec<PreparedBlob>, [u8; 32]);

fn prepare_layers(
    master: &MasterKey,
    epoch: u32,
    seq: u64,
    bundle: &ConfigBundle,
    mirror: Option<&[u8]>,
) -> Result<PreparedLayers, EngineError> {
    let key = master.blob_key(epoch);
    let mut blobs = Vec::with_capacity(LayerKey::ALL.len() + usize::from(mirror.is_some()));
    for layer in LayerKey::ALL {
        let plaintext = layer_plaintext(bundle, layer)?;
        let addr = BlobAddr::new(layer.layer(), layer.id(), seq, epoch);
        blobs.push(sealed_blob(master, &key, layer.id(), &addr, &plaintext)?);
    }
    if let Some(mirror) = mirror {
        let addr = BlobAddr::new(Layer::Message, MIRROR_LAYER_ID, seq, epoch);
        blobs.push(sealed_blob(master, &key, MIRROR_LAYER_ID, &addr, mirror)?);
    }
    Ok((blobs, content_root(bundle, mirror)?))
}

/// Seal one layer and name it, mapping a size refusal to the typed payload error.
fn sealed_blob(
    master: &MasterKey,
    key: &alloy_sync::BlobKey,
    layer: &'static str,
    addr: &BlobAddr,
    plaintext: &[u8],
) -> Result<PreparedBlob, EngineError> {
    let bytes = seal(key, addr, plaintext).map_err(|error| match error {
        alloy_sync::BlobError::PlaintextTooLarge { .. }
        | alloy_sync::BlobError::OversizedDeclaration { .. } => EngineError::payload_too_large(),
        other => EngineError::internal_error(format!("cannot seal the {layer} layer: {other}")),
    })?;
    Ok(PreparedBlob {
        id: blob_id(master, addr),
        layer,
        bytes,
    })
}

/// The stable content root of the published layers, independent of seq and epoch.
///
/// The message mirror is included only when one exists: a `prefs` root keeps its old value, and
/// a `full` root covers the message window so a changed window forces a new batch.
fn content_root(bundle: &ConfigBundle, mirror: Option<&[u8]>) -> Result<[u8; 32], EngineError> {
    let mut root = Sha256::new();
    root.update(ROOT_DOMAIN);
    for layer in LayerKey::ALL {
        let plaintext = layer_plaintext(bundle, layer)?;
        root.update([1u8]);
        root.update((layer.id().len() as u16).to_be_bytes());
        root.update(layer.id().as_bytes());
        root.update(Sha256::digest(&plaintext));
    }
    if let Some(mirror) = mirror {
        root.update([1u8]);
        root.update((MIRROR_LAYER_ID.len() as u16).to_be_bytes());
        root.update(MIRROR_LAYER_ID.as_bytes());
        root.update(Sha256::digest(mirror));
    }
    Ok(root.finalize().into())
}

/// One layer as it travels in a blob. Every layer is JSON; tags travel as the bundle's
/// array-of-pairs form because JSON object keys are strings only.
fn layer_plaintext(bundle: &ConfigBundle, layer: LayerKey) -> Result<Vec<u8>, EngineError> {
    let value = match layer {
        LayerKey::Settings => Ok(bundle.settings.clone()),
        LayerKey::Tags => serde_json::to_value(bundle.tags.iter().collect::<Vec<_>>()),
        LayerKey::Catalogue => Ok(bundle.catalogue.clone()),
        LayerKey::Backgrounds => serde_json::to_value(&bundle.backgrounds),
        LayerKey::Theme => serde_json::to_value(&bundle.theme),
    }
    .map_err(|error| {
        EngineError::internal_error(format!("cannot encode {}: {error}", layer.id()))
    })?;
    serde_json::to_vec(&value).map_err(|error| {
        EngineError::internal_error(format!("cannot encode {}: {error}", layer.id()))
    })
}

/// `HMAC-SHA256(addr_key(epoch), domain || slot)`, base64url without padding: 43 characters, the
/// only shape the server accepts and the only name the blob has on the wire.
fn blob_id(master: &MasterKey, addr: &BlobAddr) -> String {
    let addr_key = master.addr_key(addr.epoch);
    let mut mac =
        Hmac::<Sha256>::new_from_slice(addr_key.as_bytes()).expect("HMAC accepts a 32-byte key");
    mac.update(ADDR_DOMAIN);
    mac.update(&[addr.layer.as_u8()]);
    mac.update(&(addr.logical_id.len() as u16).to_be_bytes());
    mac.update(addr.logical_id.as_bytes());
    mac.update(&addr.seq.to_be_bytes());
    mac.update(&addr.epoch.to_be_bytes());
    URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
}

/// `HMAC-SHA256(addr_key(previous_epoch), domain || recipient || new_epoch)`: the name a survivor
/// can derive with the master it still holds before the rotation reaches it.
fn wrap_blob_id(
    master: &MasterKey,
    previous_epoch: u32,
    recipient_id: &str,
    wrap_epoch: u32,
) -> String {
    let addr_key = master.addr_key(previous_epoch);
    let mut mac =
        Hmac::<Sha256>::new_from_slice(addr_key.as_bytes()).expect("HMAC accepts a 32-byte key");
    mac.update(WRAP_ID_DOMAIN);
    mac.update(&(recipient_id.len() as u16).to_be_bytes());
    mac.update(recipient_id.as_bytes());
    mac.update(&wrap_epoch.to_be_bytes());
    URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
}

/// Tries to adopt the master a rotation installed, using the wrap the revoker sealed to this
/// device. Returns `Ok(None)` when the server holds no copy addressed here: the caller then pulls
/// with the key it has and reports `sync_blocked/content`, which is the honest outcome for a
/// revoked device.
///
/// The wrap must name the exact epoch the verified head demands and must be signed by the device
/// that signed that head, so a replayed copy from an earlier rotation, or a copy addressed to a
/// device that is not on this roster, is never installed.
async fn adopt_rotation(
    ctx: &EngineContext,
    endpoint: &Endpoint,
    master: &MasterKey,
    batch: &SignedBatch,
) -> Result<Option<MasterKey>, EngineError> {
    if batch.header.epoch == 0 {
        return Ok(None);
    }
    let signing = device_key(ctx).await?;
    let device_id = hex(signing.verifying_key().as_bytes());
    let secret = device_exchange_key(ctx).await?;
    let id = wrap_blob_id(
        master,
        batch.header.epoch.saturating_sub(1),
        &device_id,
        batch.header.epoch,
    );
    let Some(bytes) = endpoint.get_blob(&id).await? else {
        return Ok(None);
    };
    let sender = verifying_key(&batch.header.device_id)?;
    let (rotated, sender_id) =
        open_master(&secret, &device_id, batch.header.epoch, &sender, &bytes)
            .map_err(wrap_blocked)?;
    if sender_id != batch.header.device_id {
        return Err(EngineError::sync_blocked("content"));
    }
    Ok(Some(rotated))
}

fn wrap_blocked(error: DeviceWrapError) -> EngineError {
    tracing::debug!(%error, "a rotation key wrap was refused");
    EngineError::sync_blocked("content")
}

/// Fetches and imports the layers a verified batch names.
///
/// Every enabled layer must be present and open: a batch is published only after all five blobs
/// are uploaded, so a missing one means the server lost data or the head names an epoch this
/// device cannot derive ids for. Importing the remainder would look like a converged sync while
/// silently dropping configuration, so it blocks instead.
async fn pull_layers(
    ctx: &EngineContext,
    endpoint: &Endpoint,
    master: &MasterKey,
    batch: &SignedBatch,
    want_mirror: bool,
) -> Result<u64, EngineError> {
    let ring = BlobKeyRing::new(master, [batch.header.epoch]);
    let mut downloaded = 0u64;
    let mut values: BTreeMap<LayerKey, Value> = BTreeMap::new();
    for layer in LayerKey::ALL {
        let addr = BlobAddr::new(
            layer.layer(),
            layer.id(),
            batch.header.seq,
            batch.header.epoch,
        );
        let id = blob_id(master, &addr);
        let bytes = endpoint
            .get_blob(&id)
            .await?
            .ok_or_else(|| EngineError::sync_blocked("content"))?;
        downloaded += bytes.len() as u64;
        let plaintext =
            open(&ring, &addr, &bytes).map_err(|_| EngineError::sync_blocked("content"))?;
        let value: Value =
            serde_json::from_slice(&plaintext).map_err(|_| EngineError::sync_blocked("content"))?;
        values.insert(layer, value);
    }

    let mut bundle = ConfigBundle {
        schema: BUNDLE_SCHEMA,
        exported_at_ms: i64::try_from(batch.header.created_ms).unwrap_or(i64::MAX),
        device_label: "paired device".to_string(),
        settings: Value::Null,
        tags: BTreeMap::new(),
        catalogue: Value::Null,
        backgrounds: Vec::new(),
        theme: None,
    };
    for (layer, value) in values {
        match layer {
            LayerKey::Settings => bundle.settings = value,
            LayerKey::Tags => {
                let pairs: Vec<(ChatKey, TagSet)> = serde_json::from_value(value)
                    .map_err(|_| EngineError::sync_blocked("content"))?;
                bundle.tags = pairs.into_iter().collect();
            }
            LayerKey::Catalogue => bundle.catalogue = value,
            LayerKey::Backgrounds => {
                bundle.backgrounds = serde_json::from_value(value)
                    .map_err(|_| EngineError::sync_blocked("content"))?;
            }
            LayerKey::Theme => {
                bundle.theme = serde_json::from_value(value)
                    .map_err(|_| EngineError::sync_blocked("content"))?;
            }
        }
    }
    bundle
        .validate()
        .map_err(|_| EngineError::sync_blocked("content"))?;

    // Read and authenticate the message layer before the first store write: a swapped blob, a
    // blob from another epoch or a truncated layer then blocks with the previous state intact.
    let mirror = if want_mirror && batch.header.mirror {
        let addr = BlobAddr::new(
            Layer::Message,
            MIRROR_LAYER_ID,
            batch.header.seq,
            batch.header.epoch,
        );
        let id = blob_id(master, &addr);
        let bytes = endpoint
            .get_blob(&id)
            .await?
            .ok_or_else(|| EngineError::sync_blocked("content"))?;
        downloaded += bytes.len() as u64;
        let plaintext =
            open(&ring, &addr, &bytes).map_err(|_| EngineError::sync_blocked("content"))?;
        let mirror: MessageMirror =
            serde_json::from_slice(&plaintext).map_err(|_| EngineError::sync_blocked("content"))?;
        mirror.validate()?;
        Some(mirror)
    } else {
        None
    };

    import_bundle_value(ctx, &bundle, true).await?;
    if let Some(mirror) = &mirror {
        import_message_mirror(ctx, mirror).await?;
        record_mirror_window(ctx, mirror)?;
    }
    Ok(downloaded)
}

/// The key that signed a batch, named by the batch itself. The chain, not the server, proves it.
fn verifying_key(device_id: &str) -> Result<VerifyingKey, EngineError> {
    if device_id.len() != 64 {
        return Err(EngineError::sync_blocked("chain"));
    }
    let mut bytes = [0u8; 32];
    for (index, pair) in device_id.as_bytes().as_chunks::<2>().0.iter().enumerate() {
        let pair = std::str::from_utf8(pair).map_err(|_| EngineError::sync_blocked("chain"))?;
        bytes[index] =
            u8::from_str_radix(pair, 16).map_err(|_| EngineError::sync_blocked("chain"))?;
    }
    VerifyingKey::from_bytes(&bytes).map_err(|_| EngineError::sync_blocked("chain"))
}

fn blocked_chain(error: ChainError) -> EngineError {
    let reason = match &error {
        ChainError::Branch { .. } => "branch",
        ChainError::Fork { .. } => "fork",
        ChainError::Signature => "signature",
        _ => "chain",
    };
    EngineError::sync_blocked(reason)
}

fn reason_for_store(error: &ChainError) -> &'static str {
    match error {
        ChainError::Branch { .. } => "branch",
        ChainError::Fork { .. } => "fork",
        ChainError::Signature => "signature",
        _ => "chain",
    }
}

async fn read_token(ctx: &EngineContext) -> Result<Option<String>, EngineError> {
    match ctx.secrets.get_secret(TOKEN_SECRET).await {
        Ok(token) => Ok(Some(token)),
        Err(StorageError::SecretNotFound(_)) => Ok(None),
        Err(error) => Err(EngineError::internal_error(format!(
            "cannot read the sync token from the secret store: {error}"
        ))),
    }
}

async fn store_token(ctx: &EngineContext, token: &str) -> Result<(), EngineError> {
    ctx.secrets
        .set_secret(TOKEN_SECRET, token)
        .await
        .map_err(|error| {
            EngineError::internal_error(format!(
                "cannot store the sync token in the secret store: {error}"
            ))
        })
}

/// Read-modify-write of `sync.server`, under the settings lock the rest of the engine uses.
fn update_server(ctx: &EngineContext, update: impl FnOnce(&mut Value)) -> Result<(), EngineError> {
    SettingsHandler::with_settings_lock(ctx, || {
        let mut settings = settings_object(ctx)?;
        let mut sync = sync_section(&settings);
        let mut server = sync.get("server").cloned().unwrap_or_else(|| json!({}));
        if !server.is_object() {
            server = json!({});
        }
        update(&mut server);
        sync["server"] = server;
        settings["sync"] = sync;
        SettingsHandler::write_settings_file(ctx, settings)
    })
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blob_ids_are_the_servers_shape_and_slot_bound() {
        let master = MasterKey::generate();
        let addr = BlobAddr::new(Layer::Config, "settings", 7, 2);
        let id = blob_id(&master, &addr);
        assert_eq!(id.len(), 43);
        assert!(
            id.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        );
        let mut other = addr.clone();
        other.seq = 8;
        assert_ne!(id, blob_id(&master, &other));
    }

    #[test]
    fn the_content_root_ignores_seq_and_epoch() {
        let bundle = ConfigBundle {
            schema: BUNDLE_SCHEMA,
            exported_at_ms: 1,
            device_label: "device".to_string(),
            settings: json!({"density": "compact"}),
            tags: BTreeMap::new(),
            catalogue: Value::Null,
            backgrounds: Vec::new(),
            theme: None,
        };
        let root = content_root(&bundle, None).expect("root");
        let (_, sealed_root) =
            prepare_layers(&MasterKey::generate(), 0, 1, &bundle, None).expect("seal");
        assert_eq!(root, sealed_root);
    }
}
