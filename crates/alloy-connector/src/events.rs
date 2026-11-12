//! Events a connector pushes to its host after [`crate::Connector::start`].
//!
//! The event stream is the only way remote state changes reach the client; there is no polling.
//! The host applies each event to the local store, in order, and re-derives what the UI shows.

use serde::{Deserialize, Serialize};

use crate::types::{RemoteChat, RemoteMessage, SessionBlob};

/// Live connection state of a connector.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConnectionState {
    /// Connector is establishing the session.
    Connecting,
    /// Updates are flowing.
    Online,
    /// Live updates stopped; the connector is retrying on its own. `reason` is shown to the user.
    Offline { reason: String },
    /// The stored session is dead. No automatic retry happens; the host must run login again.
    NeedsRelogin,
}

/// One remote change, in the order the connector observed it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
// Events are sent whole through the channel and across the sidecar wire; boxing the large
// variants would only add pointer chasing to the consumer.
#[allow(clippy::large_enum_variant)]
pub enum ConnectorEvent {
    /// Connection state changed.
    Connection { state: ConnectionState },
    /// A chat appeared or its metadata changed (title, avatar, unread count, pin, mute, archive).
    ChatUpserted { chat: RemoteChat },
    /// A chat disappeared (left, deleted, or no longer visible to the account).
    ChatRemoved { remote_chat_id: String },
    /// A new message. Echoes of locally sent messages carry the same `txn_id` as the
    /// [`crate::OutgoingMessage`] they answer.
    Message { message: RemoteMessage },
    /// A known message was edited. The full body replaces the stored one; `edited_at` moves.
    MessageEdited {
        remote_chat_id: String,
        remote_msg_id: String,
        body: Option<String>,
        formatted: Option<String>,
        edited_at: i64,
    },
    /// A message was deleted, locally or for everyone.
    MessageDeleted {
        remote_chat_id: String,
        remote_msg_id: String,
    },
    /// A participant added or removed a reaction.
    Reaction {
        remote_chat_id: String,
        remote_msg_id: String,
        sender_remote_id: String,
        emoji: String,
        added: bool,
    },
    /// A participant read the chat up to a given message.
    ReadMarker {
        remote_chat_id: String,
        remote_user_id: String,
        up_to_remote_msg_id: String,
    },
    /// A participant started or stopped typing.
    Typing {
        remote_chat_id: String,
        remote_user_id: String,
        typing: bool,
    },
    /// The connector rotated its own credentials. The host must persist this blob, replacing the
    /// previous one, or the next launch will need a fresh login.
    SessionUpdated { session: SessionBlob },
}
