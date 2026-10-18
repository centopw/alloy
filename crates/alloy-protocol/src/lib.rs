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

/// One member of a room, as listed by `GetRoomMembers`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct RoomMemberDto {
    pub user_id: UserId,
    pub display_name: Option<String>,
    /// `mxc://` avatar URI, if any; fetch it with `GetMedia`.
    pub avatar_url: Option<String>,
    /// Matrix power level (0 = default member, 50 = moderator, 100 = admin).
    #[specta(type = Number)]
    pub power_level: i64,
    pub membership: RoomMembership,
}

/// A room and the joined spaces that contain it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct SpaceParentDto {
    pub room_id: RoomId,
    pub spaces: Vec<RoomId>,
}

/// Static details about a room for the details drawer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct RoomDetails {
    pub room_id: RoomId,
    pub name: Option<String>,
    pub topic: Option<String>,
    pub canonical_alias: Option<String>,
    pub avatar_url: Option<String>,
    pub is_direct: bool,
    pub encrypted: bool,
    #[specta(type = Number)]
    pub joined_members: u64,
    #[specta(type = Number)]
    pub invited_members: u64,
    /// What the active account may change, from the room's power levels.
    #[serde(default)]
    pub can_set_name: bool,
    #[serde(default)]
    pub can_set_topic: bool,
    #[serde(default)]
    pub can_set_avatar: bool,
}

/// Which kind of shared content `GetRoomMedia` lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
pub enum RoomMediaKind {
    /// Images and videos.
    Media,
    /// Other files and audio.
    Files,
    /// Messages containing web links.
    Links,
}

/// One shared item in a room: a media or file message, or a message with links.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct RoomMediaItem {
    pub event_id: EventId,
    pub sender: UserId,
    pub sender_display_name: Option<String>,
    #[specta(type = Number)]
    pub timestamp_millis: u64,
    /// `image`, `video`, `audio`, `file` or `link`.
    pub kind: String,
    /// Caption or file name, or the message text for links.
    pub body: String,
    /// `mxc://` URI, or a JSON-encoded encrypted source (starts with `{`); `None` for links.
    pub uri: Option<String>,
    pub thumbnail_uri: Option<String>,
    pub mime_type: Option<String>,
    #[specta(type = Option<Number>)]
    pub size_bytes: Option<u64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// `http(s)` links found in the message (only for `link` items).
    #[serde(default)]
    pub urls: Vec<String>,
}

/// Profile of one user, optionally as seen in a room.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct UserProfileDto {
    pub user_id: UserId,
    pub display_name: Option<String>,
    /// `mxc://` avatar URI, fetch it with `GetMedia`.
    pub avatar_url: Option<String>,
    /// `join`, `invite`, `leave`, `ban` or `knock` in the given room, if a room was given.
    pub membership: Option<String>,
    #[specta(type = Option<Number>)]
    pub power_level: Option<i64>,
    /// An existing direct-message room with this user.
    pub dm_room_id: Option<RoomId>,
    pub is_me: bool,
}

/// One room listed in a homeserver's public room directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct PublicRoomDto {
    pub room_id: RoomId,
    /// Canonical alias such as `#general:example.org`, if the room has one.
    pub alias: Option<String>,
    pub name: Option<String>,
    pub topic: Option<String>,
    /// `mxc://` avatar URI, if any; fetch it with `GetMedia`.
    pub avatar_url: Option<String>,
    #[specta(type = Number)]
    pub num_joined_members: u64,
    /// Whether guests may read the room without joining.
    pub world_readable: bool,
    pub guest_can_join: bool,
    /// Whether the active account is already joined to this room.
    pub joined: bool,
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

