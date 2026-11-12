//! Engine IPC protocol, version 2.
//!
//! Alloy connects personal Telegram and Messenger accounts from the device and keeps every byte in
//! a local SQLite store, so every id crossing this boundary is local: `AccountId`, `RoomId` and
//! `EventId` name rows in `alloy-store`, never objects on a network. Remote ids stay opaque strings
//! inside `UserId` and media references, exactly as the network produced them.
//!
//! The shell sends one [`EngineCommand`] and receives one [`EngineResponse`]; live updates arrive as
//! [`EngineEvent`]s. Login is a state machine the connector drives: the engine relays each
//! [`LoginStepDto`] to the UI and forwards the user's answer back as a [`LoginInputDto`].

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use specta::{Type, Types};
use specta_typescript::{Number, Typescript};

// === Core Identifiers ===

/// Local account id: the `accounts.account_id` row of the on-device store (UUID v4 string).
///
/// Never produced by a network; the store assigns it when an account is created.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
pub struct AccountId(pub String);

/// Local chat id: the `chats.chat_id` row of the on-device store.
///
/// One chat belongs to one account; the network-side chat id stays inside the engine.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
pub struct RoomId(pub String);

/// Local message id: the `messages.message_id` row of the on-device store.
///
/// Stable across edits and restarts; the network-side message id stays inside the engine.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
pub struct EventId(pub String);

/// A remote user id, opaque and used verbatim.
///
/// Never parsed or concatenated: the network assigns it and only the connector interprets it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
pub struct UserId(pub String);

/// Id of a live subscription created by `SubscribeRoomList` or `SubscribeTimeline`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
pub struct SubscriptionId(pub String);

/// Engine-generated id of one in-progress login attempt.
///
/// The engine returns it with every `LoginStep` and the UI passes it back with `SubmitLogin` and
/// `CancelLogin`; it is meaningless once the attempt ended.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
pub struct LoginId(pub String);

// === Networks and Accounts ===

/// A network Alloy can connect a personal account to.
///
/// `Fake` exists only for the deterministic test connector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum Network {
    Telegram,
    Messenger,
    Fake,
}

/// Lifecycle of one locally stored account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum AccountState {
    /// Credentials were entered but the connector has not been started yet.
    Pending,
    /// The connector holds a valid session and syncs.
    Active,
    /// The stored session is dead; only a new login helps.
    NeedsRelogin,
    /// The user disabled the account; nothing syncs until it is enabled again.
    Disabled,
}

/// What the network behind an account lets Alloy do.
///
/// One flag per connector capability: `true` promises the matching command works, `false` promises
/// it fails with `EngineError::not_supported`. The UI hides an action instead of offering one that
/// fails.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct CapabilitiesDto {
    /// The network allows editing sent messages.
    pub edit: bool,
    /// How long after sending an edit is still accepted, in seconds. `None` with `edit: true`
    /// means no window the connector is aware of.
    #[specta(type = Option<Number>)]
    pub edit_window_secs: Option<u32>,
    /// The network allows deleting a message for every participant.
    pub delete_for_everyone: bool,
    /// The network supports reactions.
    pub reactions: bool,
    /// Maximum number of distinct reactions one user may add per message.
    pub max_reactions_per_user: Option<u8>,
    /// The network supports replies to a specific message.
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
    #[specta(type = Option<Number>)]
    pub max_upload_bytes: Option<u64>,
    /// The network renders formatted text (`TimelineItemContent::TextMessage::formatted_body`).
    pub formatted_text: bool,
}

/// One connected account, as listed by `ListAccounts`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Account {
    pub id: AccountId,
    pub network: Network,
    /// The network's id for the signed-in user; `None` until login finished.
    pub remote_user_id: Option<UserId>,
    pub display_name: Option<String>,
    pub avatar: Option<MediaSource>,
    pub state: AccountState,
    pub capabilities: CapabilitiesDto,
}

// === Login ===

/// One cookie the user has to copy for cookie-based login (Messenger).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct CookieFieldDto {
    /// Cookie name exactly as the browser shows it, e.g. `"c_user"`.
    pub name: String,
    /// Human-readable description for the input label.
    pub label: String,
    /// The connector cannot log in without this cookie.
    pub required: bool,
}

