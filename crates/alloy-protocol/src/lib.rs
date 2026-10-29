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
    /// Whether the active account can pin messages (power >= `m.room.pinned_events`).
    #[serde(default)]
    pub can_pin: bool,
    /// Whether the active account can redact others' messages (power >= `redact`).
    #[serde(default)]
    pub can_redact_others: bool,
    /// Whether the active account can use `@room` mentions (power >= `notifications.room`).
    #[serde(default)]
    pub can_mention_room: bool,
    /// Event IDs of currently pinned messages (oldest first).
    #[serde(default)]
    pub pinned_event_ids: Vec<EventId>,
    /// Active account's power level in the room.
    #[serde(default)]
    #[specta(type = Number)]
    pub my_power_level: i64,
    /// Whether the active account can kick users (power >= kick level).
    #[serde(default)]
    pub can_kick: bool,
    /// Whether the active account can ban users (power >= ban level).
    #[serde(default)]
    pub can_ban: bool,
    /// Whether the active account can change power levels (power >= power levels state event level).
    #[serde(default)]
    pub can_set_power_levels: bool,
}

/// How loudly a room (or a kind of room) notifies, as stored in the account's push rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
pub enum NotificationLevel {
    /// Every message notifies.
    All,
    /// Only mentions and keywords notify.
    Mentions,
    /// Nothing notifies.
    Mute,
}

/// Account-wide notification rules (server-side push rules, shared by all devices).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct NotificationSettingsDto {
    /// Default for one-to-one rooms (encrypted and not).
    pub one_to_one: NotificationLevel,
    /// Default for group rooms (encrypted and not).
    pub group: NotificationLevel,
    /// `@room` mentions notify.
    pub room_mention: bool,
    /// Mentions of the user's ID or display name notify.
    pub user_mention: bool,
    /// Calls and invitations notify.
    pub invite: bool,
}

/// The signed-in user's own profile and device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct OwnProfileDto {
    pub user_id: UserId,
    pub display_name: Option<String>,
    /// `mxc://` avatar URI, fetch it with `GetMedia`.
    pub avatar_url: Option<String>,
    pub device_id: String,
    pub homeserver: String,
}

/// One login session (device) of the account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct DeviceDto {
    pub device_id: String,
    pub display_name: Option<String>,
    #[specta(type = Option<Number>)]
    pub last_seen_ts: Option<u64>,
    pub last_seen_ip: Option<String>,
    pub is_current: bool,
    /// Cross-signing verified.
    pub verified: bool,
}

/// Cryptographic identity of this session, for comparing out of band.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct SessionInfoDto {
    pub device_id: String,
    /// Unpadded base64 Ed25519 fingerprint key.
    pub ed25519_key: Option<String>,
    pub curve25519_key: Option<String>,
    pub cross_signing_ready: bool,
    pub backup_enabled: bool,
    /// This device is cross-signing verified — the same check `DeviceDto::verified` makes. When it
    /// is true the session is already trusted; when it is false the UI can offer verification.
    pub verified: bool,
}

/// Whether a room's messages were searched on the homeserver.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
pub enum RoomSearchScope {
    /// The homeserver searched the room's messages.
    Server,
    /// The room is end-to-end encrypted, so the homeserver cannot search it and no search was made.
    Unavailable,
}

/// One message matched by `SearchRoomMessages`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct RoomSearchHit {
    pub event_id: EventId,
    pub sender: UserId,
    pub sender_display_name: Option<String>,
    #[specta(type = Number)]
    pub timestamp_millis: u64,
    pub body: String,
}

/// One message matched across rooms by `SearchAllMessages`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct GlobalSearchHit {
    pub room_id: RoomId,
    pub room_name: Option<String>,
    pub event_id: EventId,
    pub sender: UserId,
    pub sender_display_name: Option<String>,
    #[specta(type = Number)]
    pub timestamp_millis: u64,
    pub body: String,
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

/// One user-directory match returned by `SearchUsers`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct UserSearchResultDto {
    pub user_id: UserId,
    pub display_name: Option<String>,
    /// `mxc://` avatar URI, if any; fetch it with `GetMedia`.
    pub avatar_url: Option<String>,
    /// The user joins or is invited to a room the active account is joined to.
    pub already_in_room: bool,
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
    /// Ids of the room's custom tags (`u.<id>` room tags with the prefix stripped), sorted.
    pub tags: Vec<String>,
    /// For a direct room, the other person's Matrix ID; `None` for every other room.
    pub direct_user_id: Option<UserId>,
    /// Whether the room is marked as unread by the user (creates bold row with accent dot).
    #[serde(default)]
    pub marked_unread: bool,
    /// First 80 chars of the room's draft message, if any.
    #[serde(default)]
    pub draft: Option<String>,
}

