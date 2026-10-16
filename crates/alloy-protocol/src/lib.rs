use serde::{Deserialize, Serialize};
use specta::{Type, Types};
use specta_typescript::{Number, Typescript};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
pub struct AccountId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
pub struct RoomId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
pub struct EventId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
pub struct UserId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub enum ConnectionStatus {
    Disconnected,
    Connecting,
    Syncing,
    Reconnecting {
        attempt: u32,
        #[specta(type = Number)]
        next_retry_millis: u64,
    },
    Terminated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub enum RoomMembership {
    Joined,
    Invited,
    Left,
    Banned,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct NotificationCount {
    #[specta(type = Number)]
    pub highlight_count: u64,
    #[specta(type = Number)]
    pub notification_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct TimelineItemPreview {
    pub sender: UserId,
    #[specta(type = Number)]
    pub timestamp_millis: u64,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct RoomSummary {
    pub id: RoomId,
    pub name: Option<String>,
    pub avatar_url: Option<String>,
    pub unread_notifications: NotificationCount,
    pub latest_event: Option<TimelineItemPreview>,
    pub is_direct: bool,
    pub is_space: bool,
    pub membership: RoomMembership,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", content = "data")]
pub enum TimelineItemContent {
    TextMessage {
        body: String,
        formatted_body: Option<String>,
    },
    Media {
        uri: String,
        mime_type: String,
        #[specta(type = Number)]
        size_bytes: u64,
        thumbnail_uri: Option<String>,
    },
    Reaction {
        key: String,
        #[specta(type = Number)]
        count: usize,
        senders: Vec<UserId>,
    },
    Poll {
        question: String,
        answers: Vec<String>,
        #[specta(type = Option<Number>)]
        end_time: Option<u64>,
    },
    CallInvite {
        call_id: String,
        is_video: bool,
    },
    Redacted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "status", content = "data")]
pub enum EventSendState {
    Sending { transaction_id: String },
    Sent { event_id: EventId },
    Failed { error: String, can_retry: bool },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct ThreadSummary {
    pub root_event_id: EventId,
    pub reply_count: u32,
    #[specta(type = Number)]
    latest_reply_timestamp: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct TimelineItemDto {
    pub internal_id: String,
    pub event_id: Option<EventId>,
    pub sender: UserId,
    #[specta(type = Number)]
    pub timestamp_millis: u64,
    pub content: TimelineItemContent,
    pub send_state: EventSendState,
    pub is_own: bool,
    pub thread_summary: Option<ThreadSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "type", content = "content")]
pub enum VectorDiffDto<T> {
    Append {
        values: Vec<T>,
    },
    Clear,
    PushFront {
        value: T,
    },
    PushBack {
        value: T,
    },
    PopFront,
    PopBack,
    Insert {
        #[specta(type = Number)]
        index: usize,
        value: T,
    },
    Set {
        #[specta(type = Number)]
        index: usize,
        value: T,
    },
    Remove {
        #[specta(type = Number)]
        index: usize,
    },
    Truncate {
        #[specta(type = Number)]
        length: usize,
    },
    Reset {
        values: Vec<T>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "action", content = "params")]
pub enum EngineCommand {
    LoginOidc {
        homeserver_url: String,
    },
    CompleteOidcFlow {
        redirect_url: String,
    },
    LoginPassword {
        homeserver_url: String,
        username: String,
        password: String,
    },
    Logout {
        account_id: AccountId,
    },
    JoinRoom {
        room_id_or_alias: String,
    },
    LeaveRoom {
        room_id: RoomId,
    },
    SelectRoom {
        room_id: RoomId,
    },
    PaginateBackwards {
        room_id: RoomId,
        #[specta(type = Number)]
        count: usize,
    },
    SendMessage {
        room_id: RoomId,
        content: String,
        reply_to: Option<EventId>,
    },
    SendMedia {
        room_id: RoomId,
        file_path: String,
        mime_type: String,
    },
    SendReaction {
        room_id: RoomId,
        event_id: EventId,
        reaction_key: String,
    },
    CancelPendingMessage {
        room_id: RoomId,
        transaction_id: String,
    },
    VerifyDeviceSasAccept {
        user_id: UserId,
        device_id: String,
    },
    ConfirmSasMatch {
        flow_id: String,
    },
    RestoreKeyBackup {
        recovery_key: String,
    },
    JoinCall {
        room_id: RoomId,
        is_video: bool,
    },
    LeaveCall {
        room_id: RoomId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "type", content = "payload")]
pub enum EngineEvent {
    ConnectionStatusChanged(ConnectionStatus),
    RoomListDiff(VectorDiffDto<RoomSummary>),
    TimelineDiff {
        room_id: RoomId,
        diff: VectorDiffDto<TimelineItemDto>,
    },
    TypingNotice {
        room_id: RoomId,
        users: Vec<UserId>,
    },
    Error {
        message: String,
    },
}

pub fn register_types() -> Types {
    Types::default()
        .register::<AccountId>()
        .register::<RoomId>()
        .register::<EventId>()
        .register::<UserId>()
        .register::<ConnectionStatus>()
        .register::<RoomMembership>()
        .register::<NotificationCount>()
        .register::<TimelineItemPreview>()
        .register::<RoomSummary>()
        .register::<TimelineItemContent>()
        .register::<EventSendState>()
        .register::<ThreadSummary>()
        .register::<TimelineItemDto>()
        .register::<VectorDiffDto<RoomSummary>>()
        .register::<VectorDiffDto<TimelineItemDto>>()
        .register::<EngineCommand>()
        .register::<EngineEvent>()
}

pub fn export_typescript() -> Result<String, specta_typescript::Error> {
    let types = register_types();
    Typescript::default().export(&types, specta_serde::Format)
}

pub fn export_typescript_to_file(
    path: impl AsRef<std::path::Path>,
) -> Result<(), specta_typescript::Error> {
    let types = register_types();
    Typescript::default().export_to(path, &types, specta_serde::Format)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_export_typescript() {
        let ts = export_typescript().expect("export typescript should succeed");
        assert!(ts.contains("export type RoomSummary"));
        assert!(ts.contains("export type TimelineItemDto"));
        assert!(ts.contains("export type EngineCommand"));
        assert!(ts.contains("export type EngineEvent"));
        assert!(ts.contains("export type VectorDiffDto"));
    }

    #[test]
    fn test_serde_roundtrip() {
        let summary = RoomSummary {
            id: RoomId("!test:example.org".into()),
            name: Some("Test Room".into()),
            avatar_url: None,
            unread_notifications: NotificationCount {
                highlight_count: 1,
                notification_count: 2,
            },
            latest_event: Some(TimelineItemPreview {
                sender: UserId("@alice:example.org".into()),
                timestamp_millis: 1234567890,
                body: "Hello world".into(),
            }),
            is_direct: false,
            is_space: false,
            membership: RoomMembership::Joined,
        };
        let json = serde_json::to_string(&summary).unwrap();
        let decoded: RoomSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(summary, decoded);

        let diff = VectorDiffDto::PushBack { value: summary };
        let diff_json = serde_json::to_string(&diff).unwrap();
        let decoded_diff: VectorDiffDto<RoomSummary> = serde_json::from_str(&diff_json).unwrap();
        assert_eq!(diff, decoded_diff);
    }

    #[test]
    fn test_export_to_ui_generated() {
        let out_dir = std::path::Path::new("../../apps/alloy/ui/src/lib/generated");
        if !out_dir.exists() {
            let _ = std::fs::create_dir_all(out_dir);
        }
        let target = out_dir.join("protocol.ts");
        let _ = export_typescript_to_file(&target);
    }
}
