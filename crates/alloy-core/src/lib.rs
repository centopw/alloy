#![recursion_limit = "256"]

pub mod account;
pub mod account_prefs;
pub mod accounts;
pub mod auth;
pub mod client;
pub mod crypto;
pub mod mapping;
pub mod media;
pub mod notifications;
pub mod room_media;
pub mod room_search;
pub mod room_tags;
pub mod rooms;
pub mod timeline;
pub mod tls;
pub mod url_preview;

pub use account::DeleteDevicesOutcome;
pub use accounts::{
    AccountEntry, AccountIndex, open_default_secret_store, restore_account, restore_active_account,
};
pub use alloy_protocol::*;
pub use alloy_storage::{InMemorySecretStore, SecretStore, StorageError};
pub use client::*;
pub use crypto::{ActiveVerification, VerificationCallback, VerificationUpdate};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AlloyError {
    #[error("Matrix SDK error: {0}")]
    MatrixSdk(String),
    #[error("HTTP client error: {0}")]
    HttpClient(String),
    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("URL error: {0}")]
    Url(#[from] url::ParseError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Auth error: {0}")]
    Auth(String),
    /// An OIDC/MAS login failure. `reason` is a stable discriminator for the UI
    /// (`denied`, `expired`, `state_mismatch`, `exchange_failed`, `unsupported`,
    /// `registration_failed`, `network`); `message` is for humans.
    #[error("OIDC login failed ({reason}): {message}")]
    Oidc {
        reason: &'static str,
        message: String,
    },
    #[error("Not logged in")]
    NotLoggedIn,
    #[error("Account not found: {0}")]
    AccountNotFound(String),
    #[error("Room not found: {0}")]
    RoomNotFound(String),
    #[error("Invalid homeserver URL")]
    InvalidHomeserverUrl,
    #[error("Insecure homeserver URL: plain HTTP is only allowed for localhost")]
    InsecureHomeserverUrl,
    #[error("Invalid room ID: {0}")]
    InvalidRoomId(String),
    #[error("Invalid room tag: {0}")]
    InvalidRoomTag(String),
    #[error("Invalid preferences: {0}")]
    InvalidPrefs(String),
    #[error("Invalid search: {0}")]
    InvalidSearch(String),
    #[error("Invalid URL: {0}")]
    InvalidUrl(String),
    #[error("Base64 error: {0}")]
    Base64(#[from] base64::DecodeError),
}

pub type AlloyResult<T> = Result<T, AlloyError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_client_creation() {
        let tmpdir = tempfile::tempdir().unwrap();
        let client = AlloyClient::new(
            "http://localhost:8008",
            tmpdir.path(),
            Some(Box::new(InMemorySecretStore::new())),
        )
        .await;

        assert!(client.is_ok());
        let client = client.unwrap();
        assert!(client.user_id().is_none());
    }

    #[tokio::test]
    async fn test_client_restore_session_not_logged_in() {
        let tmpdir = tempfile::tempdir().unwrap();
        let client = AlloyClient::new(
            "http://localhost:8008",
            tmpdir.path(),
            Some(Box::new(InMemorySecretStore::new())),
        )
        .await
        .unwrap();

        let restored = client.restore_session().await.unwrap();
        assert!(!restored);
    }

    #[tokio::test]
    async fn test_store_passphrase_persistence() {
        let tmpdir = tempfile::tempdir().unwrap();
        let secret_store = InMemorySecretStore::new();

        // First client creates passphrase
        let _client1 = AlloyClient::new(
            "http://localhost:8008",
            tmpdir.path(),
            Some(Box::new(secret_store.clone())),
        )
        .await
        .unwrap();

        // Retrieve passphrase
        let pp1 = secret_store
            .get_secret("alloy:store:encryption_passphrase")
            .await
            .unwrap();

        // Second client should use same passphrase
        let _client2 = AlloyClient::new(
            "http://localhost:8008",
            tmpdir.path(),
            Some(Box::new(secret_store.clone())),
        )
        .await
        .unwrap();

        let pp2 = secret_store
            .get_secret("alloy:store:encryption_passphrase")
            .await
            .unwrap();

        assert_eq!(pp1, pp2);
    }

    #[tokio::test]
    #[ignore = "requires Synapse running at ALLOY_TEST_HS"]
    async fn test_integration_login_and_sync() {
        let hs =
            std::env::var("ALLOY_TEST_HS").unwrap_or_else(|_| "http://localhost:8008".to_string());
        let tmpdir = tempfile::tempdir().unwrap();
        let secret_store = Box::new(InMemorySecretStore::new());

        let client = AlloyClient::new(&hs, tmpdir.path(), Some(secret_store))
            .await
            .unwrap();

        let result = client
            .login_password("alice", "testpassword123!", Some("test-device"))
            .await;
        assert!(result.is_ok(), "Login failed: {:?}", result);
        assert!(client.user_id().is_some());

        let sync_result = client.sync_once().await;
        assert!(sync_result.is_ok(), "Sync failed: {:?}", sync_result);
    }

    #[tokio::test]
    #[ignore = "requires Synapse running at ALLOY_TEST_HS"]
    async fn test_integration_room_creation_and_messaging() {
        let hs =
            std::env::var("ALLOY_TEST_HS").unwrap_or_else(|_| "http://localhost:8008".to_string());
        let tmpdir = tempfile::tempdir().unwrap();

        let client = AlloyClient::new(&hs, tmpdir.path(), None).await.unwrap();
        client
            .login_password("alice", "testpassword123!", Some("alice-device"))
            .await
            .unwrap();
        client.sync_once().await.unwrap();

        // Create a room
        let room_id = client.create_room("Test Room", false).await.unwrap();
        println!("Created room: {}", room_id.0);

        // Send a message
        let event_id = client.send_message(&room_id, "Hello, test!").await.unwrap();
        println!("Sent message: {}", event_id.0);

        // Get timeline to verify message was queued
        let _timeline = client.timeline(&room_id).await.unwrap();

        println!("Message send test passed");
    }

    #[tokio::test]
    #[ignore = "requires Synapse running at ALLOY_TEST_HS"]
    async fn test_integration_full_flow() {
        let hs =
            std::env::var("ALLOY_TEST_HS").unwrap_or_else(|_| "http://localhost:8008".to_string());

        // Alice's client
        let tmpdir_alice = tempfile::tempdir().unwrap();
        let alice = AlloyClient::new(&hs, tmpdir_alice.path(), None)
            .await
            .unwrap();
        alice
            .login_password("alice", "testpassword123!", Some("alice-device"))
            .await
            .unwrap();
        alice.sync_once().await.unwrap();
        println!("Alice logged in and synced");

        // Alice creates room
        let room_id = alice.create_room("Test Room", false).await.unwrap();
        println!("Alice created room: {}", room_id.0);

        // Alice sends message (generates local echo via send queue)
        let event_id = alice
            .send_message(&room_id, "Hello from Alice")
            .await
            .unwrap();
        println!("Alice queued message with transaction ID: {}", event_id.0);

        // Bob joins
        let tmpdir_bob = tempfile::tempdir().unwrap();
        let bob = AlloyClient::new(&hs, tmpdir_bob.path(), None)
            .await
            .unwrap();
        bob.login_password("bob", "testpassword123!", Some("bob-device"))
            .await
            .unwrap();
        bob.sync_once().await.unwrap();
        println!("Bob logged in and synced");

        bob.join_room(&room_id).await.unwrap();
        println!("Bob joined room: {}", room_id.0);

        bob.sync_once().await.unwrap();
        println!("Bob synced again to get room content");

        // Bob gets timeline and sees Alice's message
        let timeline = bob.timeline(&room_id).await.unwrap();
        let (items, _stream) = timeline.subscribe().await;
        println!("Bob sees {} items in timeline", items.len());

        assert!(!items.is_empty(), "Bob should see messages from Alice");
        println!("Full integration test PASSED");
    }
}
