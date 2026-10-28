//! OIDC (MAS) login against a local Matrix Authentication Service stack.
//! Set ALLOY_TEST_MAS=https://mas.example.org (the homeserver base URL that serves the Matrix
//! API and the authorization server metadata).
//!
//! These tests cover everything up to the user's consent in the browser. The authorization code
//! itself needs a human (or a scripted login form), so the token exchange is verified by the
//! end-to-end pass on the stack rather than here.

use alloy_engine::{AppEngine, Engine, EngineContext};
use alloy_protocol::{EngineCommand, EngineResponse};
use alloy_storage::InMemorySecretStore;
use std::collections::HashMap;
use std::sync::Arc;

fn engine(dir: &tempfile::TempDir) -> AppEngine {
    AppEngine::new(EngineContext::new(
        dir.path().to_path_buf(),
        Arc::new(InMemorySecretStore::new()),
    ))
}

fn pending_store_dirs(dir: &tempfile::TempDir) -> Vec<std::path::PathBuf> {
    let accounts = dir.path().join("accounts");
    let Ok(entries) = std::fs::read_dir(accounts) else {
        return Vec::new();
    };
    entries.flatten().map(|entry| entry.path()).collect()
}

#[tokio::test]
async fn oidc_login_registers_the_client_and_builds_an_authorization_url() {
    let Ok(homeserver) = std::env::var("ALLOY_TEST_MAS") else {
        eprintln!("skipping: ALLOY_TEST_MAS not set");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let eng = engine(&dir);

    let (authorization_url, redirect_uri) = match eng
        .handle(EngineCommand::StartOidcLogin {
            homeserver_url: homeserver,
        })
        .await
        .unwrap()
    {
        EngineResponse::OidcUrl {
            authorization_url,
            redirect_uri,
        } => (authorization_url, redirect_uri),
        other => panic!("unexpected {other:?}"),
    };

    // RFC 8252 loopback redirect: the engine listens on an ephemeral local port, because the
    // authorization server rejects a custom scheme by default.
    assert!(
        redirect_uri.starts_with("http://127.0.0.1:") && redirect_uri.ends_with("/callback"),
        "unexpected redirect URI: {redirect_uri}"
    );

    // Discovery + dynamic registration succeeded if the URL carries the registered client id,
    // our redirect URI, the authorization-code response type, PKCE S256 and a CSRF state.
    let url = url::Url::parse(&authorization_url).unwrap();
    let params: HashMap<String, String> = url.query_pairs().into_owned().collect();
    assert_eq!(
        params.get("redirect_uri").map(String::as_str),
        Some(redirect_uri.as_str()),
        "{authorization_url}"
    );
    assert_eq!(
        params.get("response_type").map(String::as_str),
        Some("code")
    );
    assert_eq!(
        params.get("code_challenge_method").map(String::as_str),
        Some("S256")
    );
    assert!(
        params.get("client_id").is_some_and(|id| !id.is_empty()),
        "no client id in {authorization_url}"
    );
    assert!(
        params.get("state").is_some_and(|state| !state.is_empty()),
        "no state in {authorization_url}"
    );

    // A callback carrying a foreign state is rejected, and the pending login is consumed.
    let err = eng
        .handle(EngineCommand::CompleteOidcLogin {
            redirect_url: format!("{redirect_uri}?code=made-up&state=not-ours"),
        })
        .await
        .expect_err("a foreign callback state must be rejected");
    assert_eq!(err.code, "auth_failed");
    assert_eq!(
        err.reason.as_deref(),
        Some("state_mismatch"),
        "the UI needs a stable reason, not prose: {err:?}"
    );
    match eng.handle(EngineCommand::ListAccounts).await.unwrap() {
        EngineResponse::AccountsList { accounts } => assert!(accounts.is_empty()),
        other => panic!("unexpected {other:?}"),
    }
    assert!(
        pending_store_dirs(&dir).is_empty(),
        "the failed login's store is purged"
    );
}

#[tokio::test]
async fn cancelling_an_oidc_login_removes_its_temporary_store() {
    let Ok(homeserver) = std::env::var("ALLOY_TEST_MAS") else {
        eprintln!("skipping: ALLOY_TEST_MAS not set");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let eng = engine(&dir);

    eng.handle(EngineCommand::StartOidcLogin {
        homeserver_url: homeserver,
    })
    .await
    .unwrap();
    assert_eq!(
        pending_store_dirs(&dir).len(),
        1,
        "the pending login owns a temporary store"
    );

    assert!(matches!(
        eng.handle(EngineCommand::CancelOidcLogin).await.unwrap(),
        EngineResponse::Unit
    ));
    assert!(
        pending_store_dirs(&dir).is_empty(),
        "cancelling removes the temporary store"
    );

    // Cancelling again is a no-op.
    assert!(matches!(
        eng.handle(EngineCommand::CancelOidcLogin).await.unwrap(),
        EngineResponse::Unit
    ));
}
