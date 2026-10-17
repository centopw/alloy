//! Auth handler against a local Synapse. Set ALLOY_TEST_HS=http://localhost:8008 (see tests/synapse).

use alloy_engine::{AppEngine, Engine, EngineContext};
use alloy_protocol::{AccountId, EngineCommand, EngineEvent, EngineResponse};
use alloy_storage::InMemorySecretStore;
use std::sync::Arc;

fn hs() -> Option<String> {
    let v = std::env::var("ALLOY_TEST_HS").ok();
    if v.is_none() {
        eprintln!("skipping: ALLOY_TEST_HS not set");
    }
    v
}

fn engine(dir: &std::path::Path, secrets: Arc<InMemorySecretStore>) -> (AppEngine, EngineContext) {
    let ctx = EngineContext::new(dir.to_path_buf(), secrets);
    (AppEngine::new(ctx.clone()), ctx)
}

#[tokio::test]
async fn login_restore_logout_flow() {
    let Some(hs) = hs() else { return };
    let dir = tempfile::tempdir().unwrap();
    let secrets = Arc::new(InMemorySecretStore::new());

    // Discovery
    let (eng, ctx) = engine(dir.path(), secrets.clone());
    match eng
        .handle(EngineCommand::DiscoverHomeserver { input: hs.clone() })
        .await
        .unwrap()
    {
        EngineResponse::HomeserverInfo { name, .. } => assert_eq!(name, "localhost"),
        other => panic!("unexpected {other:?}"),
    }

    // Wrong password -> typed auth error, nothing persisted
    let err = eng
        .handle(EngineCommand::LoginPassword {
            homeserver_url: hs.clone(),
            username: "alice".into(),
            password: "wrong-password".into(),
        })
        .await
        .unwrap_err();
    assert_eq!(err.code, "auth_failed", "{err:?}");
    assert!(matches!(
        eng.handle(EngineCommand::ListAccounts).await.unwrap(),
        EngineResponse::AccountsList { accounts } if accounts.is_empty()
    ));

    // Successful login emits AuthState and persists the account
    let mut events = ctx.sink.subscribe();
    let account = match eng
        .handle(EngineCommand::LoginPassword {
            homeserver_url: hs.clone(),
            username: "alice".into(),
            password: "testpassword123!".into(),
        })
        .await
        .unwrap()
    {
        EngineResponse::LoggedIn { account } => account,
        other => panic!("unexpected {other:?}"),
    };
    assert_eq!(account.user_id.0, "@alice:localhost");
    match events.recv().await.unwrap() {
        EngineEvent::AuthState { account: Some(a) } => assert_eq!(a.id, account.id),
        other => panic!("unexpected {other:?}"),
    }
    assert!(ctx.client.read().await.is_some());

    // A brand-new engine (app restart) restores the session without a password
    let (eng2, ctx2) = engine(dir.path(), secrets.clone());
    let listed = match eng2.handle(EngineCommand::ListAccounts).await.unwrap() {
        EngineResponse::AccountsList { accounts } => accounts,
        other => panic!("unexpected {other:?}"),
    };
    assert_eq!(listed.len(), 1);
    eng2.handle(EngineCommand::RestoreSession {
        account_id: listed[0].id.clone(),
    })
    .await
    .unwrap();
    assert_eq!(
        ctx2.client.read().await.as_ref().unwrap().user_id(),
        Some("@alice:localhost")
    );

    // Logout removes the account, store and session
    eng2.handle(EngineCommand::Logout {
        account_id: listed[0].id.clone(),
    })
    .await
    .unwrap();
    assert!(ctx2.client.read().await.is_none());
    assert!(matches!(
        eng2.handle(EngineCommand::ListAccounts).await.unwrap(),
        EngineResponse::AccountsList { accounts } if accounts.is_empty()
    ));
    let err = eng2
        .handle(EngineCommand::RestoreSession {
            account_id: AccountId("@alice:localhost".into()),
        })
        .await
        .unwrap_err();
    assert_eq!(err.code, "not_found", "{err:?}");
}