/// How the network delivered a login code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum CodeChannel {
    /// Delivered inside the network's own app or by an already logged-in session.
    App,
    Sms,
    Email,
    Other,
}

/// Why a login attempt ended. A `Failed` step is terminal for that attempt; the UI restarts with
/// `BeginLogin`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LoginFailureDto {
    /// The submitted value was missing, malformed or rejected (wrong code, wrong password, bad
    /// cookie). `field` names the input, e.g. `"code"` or `"password"`.
    InvalidInput {
        field: Option<String>,
        message: String,
    },
    /// The step the user was answering is no longer valid (QR code or code expired).
    Expired,
    /// This connector does not implement the requested login method.
    NotSupported,
    /// The network asked us to slow down before another attempt.
    RateLimited {
        #[specta(type = Number)]
        retry_after_ms: u64,
    },
    /// Transport failure while logging in.
    Network { message: String },
    /// The network blocked the account itself; the user has to resolve it on the network's own site
    /// or app, no local retry helps.
    AccountLocked { message: String },
}

/// The next thing the login UI must show.
///
/// Steps are renderer instructions, not UI state: the UI stores nothing about which step came
/// before, it renders whatever the engine returns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LoginStepDto {
    /// Ask for a phone number in international format.
    PhoneNumber,
    /// Ask for the login code the network sent.
    Code {
        via: CodeChannel,
        /// Number of characters the network expects, when it says so.
        length: Option<u8>,
    },
    /// Ask for the account's two-step verification password.
    Password {
        /// Network-provided hint, when it has one. Never the password itself.
        hint: Option<String>,
    },
    /// Show a QR code the user scans with the network's mobile app. `payload` is the string to
    /// encode; `expires_at` is milliseconds since the Unix epoch, after which the UI sends
    /// `LoginInputDto::QrRefresh`.
    Qr {
        payload: String,
        #[specta(type = Number)]
        expires_at: i64,
    },
    /// Ask the user to copy cookies from a logged-in browser session (Messenger).
    Cookies {
        fields: Vec<CookieFieldDto>,
        /// Page that explains where to find the cookies, when the network has one.
        help_url: Option<String>,
    },
    /// Ask for a username/handle and password.
    Credentials {
        /// Label for the username field as the network names it.
        username_label: String,
    },
    /// Login finished; the account is stored and the connector starts.
    Done { account: Account },
    /// The attempt failed. Terminal for this attempt.
    Failed { reason: LoginFailureDto },
}

/// What the user typed for the step the engine last returned.
///
/// Secrets travel here only. `Debug` redacts codes, passwords and cookie values so a stray log line
/// or panic message cannot leak them; cookie names stay visible for diagnostics.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LoginInputDto {
    /// Answer to [`LoginStepDto::PhoneNumber`], international format (`+15550100`).
    PhoneNumber { phone: String },
    /// Answer to [`LoginStepDto::Code`].
    Code { code: String },
    /// Answer to [`LoginStepDto::Password`].
    Password { password: String },
    /// Answer to [`LoginStepDto::Cookies`]. Keys are [`CookieFieldDto::name`] values.
    Cookies { values: BTreeMap<String, String> },
    /// Answer to [`LoginStepDto::Credentials`].
    Credentials { username: String, password: String },
    /// Ask for a fresh [`LoginStepDto::Qr`] payload (the previous one expired, or the user chose QR
    /// login).
    QrRefresh,
}

const REDACTED: &str = "<redacted>";

impl fmt::Debug for LoginInputDto {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PhoneNumber { phone } => {
                f.debug_struct("PhoneNumber").field("phone", phone).finish()
            }
            Self::Code { .. } => f.debug_struct("Code").field("code", &REDACTED).finish(),
            Self::Password { .. } => f
                .debug_struct("Password")
                .field("password", &REDACTED)
                .finish(),
            Self::Cookies { values } => {
                let redacted: BTreeMap<&str, &str> = values
                    .keys()
                    .map(|name| (name.as_str(), REDACTED))
                    .collect();
                f.debug_struct("Cookies")
                    .field("values", &redacted)
                    .finish()
            }
            Self::Credentials { username, .. } => f
                .debug_struct("Credentials")
                .field("username", username)
                .field("password", &REDACTED)
                .finish(),
            Self::QrRefresh => f.write_str("QrRefresh"),
        }
    }
}

