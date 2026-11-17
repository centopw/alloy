//! Pairing between two devices that have configured no network account at all.
//!
//! The onboarding path (and the Devices & Sync page) offers "pair a device" before a connector
//! exists, so nothing in pairing may wait for a network login: the offer builds its bundle from
//! the local store, and the new device joins, adopts the account key and enrols itself with the
//! engine alone. This test runs the real handshake over loopback between two engines with empty
//! stores, and pins the two reporting rules the person sees:
//!
//! - the code the offer produced is in the response with its expiry, so the surface can show it;
//! - the code never reaches a log line, and a pairing that completes logs no warning.

use std::fmt::Write as _;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use alloy_engine::factory::{ConnectorFactory, DefaultConnectorFactory};
use alloy_engine::{AppEngine, Engine, EngineContext};
use alloy_protocol::{
    EngineCommand, EngineEvent, EngineResponse, PAIRING_CODE_ALPHABET, PairingPhase, PairingRole,
};
use alloy_storage::InMemorySecretStore;
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::sync::broadcast;
use tokio::time::timeout;
use tracing::field::{Field, Visit};
use tracing::span;
use tracing::{Event, Level, Metadata, Subscriber};

const WAIT: Duration = Duration::from_secs(10);

async fn open_engine(data_dir: &Path) -> AppEngine {
    let secrets: Arc<dyn alloy_storage::SecretStore> = Arc::new(InMemorySecretStore::new());
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

fn write_settings(dir: &Path, value: &Value) {
    std::fs::write(
        dir.join("settings.json"),
        serde_json::to_vec_pretty(value).expect("encode settings"),
    )
    .expect("write settings");
}

fn read_settings(dir: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(dir.join("settings.json")).expect("read settings"))
        .expect("parse settings")
}

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

/// Records the level and rendered fields of every event dispatched to it.
#[derive(Default)]
struct LogCapture {
    events: Mutex<Vec<(Level, String)>>,
}

impl LogCapture {
    fn new() -> Self {
        Self::default()
    }

    fn lines(&self) -> Vec<(Level, String)> {
        self.events.lock().expect("log capture lock").clone()
    }
}

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

#[tokio::test]
async fn two_devices_without_a_configured_account_pair() {
    let offer_dir = TempDir::new().expect("offer dir");
    let accept_dir = TempDir::new().expect("accept dir");
    write_settings(offer_dir.path(), &json!({"density": "compact"}));

    let capture = Arc::new(LogCapture::new());
    let _guard = tracing::subscriber::set_default(capture.clone());

    let offer = open_engine(offer_dir.path()).await;
    let receiver = open_engine(accept_dir.path()).await;
    // Neither engine has an account: pairing must not wait for a connector.
    assert!(
        offer
            .context()
            .store
            .list_accounts()
            .await
            .expect("roster")
            .is_empty(),
        "the offering device starts with no account"
    );
    assert!(
        receiver
            .context()
            .store
            .list_accounts()
            .await
            .expect("roster")
            .is_empty(),
        "the new device starts with no account"
    );

    let mut offer_events = offer.sink().subscribe();
    let mut accept_events = receiver.sink().subscribe();

    let (offer_id, ticket, code, expires_at_ms) = match offer
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
            expires_at_ms,
            ..
        } => (pairing_id, ticket, code, expires_at_ms),
        other => panic!("unexpected response: {other:?}"),
    };

    // The surface can show the code and its remaining time: eight characters from the alphabet
    // the storage layer draws from, inside the 90-second window.
    assert_eq!(code.len(), 8, "the code is eight characters: {code:?}");
    assert!(
        code.bytes()
            .all(|byte| PAIRING_CODE_ALPHABET.as_bytes().contains(&byte)),
        "the code is typable from the shown alphabet: {code:?}"
    );
    let left_ms = expires_at_ms - alloy_engine::support::now_ms();
    assert!(
        (60_000..=91_000).contains(&left_ms),
        "the offer reports its expiry up front: {left_ms}ms left"
    );

    let accept_id = match receiver
        .handle(EngineCommand::AcceptPairingTicket {
            ticket: ticket.clone(),
            code: code.clone(),
        })
        .await
        .expect("accept ticket")
    {
        EngineResponse::PairingStarted { pairing_id, .. } => pairing_id,
        other => panic!("unexpected response: {other:?}"),
    };
    assert_eq!(offer_id, accept_id, "both sides name the same pairing");

    let offer_sas = wait_phase(&mut offer_events, &offer_id, PairingPhase::SasReady).await;
    let accept_sas = wait_phase(&mut accept_events, &accept_id, PairingPhase::SasReady).await;
    assert_eq!(offer_sas, accept_sas, "both devices show the same digits");
    assert_eq!(offer_sas.len(), 6);

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
            accept: true,
        })
        .await
        .expect("receiver confirms");

    wait_phase(&mut offer_events, &offer_id, PairingPhase::Done).await;
    wait_phase(&mut accept_events, &accept_id, PairingPhase::Done).await;

    assert_eq!(
        read_settings(accept_dir.path())["density"],
        json!("compact"),
        "the new device adopted the settings without a connector"
    );

    // The entry is dropped when the pairing ends, so the code and ticket cannot be asked for
    // afterwards: `PairingStatus` answers `not_found` and the surface stops offering them.
    let gone = offer
        .handle(EngineCommand::PairingStatus {
            pairing_id: offer_id.clone(),
        })
        .await
        .expect_err("a finished pairing is not live");
    assert_eq!(gone.code, "not_found");

    let logged = capture.lines();
    assert!(
        logged.iter().all(|(level, _)| *level != Level::WARN),
        "a completed pairing is not a warning: {logged:?}"
    );
    assert!(
        logged.iter().all(|(_, fields)| !fields.contains(&code)),
        "the pairing code never reaches a log line: {logged:?}"
    );
}
