//! Devices and sync (ADR 0020, engine slice): bundle export/import, LAN pairing, recovery phrase
//! and the local device roster.
//!
//! Nothing here touches the network except the pairing test, which moves a real bundle between
//! two engines over loopback: the point of that test is the transfer, so it runs the real
//! handshake against two real data directories rather than a fake.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use alloy_engine::factory::{ConnectorFactory, DefaultConnectorFactory};
use alloy_engine::{AppEngine, Engine, EngineContext};
use alloy_protocol::{
    ConfigBundleSummary, ConfigImportReport, DeviceDto, EngineCommand, EngineEvent, EngineResponse,
    PairingPhase, PairingRole,
};
use alloy_storage::config_bundle::ConfigState;
use alloy_storage::pairing::{self, DeviceProver, PairingError, PairingOffer, PairingTicket};
use alloy_storage::{InMemorySecretStore, SecretStore};
use alloy_store::{AccountState, ChatFlagsUpdate, ChatKind, ChatUpsert, Network, NewAccount};
use ed25519_dalek::SigningKey;
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::sync::broadcast;
use tokio::time::timeout;

const WAIT: Duration = Duration::from_secs(10);

/// Opens the engine the way the app does: a real data directory, a store key in the secret store
/// and no sidecar (nothing in this slice spawns one).
async fn open_engine(data_dir: &Path) -> AppEngine {
    let secrets: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::new());
    let factory: Arc<dyn ConnectorFactory> = Arc::new(DefaultConnectorFactory::new(
        secrets.clone(),
        None,
        data_dir.to_path_buf(),
    ));
    let ctx = EngineContext::open(data_dir.to_path_buf(), secrets, factory)
        .await
        .expect("open the engine");
    AppEngine::new(ctx)
}

/// One Telegram account with the network's own user id, one chat, and the chat tagged.
async fn seed_tagged_chat(engine: &AppEngine, tag_id: &str) -> String {
    let store = &engine.context().store;
    let account = store
        .insert_account(NewAccount {
            account_id: None,
            network: Network::Telegram,
            remote_user_id: Some("1001".into()),
            display_name: Some("Ada".into()),
            state: AccountState::Active,
        })
        .await
        .expect("insert account");
    let chat_id = store
        .upsert_chat(ChatUpsert {
            account_id: account.account_id.clone(),
            remote_chat_id: "42".into(),
            kind: ChatKind::Direct,
            title: Some("Ada".into()),
            avatar_media_id: None,
            last_message_id: None,
            last_activity_at: None,
            parent_remote_chat_id: None,
            is_forum: false,
        })
        .await
        .expect("insert chat");
    store
        .update_chat_flags(
            &chat_id,
            ChatFlagsUpdate {
                tags: Some(vec![tag_id.to_string()]),
                tags_updated_at: Some(1_700_000_000_000),
                ..ChatFlagsUpdate::default()
            },
        )
        .await
        .expect("tag the chat");
    chat_id
}

/// The account and chat a receiving device needs for the same chat to resolve.
async fn seed_untagged_chat(engine: &AppEngine) -> String {
    let store = &engine.context().store;
    let account = store
        .insert_account(NewAccount {
            account_id: None,
            network: Network::Telegram,
            remote_user_id: Some("1001".into()),
            display_name: Some("Ada".into()),
            state: AccountState::Active,
        })
        .await
        .expect("insert account");
    store
        .upsert_chat(ChatUpsert {
            account_id: account.account_id.clone(),
            remote_chat_id: "42".into(),
            kind: ChatKind::Direct,
            title: Some("Ada".into()),
            avatar_media_id: None,
            last_message_id: None,
            last_activity_at: None,
            parent_remote_chat_id: None,
            is_forum: false,
        })
        .await
        .expect("insert chat")
}

fn write_json(path: &Path, value: &Value) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create dir");
    }
    std::fs::write(path, serde_json::to_vec_pretty(value).expect("encode")).expect("write json");
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).expect("read json")).expect("parse json")
}

async fn export(
    engine: &AppEngine,
    include_backgrounds: bool,
    include_themes: bool,
) -> (String, u64, ConfigBundleSummary) {
    match engine
        .handle(EngineCommand::ExportConfigBundle {
            include_backgrounds,
            include_themes,
        })
        .await
        .expect("export")
    {
        EngineResponse::ConfigBundleExported {
            bundle_json,
            bytes,
            summary,
        } => (bundle_json, bytes, summary),
        other => panic!("unexpected response: {other:?}"),
    }
}