// === Chats ===

/// Kind of a chat, in network-neutral terms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum ChatKind {
    Direct,
    Group,
    Channel,
}

/// Pointer to one media object in the on-device store.
///
/// `media_id` is a local row id. Fetch the bytes with `GetMedia`; the engine downloads them through
/// the owning account's connector and caches the file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct MediaSource {
    pub media_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct NotificationCount {
    #[specta(type = Number)]
    pub highlight_count: u64,
    #[specta(type = Number)]
    pub notification_count: u64,
}

/// Last message of a chat, for the room list row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct TimelineItemPreview {
    pub sender: UserId,
    #[specta(type = Number)]
    pub timestamp_millis: u64,
    pub body: String,
}

/// One chat in the room list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct RoomSummary {
    pub id: RoomId,
    pub account_id: AccountId,
    pub network: Network,
    pub name: Option<String>,
    pub avatar: Option<MediaSource>,
    pub kind: ChatKind,
    /// `kind == ChatKind::Direct`, kept so a row does not have to branch on the enum.
    pub is_direct: bool,
    pub unread_notifications: NotificationCount,
    pub latest_event: Option<TimelineItemPreview>,
    pub pinned: bool,
    pub archived: bool,
    /// `None` when not muted, `u64::MAX` when muted forever, otherwise the end of the mute in
    /// milliseconds since the Unix epoch.
    #[specta(type = Option<Number>)]
    pub muted_until: Option<u64>,
    /// Whether the user marked the chat as unread (bold row with accent dot).
    #[serde(default)]
    pub marked_unread: bool,
    /// Ids of the chat's custom tags, sorted.
    pub tags: Vec<String>,
    /// First 80 chars of the chat's draft message, if any.
    #[serde(default)]
    pub draft: Option<String>,
}

/// One member of a chat, as listed by `GetRoomMembers`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct RoomMemberDto {
    pub user_id: UserId,
    pub display_name: Option<String>,
    pub avatar: Option<MediaSource>,
    /// Network role name such as `"admin"` or `"owner"`; `None` for an ordinary member.
    pub role: Option<String>,
}

/// Static details about a chat for the details drawer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct RoomDetails {
    pub room_id: RoomId,
    pub account_id: AccountId,
    pub network: Network,
    pub name: Option<String>,
    pub avatar: Option<MediaSource>,
    pub kind: ChatKind,
    pub is_direct: bool,
    #[specta(type = Number)]
    pub member_count: u64,
    /// The private note the user keeps about this chat, if any.
    pub note: Option<String>,
    /// What the account may do in this chat.
    pub capabilities: CapabilitiesDto,
}

/// Profile of one user, as the engine knows it locally.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct UserProfileDto {
    pub user_id: UserId,
    pub account_id: AccountId,
    pub display_name: Option<String>,
    pub avatar: Option<MediaSource>,
    pub is_me: bool,
    /// An existing direct chat with this user, when one is known.
    pub dm_room_id: Option<RoomId>,
}

// === Timeline ===

/// The body of one timeline item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", content = "data")]
pub enum TimelineItemContent {
    TextMessage {
        body: String,
        /// Network markup, when the network supplied it and the account renders formatted text.
        formatted_body: Option<String>,
        /// The engine already sanitised `formatted_body` and it is safe to render as HTML.
        html_sanitized: bool,
    },
    Media {
        source: MediaSource,
        mime_type: String,
        #[specta(type = Number)]
        size_bytes: u64,
        /// Caption or file name.
        body: String,
        /// `image`, `video`, `audio`, `voice`, `file` or `sticker`.
        media_kind: String,
        width: Option<u32>,
        height: Option<u32>,
        #[specta(type = Option<Number>)]
        duration_ms: Option<u64>,
        /// A recorded voice note (shown with a player instead of a file row).
        voice: bool,
    },
    Location {
        geo_uri: String,
        description: Option<String>,
    },
    /// The message was deleted, locally or for everyone.
    Deleted,
    /// A network event rendered as a neutral line (`"Ada joined"`), never as a bubble.
    System { text: String },
    /// A date separator the engine inserts between days.
    DayDivider {
        #[specta(type = Number)]
        timestamp_millis: u64,
    },
}