/// One answer of a poll. `votes` is zero while results are hidden.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct PollAnswerDto {
    pub id: String,
    pub text: String,
    pub votes: u32,
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
        /// `mxc://` URI for plain media, or a JSON-encoded encrypted source (starts with `{`).
        uri: String,
        mime_type: String,
        #[specta(type = Number)]
        size_bytes: u64,
        thumbnail_uri: Option<String>,
        /// Caption or file name.
        #[serde(default)]
        body: String,
        /// `image`, `video`, `audio`, `file` or `sticker`.
        #[serde(default)]
        media_kind: String,
        #[serde(default)]
        width: Option<u32>,
        #[serde(default)]
        height: Option<u32>,
        #[serde(default)]
        #[specta(type = Option<Number>)]
        duration_ms: Option<u64>,
    },
    Reaction {
        key: String,
        #[specta(type = Number)]
        count: usize,
        senders: Vec<UserId>,
    },
    Poll {
        question: String,
        answers: Vec<PollAnswerDto>,
        disclosed: bool,
        max_selections: u32,
        total_votes: u32,
        has_ended: bool,
        #[specta(type = Option<Number>)]
        end_time: Option<u64>,
        my_votes: Vec<String>,
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

/// Aggregated reactions for one key on a message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct ReactionSummary {
    pub key: String,
    #[specta(type = Number)]
    pub count: usize,
    pub senders: Vec<UserId>,
    /// Whether the current user reacted with this key.
    pub by_me: bool,
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
    #[serde(default)]
    pub sender_display_name: Option<String>,
    #[serde(default)]
    pub sender_avatar_url: Option<String>,
    #[serde(default)]
    pub edited: bool,
    #[serde(default)]
    pub reactions: Vec<ReactionSummary>,
    /// Transaction id for local echoes (use with RetrySend/CancelSend).
    #[serde(default)]
    pub transaction_id: Option<String>,
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
    /// Lists joined and invited members of a room (loads the full member list from the server if needed).
    GetRoomMembers {
        room_id: RoomId,
    },
    GetRoomDetails {
        room_id: RoomId,
    },
    /// Searches the public room directory. `query` matches names, topics and aliases; `since` is the
    /// `next_batch` token of a previous page; `server` defaults to the active homeserver.
    SearchPublicRooms {
        account_id: AccountId,
        query: Option<String>,
        server: Option<String>,
        since: Option<String>,
        limit: u32,
    },
    /// Renames a room (needs the power level to change the name).
    SetRoomName {
        room_id: RoomId,
        name: String,
    },
    SetRoomTopic {
        room_id: RoomId,
        topic: String,
    },
    /// Sets the room avatar from a file the user picked (same path rules as `SendAttachment`);
    /// `None` removes it.
    SetRoomAvatar {
        room_id: RoomId,
        file_path: Option<String>,
    },
    /// Reads the private note the active account keeps about a room (account data, never shared).
    GetRoomNote {
        room_id: RoomId,
    },
    /// Stores the private note; an empty note deletes it.
    SetRoomNote {
        room_id: RoomId,
        note: String,
    },
    /// Marks the whole room as read (read receipt at the latest event and clears the unread flag).
    MarkRoomRead {
        room_id: RoomId,
    },
    /// Looks up a user's profile, with membership and power level when a room is given.
    GetUserProfile {
        user_id: UserId,
        room_id: Option<RoomId>,
    },
    /// Opens the direct-message room with a user, creating it (and inviting them) if none exists.
    StartDirectMessage {
        user_id: UserId,
    },
    /// Lists shared media, files or links of a room, newest first. `from` is the `next_batch` of
    /// the previous page. Pages may hold fewer than `limit` items; keep paging while `next_batch`
    /// is set.
    GetRoomMedia {
        room_id: RoomId,
        kind: RoomMediaKind,
        from: Option<String>,
        limit: u32,
    },
    // Timeline
    SubscribeTimeline {
        room_id: RoomId,
    },
    /// Streams the timeline of one thread (the root event and its replies).
    SubscribeThread {
        room_id: RoomId,
        thread_root_event_id: EventId,
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
        /// When set, the message is posted into this thread instead of the main timeline.
        in_thread_root: Option<EventId>,
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
    SendPoll {
        room_id: RoomId,
        question: String,
        answers: Vec<String>,
        disclosed: bool,
        max_selections: u32,
    },
    RespondToPoll {
        room_id: RoomId,
        poll_event_id: EventId,
        answer_ids: Vec<String>,
    },
    EndPoll {
        room_id: RoomId,
        poll_event_id: EventId,
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
        /// Used to pick the cache file extension; pass the timeline item's mime type.
        #[serde(default)]
        mime_type: Option<String>,
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
    EnableRecovery {
        /// Account password; needed when cross-signing must be created (UIA).
        password: Option<String>,
    },
    RecoverWithKey {
        recovery_key: String,
    },
    ResetRecovery {
        password: Option<String>,
    },
    // Spaces
    /// Lists, for every joined room that sits in a joined space, its parent spaces.
    GetSpaceParents,
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
    RoomMembers {
        room_id: RoomId,
        members: Vec<RoomMemberDto>,
    },
    RoomDetailsRetrieved {
        details: RoomDetails,
    },
    /// One page of public room directory results for `SearchPublicRooms`.
    PublicRooms {
        rooms: Vec<PublicRoomDto>,
        next_batch: Option<String>,
        #[specta(type = Number)]
        total_estimate: Option<u64>,
    },
    /// The private note of a room (empty when none is set).
    RoomNote {
        room_id: RoomId,
        note: String,
    },
    UserProfile {
        profile: UserProfileDto,
    },
    /// The direct-message room opened by `StartDirectMessage`.
    DirectMessageRoom {
        room_id: RoomId,
        created: bool,
    },
    RoomMedia {
        room_id: RoomId,
        items: Vec<RoomMediaItem>,
        next_batch: Option<String>,
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
    Paginated {
        room_id: RoomId,
        /// True when the start of the room history was reached.
        reached_start: bool,
    },
    /// The message was queued; the local echo arrives via `TimelineDiff` and the final
    /// event id is delivered through the item's `send_state` once sent.
    MessageSent {
        room_id: RoomId,
        txn_id: String,
        event_id: Option<EventId>,
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
        /// Known only once the server confirmed the event; the timeline item carries it later.
        event_id: Option<EventId>,
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
    /// The recovery key: show it once, it cannot be retrieved again.
    RecoveryEnabled {
        recovery_key: String,
    },
    RecoveredFromKey {
        recovered: bool,
    },
    RecoveryReset {
        recovery_key: String,
    },
    // Spaces
    SpaceParents {
        parents: Vec<SpaceParentDto>,
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

    pub fn forbidden(msg: impl Into<String>) -> Self {
        Self {
            code: "forbidden".to_string(),
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
        diff: Box<VectorDiffDto<TimelineItemDto>>,
    },
    TypingNotice {
        room_id: RoomId,
        users: Vec<UserId>,
    },
    /// Attachment upload progress, keyed by the `txn_id` passed to `SendAttachment`.
    UploadProgress {
        room_id: RoomId,
        txn_id: String,
        #[specta(type = Number)]
        sent_bytes: u64,
        #[specta(type = Number)]
        total_bytes: u64,
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
        .register::<RoomMemberDto>()
        .register::<RoomDetails>()
        .register::<RoomMediaKind>()
        .register::<RoomMediaItem>()
        .register::<UserProfileDto>()
        .register::<SpaceParentDto>()
        .register::<NotificationCount>()
        .register::<TimelineItemPreview>()
        .register::<RoomSummary>()
        .register::<TimelineItemContent>()
        .register::<EventSendState>()
        .register::<ThreadSummary>()
        .register::<ReactionSummary>()
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