async fn import(
    engine: &AppEngine,
    bundle_json: &str,
    apply: bool,
) -> Result<(ConfigImportReport, bool), alloy_engine::EngineError> {
    match engine
        .handle(EngineCommand::ImportConfigBundle {
            bundle_json: bundle_json.to_string(),
            apply,
        })
        .await?
    {
        EngineResponse::ConfigBundleImported { report, applied } => Ok((report, applied)),
        other => panic!("unexpected response: {other:?}"),
    }
}

/// A copy of a packaged theme under a fresh id, so the theme layer is a user theme with a file.
fn user_theme(dir: &Path) -> String {
    let source = PathBuf::from("../../packages/themes/midnight-ocean.json");
    let mut theme = read_json(&source);
    theme["id"] = json!("my-midnight");
    theme["name"] = json!("My Midnight");
    write_json(&dir.join("themes/my-midnight.json"), &theme);
    "my-midnight".to_string()
}

#[tokio::test]
async fn export_import_roundtrip_carries_every_layer() {
    let sender_dir = TempDir::new().expect("sender dir");
    let receiver_dir = TempDir::new().expect("receiver dir");
    let sender = open_engine(sender_dir.path()).await;
    seed_tagged_chat(&sender, "work").await;
    let theme_id = user_theme(sender_dir.path());
    write_json(
        &sender_dir.path().join("tags.json"),
        &json!([{"id": "work", "name": "Work", "colour": "blue"}]),
    );
    write_json(
        &sender_dir.path().join("settings.json"),
        &json!({
            "density": "compact",
            "window": {"width": 1200},
            "updater": {"channel": "beta"},
            "active_theme": theme_id,
        }),
    );

    let receiver = open_engine(receiver_dir.path()).await;
    let receiver_chat = seed_untagged_chat(&receiver).await;
    // The receiver's own window size and updater channel are device-local: an import keeps them.
    write_json(
        &receiver_dir.path().join("settings.json"),
        &json!({
            "density": "spacious",
            "window": {"width": 800},
            "updater": {"channel": "stable"},
        }),
    );
    // The export stamp is `now_ms`; the merge is strictly newer, so the receiver's files must be
    // stamped before it.
    std::thread::sleep(Duration::from_millis(5));

    let (bundle_json, bytes, summary) = export(&sender, false, true).await;
    assert!(summary.settings);
    assert!(summary.catalogue);
    assert_eq!(summary.tags, 1);
    assert_eq!(summary.theme.as_deref(), Some("my-midnight"));
    assert_eq!(bytes, bundle_json.len() as u64);

    let (report, applied) = import(&receiver, &bundle_json, true).await.expect("import");
    assert!(applied, "the first import changed something");
    assert!(report.settings);
    assert!(report.catalogue);
    assert_eq!(report.tags, 1);
    assert!(report.theme);

    let settings = read_json(&receiver_dir.path().join("settings.json"));
    assert_eq!(settings["density"], json!("compact"));
    assert_eq!(settings["active_theme"], json!("my-midnight"));
    assert_eq!(
        settings["window"],
        json!({"width": 800}),
        "the receiver's window stays its own"
    );
    assert_eq!(
        settings["updater"],
        json!({"channel": "stable"}),
        "the receiver's updater channel stays its own"
    );
    assert_eq!(
        read_json(&receiver_dir.path().join("tags.json")),
        json!([{"id": "work", "name": "Work", "colour": "blue"}])
    );
    let chat = receiver
        .context()
        .store
        .get_chat(&receiver_chat)
        .await
        .expect("read chat")
        .expect("chat exists");
    assert_eq!(chat.local_tags, vec!["work".to_string()]);
    assert_eq!(chat.tags_updated_at, 1_700_000_000_000);
    assert!(
        receiver_dir.path().join("themes/my-midnight.json").exists(),
        "the user theme file crossed with the selection"
    );

    // The same bundle twice is a no-op: the second import applies nothing.
    let (report, applied) = import(&receiver, &bundle_json, true).await.expect("import");
    assert!(!applied, "nothing newer was in the second import");
    assert_eq!(report, ConfigImportReport::default());
}

