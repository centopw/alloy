//! The trait every network connector implements, and the values its methods carry.
//!
//! The trait is object-safe and `Send + Sync`: the engine holds one `Arc<dyn Connector>` per
//! account and drives it from a single actor task, so implementations use interior mutability.
//! Every value that crosses the trait has a serializable form, which is what lets the same
//! implementation run in-process (Telegram, M2), behind the NDJSON sidecar bridge (Messenger, M3),
//! or inside the M6 gateway without a second trait.

use std::path::Path;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::Sender;

use crate::capabilities::Capabilities;
use crate::error::ConnectorError;
use crate::events::ConnectorEvent;
use crate::login::{LoginInput, LoginStep};
use crate::types::{MediaRef, Network, RemoteChat, RemoteMessage, RemoteUser, SessionBlob};

/// One page of a paged call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Page<T> {
    /// Items in connector-defined order (see the method that produced the page).
    pub items: Vec<T>,
    /// Opaque cursor for the next page, or `None` when the list is exhausted.
    pub next_cursor: Option<String>,
}

/// A file on the local device that the connector must upload with the message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalAttachment {
    /// Absolute path on the device running the connector. Never sent to the network.
    pub path: String,
    /// MIME type when the host already knows it; the connector may sniff otherwise.
    pub mime: Option<String>,
    /// File name to present to recipients; defaults to the path's file name.
    pub file_name: Option<String>,
}

/// A message the host asks the connector to send.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutgoingMessage {
    /// Client-generated UUID. The remote echo must repeat it so the outbox can reconcile.
    pub txn_id: String,
    /// Target chat.
    pub remote_chat_id: String,
    /// Plain-text body. `None` only when `attachment` is present.
    pub body: Option<String>,
    /// Formatted body, when the target supports it (`Capabilities::formatted_text`).
    pub formatted: Option<String>,
    /// Remote message id to reply to.
    pub reply_to: Option<String>,
    /// Media to upload with the message.
    pub attachment: Option<LocalAttachment>,
}

/// What the network returned for an accepted send.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SendAck {
    /// Echo of [`OutgoingMessage::txn_id`]; lets the host match the ack to its outbox row.
    pub txn_id: String,
    /// Remote message id assigned by the network.
    pub remote_msg_id: String,
    /// Network timestamp of the accepted message, milliseconds since the Unix epoch.
    pub sent_at: i64,
}

/// A connected network account.
///
/// # Contract
///
/// - All remote ids are opaque strings; the connector never reinterprets an id it received.
/// - A connector never touches the local database or keyring. It returns [`SessionBlob`] values
///   and the host stores them.
/// - Login is a total state machine: [`begin_login`](Self::begin_login) and
///   [`submit_login`](Self::submit_login) always return a step, and failures are
///   [`LoginStep::Failed`] steps rather than `Err`. Every `Failed` step is terminal; the host
///   restarts with `begin_login`.
/// - A method for a capability that is `false` must fail with
///   [`ConnectorError::NotSupported`] naming the capability, not silently do nothing.
/// - After `start`, every remote change the connector observes is reported as a
///   [`ConnectorEvent`], in observation order. Events are never replayed; backfill covers history.
/// - A dead session is [`ConnectorError::Unauthorized`] on data calls, plus
///   [`ConnectionState::NeedsRelogin`](crate::ConnectionState) on the event stream. No automatic
///   retry happens.
#[async_trait]
pub trait Connector: Send + Sync {
    /// Network this connector speaks.
    fn network(&self) -> Network;

    /// What this connector can do on this network. See [`Capabilities`].
    fn capabilities(&self) -> Capabilities;

    /// Starts a login attempt and returns the first step to show.
    ///
    /// The method may contact the network (a Telegram QR token needs a round-trip), so initial
    /// transport failures are returned as [`LoginStep::Failed`]. Any previous attempt is
    /// discarded; a successful session is left untouched until a new login succeeds.
    async fn begin_login(&self) -> LoginStep;

    /// Answers the step the connector last returned and yields the next one.
    ///
    /// The input must match the outstanding step; an out-of-order input yields
    /// [`LoginStep::Failed`] with [`LoginFailure::InvalidInput`](crate::LoginFailure::InvalidInput).
    async fn submit_login(&self, input: LoginInput) -> LoginStep;

    /// Re-establishes a session from a stored blob and returns the account it belongs to.
    ///
    /// Fails with [`ConnectorError::Unauthorized`] when the blob is invalid or expired.
    async fn restore(&self, session: SessionBlob) -> Result<RemoteUser, ConnectorError>;