/// A user-defined room tag, as stored in the account's tag catalogue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct RoomTagDef {
    pub id: String,
    pub name: String,
    pub colour: TagColour,
}

/// Palette colour of a room tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum TagColour {
    Grey,
    Red,
    Orange,
    Amber,
    Green,
    Teal,
    Blue,
    Violet,
    Pink,
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
        #[serde(default)]
        voice: bool,
        #[serde(default)]
        waveform: Option<Vec<u16>>,
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
    Location {
        geo_uri: String,
        description: Option<String>,
        #[serde(default)]
        live: bool,
        #[serde(default)]
        #[specta(type = Option<Number>)]
        live_until_millis: Option<u64>,
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
    /// Whether this event mentions the current user (causes highlighting in the UI).
    #[serde(default)]
    pub mentions_me: bool,
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

/// Open Graph summary of a link, as the homeserver reports it. Every field is optional: a page may not
/// declare it, and the image is an `mxc://` URI that is fetched through `GetMedia`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct UrlPreviewDto {
    pub url: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub site_name: Option<String>,
    pub image: Option<String>,
    #[specta(type = Option<Number>)]
    pub image_width: Option<u32>,
    #[specta(type = Option<Number>)]
    pub image_height: Option<u32>,
}

/// A message draft for a room.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct DraftDto {
    pub text: String,
    pub reply_to: Option<EventId>,
    pub edit_of: Option<EventId>,
}

/// Summary of a room's draft for `ListDrafts`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct DraftSummary {
    pub room_id: RoomId,
    pub text: String,
}

/// Failure details when forwarding an event to a target room.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct ForwardFailure {
    pub room_id: RoomId,
    pub error: String,
}