#[tokio::test]
async fn dry_run_reports_without_writing() {
    let sender_dir = TempDir::new().expect("sender dir");
    let receiver_dir = TempDir::new().expect("receiver dir");
    let sender = open_engine(sender_dir.path()).await;
    seed_tagged_chat(&sender, "work").await;
    write_json(
        &sender_dir.path().join("tags.json"),
        &json!([{"id": "work", "name": "Work", "colour": "blue"}]),
    );
    write_json(
        &sender_dir.path().join("settings.json"),
        &json!({"density": "compact"}),
    );
    let receiver = open_engine(receiver_dir.path()).await;
    let receiver_chat = seed_untagged_chat(&receiver).await;
    write_json(
        &receiver_dir.path().join("settings.json"),
        &json!({"density": "spacious"}),
    );
    std::thread::sleep(Duration::from_millis(5));
    let (bundle_json, _, _) = export(&sender, false, false).await;
    let before = receiver_dir.path().join("settings.json");
    let before_bytes = std::fs::read(&before).expect("receiver settings exist");

    let (report, applied) = import(&receiver, &bundle_json, false)
        .await
        .expect("dry run");
    assert!(!applied);
    assert!(report.settings);
    assert!(report.catalogue);
    assert_eq!(report.tags, 1);
    assert_eq!(
        std::fs::read(&before).expect("settings unchanged"),
        before_bytes
    );
    assert!(
        !receiver_dir.path().join("tags.json").exists(),
        "a dry run writes no catalogue"
    );
    let chat = receiver
        .context()
        .store
        .get_chat(&receiver_chat)
        .await
        .expect("read chat")
        .expect("chat exists");
    assert!(chat.local_tags.is_empty(), "a dry run tags nothing");
}

#[tokio::test]
async fn a_bundle_from_another_schema_is_refused_naming_both_versions() {
    let dir = TempDir::new().expect("dir");
    let engine = open_engine(dir.path()).await;
    let (bundle_json, _, _) = export(&engine, false, false).await;
    // What a later build writes: the exact shape, one schema ahead of this one.
    let mut bundle = serde_json::from_str::<Value>(&bundle_json).expect("parse bundle");
    bundle["schema"] = json!(99);

    let err = import(&engine, &bundle.to_string(), true)
        .await
        .expect_err("a schema 99 bundle is refused");
    assert_eq!(err.code, "bundle_rejected");
    assert!(
        err.message.contains("99") && err.message.contains("1"),
        "the message names both versions: {}",
        err.message
    );
    assert_eq!(err.reason.as_deref(), Some("schema:99"));
}

#[tokio::test]
async fn a_wrong_word_fails_without_saying_which() {
    let dir = TempDir::new().expect("dir");
    let engine = open_engine(dir.path()).await;

    let words = match engine
        .handle(EngineCommand::CreateRecoveryPhrase)
        .await
        .expect("create phrase")
    {
        EngineResponse::RecoveryPhraseCreated { words } => words,
        other => panic!("unexpected response: {other:?}"),
    };
    assert_eq!(words.len(), 24);
    engine
        .handle(EngineCommand::VerifyRecoveryPhrase {
            words: words.clone(),
        })
        .await
        .expect("the phrase verifies");

    // A different BIP39 word, so the phrase parses and the failure is the verifier's.
    let mut wrong = words.clone();
    let replacement = if wrong[7] == "abandon" {
        "ability"
    } else {
        "abandon"
    };
    wrong[7] = replacement.to_string();
    let err = engine
        .handle(EngineCommand::VerifyRecoveryPhrase {
            words: wrong.clone(),
        })
        .await
        .expect_err("a wrong word is refused");
    assert_eq!(err.code, "recovery_rejected");
    assert!(
        !err.message.contains(replacement) && !err.message.contains(&words[7]),
        "the message names no word: {}",
        err.message
    );
    assert!(
        !dir.path().join("settings.json").exists()
            || !std::fs::read_to_string(dir.path().join("settings.json"))
                .expect("settings")
                .contains(&words.join(" ")),
        "the phrase is never written to disk"
    );
}

async fn begin_pairing(engine: &AppEngine) -> (String, String, String) {
    match engine
        .handle(EngineCommand::BeginPairing {
            role: PairingRole::Offer,
        })
        .await
        .expect("begin pairing")
    {
        EngineResponse::PairingStarted {
            pairing_id,
            ticket,
            code,
            ..
        } => (pairing_id, ticket, code),
        other => panic!("unexpected response: {other:?}"),
    }
}