/// Delivery state of one message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "status", content = "data")]
pub enum EventSendState {
    /// Stored in the outbox, not handed to the connector yet.
    Queued { transaction_id: String },
    /// Handed to the connector and awaiting its ack.
    Sending { transaction_id: String },
    /// The network accepted the message; `event_id` is set once the echo arrives.
    Sent { event_id: Option<EventId> },
    /// The outbox worker gave up; `can_retry` tells the UI whether RetrySend helps.
    Failed { error: String, can_retry: bool },
}

/// Aggregated reactions for one key on a message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct ReactionSummary {
    pub key: String,
    #[specta(type = Number)]
    pub count: usize,
    pub senders: Vec<UserId>,
    /// Whether the signed-in user of the owning account reacted with this key.
    pub by_me: bool,
}

/// One message in a chat timeline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct TimelineItemDto {
    /// Stable id of this item within the timeline.
    pub internal_id: String,
    /// Local message id; `None` while the local echo has no stored row yet.
    pub event_id: Option<EventId>,
    pub sender: UserId,
    pub sender_display_name: Option<String>,
    pub sender_avatar: Option<MediaSource>,
    #[specta(type = Number)]
    pub timestamp_millis: u64,
    pub content: TimelineItemContent,
    pub send_state: EventSendState,
    pub is_own: bool,
    pub reply_to: Option<Box<TimelineItemDto>>,
    /// Users the network reported as having read this message (newest first).
    pub read_by: Vec<UserId>,
    pub edited: bool,
    pub reactions: Vec<ReactionSummary>,
    /// Transaction id for local echoes (use with RetrySend/CancelSend).
    pub transaction_id: Option<String>,
}

/// One incremental change to a streamed vector (the room list or one timeline).
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

// === Search and shared media ===

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

/// One message matched across chats by `SearchAllMessages`.
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

/// One shared item in a chat: a media or file message, or a message with links.
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
    /// The stored media object; `None` for link items.
    pub source: Option<MediaSource>,
    pub mime_type: Option<String>,
    #[specta(type = Option<Number>)]
    pub size_bytes: Option<u64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// `http(s)` links found in the message (only for `link` items).
    #[serde(default)]
    pub urls: Vec<String>,
}

// === Tags, drafts and forwarding ===

/// A user-defined chat tag, as stored in the device-local tag catalogue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct RoomTagDef {
    pub id: String,
    pub name: String,
    pub colour: TagColour,
}

/// Palette colour of a chat tag.
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

/// A message draft for a chat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct DraftDto {
    pub text: String,
    pub reply_to: Option<EventId>,
}

/// Summary of a chat's draft for `ListDrafts`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct DraftSummary {
    pub room_id: RoomId,
    pub text: String,
}

/// Failure details when forwarding a message to a target chat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct ForwardFailure {
    pub room_id: RoomId,
    pub error: String,
}