    /// Begins live updates, sending events to `events`.
    ///
    /// The connector emits [`ConnectionState::Connecting`](crate::ConnectionState::Connecting) and
    /// then [`ConnectionState::Online`](crate::ConnectionState::Online) once the stream is live.
    /// Calling it again replaces the previous sender. Fails with `Unauthorized` when no session is
    /// established.
    async fn start(&self, events: Sender<ConnectorEvent>) -> Result<(), ConnectorError>;

    /// Stops live updates and drops the event sender. The session stays valid; `start` resumes.
    /// Idempotent.
    async fn stop(&self) -> Result<(), ConnectorError>;

    /// Lists chats, newest activity first. Archived chats are included; the host filters.
    ///
    /// `cursor` is the opaque `next_cursor` from the previous page. `limit` must be greater than
    /// zero. Ordering is stable for a given connector state.
    async fn list_chats(
        &self,
        cursor: Option<String>,
        limit: usize,
    ) -> Result<Page<RemoteChat>, ConnectorError>;

    /// Pages history backwards, newest message first.
    ///
    /// `before` is exclusive: only messages strictly older than that remote message id are
    /// returned; `None` starts at the newest message. `Page::next_cursor` is the oldest message id
    /// of the page and is `None` when nothing older remains. Unknown ids fail with
    /// [`ConnectorError::InvalidInput`].
    async fn backfill(
        &self,
        remote_chat_id: &str,
        before: Option<String>,
        limit: usize,
    ) -> Result<Page<RemoteMessage>, ConnectorError>;

    /// Sends a message and returns the network's acknowledgement.
    ///
    /// The returned [`SendAck`] must repeat the caller's `txn_id`, and the echo delivered as
    /// [`ConnectorEvent::Message`] must carry it too. The echo may arrive before or after the ack.
    async fn send(&self, message: OutgoingMessage) -> Result<SendAck, ConnectorError>;

    /// Edits a message this account sent. `formatted` replaces the formatted body when the network
    /// supports it. Fails with `NotSupported` when [`Capabilities::edit`] is false or the edit
    /// window has passed.
    async fn edit(
        &self,
        remote_chat_id: &str,
        remote_msg_id: &str,
        body: String,
        formatted: Option<String>,
    ) -> Result<(), ConnectorError>;

    /// Deletes a message. `for_everyone` asks the network to delete it for all participants; when
    /// [`Capabilities::delete_for_everyone`] is false only a local delete is allowed.
    async fn delete(
        &self,
        remote_chat_id: &str,
        remote_msg_id: &str,
        for_everyone: bool,
    ) -> Result<(), ConnectorError>;

    /// Adds a reaction. Fails with `NotSupported` when [`Capabilities::reactions`] is false, and
    /// with `InvalidInput` when the per-user limit is already reached.
    async fn react(
        &self,
        remote_chat_id: &str,
        remote_msg_id: &str,
        emoji: &str,
    ) -> Result<(), ConnectorError>;

    /// Removes this account's reaction.
    async fn unreact(
        &self,
        remote_chat_id: &str,
        remote_msg_id: &str,
        emoji: &str,
    ) -> Result<(), ConnectorError>;

    /// Marks the chat read up to and including the given remote message id. Idempotent.
    ///
    /// Connectors must actually report this to the network: suppressing read receipts violates
    /// Telegram's API terms (ADR 0016) and leaves recipients without acknowledgement.
    async fn mark_read(
        &self,
        remote_chat_id: &str,
        up_to_remote_msg_id: &str,
    ) -> Result<(), ConnectorError>;

    /// Sends or stops a typing indicator in the chat.
    async fn set_typing(&self, remote_chat_id: &str, typing: bool) -> Result<(), ConnectorError>;

    /// Downloads media to `dest_path` (creating or truncating the file) and returns the number of
    /// bytes written. Filesystem failures, including a missing parent directory, are
    /// [`ConnectorError::InvalidInput`]. Over the RPC bridge the destination crosses as a UTF-8
    /// path string.
    async fn fetch_media(&self, media: &MediaRef, dest_path: &Path) -> Result<u64, ConnectorError>;

    /// Logs out: best-effort remote revocation, then local credential wipe. The connector must
    /// reject data calls with `Unauthorized` afterwards. Idempotent.
    async fn logout(&self) -> Result<(), ConnectorError>;
}