async fn accept(engine: &AppEngine, ticket: &str, code: &str) -> String {
    match engine
        .handle(EngineCommand::AcceptPairingTicket {
            ticket: ticket.to_string(),
            code: code.to_string(),
        })
        .await
        .expect("accept ticket")
    {
        EngineResponse::PairingStarted { pairing_id, .. } => pairing_id,
        other => panic!("unexpected response: {other:?}"),
    }
}

async fn pairing_phase(engine: &AppEngine, pairing_id: &str) -> PairingPhase {
    match engine
        .handle(EngineCommand::PairingStatus {
            pairing_id: pairing_id.to_string(),
        })
        .await
        .expect("pairing status")
    {
        EngineResponse::PairingState { state, .. } => state,
        other => panic!("unexpected response: {other:?}"),
    }
}

/// Waits for one `PairingChanged` of `phase` on a task-local receiver.
async fn wait_phase(
    events: &mut broadcast::Receiver<EngineEvent>,
    pairing_id: &str,
    phase: PairingPhase,
) -> String {
    let deadline = tokio::time::Instant::now() + WAIT;
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        let event = timeout(remaining, events.recv())
            .await
            .unwrap_or_else(|_| panic!("no {phase:?} event for pairing {pairing_id}"))
            .expect("event channel open");
        if let EngineEvent::PairingChanged {
            pairing_id: id,
            state,
            sas,
        } = event
            && id == pairing_id
            && state == phase
        {
            return sas.unwrap_or_default();
        }
    }
}

