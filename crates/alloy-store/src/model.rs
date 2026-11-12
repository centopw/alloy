//! Records and closed vocabularies exchanged by the store and its callers.
//!
//! Every enum mirrors a `CHECK` constraint in the schema and serialises to the same lowercase
//! string used on the wire, so records can be embedded in `event_log` payloads without a second
//! mapping layer.

use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::StoreError;

macro_rules! text_enum {
    ($name:ident { $($variant:ident => $text:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum $name {
            $($variant),+
        }

        impl $name {
            pub const fn as_str(&self) -> &'static str {
                match self {
                    $(Self::$variant => $text),+
                }
            }
        }

        impl FromStr for $name {
            type Err = StoreError;

            fn from_str(text: &str) -> Result<Self, Self::Err> {
                match text {
                    $($text => Ok(Self::$variant),)+
                    other => Err(StoreError::InvalidValue {
                        field: stringify!($name),
                        value: other.to_owned(),
                    }),
                }
            }
        }

        impl rusqlite::types::ToSql for $name {
            fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
                Ok(rusqlite::types::ToSqlOutput::from(self.as_str()))
            }
        }

        impl rusqlite::types::FromSql for $name {
            fn column_result(
                value: rusqlite::types::ValueRef<'_>,
            ) -> rusqlite::types::FromSqlResult<Self> {
                let text = value.as_str()?;
                text.parse().map_err(|err: StoreError| {
                    rusqlite::types::FromSqlError::Other(Box::new(err))
                })
            }
        }
    };
}

text_enum!(Network {
    Telegram => "telegram",
    Messenger => "messenger",
    Fake => "fake",
});

text_enum!(AccountState {
    Pending => "pending",
    Active => "active",
    NeedsRelogin => "needs_relogin",
    Disabled => "disabled",
});

text_enum!(ChatKind {
    Direct => "direct",
    Group => "group",
    Channel => "channel",
});

text_enum!(MessageKind {
    Text => "text",
    Image => "image",
    Video => "video",
    Audio => "audio",
    Voice => "voice",
    File => "file",
    Sticker => "sticker",
    Location => "location",
    System => "system",
});

text_enum!(SendState {
    Queued => "queued",
    Sending => "sending",
    Sent => "sent",
    Delivered => "delivered",
    Read => "read",
    Failed => "failed",
});

text_enum!(OutboxOp {
    Send => "send",
    Edit => "edit",
    Delete => "delete",
    React => "react",
    Unreact => "unreact",
    Read => "read",
    Typing => "typing",
});

text_enum!(OutboxState {
    Pending => "pending",
    InFlight => "in_flight",
    Done => "done",
    Failed => "failed",
});

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountRecord {
    pub account_id: String,
    pub network: Network,
    pub remote_user_id: Option<String>,
    pub display_name: Option<String>,
    pub avatar_media_id: Option<String>,
    pub state: AccountState,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Fields a caller supplies when linking a new account; `account_id` and timestamps are local.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewAccount {
    pub network: Network,
    pub remote_user_id: Option<String>,
    pub display_name: Option<String>,
    pub state: AccountState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatRecord {
    pub chat_id: String,
    pub account_id: String,
    pub remote_chat_id: String,
    pub kind: ChatKind,
    pub title: Option<String>,
    pub avatar_media_id: Option<String>,
    pub last_message_id: Option<String>,
    pub last_activity_at: Option<i64>,
    pub unread_count: i64,
    pub mention_count: i64,
    pub is_pinned: bool,
    pub is_archived: bool,
    pub is_muted: bool,
    pub muted_until: Option<i64>,
    pub is_marked_unread: bool,
    pub draft: Option<String>,
    pub local_tags: Vec<String>,
    pub note: Option<String>,
}

/// Remote chat fields that sync may refresh. User-owned flags (pin, archive, draft, ...) are never
/// touched by an upsert.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatUpsert {
    pub account_id: String,
    pub remote_chat_id: String,
    pub kind: ChatKind,
    pub title: Option<String>,
    pub avatar_media_id: Option<String>,
    pub last_message_id: Option<String>,
    pub last_activity_at: Option<i64>,
}

/// User-owned chat state. `None` keeps the stored value; `Some(None)` clears a nullable column.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChatFlagsUpdate {
    pub is_pinned: Option<bool>,
    pub is_archived: Option<bool>,
    /// `Some(None)` unmutes, `Some(Some(ts))` mutes until `ts` (use `i64::MAX` for "forever").
    /// `is_muted` follows this field.
    pub muted_until: Option<Option<i64>>,
    pub is_marked_unread: Option<bool>,
    pub draft: Option<Option<String>>,
    pub tags: Option<Vec<String>>,
    pub note: Option<Option<String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MessageRecord {
    pub message_id: String,
    pub chat_id: String,
    pub remote_msg_id: Option<String>,
    pub txn_id: Option<String>,
    pub sender_remote_id: String,
    pub is_own: bool,
    pub sent_at: i64,
    pub edited_at: Option<i64>,
    pub deleted_at: Option<i64>,
    pub kind: MessageKind,
    pub body: Option<String>,
    pub formatted: Option<String>,
    pub reply_to_message_id: Option<String>,
    pub media_id: Option<String>,
    pub send_state: Option<SendState>,
    pub extra: Option<serde_json::Value>,
}

/// Message fields for insert-or-update. Identity resolution order: `message_id`, then `txn_id`
/// (reconciles a local queued row with its remote echo), then `(chat_id, remote_msg_id)`.
#[derive(Debug, Clone, PartialEq)]
pub struct NewMessage {
    pub chat_id: String,
    /// Local id; generated when absent.
    pub message_id: Option<String>,
    pub remote_msg_id: Option<String>,
    /// Client-generated id shared by the outbox row and the local placeholder.
    pub txn_id: Option<String>,
    pub sender_remote_id: String,
    pub is_own: bool,
    pub sent_at: i64,
    pub edited_at: Option<i64>,
    pub kind: MessageKind,
    pub body: Option<String>,
    pub formatted: Option<String>,
    pub reply_to_message_id: Option<String>,
    pub media_id: Option<String>,
    pub send_state: Option<SendState>,
    pub extra: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutboxItem {
    pub txn_id: String,
    pub account_id: String,
    pub chat_id: String,
    pub op: OutboxOp,
    pub payload: serde_json::Value,
    pub state: OutboxState,
    pub attempts: i64,
    pub next_attempt_at: i64,
    pub last_error: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewOutboxItem {
    /// Client-generated UUID; generated when absent.
    pub txn_id: Option<String>,
    pub account_id: String,
    pub chat_id: String,
    pub op: OutboxOp,
    pub payload: serde_json::Value,
    pub next_attempt_at: i64,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventRecord {
    pub seq: i64,
    pub account_id: String,
    pub chat_id: Option<String>,
    pub kind: String,
    pub payload: serde_json::Value,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewEvent {
    pub account_id: String,
    pub chat_id: Option<String>,
    pub kind: String,
    pub payload: serde_json::Value,
}

pub(crate) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_names_round_trip() {
        for network in [Network::Telegram, Network::Messenger, Network::Fake] {
            assert_eq!(network.as_str().parse::<Network>().unwrap(), network);
        }
        assert_eq!(
            "needs_relogin".parse::<AccountState>().unwrap(),
            AccountState::NeedsRelogin
        );
        assert_eq!(
            "in_flight".parse::<OutboxState>().unwrap(),
            OutboxState::InFlight
        );
        assert!(matches!(
            "unknown".parse::<MessageKind>(),
            Err(StoreError::InvalidValue { .. })
        ));
    }
}