// === Commands ===

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "action", content = "params")]
pub enum EngineCommand {
    // Accounts
    /// Lists every locally stored account, with its state and capabilities.
    ListAccounts,
    /// Lists the networks this build can connect (a build without the Messenger sidecar omits it).
    ListNetworks,
    /// Starts a login attempt for `network` and returns the first step.
    BeginLogin {
        network: Network,
    },
    /// Answers the last `LoginStepDto` of the attempt `login_id`.
    SubmitLogin {
        login_id: LoginId,
        input: LoginInputDto,
    },
    /// Abandons the attempt `login_id`; nothing is stored.
    CancelLogin {
        login_id: LoginId,
    },
    /// Removes the account and its local data after revoking the session on the network.
    Logout {
        account_id: AccountId,
    },
    // Chats
    /// Starts the room list stream; `filter` narrows the projection by chat name.
    SubscribeRoomList {
        filter: Option<String>,
    },
    /// Aborts any subscription (room list or timeline).
    Unsubscribe {
        subscription_id: SubscriptionId,
    },
    /// Lists the known members of a chat.
    GetRoomMembers {
        room_id: RoomId,
    },
    GetRoomDetails {
        room_id: RoomId,
    },
    /// Reads one user's profile inside the account that knows them.
    GetUserProfile {
        account_id: AccountId,
        user_id: UserId,
    },
    /// Reads the private note the user keeps about a chat (local, never sent to the network).
    GetRoomNote {
        room_id: RoomId,
    },
    /// Stores the private note; an empty note deletes it.
    SetRoomNote {
        room_id: RoomId,
        note: String,
    },
    // Tag catalogue (device-local, shared by every account)
    /// Reads the tag catalogue in display order.
    GetTagCatalog,
    /// Adds one tag to the catalogue; the engine assigns its id and returns the stored definition.
    CreateRoomTag {
        name: String,
        colour: TagColour,
    },
    /// Renames and/or recolours one tag. At least one of `name`/`colour` must be set.
    RenameRoomTag {
        id: String,
        name: Option<String>,
        colour: Option<TagColour>,
    },
    /// Sets the catalogue's display order. `ids` must list every tag exactly once; a stale list is
    /// refused rather than dropping the entries it omits.
    ReorderRoomTags {
        ids: Vec<String>,
    },
    /// Assigns (`enabled: true`) or removes the tag `tag_id` on a chat.
    SetRoomTag {
        room_id: RoomId,
        tag_id: String,
        enabled: bool,
    },
    /// Removes a tag from the catalogue and from every chat that carries it.
    DeleteRoomTag {
        tag_id: String,
    },
    // Chat flags
    /// Marks the whole chat as read and clears its unread notifications.
    MarkRoomRead {
        room_id: RoomId,
    },
    /// Pins or unpins the chat in the room list.
    SetRoomPinned {
        room_id: RoomId,
        pinned: bool,
    },
    /// Archives or unarchives the chat.
    SetRoomArchived {
        room_id: RoomId,
        archived: bool,
    },
    /// Sets the chat's marked-as-unread flag.
    SetRoomMarkedUnread {
        room_id: RoomId,
        unread: bool,
    },
    /// Mutes the chat until `muted_until` (milliseconds since the Unix epoch), `None` to unmute.
    SetRoomMuted {
        room_id: RoomId,
        #[specta(type = Option<Number>)]
        muted_until: Option<u64>,
    },
    // Local search and shared media
    /// Lists shared media, files or links of a chat, newest first. `before` is the `next` cursor of
    /// the previous page.
    GetRoomMedia {
        room_id: RoomId,
        kind: RoomMediaKind,
        before: Option<String>,
        limit: u32,
    },
    /// Searches the text messages of one chat in the local full-text index, newest first.
    SearchRoomMessages {
        room_id: RoomId,
        query: String,
        limit: u32,
    },
    /// Searches the text messages of every chat in the local full-text index, newest first.
    SearchAllMessages {
        query: String,
        limit: Option<u32>,
    },
    /// Deletes cached downloaded media; it is fetched again when needed.
    ClearMediaCache,
    /// Reconnects every account's connector; live updates resume as they catch up.
    RefreshSync,
    // Timeline
    /// Starts the timeline stream of one chat.
    SubscribeTimeline {
        room_id: RoomId,
    },
    /// Aborts the timeline subscription of one chat and drops its cached timeline. Best-effort: the
    /// next subscription replays a full `Reset`.
    CloseTimeline {
        room_id: RoomId,
    },
    /// Loads older messages into the timeline of one chat.
    PaginateBackwards {
        room_id: RoomId,
        #[specta(type = Number)]
        count: usize,
    },
    SendText {
        room_id: RoomId,
        body: String,
        reply_to: Option<EventId>,
        /// Client-generated id that reconciles the outbox row with the remote echo.
        txn_id: Option<String>,
    },
    EditMessage {
        room_id: RoomId,
        event_id: EventId,
        body: String,
    },
    DeleteMessage {
        room_id: RoomId,
        event_id: EventId,
        for_everyone: bool,
    },
    /// Adds the reaction `key`, or removes it when the user already reacted with it.
    ToggleReaction {
        room_id: RoomId,
        event_id: EventId,
        key: String,
    },
    /// Forwards messages to one or more target chats, dropping replies and reactions.
    ForwardEvents {
        room_id: RoomId,
        event_ids: Vec<EventId>,
        targets: Vec<RoomId>,
    },
    /// Saves a draft for a chat; empty text clears it.
    SaveDraft {
        room_id: RoomId,
        text: String,
        reply_to: Option<EventId>,
    },
    GetDraft {
        room_id: RoomId,
    },
    /// Lists every chat with a non-empty draft (first 80 chars of each).
    ListDrafts,
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
        body: Option<String>,
        txn_id: Option<String>,
    },
    /// Sends a voice note; the file stays the user's, the engine copies it into the store.
    SendVoice {
        room_id: RoomId,
        file_path: String,
        #[specta(type = Number)]
        duration_ms: u64,
        txn_id: Option<String>,
    },
    /// Reports that the user read the chat up to `event_id`.
    MarkRead {
        room_id: RoomId,
        event_id: EventId,
    },
    SetTyping {
        room_id: RoomId,
        typing: bool,
    },
    // Media
    /// Fetches a stored media object into the local file cache and returns its path.
    GetMedia {
        source: MediaSource,
        /// `"thumbnail"` or `"file"`.
        kind: String,
        /// Used to pick the cache file extension.
        mime_type: Option<String>,
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
    // Accounts
    AccountsList {
        accounts: Vec<Account>,
    },
    /// The networks this build can connect.
    Networks {
        networks: Vec<Network>,
    },
    /// The next login step of `login_id`; answers both `BeginLogin` and `SubmitLogin`.
    LoginStep {
        login_id: LoginId,
        step: LoginStepDto,
    },
    LoginCancelled,
    LoggedOut {
        account_id: AccountId,
    },
    // Room list
    RoomListSubscribed {
        subscription_id: SubscriptionId,
    },
    Unsubscribed,
    RoomMembers {
        members: Vec<RoomMemberDto>,
    },
    RoomDetailsRetrieved {
        details: RoomDetails,
    },
    UserProfile {
        profile: UserProfileDto,
    },
    /// The private note of a chat, `None` when none is set.
    RoomNote {
        note: Option<String>,
    },
    RoomNoteSet,
    /// The tag catalogue, in display order.
    TagCatalog {
        tags: Vec<RoomTagDef>,
    },
    /// The tag `CreateRoomTag` added, with the id the engine assigned.
    RoomTagCreated {
        tag: RoomTagDef,
    },
    /// A tag was renamed, recoloured, reordered or removed.
    RoomTagsUpdated,
    /// A chat flag changed (pinned, archived, marked unread, muted or read); the room list diff
    /// carries the new values.
    RoomFlagsUpdated,
    RoomMedia {
        items: Vec<RoomMediaItem>,
        /// Cursor for the next page, `None` when this was the last one.
        next: Option<String>,
    },
    RoomSearchResults {
        hits: Vec<RoomSearchHit>,
    },
    AllMessagesFound {
        hits: Vec<GlobalSearchHit>,
    },
    MediaCacheCleared,
    SyncRefreshed,
    // Timeline
    TimelineSubscribed {
        subscription_id: SubscriptionId,
    },
    TimelineClosed,
    Paginated {
        /// True when the start of the chat history was reached.
        reached_start: bool,
    },
    /// The message was queued; the local echo arrives through `TimelineDiff` and the final state
    /// through the item's `send_state`.
    MessageQueued {
        txn_id: String,
    },
    MessageEdited,
    MessageDeleted,
    ReactionToggled,
    /// Events forwarded; one entry per target chat that failed.
    EventsForwarded {
        failures: Vec<ForwardFailure>,
    },
    DraftSaved,
    /// The chat's draft, `None` when it has none.
    Draft {
        draft: Option<DraftDto>,
    },
    /// Every chat with a non-empty draft.
    Drafts {
        drafts: Vec<DraftSummary>,
    },
    SendRetried,
    SendCancelled,
    ReadMarked,
    Typing,
    // Media
    Media {
        /// Local file path of the cached media.
        path: String,
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
}

// === Errors ===

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct EngineError {
    pub code: String,
    pub message: String,
    /// Stable machine-readable detail for a coarse `code`, for example why a login attempt
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

    /// The network behind the account cannot do `feature`; the UI should have hidden the action.
    pub fn not_supported(feature: impl Into<String>) -> Self {
        let feature = feature.into();
        Self {
            code: "not_supported".to_string(),
            message: format!("Feature '{feature}' is not supported by this network"),
            reason: Some(feature),
        }
    }

    /// The request is valid but cannot run now (no such account, disabled account, no cached data).
    pub fn unavailable(msg: impl Into<String>) -> Self {
        Self {
            code: "unavailable".to_string(),
            message: msg.into(),
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

/// Connection state of one account's connector.
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
#[serde(tag = "type", content = "payload")]
pub enum EngineEvent {
    ConnectionStatus {
        account_id: AccountId,
        status: ConnectionStatus,
    },
    /// The stored account list changed (login finished, logout, state transition).
    AccountsChanged {
        accounts: Vec<Account>,
    },
    RoomListDiff {
        subscription_id: SubscriptionId,
        diff: Box<VectorDiffDto<RoomSummary>>,
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
    /// Attachment upload progress, keyed by the `txn_id` passed to `SendAttachment`/`SendVoice`.
    UploadProgress {
        room_id: RoomId,
        txn_id: String,
        #[specta(type = Number)]
        sent_bytes: u64,
        #[specta(type = Number)]
        total_bytes: u64,
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
        .register::<LoginId>()
        .register::<Network>()
        .register::<AccountState>()
        .register::<CapabilitiesDto>()
        .register::<Account>()
        .register::<CookieFieldDto>()
        .register::<CodeChannel>()
        .register::<LoginFailureDto>()
        .register::<LoginStepDto>()
        .register::<LoginInputDto>()
        .register::<ChatKind>()
        .register::<MediaSource>()
        .register::<NotificationCount>()
        .register::<TimelineItemPreview>()
        .register::<RoomSummary>()
        .register::<RoomMemberDto>()
        .register::<RoomDetails>()
        .register::<UserProfileDto>()
        .register::<TimelineItemContent>()
        .register::<EventSendState>()
        .register::<ReactionSummary>()
        .register::<TimelineItemDto>()
        .register::<RoomSearchHit>()
        .register::<GlobalSearchHit>()
        .register::<RoomMediaKind>()
        .register::<RoomMediaItem>()
        .register::<RoomTagDef>()
        .register::<TagColour>()
        .register::<DraftDto>()
        .register::<DraftSummary>()
        .register::<ForwardFailure>()
        .register::<VectorDiffDto<RoomSummary>>()
        .register::<VectorDiffDto<TimelineItemDto>>()
        .register::<ConnectionStatus>()
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
    use serde_json::json;

    #[test]
    fn test_export_typescript() {
        let ts = export_typescript().expect("export typescript should succeed");
        assert!(ts.contains("export type Account"));
        assert!(ts.contains("export type CapabilitiesDto"));
        assert!(ts.contains("export type LoginStepDto"));
        assert!(ts.contains("export type LoginInputDto"));
        assert!(ts.contains("export type MediaSource"));
        assert!(ts.contains("export type RoomSummary"));
        assert!(ts.contains("export type TimelineItemDto"));
        assert!(ts.contains("export type EngineCommand"));
        assert!(ts.contains("export type EngineResponse"));
        assert!(ts.contains("export type EngineError"));
        assert!(ts.contains("export type EngineEvent"));
        assert!(ts.contains("export type VectorDiffDto"));
    }

    fn sample_summary() -> RoomSummary {
        RoomSummary {
            id: RoomId("chat-1".into()),
            account_id: AccountId("acc-1".into()),
            network: Network::Telegram,
            name: Some("Ada".into()),
            avatar: Some(MediaSource {
                media_id: "media-1".into(),
            }),
            kind: ChatKind::Direct,
            is_direct: true,
            unread_notifications: NotificationCount {
                highlight_count: 1,
                notification_count: 2,
            },
            latest_event: Some(TimelineItemPreview {
                sender: UserId("u-1".into()),
                timestamp_millis: 1_700_000_000_000,
                body: "Hello world".into(),
            }),
            pinned: false,
            archived: false,
            muted_until: None,
            marked_unread: true,
            tags: vec!["work".into()],
            draft: Some("draft".into()),
        }
    }

    #[test]
    fn test_serde_roundtrip() {
        let summary = sample_summary();
        let encoded = serde_json::to_string(&summary).unwrap();
        let decoded: RoomSummary = serde_json::from_str(&encoded).unwrap();
        assert_eq!(summary, decoded);

        let diff = VectorDiffDto::PushBack { value: summary };
        let diff_json = serde_json::to_string(&diff).unwrap();
        let decoded_diff: VectorDiffDto<RoomSummary> = serde_json::from_str(&diff_json).unwrap();
        assert_eq!(diff, decoded_diff);
    }

    #[test]
    fn test_qr_step_roundtrip() {
        let step = LoginStepDto::Qr {
            payload: "tg://login?token=abc".into(),
            expires_at: 1_700_000_060_000,
        };
        let value = serde_json::to_value(&step).unwrap();
        assert_eq!(
            value,
            json!({
                "type": "qr",
                "payload": "tg://login?token=abc",
                "expires_at": 1_700_000_060_000u64,
            })
        );
        let decoded: LoginStepDto = serde_json::from_value(value).unwrap();
        assert_eq!(step, decoded);
    }

    #[test]
    fn test_cookies_input_roundtrip() {
        let input = LoginInputDto::Cookies {
            values: BTreeMap::from([(String::from("c_user"), String::from("1000"))]),
        };
        let value = serde_json::to_value(&input).unwrap();
        assert_eq!(
            value,
            json!({"type": "cookies", "values": {"c_user": "1000"}})
        );
        let decoded: LoginInputDto = serde_json::from_value(value).unwrap();
        assert_eq!(input, decoded);
    }

    #[test]
    fn test_begin_login_wire_shape() {
        let command = EngineCommand::BeginLogin {
            network: Network::Telegram,
        };
        assert_eq!(
            serde_json::to_value(&command).unwrap(),
            json!({"action": "BeginLogin", "params": {"network": "telegram"}})
        );

        let command = EngineCommand::BeginLogin {
            network: Network::Messenger,
        };
        assert_eq!(
            serde_json::to_value(&command).unwrap(),
            json!({"action": "BeginLogin", "params": {"network": "messenger"}})
        );
    }

    #[test]
    fn test_login_input_debug_redacts_secrets() {
        let secrets = ["123456", "hunter2", "cookie-secret"];
        let inputs = [
            LoginInputDto::Code {
                code: secrets[0].into(),
            },
            LoginInputDto::Password {
                password: secrets[1].into(),
            },
            LoginInputDto::Credentials {
                username: "ada".into(),
                password: secrets[1].into(),
            },
            LoginInputDto::Cookies {
                values: BTreeMap::from([(String::from("sessionid"), String::from(secrets[2]))]),
            },
        ];
        for input in &inputs {
            let rendered = format!("{input:?}");
            for secret in secrets {
                assert!(
                    !rendered.contains(secret),
                    "Debug output leaked a secret: {rendered}"
                );
            }
        }

        // Usernames, phone numbers and cookie names stay visible for diagnostics.
        assert!(format!("{:?}", inputs[2]).contains("ada"));
        assert!(format!("{:?}", inputs[3]).contains("sessionid"));
        assert!(
            format!(
                "{:?}",
                LoginInputDto::PhoneNumber {
                    phone: "+15550100".into()
                }
            )
            .contains("+15550100")
        );
    }

    #[test]
    fn test_engine_error_constructors() {
        let err = EngineError::not_implemented("SendMessage");
        assert_eq!(err.code, "not_implemented");
        assert!(err.message.contains("SendMessage"));

        let err = EngineError::auth_failed("Invalid credentials");
        assert_eq!(err.code, "auth_failed");

        let err = EngineError::not_supported("polls");
        assert_eq!(err.code, "not_supported");
        assert!(err.message.contains("polls"));
        assert_eq!(err.reason.as_deref(), Some("polls"));

        let err = EngineError::unavailable("account is disabled");
        assert_eq!(err.code, "unavailable");
        assert_eq!(err.message, "account is disabled");
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
