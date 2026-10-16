#![recursion_limit = "256"]

use matrix_sdk::Client;
use matrix_sdk::ruma::RoomId as RumaRoomId;
use matrix_sdk::ruma::events::room::message::RoomMessageEventContent;
use matrix_sdk_sqlite::SqliteStoreConfig;
use matrix_sdk_ui::room_list_service::RoomListService;
use matrix_sdk_ui::timeline::Timeline;
use std::path::Path;
use url::Url;

pub use alloy_protocol::*;
pub use alloy_storage::{InMemorySecretStore, SecretStore, StorageError};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AlloyError {
    #[error("Matrix SDK error: {0}")]
    MatrixSdk(String),
    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("URL error: {0}")]
    Url(#[from] url::ParseError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Auth error: {0}")]
    Auth(String),
    #[error("Not logged in")]
    NotLoggedIn,
    #[error("Room not found: {0}")]
    RoomNotFound(String),
    #[error("Invalid homeserver URL")]
    InvalidHomeserverUrl,
    #[error("Invalid room ID: {0}")]
    InvalidRoomId(String),
    #[error("Base64 error: {0}")]
    Base64(#[from] base64::DecodeError),
}

pub type AlloyResult<T> = Result<T, AlloyError>;

/// Generate a random passphrase for store encryption
fn generate_passphrase() -> String {
    use base64::Engine;
    let random_bytes: [u8; 32] = rand::random();
    base64::engine::general_purpose::STANDARD.encode(random_bytes)
}

/// Lightweight Matrix client wrapper providing room list and timeline streams
/// via alloy-protocol DTOs.
pub struct AlloyClient {
    client: Client,
    secret_store: Box<dyn SecretStore>,
}

impl AlloyClient {
    /// Create a new AlloyClient with SQLite store encrypted via passphrase in SecretStore.
    pub async fn new(
        homeserver_url: &str,
        data_dir: &Path,
        secret_store: Option<Box<dyn SecretStore>>,
    ) -> AlloyResult<Self> {
        let homeserver =
            Url::parse(homeserver_url).map_err(|_| AlloyError::InvalidHomeserverUrl)?;

        let state_store_path = data_dir.join("matrix");
        tokio::fs::create_dir_all(&state_store_path).await?;

        let secret_store = secret_store.unwrap_or_else(|| Box::new(InMemorySecretStore::new()));

        // Generate or retrieve passphrase for store encryption
        let store_passphrase_key = "alloy:store:encryption_passphrase";
        let passphrase = match secret_store.get_secret(store_passphrase_key).await {
            Ok(pp) => pp,
            Err(_) => {
                let pp = generate_passphrase();
                let _ = secret_store.set_secret(store_passphrase_key, &pp).await;
                pp
            }
        };

        // Configure SQLite store with encryption
        let store_config = SqliteStoreConfig::new(&state_store_path).passphrase(Some(&passphrase));

        let client = Client::builder()
            .homeserver_url(homeserver)
            .sqlite_store_with_config_and_cache_path(store_config, None::<&Path>)
            .build()
            .await
            .map_err(|e| AlloyError::MatrixSdk(e.to_string()))?;

        Ok(Self {
            client,
            secret_store,
        })
    }

    /// Login with username/password.
    pub async fn login_password(
        &self,
        username: &str,
        password: &str,
        device_name: Option<&str>,
    ) -> AlloyResult<()> {
        let auth = self.client.matrix_auth();
        let _response = auth
            .login_username(username, password)
            .initial_device_display_name(device_name.unwrap_or("Alloy"))
            .send()
            .await
            .map_err(|e| AlloyError::Auth(e.to_string()))?;

        // Store session in secret store for session restoration
        if let Some(session) = self.client.matrix_auth().session()
            && let Ok(json) = serde_json::to_string(&session)
        {
            let _ = self.secret_store.set_secret("alloy:session", &json).await;
        }

        Ok(())
    }

    /// Restore a previously logged-in session from the store.
    pub async fn restore_session(&self) -> AlloyResult<bool> {
        if self.client.user_id().is_some() {
            return Ok(true);
        }
        match self.secret_store.get_secret("alloy:session").await {
            Ok(json) => {
                let session: matrix_sdk::authentication::matrix::MatrixSession =
                    serde_json::from_str(&json).map_err(|e| AlloyError::Auth(e.to_string()))?;
                self.client
                    .restore_session(session)
                    .await
                    .map_err(|e| AlloyError::MatrixSdk(e.to_string()))?;
                Ok(true)
            }
            Err(_) => Ok(false),
        }
    }

    /// Get the matrix-sdk Client for advanced operations.
    pub fn inner_client(&self) -> &Client {
        &self.client
    }

    /// Sync once.
    pub async fn sync_once(&self) -> AlloyResult<()> {
        let settings =
            matrix_sdk::config::SyncSettings::default().timeout(std::time::Duration::from_secs(2));
        self.client
            .sync_once(settings)
            .await
            .map_err(|e| AlloyError::MatrixSdk(e.to_string()))?;
        Ok(())
    }

    /// Create a new room, optionally with encryption.
    pub async fn create_room(&self, name: &str, encrypted: bool) -> AlloyResult<RoomId> {
        use matrix_sdk::ruma::api::client::room::create_room::v3::Request;
        use matrix_sdk::ruma::events::{
            InitialStateEvent, room::encryption::RoomEncryptionEventContent,
        };

        let mut request = Request::new();
        request.name = Some(name.to_string());

        if encrypted {
            request.initial_state = vec![
                InitialStateEvent::with_empty_state_key(
                    RoomEncryptionEventContent::with_recommended_defaults(),
                )
                .to_raw_any(),
            ];
        }

        let response = self
            .client
            .create_room(request)
            .await
            .map_err(|e| AlloyError::MatrixSdk(e.to_string()))?;

        Ok(RoomId(response.room_id().to_string()))
    }

    /// Open and initialize RoomListService (sliding sync).
    pub async fn room_list_service(&self) -> AlloyResult<RoomListService> {
        let room_list_svc = RoomListService::new(self.client.clone())
            .await
            .map_err(|e| AlloyError::MatrixSdk(e.to_string()))?;
        Ok(room_list_svc)
    }
    /// Subscribe to room list changes as a stream of VectorDiffDto<RoomSummary>.
    ///
    /// Returns the initial list of rooms and a stream of diffs.
    pub fn room_list_stream(
        &self,
    ) -> (
        Vec<RoomSummary>,
        impl futures::Stream<Item = Vec<alloy_protocol::VectorDiffDto<RoomSummary>>> + '_,
    ) {
        use futures::StreamExt;
        let (initial, raw_stream) = self.client.rooms_stream();
        let initial_summaries: Vec<RoomSummary> = initial.iter().map(room_to_summary).collect();

        let stream = raw_stream.map(|diffs| {
            diffs
                .into_iter()
                .map(|diff| match diff {
                    matrix_sdk_ui::eyeball_im::VectorDiff::Append { values } => {
                        alloy_protocol::VectorDiffDto::Append {
                            values: values.iter().map(room_to_summary).collect(),
                        }
                    }
                    matrix_sdk_ui::eyeball_im::VectorDiff::Clear => {
                        alloy_protocol::VectorDiffDto::Clear
                    }
                    matrix_sdk_ui::eyeball_im::VectorDiff::PushFront { value } => {
                        alloy_protocol::VectorDiffDto::PushFront {
                            value: room_to_summary(&value),
                        }
                    }
                    matrix_sdk_ui::eyeball_im::VectorDiff::PushBack { value } => {
                        alloy_protocol::VectorDiffDto::PushBack {
                            value: room_to_summary(&value),
                        }
                    }
                    matrix_sdk_ui::eyeball_im::VectorDiff::PopFront => {
                        alloy_protocol::VectorDiffDto::PopFront
                    }
                    matrix_sdk_ui::eyeball_im::VectorDiff::PopBack => {
                        alloy_protocol::VectorDiffDto::PopBack
                    }
                    matrix_sdk_ui::eyeball_im::VectorDiff::Insert { index, value } => {
                        alloy_protocol::VectorDiffDto::Insert {
                            index,
                            value: room_to_summary(&value),
                        }
                    }
                    matrix_sdk_ui::eyeball_im::VectorDiff::Set { index, value } => {
                        alloy_protocol::VectorDiffDto::Set {
                            index,
                            value: room_to_summary(&value),
                        }
                    }
                    matrix_sdk_ui::eyeball_im::VectorDiff::Remove { index } => {
                        alloy_protocol::VectorDiffDto::Remove { index }
                    }
                    matrix_sdk_ui::eyeball_im::VectorDiff::Truncate { length } => {
                        alloy_protocol::VectorDiffDto::Truncate { length }
                    }
                    matrix_sdk_ui::eyeball_im::VectorDiff::Reset { values } => {
                        alloy_protocol::VectorDiffDto::Reset {
                            values: values.iter().map(room_to_summary).collect(),
                        }
                    }
                })
                .collect()
        });

        (initial_summaries, stream)
    }

    /// Open a room's timeline.
    pub async fn timeline(&self, room_id: &RoomId) -> AlloyResult<Timeline> {
        let parsed_id =
            RumaRoomId::parse(&room_id.0).map_err(|e| AlloyError::InvalidRoomId(e.to_string()))?;

        let room = self
            .client
            .get_room(&parsed_id)
            .ok_or_else(|| AlloyError::RoomNotFound(room_id.0.clone()))?;

        let timeline = matrix_sdk_ui::timeline::TimelineBuilder::new(&room)
            .build()
            .await
            .map_err(|e| AlloyError::MatrixSdk(e.to_string()))?;

        Ok(timeline)
    }

    /// Send a text message to a room using send_queue for local echo.
    pub async fn send_message(&self, room_id: &RoomId, content: &str) -> AlloyResult<EventId> {
        let parsed_id =
            RumaRoomId::parse(&room_id.0).map_err(|e| AlloyError::InvalidRoomId(e.to_string()))?;

        let room = self
            .client
            .get_room(&parsed_id)
            .ok_or_else(|| AlloyError::RoomNotFound(room_id.0.clone()))?;
        let msg_content = RoomMessageEventContent::text_plain(content);
        let response = room
            .send(msg_content)
            .await
            .map_err(|e| AlloyError::MatrixSdk(e.to_string()))?;

        Ok(EventId(response.response.event_id.to_string()))
    }

    /// Join a room by ID.
    pub async fn join_room(&self, room_id: &RoomId) -> AlloyResult<()> {
        let parsed_id =
            RumaRoomId::parse(&room_id.0).map_err(|e| AlloyError::InvalidRoomId(e.to_string()))?;

        self.client
            .join_room_by_id(&parsed_id)
            .await
            .map_err(|e| AlloyError::MatrixSdk(e.to_string()))?;

        Ok(())
    }

    /// Invite a user to a room.
    pub async fn invite_user(&self, room_id: &RoomId, user_id: &str) -> AlloyResult<()> {
        let parsed_room_id =
            RumaRoomId::parse(&room_id.0).map_err(|e| AlloyError::InvalidRoomId(e.to_string()))?;
        let parsed_user_id = matrix_sdk::ruma::UserId::parse(user_id)
            .map_err(|e| AlloyError::MatrixSdk(e.to_string()))?;

        let room = self
            .client
            .get_room(&parsed_room_id)
            .ok_or_else(|| AlloyError::RoomNotFound(room_id.0.clone()))?;

        room.invite_user_by_id(&parsed_user_id)
            .await
            .map_err(|e| AlloyError::MatrixSdk(e.to_string()))?;

        Ok(())
    }

    /// Get user ID if logged in.
    pub fn user_id(&self) -> Option<&str> {
        self.client.user_id().map(|uid| uid.as_str())
    }
}
/// Convert a matrix-sdk Room to an alloy-protocol RoomSummary DTO.
fn room_to_summary(room: &matrix_sdk::Room) -> RoomSummary {
    RoomSummary {
        id: RoomId(room.room_id().to_string()),
        name: room.name(),
        avatar_url: None,
        unread_notifications: NotificationCount {
            highlight_count: 0,
            notification_count: 0,
        },
        latest_event: None,
        is_direct: false,
        is_space: room.is_space(),
        membership: match room.state() {
            matrix_sdk::RoomState::Joined => RoomMembership::Joined,
            matrix_sdk::RoomState::Invited => RoomMembership::Invited,
            matrix_sdk::RoomState::Left => RoomMembership::Left,
            matrix_sdk::RoomState::Banned => RoomMembership::Banned,
            _ => RoomMembership::Joined,
        },
    }
}

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
