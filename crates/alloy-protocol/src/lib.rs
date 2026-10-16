use serde::{Deserialize, Serialize};
use specta::{Type, Types};
use specta_typescript::{Number, Typescript};

// === Core Identifiers ===

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
pub struct AccountId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
pub struct RoomId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
pub struct EventId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
pub struct UserId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
pub struct SubscriptionId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub enum ConnectionStatus {
    Disconnected,
    Connecting,
    Syncing,
    Connected,
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
    pub encrypted: bool,
    pub membership: RoomMembership,
    pub favourite: Option<bool>,
    pub low_priority: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", content = "data")]
pub enum TimelineItemContent {
    TextMessage {
        body: String,
        formatted_body: Option<String>,
        html_sanitized: bool,
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
    RoomState {
        state_type: String,
        content_json: String,
    },
    DayDivider {
        #[specta(type = Number)]
        timestamp_millis: u64,
    },
    Virtual,
    UnableToDecrypt {
        reason: String,
    },
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
    pub latest_reply_timestamp: u64,
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
    pub reply_to: Option<Box<TimelineItemDto>>,
    pub thread_summary: Option<ThreadSummary>,
    pub read_receipts: Option<Vec<UserId>>,
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

// === Authentication ===

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct AuthSession {
    pub account_id: AccountId,
    pub user_id: UserId,
    pub homeserver_url: String,
    pub access_token: String,
    pub device_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Account {
    pub id: AccountId,
    pub user_id: UserId,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub homeserver_url: String,
}

// === Verification ===

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct SasEmoji {
    pub emoji: String,
    pub decimal: u32,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub enum VerificationState {
    Ready,
    WaitingForPartner,
    CompareEmojis {
        emojis: Vec<SasEmoji>,
        decimal_1: u32,
        decimal_2: u32,
        decimal_3: u32,
    },
    Verified,
    Cancelled {
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub enum RecoveryState {
    Unknown,
    Enabled,
    Disabled,
    Lost,
}

// === Media ===

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", content = "data")]
pub enum MediaSource {
    Remote { uri: String },
    Local { file_path: String },
}

// === Commands ===

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "action", content = "params")]
pub enum EngineCommand {
    // Auth
    DiscoverHomeserver {
        input: String,
    },
    LoginPassword {
        homeserver_url: String,
        username: String,
        password: String,
    },
    StartOidcLogin {
        homeserver_url: String,
    },
    CompleteOidcLogin {
        redirect_url: String,
    },
    RestoreSession {
        account_id: AccountId,
    },
    Logout {
        account_id: AccountId,
    },
    ListAccounts,
    // Rooms
    SubscribeRoomList {
        filter: Option<String>,
    },
    Unsubscribe {
        subscription_id: SubscriptionId,
    },
    JoinRoom {
        room_id_or_alias: String,
    },
    LeaveRoom {
        room_id: RoomId,
    },
    CreateRoom {
        name: String,
        encrypted: bool,
        invite: Option<Vec<UserId>>,
    },
    // Timeline
    SubscribeTimeline {
        room_id: RoomId,
    },
    PaginateBackwards {
        room_id: RoomId,
        #[specta(type = Number)]
        count: usize,
    },
    SendText {
        room_id: RoomId,
        body: String,
        markdown: Option<bool>,
        reply_to: Option<EventId>,
        mentions: Option<Vec<UserId>>,
        txn_id: Option<String>,
    },
    EditMessage {
        room_id: RoomId,
        event_id: EventId,
        body: String,
        markdown: Option<bool>,
    },
    RedactEvent {
        room_id: RoomId,
        event_id: EventId,
        reason: Option<String>,
    },
    ToggleReaction {
        room_id: RoomId,
        event_id: EventId,
        key: String,
    },
    RetrySend {
        room_id: RoomId,
        txn_id: String,
    },
    CancelSend {
        room_id: RoomId,
        txn_id: String,
    },
    SendAttachment {
        room_id: RoomId,
        file_path: String,
        mime_type: Option<String>,
        txn_id: Option<String>,
    },
    MarkRead {
        room_id: RoomId,
        event_id: EventId,
    },
    SetTyping {
        room_id: RoomId,
        typing: bool,
    },
    // Media
    GetMedia {
        source: MediaSource,
        kind: String, // "thumbnail" | "file"
        width: Option<u32>,
        height: Option<u32>,
    },
    // Crypto
    RequestSelfVerification,
    AcceptVerification {
        user_id: UserId,
        device_id: String,
    },
    StartSas {
        user_id: UserId,
        device_id: String,
    },
    ConfirmSas {
        user_id: UserId,
        device_id: String,
    },
    CancelVerification {
        user_id: UserId,
        device_id: String,
    },
    GetRecoveryStatus,
    EnableRecovery,
    RecoverWithKey {
        recovery_key: String,
    },
    ResetRecovery,
    // Spaces
    SubscribeSpaces,
    SubscribeSpaceChildren {
        space_id: RoomId,
    },
    // Settings / Theme
    ListThemes,
    GetTheme {
        id: String,
    },
    SetActiveTheme {
        id: String,
    },
    GetSettings,
    SetSetting {
        key: String,
        value_json: String,
    },
}

// === Responses ===

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "type", content = "payload")]
pub enum EngineResponse {
    // Auth responses
    HomeserverInfo {
        url: String,
        name: String,
        supports_oidc: bool,
    },
    OidcUrl {
        redirect_url: String,
    },
    SessionRestored {
        account: Account,
    },
    LoggedIn {
        account: Account,
    },
    LoggedOut,
    AccountsList {
        accounts: Vec<Account>,
    },
    // Room list
    RoomListSubscribed {
        subscription_id: SubscriptionId,
    },
    Unsubscribed {
        subscription_id: SubscriptionId,
    },
    RoomJoined {
        room_id: RoomId,
    },
    RoomLeft {
        room_id: RoomId,
    },
    RoomCreated {
        room_id: RoomId,
    },
    // Timeline
    TimelineSubscribed {
        subscription_id: SubscriptionId,
    },
    MessageSent {
        room_id: RoomId,
        txn_id: String,
        event_id: EventId,
    },
    MessageEdited {
        room_id: RoomId,
        event_id: EventId,
    },
    EventRedacted {
        room_id: RoomId,
        event_id: EventId,
    },
    ReactionToggled {
        room_id: RoomId,
        event_id: EventId,
        key: String,
        added: bool,
    },
    SendRetried {
        room_id: RoomId,
        txn_id: String,
    },
    SendCancelled {
        room_id: RoomId,
        txn_id: String,
    },
    AttachmentSent {
        room_id: RoomId,
        txn_id: String,
        event_id: EventId,
    },
    ReadMarked {
        room_id: RoomId,
    },
    TypingStatusSet {
        room_id: RoomId,
    },
    // Media
    MediaRetrieved {
        local_path: String,
    },
    // Crypto / Verification
    VerificationRequested {
        user_id: UserId,
        device_id: String,
    },
    VerificationAccepted {
        user_id: UserId,
        device_id: String,
    },
    SasStarted {
        user_id: UserId,
        device_id: String,
        emojis: Vec<SasEmoji>,
    },
    SasConfirmed {
        user_id: UserId,
        device_id: String,
    },
    VerificationCancelled {
        user_id: UserId,
        device_id: String,
    },
    RecoveryStatusRetrieved {
        state: RecoveryState,
    },
    RecoveryEnabled,
    RecoveredFromKey {
        recovered: bool,
    },
    RecoveryReset,
    // Spaces
    SpacesSubscribed {
        subscription_id: SubscriptionId,
    },
    SpaceChildrenSubscribed {
        subscription_id: SubscriptionId,
    },
    // Settings / Theme
    ThemesList {
        themes: Vec<(String, String)>, // (id, name)
    },
    ThemeCss {
        css: String,
    },
    ThemeSetActive,
    SettingsRetrieved {
        settings_json: String,
    },
    SettingSet {
        key: String,
    },
    Unit,
}

// === Errors ===

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct EngineError {
    pub code: String,
    pub message: String,
}

impl EngineError {
    pub fn not_implemented(command: &str) -> Self {
        Self {
            code: "not_implemented".to_string(),
            message: format!("Command '{}' not yet implemented", command),
        }
    }

    pub fn auth_failed(msg: impl Into<String>) -> Self {
        Self {
            code: "auth_failed".to_string(),
            message: msg.into(),
        }
    }

    pub fn network_error(msg: impl Into<String>) -> Self {
        Self {
            code: "network_error".to_string(),
            message: msg.into(),
        }
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        Self {
            code: "not_found".to_string(),
            message: msg.into(),
        }
    }

    pub fn invalid_argument(msg: impl Into<String>) -> Self {
        Self {
            code: "invalid_argument".to_string(),
            message: msg.into(),
        }
    }

    pub fn internal_error(msg: impl Into<String>) -> Self {
        Self {
            code: "internal_error".to_string(),
            message: msg.into(),
        }
    }
}

// === Events ===

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "type", content = "payload")]
pub enum EngineEvent {
    ConnectionStatus {
        status: ConnectionStatus,
    },
    AuthState {
        account: Option<Account>,
    },
    RoomListDiff {
        subscription_id: SubscriptionId,
        diff: VectorDiffDto<RoomSummary>,
    },
    TimelineDiff {
        subscription_id: SubscriptionId,
        room_id: RoomId,
        diff: VectorDiffDto<TimelineItemDto>,
    },
    TypingNotice {
        room_id: RoomId,
        users: Vec<UserId>,
    },
    VerificationState {
        user_id: UserId,
        device_id: String,
        state: VerificationState,
    },
    RecoveryState {
        state: RecoveryState,
    },
    ToastNotice {
        message: String,
        level: String, // "info" | "warning" | "error"
    },
}

pub fn register_types() -> Types {
    Types::default()
        .register::<AccountId>()
        .register::<RoomId>()
        .register::<EventId>()
        .register::<UserId>()
        .register::<SubscriptionId>()
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
        .register::<AuthSession>()
        .register::<Account>()
        .register::<SasEmoji>()
        .register::<VerificationState>()
        .register::<RecoveryState>()
        .register::<MediaSource>()
        .register::<EngineCommand>()
        .register::<EngineResponse>()
        .register::<EngineError>()
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
        assert!(ts.contains("export type EngineResponse"));
        assert!(ts.contains("export type EngineError"));
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
            encrypted: false,
            membership: RoomMembership::Joined,
            favourite: None,
            low_priority: None,
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
    fn test_engine_error_constructors() {
        let err = EngineError::not_implemented("SendMessage");
        assert_eq!(err.code, "not_implemented");
        assert!(err.message.contains("SendMessage"));

        let err = EngineError::auth_failed("Invalid credentials");
        assert_eq!(err.code, "auth_failed");
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
