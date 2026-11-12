//! The host side of an out-of-process connector.
//!
//! [`SidecarConnector`] implements [`Connector`] by driving an external executable over the
//! newline-delimited JSON framing defined in [`alloy_connector::rpc`]: one JSON object per line on
//! stdin and stdout, a `hello` handshake first, requests correlated by id, and events interleaved
//! between responses. The Messenger connector (`sidecars/alloy-meta`) is the first user; the same
//! host serves any executable that speaks the protocol.
//!
//! # Supervision
//!
//! The host owns the process. When the child exits or the pipe breaks, every pending call fails
//! with [`ConnectorError::Network`] (`"sidecar exited: <status>"`), the host emits
//! [`ConnectionState::Offline`](alloy_connector::ConnectionState::Offline), and the process is
//! relaunched with exponential backoff (1 s, 2 s, 4 s, ... capped at 60 s). At most
//! [`SidecarConfig::max_restarts`] relaunches happen inside a rolling ten-minute window; once they
//! are spent the connector reports `"sidecar crashed repeatedly"` and stays down until
//! [`Connector::start`] is called again.
//!
//! After a restart the host replays the session it was given (through [`Connector::restore`], a
//! completed login, or [`ConnectorEvent::SessionUpdated`]) and re-sends `start`, so a live account
//! keeps receiving events without the engine repeating the login. Live updates resume with
//! [`ConnectionState::Online`](alloy_connector::ConnectionState::Online).
//!
//! # Events
//!
//! Events flow to the sender passed to [`Connector::start`] in arrival order. Before `start` is
//! called they are buffered in a 1024-entry queue; when the queue is full the oldest event is
//! dropped with a warning, because a connector that has not been started has no consumer.
//!
//! # Framing limits
//!
//! A line longer than 16 MiB is a protocol violation: the host fails the pending calls with
//! [`ConnectorError::Protocol`] and replaces the process. Requests time out after
//! [`SidecarConfig::request_timeout`]; the timeout is reported as a [`ConnectorError::Network`]
//! failure and the process is left running, so a sidecar that is merely slow is not killed.

use std::path::Path;
use std::sync::Arc;

use alloy_connector::{
    Capabilities, Connector, ConnectorError, ConnectorEvent, LoginFailure, LoginInput, LoginStep,
    MediaRef, Network, OutgoingMessage, Page, RemoteChat, RemoteMessage, RemoteUser, SendAck,
    SessionBlob,
};
use async_trait::async_trait;
use serde_json::json;
use tokio::sync::mpsc::Sender;

mod config;
mod process;
mod runtime;

pub use config::SidecarConfig;

use runtime::Runtime;

/// A [`Connector`] that supervises an external executable speaking the connector protocol.
pub struct SidecarConnector {
    runtime: Arc<Runtime>,
}

impl SidecarConnector {
    /// Starts the executable and performs the `hello` handshake.
    ///
    /// Fails with [`ConnectorError::Protocol`] when the sidecar announces a different protocol
    /// version or does not send `hello` first; such a process is terminated instead of used. The
    /// network and capabilities reported here back [`Connector::network`] and
    /// [`Connector::capabilities`].
    pub async fn spawn(config: SidecarConfig) -> Result<Self, ConnectorError> {
        Ok(Self {
            runtime: Runtime::spawn(config).await?,
        })
    }
}

#[async_trait]
impl Connector for SidecarConnector {
    fn network(&self) -> Network {
        self.runtime.identity.network
    }

    fn capabilities(&self) -> Capabilities {
        self.runtime.identity.capabilities.clone()
    }

    async fn begin_login(&self) -> LoginStep {
        match self.runtime.call("begin_login", json!({})).await {
            Ok(step) => step,
            Err(error) => LoginStep::Failed {
                reason: login_failure(error),
            },
        }
    }

    async fn submit_login(&self, input: LoginInput) -> LoginStep {
        let params = json!({ "input": input });
        match self.runtime.call("submit_login", params).await {
            Ok(step) => {
                if let LoginStep::Done { session, .. } = &step {
                    self.runtime.remember_session(session.clone()).await;
                }
                step
            }
            Err(error) => LoginStep::Failed {
                reason: login_failure(error),
            },
        }
    }

    async fn restore(&self, session: SessionBlob) -> Result<RemoteUser, ConnectorError> {
        let params = json!({ "session": session.clone() });
        let user = self.runtime.call("restore", params).await?;
        self.runtime.remember_session(session).await;
        Ok(user)
    }

    async fn start(&self, events: Sender<ConnectorEvent>) -> Result<(), ConnectorError> {
        self.runtime.start(events).await
    }

