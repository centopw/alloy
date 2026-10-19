// Integration test against a running Synapse server
// Run with: ALLOY_TEST_HS=http://localhost:8008 cargo test -p alloy-core --test url_preview_integration -- --nocapture

use alloy_core::{AlloyClient, AlloyError, InMemorySecretStore};
use tempfile::TempDir;

#[tokio::test]
async fn test_url_preview_rejects_non_http_and_never_fails_for_missing_previews() {
    let hs = match std::env::var("ALLOY_TEST_HS") {
        Ok(hs) => hs,
        Err(_) => {
            println!("ALLOY_TEST_HS not set, skipping integration test");
            return;
        }
    };

    let tmpdir = TempDir::new().unwrap();
    let alice = AlloyClient::new(
        &hs,
        tmpdir.path(),
        Some(Box::new(InMemorySecretStore::new())),
    )
    .await
    .unwrap();
    alice
        .login_password("alice", "testpassword123!", Some("alice-device"))
        .await
        .expect("Alice login failed");

    assert!(matches!(
        alice.url_preview("file:///etc/passwd").await,
        Err(AlloyError::InvalidUrl(_))
    ));

    // Previews may be disabled or unfetchable on the test server; either way the caller gets Ok.
    let preview = alice.url_preview("https://example.com/").await;
    assert!(preview.is_ok(), "preview request failed: {preview:?}");
    println!("✓ url_preview returned {preview:?}");
}
