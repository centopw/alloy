//! What a connector can actually do on its network.
//!
//! Capabilities are declared per network, never inferred from a shared default: a connector that
//! does not implement polls reports `polls: false` instead of inheriting a default that pretends
//! support. The UI must hide an action rather than offer one that fails. A method a connector does
//! not implement must fail with [`crate::ConnectorError::NotSupported`] naming the capability.

use serde::{Deserialize, Serialize};

/// The feature set of one connector instance.
///
/// A `true` flag promises the matching [`crate::Connector`] method works; `false` promises it
/// returns `NotSupported` (or, for optional limits, that the field is `None`). Limits describe the
/// network, not the local client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    /// The network allows editing sent messages.
    pub edit: bool,
    /// How long after sending an edit is still accepted, in seconds. `None` with `edit: true`
    /// means no window the connector is aware of.
    pub edit_window_secs: Option<u32>,
    /// The network allows deleting a message for every participant.
    pub delete_for_everyone: bool,
    /// The network supports reactions.
    pub reactions: bool,
    /// Maximum number of distinct reactions one user may add per message.
    pub max_reactions_per_user: Option<u8>,
    /// The network supports threaded replies to a specific message.
    pub replies: bool,
    /// The network exposes message threads.
    pub threads: bool,
    /// The connector can send typing indicators.
    pub typing: bool,
    /// The network reports read receipts from other participants.
    pub read_receipts: bool,
    /// The network reports delivery receipts from other participants.
    pub delivered_receipts: bool,
    /// The connector can send and receive voice notes.
    pub voice: bool,
    /// The network supports polls.
    pub polls: bool,
    /// The network supports shared locations.
    pub location: bool,
    /// Largest attachment the network accepts, in bytes. `None` when unknown.
    pub max_upload_bytes: Option<u64>,
    /// The network renders formatted text (markup in [`crate::RemoteMessage::formatted`]).
    pub formatted_text: bool,
}
