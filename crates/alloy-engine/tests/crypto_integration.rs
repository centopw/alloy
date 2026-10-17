//! Recovery and SAS verification between two devices of one account against a local Synapse.
//! Set ALLOY_TEST_HS=http://localhost:8008.

use alloy_engine::{AppEngine, Engine, EngineContext};
use alloy_protocol::{
    EngineCommand, EngineEvent, EngineResponse, RecoveryState, UserId, VerificationState,
};
use alloy_storage::InMemorySecretStore;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast::Receiver;

const PW: &str = "testpassword123!";

/// Register a brand-new account so repeated runs never inherit cross-signing/recovery state.
async fn register_unique(hs: &str) -> String {
    use matrix_sdk::ruma::api::client::account::register::v3::Request;
    use matrix_sdk::ruma::api::client::uiaa::{AuthData, Dummy};
    let dir = tempfile::tempdir().unwrap();
    let client = alloy_core::AlloyClient::new(hs, dir.path(), None)
        .await
        .unwrap();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let username = format!("crypto{nanos:x}");
    let mut req = Request::new();
    req.username = Some(username.clone());
    req.password = Some(PW.into());
    req.auth = Some(AuthData::Dummy(Dummy::new()));
    client
        .inner_client()
        .matrix_auth()
        .register(req)
        .await
        .expect("registration (enable_registration must be on in tests/synapse)");
    username
}

async fn device(hs: &str, user: &str) -> (AppEngine, EngineContext, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let ctx = EngineContext::new(
        dir.path().to_path_buf(),
        Arc::new(InMemorySecretStore::new()),
    );
    let eng = AppEngine::new(ctx.clone());
    eng.handle(EngineCommand::LoginPassword {
        homeserver_url: hs.into(),
        username: user.into(),
        password: PW.into(),
    })
    .await
    .unwrap();
    (eng, ctx, dir)
}

/// Poll `GetRecoveryStatus` until it reports `want`.
async fn wait_status(eng: &AppEngine, want: RecoveryState) {
    let mut last = None;
    for _ in 0..60 {
        if let Ok(EngineResponse::RecoveryStatusRetrieved { state }) =
            eng.handle(EngineCommand::GetRecoveryStatus).await
        {
            if state == want {
                return;
            }
            last = Some(state);
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    panic!("recovery status never became {want:?}, last {last:?}");
}

async fn wait_for<T>(
    rx: &mut Receiver<EngineEvent>,
    what: &str,
    mut pick: impl FnMut(EngineEvent) -> Option<T>,
) -> T {
    tokio::time::timeout(Duration::from_secs(90), async {
        loop {
            if let Some(v) = pick(rx.recv().await.unwrap()) {
                return v;
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting for {what}"))
}

#[tokio::test]
async fn recovery_key_and_sas_verification() {
    let Ok(hs) = std::env::var("ALLOY_TEST_HS") else {
        eprintln!("skipping: ALLOY_TEST_HS not set");
        return;
    };

    let user = register_unique(&hs).await;
    let alice_id = format!("@{user}:localhost");

    // Device A sets up recovery (creates cross-signing, secret storage, backup).
    let (a, a_ctx, _da) = device(&hs, &user).await;
    let mut a_rx = a_ctx.sink.subscribe();
    let key = match a
        .handle(EngineCommand::EnableRecovery {
            password: Some(PW.into()),
        })
        .await
    {
        Ok(EngineResponse::RecoveryEnabled { recovery_key }) => recovery_key,
        other => panic!("unexpected {other:?}"),
    };
    assert!(
        key.split_whitespace().count() >= 4,
        "recovery key looks wrong: {key}"
    );
    wait_status(&a, RecoveryState::Enabled).await;

    // Device B: wrong key is a typed error, the right key restores the secrets.
    let (b, _, _db) = device(&hs, &user).await;
    let err = b
        .handle(EngineCommand::RecoverWithKey {
            recovery_key:
                "EsTc 1111 2222 3333 4444 5555 6666 7777 8888 9999 aaaa bbbb cccc dddd eeee ffff"
                    .into(),
        })
        .await
        .unwrap_err();
    assert_eq!(err.code, "auth_failed", "{err:?}");
    wait_status(&b, RecoveryState::Lost).await;
    assert!(matches!(
        b.handle(EngineCommand::RecoverWithKey {
            recovery_key: key.clone()
        })
        .await
        .unwrap(),
        EngineResponse::RecoveredFromKey { recovered: true }
    ));

    // Device C: fresh device verifies against A with emojis.
    let (c, c_ctx, _dc) = device(&hs, &user).await;
    let mut c_rx = c_ctx.sink.subscribe();
    let mut tries = 0;
    loop {
        match c.handle(EngineCommand::RequestSelfVerification).await {
            Ok(_) => break,
            Err(e) if tries < 20 => {
                tries += 1;
                eprintln!("request not ready yet: {e:?}");
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            Err(e) => panic!("could not request verification: {e:?}"),
        }
    }
    // A (or B: all other devices get the request) is told about it; A accepts.
    wait_for(&mut a_rx, "incoming verification on A", |e| match e {
        EngineEvent::VerificationState {
            state: VerificationState::Ready,
            ..
        } => Some(()),
        _ => None,
    })
    .await;
    let alice = UserId(alice_id.clone());
    a.handle(EngineCommand::AcceptVerification {
        user_id: alice.clone(),
        device_id: String::new(),
    })
    .await
    .unwrap();

    let emojis_a = wait_for(&mut a_rx, "emojis on A", |e| match e {
        EngineEvent::VerificationState {
            state: VerificationState::CompareEmojis { emojis, .. },
            ..
        } => Some(emojis),
        _ => None,
    })
    .await;
    let emojis_c = wait_for(&mut c_rx, "emojis on C", |e| match e {
        EngineEvent::VerificationState {
            state: VerificationState::CompareEmojis { emojis, .. },
            ..
        } => Some(emojis),
        _ => None,
    })
    .await;
    assert_eq!(emojis_a.len(), 7);
    assert_eq!(emojis_a, emojis_c, "both sides must show the same emojis");

    for eng in [&a, &c] {
        eng.handle(EngineCommand::ConfirmSas {
            user_id: alice.clone(),
            device_id: String::new(),
        })
        .await
        .unwrap();
    }
    for rx in [&mut a_rx, &mut c_rx] {
        wait_for(rx, "verified", |e| match e {
            EngineEvent::VerificationState {
                state: VerificationState::Verified,
                ..
            } => Some(()),
            _ => None,
        })
        .await;
    }
}
