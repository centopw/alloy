//! Domain types that cross the connector contract.
//!
//! Every type here serializes, because the same values travel in-process (Telegram, M2), over
//! NDJSON stdio (Messenger sidecar, M3) and over WSS (gateway, M6). Ids are opaque strings taken
//! verbatim from the network; timestamps are milliseconds since the Unix epoch, UTC.

use std::fmt;

use base64::Engine as _;
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A network Alloy can connect a personal account to.
///
/// Names are the wire and SQLite spelling: `"telegram"`, `"messenger"`. `"fake"` exists for the
/// deterministic test connector only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Network {
    Telegram,
    Messenger,
    Fake,
}

impl Network {
    /// The lowercase wire/DB name. Matches the serde representation.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Telegram => "telegram",
            Self::Messenger => "messenger",
            Self::Fake => "fake",
        }
    }
}

impl fmt::Display for Network {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Kind of a remote chat, in network-neutral terms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatKind {
    Direct,
    Group,
    Channel,
}

/// Kind of a remote message body. The body itself stays in [`RemoteMessage::body`]; this only
/// classifies it for rendering and for deciding which media path to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageKind {
    Text,
    Image,
    Video,
    Audio,
    Voice,
    File,
    Sticker,
    Location,
    System,
}

/// An opaque pointer to media on the network, plus the metadata the UI needs before downloading.
///
/// `remote_ref` is meaningful only to the connector that produced it. The host stores it verbatim
/// and passes it back to [`crate::Connector::fetch_media`]; it must never be interpreted, parsed or
/// concatenated. `avatar` uses the same type for chat and user pictures.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaRef {
    /// Opaque network-side reference (Telegram file id, Messenger attachment URL id, ...).
    pub remote_ref: String,
    /// MIME type as reported by the network, e.g. `"image/jpeg"`.
    pub mime: String,
    /// Size in bytes when the network reports it before download.
    pub size_bytes: Option<u64>,
    /// Pixel width for visual media.
    pub width: Option<u32>,
    /// Pixel height for visual media.
    pub height: Option<u32>,
    /// Duration for audio, voice and video media.
    pub duration_ms: Option<u64>,
    /// Suggested file name for downloads, without any path.
    pub file_name: Option<String>,
}

/// A chat (conversation, group or channel) as the network describes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteChat {
    /// Opaque network chat id, stored verbatim.
    pub remote_chat_id: String,
    pub kind: ChatKind,
    /// Display title. For direct chats this is the other participant's display name.
    pub title: String,
    /// Chat picture, absent when the network has none or it cannot be fetched on this device.
    pub avatar: Option<MediaRef>,
    /// Milliseconds since the Unix epoch of the newest activity the connector has seen.
    pub last_activity_at: i64,
    /// Unread count as the network reports it; Telegram counts notifications, not messages.
    pub unread_count: u32,
    pub is_pinned: bool,
    pub is_archived: bool,
    pub is_muted: bool,
}

/// A participant in a remote chat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteUser {
    /// Opaque network user id, stored verbatim.
    pub remote_user_id: String,
    pub display_name: String,
    pub avatar: Option<MediaRef>,
}

/// A message as the network describes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteMessage {
    /// Chat the message belongs to.
    pub remote_chat_id: String,
    /// Opaque network message id, stored verbatim.
    pub remote_msg_id: String,
    /// Present only on echoes of messages this client sent, so the outbox can reconcile the
    /// remote echo with the locally queued row. Absent for messages from other participants.
    pub txn_id: Option<String>,
    /// Opaque id of the sender.
    pub sender_remote_id: String,
    /// Whether the message was authored by the account this connector is logged in as.
    pub is_own: bool,
    /// Milliseconds since the Unix epoch, as the network reports it.
    pub sent_at: i64,
    /// Milliseconds since the Unix epoch of the last edit, absent when never edited.
    pub edited_at: Option<i64>,
    pub kind: MessageKind,
    /// Plain-text body. Absent for media-only messages.
    pub body: Option<String>,
    /// Network-provided markup (HTML subset, Markdown) when the network supplies it.
    pub formatted: Option<String>,
    /// Remote message id this message replies to, when it is a reply.
    pub reply_to_remote_msg_id: Option<String>,
    /// Attached media, when the message carries any.
    pub media: Option<MediaRef>,
}

/// Opaque credential material produced by a connector and persisted by the host.
///
/// The bytes mean whatever the connector decides (Telegram session, Messenger cookies and device
/// ids, a sidecar-specific token). The host never inspects them: it encrypts them at rest, hands
/// them back through [`crate::Connector::restore`], and replaces them when
/// [`crate::ConnectorEvent::SessionUpdated`] arrives.
///
/// On the wire the blob is a base64 string (standard alphabet, padded). `Debug` prints only the
/// byte count, because the content is a credential.
#[derive(Clone, PartialEq, Eq)]
pub struct SessionBlob(pub Vec<u8>);

impl SessionBlob {
    /// Wraps raw connector session bytes.
    pub fn new(bytes: impl Into<Vec<u8>>) -> Self {
        Self(bytes.into())
    }

    /// Borrows the raw bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Consumes the blob and returns the raw bytes.
    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }

    /// Length in bytes, the only thing `Debug` exposes.
    pub fn byte_len(&self) -> usize {
        self.0.len()
    }
}

impl fmt::Debug for SessionBlob {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SessionBlob(<{} bytes redacted>)", self.0.len())
    }
}

impl Serialize for SessionBlob {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&base64::engine::general_purpose::STANDARD.encode(&self.0))
    }
}

impl<'de> Deserialize<'de> for SessionBlob {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct Base64Visitor;

        impl Visitor<'_> for Base64Visitor {
            type Value = SessionBlob;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a base64-encoded session blob")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                base64::engine::general_purpose::STANDARD
                    .decode(value)
                    .map(SessionBlob)
                    .map_err(|error| {
                        <E as de::Error>::custom(format!("invalid base64 session blob: {error}"))
                    })
            }
        }

        deserializer.deserialize_str(Base64Visitor)
    }
}