// === Commands ===

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
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
    CancelOidcLogin,
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
    /// Reads the account's room tag catalogue (display order preserved).
    GetTagCatalog,
    /// Replaces the room tag catalogue; entries are validated and normalised before storing.
    ///
    /// Superseded by [`EngineCommand::CreateRoomTag`], [`EngineCommand::RenameRoomTag`] and
    /// [`EngineCommand::ReorderRoomTags`], which change the catalogue the server holds instead of
    /// writing a whole list read from a possibly stale local copy.
    SaveTagCatalog {
        tags: Vec<RoomTagDef>,
    },
    /// Adds one tag to the catalogue; the engine assigns its id and returns the stored definition.
    CreateRoomTag {
        name: String,
        colour: TagColour,
    },
    /// Renames and/or recolours one tag. At least one of `name`/`colour` must be set and `id` must
    /// be in the catalogue.
    RenameRoomTag {
        id: String,
        name: Option<String>,
        colour: Option<TagColour>,
    },
    /// Sets the catalogue's display order. `ids` must list every tag in the catalogue exactly once;
    /// a list built from a stale copy is refused rather than dropping the entries it omits.
    ReorderRoomTags {
        ids: Vec<String>,
    },
    /// Assigns (`enabled: true`) or removes the custom tag `tag_id` on a room.
    SetRoomTag {
        room_id: RoomId,
        tag_id: String,
        enabled: bool,
    },
    /// Removes a tag from the catalogue and from every room that carries it.
    DeleteRoomTag {
        tag_id: String,
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
    /// Searches the homeserver user directory by user id and display name. `query` must not be
    /// blank and is sent as given; `limit` is clamped to 1..=50. Each result's `already_in_room`
    /// is derived from locally known joined-room membership.
    SearchUsers {
        query: String,
        limit: u32,
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
    /// Searches the text messages of a room on the homeserver, newest first. `query` must not be
    /// blank and `limit` is clamped to 1..=50. `next_batch` is the token of the previous page.
    /// Encrypted rooms are not searched; the response then reports `RoomSearchScope::Unavailable`.
    SearchRoomMessages {
        room_id: RoomId,
        query: String,
        sender: Option<UserId>,
        limit: u32,
        next_batch: Option<String>,
    },
    /// Reads the account-wide notification rules.
    GetNotificationSettings,
    /// Sets the default level for one-to-one or group rooms (encrypted and not).
    SetDefaultNotificationLevel {
        one_to_one: bool,
        level: NotificationLevel,
    },
    /// Toggles mention rules; a `None` field is left unchanged.
    SetMentionRules {
        room_mention: Option<bool>,
        user_mention: Option<bool>,
        invite: Option<bool>,
    },
    /// Reads the level of one room and whether it follows the default.
    GetRoomNotificationLevel {
        room_id: RoomId,
    },
    /// Sets one room's level; `None` restores the default.
    SetRoomNotificationLevel {
        room_id: RoomId,
        level: Option<NotificationLevel>,
    },
    /// Reads the signed-in user's profile.
    GetOwnProfile,
    SetDisplayName {
        name: String,
    },
    /// Sets the user's avatar from a picked file (same path rules as `SendAttachment`); `None` removes it.
    SetOwnAvatar {
        file_path: Option<String>,
    },
    /// Lists the account's sessions.
    ListDevices,
    /// This session's cryptographic identity.
    GetSessionInfo,
    /// Deletes cached downloaded media (it is fetched again when needed).
    ClearMediaCache,
    // Timeline
    SubscribeTimeline {
        room_id: RoomId,
    },
    /// Streams the timeline of one thread (the root event and its replies).
    SubscribeThread {
        room_id: RoomId,
        thread_root_event_id: EventId,
    },
    /// Aborts every subscription for this room and drops its cached timelines — the room timeline
    /// and each of its threads. Best-effort: any later command that needs the timeline rebuilds it,
    /// and the next subscription replays a full `Reset`. Intended for evicting rooms the app no
    /// longer shows; calling it for a room still on screen blanks that view until it subscribes
    /// again.
    CloseTimeline {
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
        #[serde(default)]
        mention_room: bool,
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
    /// Pin a message in a room (requires `can_pin`).
    SetEventPinned {
        room_id: RoomId,
        event_id: EventId,
        pinned: bool,
    },
    /// Load pinned messages in a room.
    GetPinnedEvents {
        room_id: RoomId,
    },
    /// Forward one or more events to one or more target rooms, stripping relations.
    ForwardEvents {
        room_id: RoomId,
        event_ids: Vec<EventId>,
        targets: Vec<RoomId>,
    },
    /// Restart background sync and rebuild open timelines: the manual Refresh. Returns at once; sync resumes in the background.
    RefreshSync,
    /// Set the room's favourite flag (pin to top of the list).
    SetRoomFavourite {
        room_id: RoomId,
        favourite: bool,
    },
    /// Set the room's low-priority flag (archive).
    SetRoomLowPriority {
        room_id: RoomId,
        low_priority: bool,
    },
    /// Set the room's marked-as-unread flag.
    SetRoomMarkedUnread {
        room_id: RoomId,
        unread: bool,
    },
    /// Save a message draft for a room (empty text clears it).
    SaveDraft {
        room_id: RoomId,
        text: String,
        reply_to: Option<EventId>,
        edit_of: Option<EventId>,
    },
    /// Load the message draft for a room.
    GetDraft {
        room_id: RoomId,
    },
    /// Load all room drafts (returns the first 80 chars of each).
    ListDrafts,
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
        /// Optional caption, sent as the media message body.
        #[serde(default)]
        body: Option<String>,
        txn_id: Option<String>,
    },
    /// Send a voice message (audio with waveform): file must be registered for attachment.
    SendVoice {
        room_id: RoomId,
        file_path: String,
        #[specta(type = Number)]
        duration_ms: u64,
        waveform: Vec<u16>,
        #[serde(default)]
        txn_id: Option<String>,
    },
    /// Send a static location message.
    SendLocation {
        room_id: RoomId,
        #[specta(type = Number)]
        latitude: f64,
        #[specta(type = Number)]
        longitude: f64,
        #[serde(default)]
        #[specta(type = Option<Number>)]
        accuracy_m: Option<f64>,
        #[serde(default)]
        description: Option<String>,
    },
    /// Start a live location share.
    StartLiveLocation {
        room_id: RoomId,
        #[specta(type = Number)]
        duration_ms: u64,
    },
    /// Send a location beacon update (only valid while live share is active).
    SendLocationBeacon {
        room_id: RoomId,
        geo_uri: String,
    },
    /// Stop the active live location share in this room.
    StopLiveLocation {
        room_id: RoomId,
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
    /// Fetches the homeserver's link preview for `url`. The response is `None` when the server has no preview.
    GetUrlPreview {
        url: String,
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
    /// Reads the account's synced preferences document (`chat.alloy.prefs`).
    GetAccountPrefs,
    /// Stores the account's synced preferences document. `json` must be a JSON object of at most
    /// 64 KiB with an integer `updated_at`.
    SetAccountPrefs {
        json: String,
    },
    // Moderation & Account
    /// Delete one or more of the active account's devices (login sessions).
    /// Requires the active account's password when UIA is required.
    DeleteDevices {
        device_ids: Vec<String>,
        #[serde(default)]
        password: Option<String>,
    },
    /// Kick a user from a room.
    KickUser {
        room_id: RoomId,
        user_id: UserId,
        #[serde(default)]
        reason: Option<String>,
    },
    /// Ban a user from a room.
    BanUser {
        room_id: RoomId,
        user_id: UserId,
        #[serde(default)]
        reason: Option<String>,
    },
    /// Unban a user from a room.
    UnbanUser {
        room_id: RoomId,
        user_id: UserId,
        #[serde(default)]
        reason: Option<String>,
    },
    /// Set the power level of a user in a room.
    SetPowerLevel {
        room_id: RoomId,
        user_id: UserId,
        #[specta(type = Number)]
        level: i64,
    },
    /// Add a user to the active account's block list.
    IgnoreUser {
        user_id: UserId,
    },
    /// Remove a user from the active account's block list.
    UnignoreUser {
        user_id: UserId,
    },
    /// List users on the active account's block list.
    ListIgnoredUsers,
    /// Report a message to room moderators.
    ReportEvent {
        room_id: RoomId,
        event_id: EventId,
        #[serde(default)]
        reason: Option<String>,
    },
    /// Search messages across all unencrypted rooms.
    SearchAllMessages {
        query: String,
        #[serde(default)]
        next_batch: Option<String>,
        #[serde(default)]
        limit: Option<u32>,
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
        /// Authorization URL to open in the system browser.
        authorization_url: String,
        /// Redirect URI registered with the server; the app must receive this URL and pass it
        /// back with `CompleteOidcLogin`.
        redirect_uri: String,
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
    /// The account's room tag catalogue, in display order.
    TagCatalog {
        tags: Vec<RoomTagDef>,
    },
    /// The tag `CreateRoomTag` added, with the id the engine assigned.
    RoomTagCreated {
        tag: RoomTagDef,
    },
    UserProfile {
        profile: UserProfileDto,
    },
    /// One page of user-directory results for `SearchUsers`; `limited` is the server's flag that
    /// more matches exist than the returned page holds.
    UserSearchResults {
        results: Vec<UserSearchResultDto>,
        limited: bool,
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
    /// The page of hits returned by `SearchRoomMessages`.
    RoomSearchResults {
        hits: Vec<RoomSearchHit>,
        next_batch: Option<String>,
        scope: RoomSearchScope,
    },
    NotificationSettings {
        settings: NotificationSettingsDto,
    },
    RoomNotificationLevel {
        room_id: RoomId,
        level: NotificationLevel,
        /// The room follows the account default.
        is_default: bool,
    },
    OwnProfile {
        profile: OwnProfileDto,
    },
    Devices {
        devices: Vec<DeviceDto>,
    },
    SessionInfo {
        info: SessionInfoDto,
    },
    /// Bytes freed by `ClearMediaCache`.
    MediaCacheCleared {
        #[specta(type = Number)]
        freed_bytes: u64,
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
    /// Whether `CloseTimeline` had anything to abort or drop; `false` means it was already closed
    /// and the call changed nothing.
    TimelineClosed {
        room_id: RoomId,
        closed: bool,
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
    /// Pin state of a message changed.
    EventPinned {
        room_id: RoomId,
        event_id: EventId,
        pinned: bool,
    },
    /// Pinned messages for a room (first load or refresh).
    PinnedEvents {
        room_id: RoomId,
        items: Vec<TimelineItemDto>,
    },
    /// Events forwarded to target rooms.
    EventsForwarded {
        forwarded: Vec<RoomId>,
        failed: Vec<ForwardFailure>,
    },
    /// Sync was restarted; messages arrive again as it catches up.
    SyncRefreshed,
    /// Room's favourite flag changed.
    RoomFavouriteSet {
        room_id: RoomId,
        favourite: bool,
    },
    /// Room's low-priority flag changed.
    RoomLowPrioritySet {
        room_id: RoomId,
        low_priority: bool,
    },
    /// Room's marked-as-unread flag changed.
    RoomMarkedUnreadSet {
        room_id: RoomId,
        unread: bool,
    },
    /// A room's draft was saved.
    DraftSaved {
        room_id: RoomId,
    },
    /// A room's draft (or None if cleared).
    Draft {
        room_id: RoomId,
        draft: Option<DraftDto>,
    },
    /// All rooms with non-empty drafts.
    Drafts {
        items: Vec<DraftSummary>,
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
    VoiceMessageSent {
        room_id: RoomId,
        txn_id: String,
        event_id: Option<EventId>,
    },
    LocationSent {
        room_id: RoomId,
    },
    LiveLocationStarted {
        room_id: RoomId,
    },
    LocationBeaconSent {
        room_id: RoomId,
    },
    LiveLocationStopped {
        room_id: RoomId,
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
    /// `None` when the homeserver has no preview for the URL, or previews are disabled there.
    UrlPreview {
        preview: Option<UrlPreviewDto>,
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
    /// The synced preferences document, or `None` when the account has never stored one.
    AccountPrefs {
        json: Option<String>,
    },
    DevicesDeleted {
        deleted: Vec<String>,
    },
    AuthRequired {
        flows: Vec<String>,
    },
    UserKicked {
        room_id: RoomId,
        user_id: UserId,
    },
    UserBanned {
        room_id: RoomId,
        user_id: UserId,
    },
    UserUnbanned {
        room_id: RoomId,
        user_id: UserId,
    },
    PowerLevelSet {
        room_id: RoomId,
        user_id: UserId,
        #[specta(type = Number)]
        level: i64,
    },
    UserIgnored {
        user_id: UserId,
    },
    UserUnignored {
        user_id: UserId,
    },
    IgnoredUsers {
        user_ids: Vec<UserId>,
    },
    EventReported {
        room_id: RoomId,
        event_id: EventId,
    },
    AllMessagesFound {
        hits: Vec<GlobalSearchHit>,
        next_batch: Option<String>,
    },
    Unit,
}

// === Errors ===

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct EngineError {
    pub code: String,
    pub message: String,
    /// Stable machine-readable detail for a coarse `code`, for example why an auth attempt
    /// failed. The UI branches on this; `message` is for humans and may change.
    pub reason: Option<String>,
}

impl EngineError {
    pub fn not_implemented(command: &str) -> Self {
        Self {
            code: "not_implemented".to_string(),
            message: format!("Command '{}' not yet implemented", command),
            reason: None,
        }
    }

    pub fn auth_failed(msg: impl Into<String>) -> Self {
        Self {
            code: "auth_failed".to_string(),
            message: msg.into(),
            reason: None,
        }
    }

    /// An auth failure with a stable machine-readable reason for the UI to branch on.
    pub fn auth_failed_with(reason: impl Into<String>, msg: impl Into<String>) -> Self {
        Self {
            code: "auth_failed".to_string(),
            message: msg.into(),
            reason: Some(reason.into()),
        }
    }

    pub fn network_error(msg: impl Into<String>) -> Self {
        Self {
            code: "network_error".to_string(),
            message: msg.into(),
            reason: None,
        }
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        Self {
            code: "not_found".to_string(),
            message: msg.into(),
            reason: None,
        }
    }

    pub fn invalid_argument(msg: impl Into<String>) -> Self {
        Self {
            code: "invalid_argument".to_string(),
            message: msg.into(),
            reason: None,
        }
    }

    pub fn forbidden(msg: impl Into<String>) -> Self {
        Self {
            code: "forbidden".to_string(),
            message: msg.into(),
            reason: None,
        }
    }

    pub fn internal_error(msg: impl Into<String>) -> Self {
        Self {
            code: "internal_error".to_string(),
            message: msg.into(),
            reason: None,
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
    /// A browser sign-in failed after it started, so the app can leave its waiting state.
    /// `reason` is the same stable discriminator `EngineError` carries; `message` is for logs.
    AuthFailed {
        reason: String,
        message: String,
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
    /// The account's synced preferences changed on another device; `json` is the new document.
    AccountPrefsChanged {
        json: String,
    },
    /// The pinned events for a room changed.
    PinnedEventsChanged {
        room_id: RoomId,
        event_ids: Vec<EventId>,
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
        .register::<NotificationLevel>()
        .register::<NotificationSettingsDto>()
        .register::<OwnProfileDto>()
        .register::<DeviceDto>()
        .register::<SessionInfoDto>()
        .register::<RoomMediaItem>()
        .register::<RoomSearchScope>()
        .register::<RoomSearchHit>()
        .register::<UserProfileDto>()
        .register::<UserSearchResultDto>()
        .register::<SpaceParentDto>()
        .register::<NotificationCount>()
        .register::<TimelineItemPreview>()
        .register::<RoomSummary>()
        .register::<RoomTagDef>()
        .register::<TagColour>()
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
        .register::<UrlPreviewDto>()
        .register::<DraftDto>()
        .register::<DraftSummary>()
        .register::<ForwardFailure>()
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
            tags: Vec::new(),
            direct_user_id: None,
            marked_unread: false,
            draft: None,
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