    async fn stop(&self) -> Result<(), ConnectorError> {
        self.runtime.stop().await
    }

    async fn list_chats(
        &self,
        cursor: Option<String>,
        limit: usize,
    ) -> Result<Page<RemoteChat>, ConnectorError> {
        let params = json!({ "cursor": cursor, "limit": limit });
        self.runtime.call("list_chats", params).await
    }

    async fn backfill(
        &self,
        remote_chat_id: &str,
        before: Option<String>,
        limit: usize,
    ) -> Result<Page<RemoteMessage>, ConnectorError> {
        let params = json!({
            "remote_chat_id": remote_chat_id,
            "before": before,
            "limit": limit,
        });
        self.runtime.call("backfill", params).await
    }

    async fn send(&self, message: OutgoingMessage) -> Result<SendAck, ConnectorError> {
        let params = json!({ "message": message });
        self.runtime.call("send", params).await
    }

    async fn edit(
        &self,
        remote_chat_id: &str,
        remote_msg_id: &str,
        body: String,
        formatted: Option<String>,
    ) -> Result<(), ConnectorError> {
        let params = json!({
            "remote_chat_id": remote_chat_id,
            "remote_msg_id": remote_msg_id,
            "body": body,
            "formatted": formatted,
        });
        self.runtime.call_unit("edit", params).await
    }

    async fn delete(
        &self,
        remote_chat_id: &str,
        remote_msg_id: &str,
        for_everyone: bool,
    ) -> Result<(), ConnectorError> {
        let params = json!({
            "remote_chat_id": remote_chat_id,
            "remote_msg_id": remote_msg_id,
            "for_everyone": for_everyone,
        });
        self.runtime.call_unit("delete", params).await
    }

    async fn react(
        &self,
        remote_chat_id: &str,
        remote_msg_id: &str,
        emoji: &str,
    ) -> Result<(), ConnectorError> {
        let params = json!({
            "remote_chat_id": remote_chat_id,
            "remote_msg_id": remote_msg_id,
            "emoji": emoji,
        });
        self.runtime.call_unit("react", params).await
    }

    async fn unreact(
        &self,
        remote_chat_id: &str,
        remote_msg_id: &str,
        emoji: &str,
    ) -> Result<(), ConnectorError> {
        let params = json!({
            "remote_chat_id": remote_chat_id,
            "remote_msg_id": remote_msg_id,
            "emoji": emoji,
        });
        self.runtime.call_unit("unreact", params).await
    }

    async fn mark_read(
        &self,
        remote_chat_id: &str,
        up_to_remote_msg_id: &str,
    ) -> Result<(), ConnectorError> {
        let params = json!({
            "remote_chat_id": remote_chat_id,
            "up_to_remote_msg_id": up_to_remote_msg_id,
        });
        self.runtime.call_unit("mark_read", params).await
    }

    async fn set_typing(&self, remote_chat_id: &str, typing: bool) -> Result<(), ConnectorError> {
        let params = json!({ "remote_chat_id": remote_chat_id, "typing": typing });
        self.runtime.call_unit("set_typing", params).await
    }

    async fn fetch_media(&self, media: &MediaRef, dest_path: &Path) -> Result<u64, ConnectorError> {
        let dest_path = dest_path
            .to_str()
            .ok_or_else(|| ConnectorError::InvalidInput {
                message: format!(
                    "the media destination is not a UTF-8 path: {}",
                    dest_path.display()
                ),
            })?;
        let params = json!({ "media": media, "dest_path": dest_path });
        self.runtime.call("fetch_media", params).await
    }

    async fn logout(&self) -> Result<(), ConnectorError> {
        self.runtime.call_unit("logout", json!({})).await
    }
}

/// Turns a host-side transport failure into the `Failed` step login callers expect.
///
/// Login never returns `Err`: the trait's contract is a total state machine, so a sidecar that
/// cannot be reached becomes a failure the UI can render and retry.
fn login_failure(error: ConnectorError) -> LoginFailure {
    match error {
        ConnectorError::Unauthorized => LoginFailure::Network {
            message: "the connector reported an unauthorized session".to_string(),
        },
        ConnectorError::RateLimited { retry_after_ms } => {
            LoginFailure::RateLimited { retry_after_ms }
        }
        ConnectorError::NotSupported { .. } => LoginFailure::NotSupported,
        ConnectorError::Network { message } => LoginFailure::Network { message },
        ConnectorError::Protocol { message } => LoginFailure::Network {
            message: format!("connector protocol error: {message}"),
        },
        ConnectorError::InvalidInput { message } => LoginFailure::InvalidInput {
            field: None,
            message,
        },
    }
}