async fn wait_sas(engine: &AppEngine, pairing_id: &str, sas: &str) {
    let deadline = tokio::time::Instant::now() + WAIT;
    loop {
        if pairing_phase(engine, pairing_id).await == PairingPhase::SasReady {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "pairing {pairing_id} never showed the SAS {sas}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn pairing_moves_a_real_bundle_between_two_engines() {
    let offer_dir = TempDir::new().expect("offer dir");
    let accept_dir = TempDir::new().expect("accept dir");
    let offer = open_engine(offer_dir.path()).await;
    seed_tagged_chat(&offer, "work").await;
    write_json(
        &offer_dir.path().join("settings.json"),
        &json!({"density": "compact", "window": {"width": 1200}}),
    );
    let receiver = open_engine(accept_dir.path()).await;
    let receiver_chat = seed_untagged_chat(&receiver).await;

    let mut offer_events = offer.sink().subscribe();
    let mut accept_events = receiver.sink().subscribe();

    let (offer_id, ticket, code) = begin_pairing(&offer).await;
    let accept_id = accept(&receiver, &ticket, &code).await;
    assert_eq!(offer_id, accept_id, "both sides name the same pairing");

    let offer_sas = wait_phase(&mut offer_events, &offer_id, PairingPhase::SasReady).await;
    let accept_sas = wait_phase(&mut accept_events, &accept_id, PairingPhase::SasReady).await;
    assert_eq!(
        offer_sas, accept_sas,
        "both screens show the same six digits"
    );
    assert_eq!(offer_sas.len(), 6);
    wait_sas(&offer, &offer_id, &offer_sas).await;
    wait_sas(&receiver, &accept_id, &accept_sas).await;

    match offer
        .handle(EngineCommand::ConfirmPairing {
            pairing_id: offer_id.clone(),
            accept: true,
        })
        .await
        .expect("offer confirms")
    {
        EngineResponse::PairingState { state, .. } => assert_eq!(state, PairingPhase::SasReady),
        other => panic!("unexpected response: {other:?}"),
    }
    receiver
        .handle(EngineCommand::ConfirmPairing {
            pairing_id: accept_id.clone(),
            accept: true,
        })
        .await
        .expect("receiver confirms");

    wait_phase(&mut offer_events, &offer_id, PairingPhase::Done).await;
    wait_phase(&mut accept_events, &accept_id, PairingPhase::Done).await;

    let settings = read_json(&accept_dir.path().join("settings.json"));
    assert_eq!(settings["density"], json!("compact"));
    assert_eq!(
        settings["window"],
        Value::Null,
        "the offering device's window is device-local and never crosses"
    );
    let chat = receiver
        .context()
        .store
        .get_chat(&receiver_chat)
        .await
        .expect("read chat")
        .expect("chat exists");
    assert_eq!(chat.local_tags, vec!["work".to_string()]);
}

#[tokio::test]
async fn declining_the_sas_transfers_nothing() {
    let offer_dir = TempDir::new().expect("offer dir");
    let accept_dir = TempDir::new().expect("accept dir");
    let offer = open_engine(offer_dir.path()).await;
    seed_tagged_chat(&offer, "work").await;
    write_json(
        &offer_dir.path().join("settings.json"),
        &json!({"density": "compact"}),
    );
    let receiver = open_engine(accept_dir.path()).await;
    let receiver_chat = seed_untagged_chat(&receiver).await;
    write_json(
        &accept_dir.path().join("settings.json"),
        &json!({"density": "spacious"}),
    );
    let before = read_json(&accept_dir.path().join("settings.json"));

    let mut offer_events = offer.sink().subscribe();
    let mut accept_events = receiver.sink().subscribe();

    let (offer_id, ticket, code) = begin_pairing(&offer).await;
    let accept_id = accept(&receiver, &ticket, &code).await;
    wait_phase(&mut offer_events, &offer_id, PairingPhase::SasReady).await;
    wait_phase(&mut accept_events, &accept_id, PairingPhase::SasReady).await;

    offer
        .handle(EngineCommand::ConfirmPairing {
            pairing_id: offer_id.clone(),
            accept: true,
        })
        .await
        .expect("offer confirms");
    receiver
        .handle(EngineCommand::ConfirmPairing {
            pairing_id: accept_id.clone(),
            accept: false,
        })
        .await
        .expect("receiver declines");

    wait_phase(&mut offer_events, &offer_id, PairingPhase::Declined).await;
    wait_phase(&mut accept_events, &accept_id, PairingPhase::Declined).await;

    assert_eq!(
        read_json(&accept_dir.path().join("settings.json")),
        before,
        "a declined pairing writes no settings"
    );
    assert!(!accept_dir.path().join("tags.json").exists());
    let chat = receiver
        .context()
        .store
        .get_chat(&receiver_chat)
        .await
        .expect("read chat")
        .expect("chat exists");
    assert!(chat.local_tags.is_empty(), "no tags crossed");
}

#[tokio::test]
async fn the_roster_names_this_device_and_revocation_advances_the_epoch() {
    let dir = TempDir::new().expect("dir");
    let engine = open_engine(dir.path()).await;

    let devices = match engine
        .handle(EngineCommand::ListDevices)
        .await
        .expect("list devices")
    {
        EngineResponse::DevicesList { devices } => devices,
        other => panic!("unexpected response: {other:?}"),
    };
    assert_eq!(devices.len(), 1, "the roster starts with this device");
    let this: &DeviceDto = &devices[0];
    assert!(this.current);
    assert_eq!(this.epoch, 0);

    // A second entry, as a later enrolment will add: revoking it bumps the epoch.
    let other = json!({
        "device_id": "peer-device",
        "label": "Laptop",
        "enrolled_ms": 1_700_000_000_000i64,
        "epoch": 0,
    });
    let mut settings = read_json(&dir.path().join("settings.json"));
    settings["sync"]["devices"]
        .as_array_mut()
        .expect("devices")
        .push(other);
    write_json(&dir.path().join("settings.json"), &settings);

    let epoch = match engine
        .handle(EngineCommand::RevokeDevice {
            device_id: "peer-device".to_string(),
        })
        .await
        .expect("revoke")
    {
        EngineResponse::DeviceRevoked { epoch } => epoch,
        other => panic!("unexpected response: {other:?}"),
    };
    assert_eq!(epoch, 1);

    let settings = read_json(&dir.path().join("settings.json"));
    let devices = settings["sync"]["devices"].as_array().expect("devices");
    assert_eq!(devices.len(), 1, "the revoked entry is gone");
    assert_eq!(devices[0]["device_id"], json!(this.device_id));
    assert_eq!(
        devices[0]["epoch"],
        json!(1),
        "the remaining device moved on"
    );
    assert_eq!(settings["sync"]["epoch"], json!(1));

    let err = engine
        .handle(EngineCommand::RevokeDevice {
            device_id: this.device_id.clone(),
        })
        .await
        .expect_err("a device cannot revoke itself");
    assert_eq!(err.code, "invalid_argument");
}

#[tokio::test]
async fn a_fresh_device_accepts_an_older_bundle() {
    let sender_dir = TempDir::new().expect("sender dir");
    let receiver_dir = TempDir::new().expect("receiver dir");
    let sender = open_engine(sender_dir.path()).await;
    write_json(
        &sender_dir.path().join("settings.json"),
        &json!({"density": "compact"}),
    );
    let (bundle_json, _, _) = export(&sender, false, false).await;

    // The receiving engine writes its own device-local bookkeeping before the bundle arrives.
    // That write must not look like a user edit, or the import would be refused as stale.
    let receiver = open_engine(receiver_dir.path()).await;
    receiver
        .handle(EngineCommand::ListDevices)
        .await
        .expect("the receiver writes its roster");
    let (report, applied) = import(&receiver, &bundle_json, true).await.expect("import");
    assert!(report.settings);
    assert!(applied);
    assert_eq!(
        read_json(&receiver_dir.path().join("settings.json"))["density"],
        json!("compact")
    );
}

async fn list_devices(engine: &AppEngine) -> Vec<DeviceDto> {
    match engine
        .handle(EngineCommand::ListDevices)
        .await
        .expect("list devices")
    {
        EngineResponse::DevicesList { devices } => devices,
        other => panic!("unexpected response: {other:?}"),
    }
}

async fn confirm(engine: &AppEngine, pairing_id: &str, accept: bool) {
    engine
        .handle(EngineCommand::ConfirmPairing {
            pairing_id: pairing_id.to_string(),
            accept,
        })
        .await
        .expect("confirm pairing");
}

/// Runs one accepted pairing between two engines and waits for both Done events.
async fn pair_engines(offer: &AppEngine, receiver: &AppEngine) -> (String, String) {
    let mut offer_events = offer.sink().subscribe();
    let mut accept_events = receiver.sink().subscribe();

    let (offer_id, ticket, code) = begin_pairing(offer).await;
    let accept_id = accept(receiver, &ticket, &code).await;
    assert_eq!(offer_id, accept_id, "both sides name the same pairing");

    let offer_sas = wait_phase(&mut offer_events, &offer_id, PairingPhase::SasReady).await;
    let accept_sas = wait_phase(&mut accept_events, &accept_id, PairingPhase::SasReady).await;
    assert_eq!(offer_sas, accept_sas, "both screens show the same digits");

    confirm(offer, &offer_id, true).await;
    confirm(receiver, &accept_id, true).await;

    wait_phase(&mut offer_events, &offer_id, PairingPhase::Done).await;
    wait_phase(&mut accept_events, &accept_id, PairingPhase::Done).await;
    (offer_id, accept_id)
}

#[tokio::test]
async fn pairing_enrols_both_devices_in_each_roster() {
    let offer_dir = TempDir::new().expect("offer dir");
    let accept_dir = TempDir::new().expect("accept dir");
    let offer = open_engine(offer_dir.path()).await;
    let receiver = open_engine(accept_dir.path()).await;

    pair_engines(&offer, &receiver).await;

    let offer_devices = list_devices(&offer).await;
    let accept_devices = list_devices(&receiver).await;
    assert_eq!(offer_devices.len(), 2, "the offering roster holds both");
    assert_eq!(accept_devices.len(), 2, "the accepting roster holds both");

    let offer_this = offer_devices
        .iter()
        .find(|device| device.current)
        .expect("the offering device is marked current");
    let accept_this = accept_devices
        .iter()
        .find(|device| device.current)
        .expect("the accepting device is marked current");
    assert_eq!(
        offer_devices.iter().filter(|device| device.current).count(),
        1
    );
    assert_eq!(
        accept_devices
            .iter()
            .filter(|device| device.current)
            .count(),
        1
    );

    // Each roster names the id the other device reports for itself.
    assert!(
        offer_devices
            .iter()
            .any(|device| device.device_id == accept_this.device_id),
        "the offering roster must contain the accepting device's own id"
    );
    assert!(
        accept_devices
            .iter()
            .any(|device| device.device_id == offer_this.device_id),
        "the accepting roster must contain the offering device's own id"
    );
    let mut offer_ids: Vec<&str> = offer_devices.iter().map(|d| d.device_id.as_str()).collect();
    let mut accept_ids: Vec<&str> = accept_devices
        .iter()
        .map(|d| d.device_id.as_str())
        .collect();
    offer_ids.sort_unstable();
    accept_ids.sort_unstable();
    assert_eq!(offer_ids, accept_ids, "both rosters agree on the ids");
    assert!(offer_devices.iter().all(|device| device.epoch == 0));

    // Revocation works on the enrolled peer, and this device stays current afterwards.
    let epoch = match receiver
        .handle(EngineCommand::RevokeDevice {
            device_id: offer_this.device_id.clone(),
        })
        .await
        .expect("revoke the enrolled peer")
    {
        EngineResponse::DeviceRevoked { epoch } => epoch,
        other => panic!("unexpected response: {other:?}"),
    };
    assert_eq!(epoch, 1);
    let after = list_devices(&receiver).await;
    assert_eq!(after.len(), 1, "the revoked peer is gone");
    assert!(after[0].current, "ListDevices still marks this device");
    assert_eq!(after[0].device_id, accept_this.device_id);
}

/// A device that completes the handshake but signs a fixed value instead of the transcript: it
/// cannot prove the identity it claims. The engine must reject it and leave its roster alone.
struct TamperedProver {
    identity: Vec<u8>,
}

impl TamperedProver {
    fn new(label: &str, seed: u8) -> Self {
        let key = SigningKey::from_bytes(&[seed; 32]);
        let mut identity = key.verifying_key().as_bytes().to_vec();
        identity.extend_from_slice(label.as_bytes());
        Self { identity }
    }

    fn shared(label: &str, seed: u8) -> Arc<dyn DeviceProver + Send + Sync> {
        Arc::new(Self::new(label, seed))
    }
}

impl DeviceProver for TamperedProver {
    fn identity(&self) -> &[u8] {
        &self.identity
    }

    fn sign(&self, _transcript: &[u8; 32]) -> [u8; 64] {
        [0u8; 64]
    }
}

#[tokio::test]
async fn a_tampered_identity_proof_fails_and_leaves_both_rosters_alone() {
    // Direction 1: the engine offers, the peer cannot prove itself. The offering roster must stay
    // at one entry and the pairing must report Failed, not Done.
    let offer_dir = TempDir::new().expect("offer dir");
    let offer = open_engine(offer_dir.path()).await;
    let mut offer_events = offer.sink().subscribe();
    let (offer_id, ticket, code) = begin_pairing(&offer).await;

    let parsed = PairingTicket::parse(&ticket).expect("parse ticket");
    let imposter = TamperedProver::new("imposter", 41);
    let pending = pairing::connect(&parsed.endpoint, &parsed, &code, &imposter)
        .await
        .expect("the transport handshake completes");

    wait_phase(&mut offer_events, &offer_id, PairingPhase::SasReady).await;
    confirm(&offer, &offer_id, true).await;
    pending.finish(true).await.expect("the imposter accepts");

    wait_phase(&mut offer_events, &offer_id, PairingPhase::Failed).await;
    assert_eq!(
        list_devices(&offer).await.len(),
        1,
        "a failed proof must not add a roster entry on the offering device"
    );

    // Direction 2: the engine accepts from a peer that cannot prove itself. Its roster must stay
    // at one entry too, and no config layer may be imported.
    let accept_dir = TempDir::new().expect("accept dir");
    let receiver = open_engine(accept_dir.path()).await;
    let mut accept_events = receiver.sink().subscribe();
    let malicious = PairingOffer::bind(
        ConfigState::default().export("imposter", 1),
        "127.0.0.1:0".parse().expect("loopback"),
        None,
        TamperedProver::shared("imposter", 42),
    )
    .await
    .expect("bind the imposter offer");
    let ticket = malicious.ticket().to_qr_string();
    let code = malicious.code().to_string();
    let offer_task = tokio::spawn(async move {
        let handshake = malicious.wait_for_peer().await?;
        handshake.finish(true).await?;
        Ok::<(), PairingError>(())
    });

    let accept_id = accept(&receiver, &ticket, &code).await;
    wait_phase(&mut accept_events, &accept_id, PairingPhase::SasReady).await;
    confirm(&receiver, &accept_id, true).await;
    offer_task
        .await
        .expect("offer task")
        .expect("imposter offer");

    wait_phase(&mut accept_events, &accept_id, PairingPhase::Failed).await;
    assert_eq!(
        list_devices(&receiver).await.len(),
        1,
        "a failed proof must not add a roster entry on the accepting device"
    );
    assert!(
        !accept_dir.path().join("tags.json").exists(),
        "no config layer is imported from a peer that cannot prove itself"
    );
}
