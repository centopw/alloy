//! Pure conversions from grammers/TL values to connector contract values.
//!
//! Nothing here touches the network, the clock or the session: every function takes TL data (or a
//! string) and returns contract data, which is what the unit tests in this module drive. The
//! connector and the update pump call into it.
//!
//! Remote ids use a one-letter type prefix so an id can never be confused with another peer’s:
//! `u123` is user 123, `c456` is small group 456, `ch789` is channel or supergroup 789. The same
//! spelling is stable across sessions, because it only carries ids Telegram assigned.

use std::time::Duration;

use alloy_connector::{
    ChatKind, CodeChannel, ConnectorError, MediaRef, MessageKind, RemoteChat, RemoteMessage,
};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use grammers_client::media::Downloadable;
use grammers_session::types::{PeerAuth, PeerId, PeerRef};
use grammers_tl_types as tl;
use serde::{Deserialize, Serialize};

/// Which kind of peer an id refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PeerKindTag {
    /// A user account.
    User,
    /// A small group chat.
    Chat,
    /// A broadcast channel, supergroup or megagroup.
    Channel,
}

impl PeerKindTag {
    /// The prefix used in remote ids.
    const fn prefix(self) -> &'static str {
        match self {
            Self::User => "u",
            Self::Chat => "c",
            Self::Channel => "ch",
        }
    }
}

/// Builds the stable remote id for a peer.
pub(crate) fn encode_peer(kind: PeerKindTag, bare_id: i64) -> String {
    format!("{}{bare_id}", kind.prefix())
}

/// Parses a remote id back into a peer kind and bare id.
///
/// Returns `None` for anything that is not exactly `<prefix><positive number>` with an id inside
/// Telegram’s range for that kind, which is how garbage ids are rejected instead of being turned
/// into a plausible-looking peer.
pub(crate) fn decode_peer(id: &str) -> Option<(PeerKindTag, i64)> {
    let (kind, digits) = if let Some(rest) = id.strip_prefix(PeerKindTag::Channel.prefix()) {
        (PeerKindTag::Channel, rest)
    } else if let Some(rest) = id.strip_prefix(PeerKindTag::Chat.prefix()) {
        (PeerKindTag::Chat, rest)
    } else {
        let rest = id.strip_prefix(PeerKindTag::User.prefix())?;
        (PeerKindTag::User, rest)
    };
    let bare_id = digits.parse::<i64>().ok()?;
    peer_id(kind, bare_id)?;
    Some((kind, bare_id))
}

/// The grammers identity for a peer, or `None` when the id is outside the range Telegram uses for
/// that kind.
pub(crate) fn peer_id(kind: PeerKindTag, bare_id: i64) -> Option<PeerId> {
    match kind {
        PeerKindTag::User => PeerId::user(bare_id),
        PeerKindTag::Chat => PeerId::chat(bare_id),
        PeerKindTag::Channel => PeerId::channel(bare_id),
    }
}

/// A resolvable reference for a remote id plus the access hash cached with it.
pub(crate) fn peer_ref(id: &str, auth: i64) -> Option<PeerRef> {
    let (kind, bare_id) = decode_peer(id)?;
    Some(PeerRef {
        id: peer_id(kind, bare_id)?,
        auth: PeerAuth::from_hash(auth),
    })
}

/// The connector id of a TL peer.
pub(crate) fn tl_peer(peer: &tl::enums::Peer) -> Option<(PeerKindTag, i64)> {
    match peer {
        tl::enums::Peer::User(user) => Some((PeerKindTag::User, user.user_id)),
        tl::enums::Peer::Chat(chat) => Some((PeerKindTag::Chat, chat.chat_id)),
        tl::enums::Peer::Channel(channel) => Some((PeerKindTag::Channel, channel.channel_id)),
    }
}

/// The peer kind grammers reports, in the connector’s spelling.
pub(crate) fn tag_of(peer: PeerRef) -> Option<(PeerKindTag, i64)> {
    let bare_id = peer.id.bare_id()?;
    let kind = match peer.id.kind() {
        grammers_session::types::PeerKind::User => PeerKindTag::User,
        grammers_session::types::PeerKind::Chat => PeerKindTag::Chat,
        grammers_session::types::PeerKind::Channel => PeerKindTag::Channel,
    };
    Some((kind, bare_id))
}

/// Epoch seconds as the `i64` milliseconds the contract uses.
pub(crate) fn seconds_to_ms(seconds: i32) -> i64 {
    i64::from(seconds) * 1_000
}

/// Current time in milliseconds since the Unix epoch.
pub(crate) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or_default()
}

/// Parses a `RemoteMessage::remote_msg_id`.
pub(crate) fn parse_msg_id(id: &str) -> Result<i32, ConnectorError> {
    id.parse::<i32>().map_err(|_| ConnectorError::InvalidInput {
        message: format!("invalid message id: {id}"),
    })
}

/// The chat a TL message belongs to.
pub(crate) fn message_chat(raw: &tl::enums::Message) -> Option<String> {
    let peer = match raw {
        tl::enums::Message::Empty(message) => message.peer_id.as_ref()?,
        tl::enums::Message::Message(message) => &message.peer_id,
        tl::enums::Message::Service(message) => &message.peer_id,
    };
    let (kind, bare_id) = tl_peer(peer)?;
    Some(encode_peer(kind, bare_id))
}

/// The id of a TL message, whichever constructor it uses.
pub(crate) fn message_id(raw: &tl::enums::Message) -> i32 {
    raw.id()
}

/// The date of a TL message, in seconds, whichever constructor it uses.
pub(crate) fn raw_date(raw: &tl::enums::Message) -> Option<i32> {
    match raw {
        tl::enums::Message::Empty(_) => None,
        tl::enums::Message::Message(message) => Some(message.date),
        tl::enums::Message::Service(message) => Some(message.date),
    }
}

/// Display name of a TL user: first plus last name, falling back to the username.
fn user_full_name(user: &tl::types::User) -> String {
    let first = user.first_name.as_deref().unwrap_or_default();
    let last = user.last_name.as_deref().unwrap_or_default();
    let full = format!("{first} {last}");
    let full = full.trim();
    if !full.is_empty() {
        return full.to_string();
    }
    match user.username.as_deref() {
        Some(username) => format!("@{username}"),
        None => "Telegram account".to_string(),
    }
}

/// Converts a TL message into a contract message.
///
/// `self_id` is this account’s bare user id, used to attribute outgoing messages in direct chats
/// that omit `from_id`. `txn_id` is set only for the echo of a local send.
pub(crate) fn message_from_raw(
    raw: &tl::enums::Message,
    chat_id: &str,
    self_id: Option<i64>,
    txn_id: Option<String>,
) -> Option<RemoteMessage> {
    match raw {
        tl::enums::Message::Empty(_) => None,
        tl::enums::Message::Message(message) => {
            Some(chat_message(message, chat_id, self_id, txn_id))
        }
        tl::enums::Message::Service(message) => Some(service_message(message, chat_id, self_id)),
    }
}

/// The plain body and the HTML body of a TL text plus its entities.
pub(crate) fn text_and_formatted(
    text: &str,
    entities: Option<&[tl::enums::MessageEntity]>,
) -> (Option<String>, Option<String>) {
    let body = if text.is_empty() {
        None
    } else {
        Some(text.to_string())
    };
    let formatted = match entities {
        Some(entities) if !entities.is_empty() => Some(
            grammers_client::parsers::generate_html_message(text, entities),
        ),
        _ => None,
    };
    (body, formatted)
}

fn chat_message(
    message: &tl::types::Message,
    chat_id: &str,
    self_id: Option<i64>,
    txn_id: Option<String>,
) -> RemoteMessage {
    let (mut body, formatted) = text_and_formatted(&message.message, message.entities.as_deref());
    let mut kind = MessageKind::Text;
    let mut media = None;

    if let Some(raw_media) = &message.media
        && let tl::enums::MessageMedia::Geo(geo) = raw_media
        && let tl::enums::GeoPoint::Point(point) = &geo.geo
    {
        kind = MessageKind::Location;
        body = Some(format!("geo:{},{}", point.lat, point.long));
    } else if let Some(raw_media) = &message.media
        && let Some((classified, reference)) = classify_media(raw_media, chat_id, message.id)
    {
        kind = classified;
        media = Some(reference);
    }

    RemoteMessage {
        remote_chat_id: chat_id.to_string(),
        remote_msg_id: message.id.to_string(),
        txn_id,
        sender_remote_id: sender_of(
            message.from_id.as_ref(),
            &message.peer_id,
            message.out,
            self_id,
            chat_id,
        ),
        is_own: message.out,
        sent_at: seconds_to_ms(message.date),
        edited_at: message.edit_date.map(seconds_to_ms),
        kind,
        body,
        formatted,
        reply_to_remote_msg_id: reply_id_of(message.reply_to.as_ref()),
        media,
    }
}

fn service_message(
    message: &tl::types::MessageService,
    chat_id: &str,
    self_id: Option<i64>,
) -> RemoteMessage {
    RemoteMessage {
        remote_chat_id: chat_id.to_string(),
        remote_msg_id: message.id.to_string(),
        txn_id: None,
        sender_remote_id: sender_of(
            message.from_id.as_ref(),
            &message.peer_id,
            message.out,
            self_id,
            chat_id,
        ),
        is_own: message.out,
        sent_at: seconds_to_ms(message.date),
        edited_at: None,
        kind: MessageKind::System,
        body: Some(service_body(&message.action)),
        formatted: None,
        reply_to_remote_msg_id: reply_id_of(message.reply_to.as_ref()),
        media: None,
    }
}

/// Who sent a message.
///
/// Direct chats omit `from_id` since layer 119, so the peer is the sender there; for an outgoing
/// message without `from_id` it is this account. Anything unattributable falls back to the chat id,
/// which keeps the field non-empty for the host’s sender index.
fn sender_of(
    from_id: Option<&tl::enums::Peer>,
    peer_id: &tl::enums::Peer,
    out: bool,
    self_id: Option<i64>,
    chat_id: &str,
) -> String {
    if let Some((kind, bare_id)) = from_id.and_then(tl_peer) {
        return encode_peer(kind, bare_id);
    }
    if let Some((kind, bare_id)) = tl_peer(peer_id)
        && kind == PeerKindTag::User
    {
        return if out {
            self_id.map_or_else(
                || encode_peer(kind, bare_id),
                |id| encode_peer(PeerKindTag::User, id),
            )
        } else {
            encode_peer(kind, bare_id)
        };
    }
    chat_id.to_string()
}

fn reply_id_of(header: Option<&tl::enums::MessageReplyHeader>) -> Option<String> {
    let tl::enums::MessageReplyHeader::Header(header) = header? else {
        return None;
    };
    header.reply_to_msg_id.map(|id| id.to_string())
}

/// Classifies attached media and builds its opaque pointer.
fn classify_media(
    media: &tl::enums::MessageMedia,
    chat_id: &str,
    msg_id: i32,
) -> Option<(MessageKind, MediaRef)> {
    if let tl::enums::MessageMedia::Photo(photo) = media {
        return photo_ref(photo, chat_id, msg_id).map(|reference| (MessageKind::Image, reference));
    }
    if let tl::enums::MessageMedia::Document(document) = media {
        return document_ref(document, chat_id, msg_id);
    }
    None
}

/// The largest downloadable rendition of a photo, as `(type, width, height, bytes)`.
fn largest_photo_size(
    sizes: &[tl::enums::PhotoSize],
) -> (String, Option<u32>, Option<u32>, Option<usize>) {
    let mut best: Option<(String, i32, i32, i64)> = None;
    for size in sizes {
        let candidate = match size {
            tl::enums::PhotoSize::Size(size) => {
                Some((size.r#type.clone(), size.w, size.h, i64::from(size.size)))
            }
            tl::enums::PhotoSize::Progressive(size) => size
                .sizes
                .last()
                .map(|bytes| (size.r#type.clone(), size.w, size.h, i64::from(*bytes))),
            _ => None,
        };
        if let Some(candidate) = candidate
            && best.as_ref().is_none_or(|current| candidate.3 > current.3)
        {
            best = Some(candidate);
        }
    }
    match best {
        Some((thumb, width, height, bytes)) => (
            thumb,
            u32::try_from(width).ok(),
            u32::try_from(height).ok(),
            usize::try_from(bytes).ok(),
        ),
        // Telegram always ships at least a stripped preview; `w` is the type official clients fall
        // back to when no downloadable rendition is listed.
        None => ("w".to_string(), None, None, None),
    }
}

fn photo_ref(media: &tl::types::MessageMediaPhoto, chat_id: &str, msg_id: i32) -> Option<MediaRef> {
    let tl::enums::Photo::Photo(photo) = media.photo.as_ref()? else {
        return None;
    };
    let (thumb, width, height, size) = largest_photo_size(&photo.sizes);
    let pointer = MediaPointer {
        kind: PointerKind::Photo,
        chat: chat_id.to_string(),
        msg: msg_id,
        id: photo.id,
        access_hash: photo.access_hash,
        file_reference: BASE64.encode(&photo.file_reference),
        dc_id: Some(photo.dc_id),
        thumb,
        size: size.map(|bytes| bytes as u64),
    };
    Some(MediaRef {
        remote_ref: encode_pointer(&pointer),
        mime: "image/jpeg".to_string(),
        size_bytes: size.map(|bytes| bytes as u64),
        width,
        height,
        duration_ms: None,
        file_name: Some(format!("photo-{}.jpg", photo.id)),
    })
}

fn document_ref(
    media: &tl::types::MessageMediaDocument,
    chat_id: &str,
    msg_id: i32,
) -> Option<(MessageKind, MediaRef)> {
    let tl::enums::Document::Document(document) = media.document.as_ref()? else {
        return None;
    };

    let mut sticker = false;
    let mut audio = false;
    let mut voice = media.voice;
    let mut video = media.video;
    let mut duration_ms = None;
    let mut width = None;
    let mut height = None;
    let mut file_name = None;

    for attribute in &document.attributes {
        match attribute {
            tl::enums::DocumentAttribute::Sticker(_) => sticker = true,
            tl::enums::DocumentAttribute::Audio(attribute) => {
                audio = true;
                voice = voice || attribute.voice;
                duration_ms = u64::try_from(attribute.duration)
                    .ok()
                    .map(|seconds| seconds * 1_000);
            }
            tl::enums::DocumentAttribute::Video(attribute) => {
                video = true;
                if attribute.duration > 0.0 {
                    duration_ms = Some((attribute.duration * 1_000.0).round() as u64);
                }
                width = u32::try_from(attribute.w).ok();
                height = u32::try_from(attribute.h).ok();
            }
            tl::enums::DocumentAttribute::ImageSize(attribute) => {
                width = u32::try_from(attribute.w).ok();
                height = u32::try_from(attribute.h).ok();
            }
            tl::enums::DocumentAttribute::Filename(attribute) => {
                file_name = Some(attribute.file_name.clone())
            }
            _ => {}
        }
    }

    let mime = if document.mime_type.is_empty() {
        "application/octet-stream".to_string()
    } else {
        document.mime_type.clone()
    };
    let kind = if sticker {
        MessageKind::Sticker
    } else if voice {
        MessageKind::Voice
    } else if video || mime.starts_with("video/") {
        MessageKind::Video
    } else if audio || mime.starts_with("audio/") {
        MessageKind::Audio
    } else {
        MessageKind::File
    };

    let size = u64::try_from(document.size).ok();
    let pointer = MediaPointer {
        kind: PointerKind::Document,
        chat: chat_id.to_string(),
        msg: msg_id,
        id: document.id,
        access_hash: document.access_hash,
        file_reference: BASE64.encode(&document.file_reference),
        dc_id: Some(document.dc_id),
        thumb: String::new(),
        size,
    };
    let reference = MediaRef {
        remote_ref: encode_pointer(&pointer),
        file_name,
        mime,
        size_bytes: size,
        width,
        height,
        duration_ms,
    };
    Some((kind, reference))
}

/// A peer as the dialog list describes it, ready to become a [`RemoteChat`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PeerEntry {
    /// Connector id (`u123`).
    pub id: String,
    /// Telegram’s bare id.
    pub bare_id: i64,
    /// Which kind of peer this is.
    pub kind: PeerKindTag,
    /// Access hash to resolve the peer later; `0` when Telegram did not give us one.
    pub auth: i64,
    /// Chat kind for the contract.
    pub chat_kind: ChatKind,
    /// Display title.
    pub title: String,
    /// Avatar pointer, when the peer has a picture.
    pub avatar: Option<MediaRef>,
}

/// Builds a peer entry from a TL user.
pub(crate) fn entry_from_user(user: &tl::enums::User) -> Option<PeerEntry> {
    let tl::enums::User::User(user) = user else {
        return None;
    };
    let kind = PeerKindTag::User;
    let id = encode_peer(kind, user.id);
    let auth = if user.min {
        0
    } else {
        user.access_hash.unwrap_or_default()
    };
    Some(PeerEntry {
        avatar: user_avatar(&id, user.photo.as_ref()),
        bare_id: user.id,
        chat_kind: ChatKind::Direct,
        id,
        kind,
        auth,
        title: user_full_name(user),
    })
}

/// Builds a peer entry from a TL chat, group or channel.
///
/// Communities and malformed entries are not chats a user can open here, so they are dropped;
/// the caller simply does not list them.
pub(crate) fn entry_from_chat(chat: &tl::enums::Chat) -> Option<PeerEntry> {
    match chat {
        tl::enums::Chat::Chat(group) => {
            let id = encode_peer(PeerKindTag::Chat, group.id);
            Some(PeerEntry {
                avatar: chat_avatar(&id, &group.photo),
                bare_id: group.id,
                chat_kind: ChatKind::Group,
                id,
                kind: PeerKindTag::Chat,
                auth: 0,
                title: group.title.clone(),
            })
        }
        tl::enums::Chat::Forbidden(group) => {
            let id = encode_peer(PeerKindTag::Chat, group.id);
            Some(PeerEntry {
                avatar: None,
                bare_id: group.id,
                chat_kind: ChatKind::Group,
                id,
                kind: PeerKindTag::Chat,
                auth: 0,
                title: group.title.clone(),
            })
        }
        tl::enums::Chat::Channel(channel) => {
            let id = encode_peer(PeerKindTag::Channel, channel.id);
            Some(PeerEntry {
                avatar: chat_avatar(&id, &channel.photo),
                bare_id: channel.id,
                chat_kind: if channel.megagroup {
                    ChatKind::Group
                } else {
                    ChatKind::Channel
                },
                id,
                kind: PeerKindTag::Channel,
                auth: if channel.min {
                    0
                } else {
                    channel.access_hash.unwrap_or_default()
                },
                title: channel.title.clone(),
            })
        }
        tl::enums::Chat::ChannelForbidden(channel) => {
            let id = encode_peer(PeerKindTag::Channel, channel.id);
            Some(PeerEntry {
                avatar: None,
                bare_id: channel.id,
                chat_kind: if channel.megagroup {
                    ChatKind::Group
                } else {
                    ChatKind::Channel
                },
                id,
                kind: PeerKindTag::Channel,
                auth: channel.access_hash,
                title: channel.title.clone(),
            })
        }
        _ => None,
    }
}

/// The id of a dialog’s newest message, used as the offset for the next page.
pub(crate) fn dialog_top_message(dialog: &tl::enums::Dialog) -> i32 {
    match dialog {
        tl::enums::Dialog::Dialog(dialog) => dialog.top_message,
        _ => 0,
    }
}

/// The peer a dialog entry belongs to, or `None` for the archive-folder pseudo-dialogs.
pub(crate) fn dialog_peer(dialog: &tl::enums::Dialog) -> Option<(PeerKindTag, i64)> {
    match dialog {
        tl::enums::Dialog::Dialog(dialog) => tl_peer(&dialog.peer),
        _ => None,
    }
}

/// The newest activity of a dialog’s top message, in milliseconds, given the message dates in the
/// same response.
pub(crate) fn dialog_activity(
    dialog: &tl::enums::Dialog,
    dates: &std::collections::HashMap<(String, i32), i32>,
) -> i64 {
    let tl::enums::Dialog::Dialog(dialog) = dialog else {
        return 0;
    };
    let Some((kind, bare_id)) = tl_peer(&dialog.peer) else {
        return 0;
    };
    dates
        .get(&(encode_peer(kind, bare_id), dialog.top_message))
        .map_or(0, |seconds| seconds_to_ms(*seconds))
}

/// Converts a dialog plus its peer entry into a [`RemoteChat`].
pub(crate) fn chat_from_dialog(
    dialog: &tl::enums::Dialog,
    entry: &PeerEntry,
    activity_ms: i64,
    now_seconds: i64,
) -> Option<RemoteChat> {
    let tl::enums::Dialog::Dialog(dialog) = dialog else {
        return None;
    };
    let settings: tl::types::PeerNotifySettings = dialog.notify_settings.clone().into();
    let muted_until = settings.mute_until.unwrap_or_default();
    Some(RemoteChat {
        remote_chat_id: entry.id.clone(),
        kind: entry.chat_kind,
        title: entry.title.clone(),
        avatar: entry.avatar.clone(),
        last_activity_at: activity_ms,
        unread_count: u32::try_from(dialog.unread_count).unwrap_or_default(),
        is_pinned: dialog.pinned,
        is_archived: dialog.folder_id == Some(1),
        is_muted: i64::from(muted_until) > now_seconds,
    })
}

/// A resume position inside the dialog list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ChatCursor {
    /// 0 for the main folder, 1 for the archive folder.
    pub folder: i32,
    /// `offset_date` for `messages.getDialogs`, in seconds.
    pub date: i32,
    /// `offset_id` for `messages.getDialogs`.
    pub id: i32,
    /// Connector id of the last dialog’s peer, if any.
    pub peer: Option<String>,
}

/// Renders the opaque cursor handed to the host.
pub(crate) fn encode_cursor(cursor: &ChatCursor) -> String {
    serde_json::to_string(cursor).expect("a chat cursor is always serializable")
}

/// Parses an opaque cursor handed back by the host.
pub(crate) fn decode_cursor(cursor: &str) -> Result<ChatCursor, ConnectorError> {
    serde_json::from_str(cursor).map_err(|error| ConnectorError::InvalidInput {
        message: format!("invalid chat cursor: {error}"),
    })
}

/// How a login code was delivered, from TL’s `auth.SentCodeType`.
pub(crate) fn code_channel(kind: &tl::enums::auth::SentCodeType) -> (CodeChannel, Option<u8>) {
    fn length(value: i32) -> Option<u8> {
        u8::try_from(value).ok().filter(|length| *length > 0)
    }

    match kind {
        tl::enums::auth::SentCodeType::App(sent) => (CodeChannel::App, length(sent.length)),
        tl::enums::auth::SentCodeType::Sms(sent) => (CodeChannel::Sms, length(sent.length)),
        tl::enums::auth::SentCodeType::Call(sent) => (CodeChannel::Sms, length(sent.length)),
        tl::enums::auth::SentCodeType::EmailCode(sent) => (CodeChannel::Email, length(sent.length)),
        tl::enums::auth::SentCodeType::SetUpEmailRequired(_) => (CodeChannel::Email, None),
        tl::enums::auth::SentCodeType::MissedCall(sent) => (CodeChannel::Sms, length(sent.length)),
        _ => (CodeChannel::Sms, None),
    }
}

/// Short English description of a service message, for [`MessageKind::System`].
pub(crate) fn service_body(action: &tl::enums::MessageAction) -> String {
    match action {
        tl::enums::MessageAction::ChatCreate(action) => {
            format!("created the group \"{}\"", action.title)
        }
        tl::enums::MessageAction::ChatEditTitle(action) => {
            format!("changed the group name to \"{}\"", action.title)
        }
        tl::enums::MessageAction::ChatAddUser(_) => "added members to the group".to_string(),
        tl::enums::MessageAction::ChatDeleteUser(_) => {
            "removed a member from the group".to_string()
        }
        tl::enums::MessageAction::ChatJoinedByLink(_) => "joined via an invite link".to_string(),
        tl::enums::MessageAction::ChannelCreate(action) => {
            format!("created the channel \"{}\"", action.title)
        }
        tl::enums::MessageAction::ChatMigrateTo(_) => {
            "moved this group to a supergroup".to_string()
        }
        tl::enums::MessageAction::ChannelMigrateFrom(action) => {
            format!("moved the group \"{}\" here", action.title)
        }
        tl::enums::MessageAction::ChatEditPhoto(_) => "changed the chat photo".to_string(),
        tl::enums::MessageAction::ChatDeletePhoto => "removed the chat photo".to_string(),
        tl::enums::MessageAction::PinMessage => "pinned a message".to_string(),
        tl::enums::MessageAction::ScreenshotTaken => "took a screenshot".to_string(),
        tl::enums::MessageAction::SetMessagesTtl(_) => {
            "changed the message auto-delete timer".to_string()
        }
        _ => "service message".to_string(),
    }
}

fn user_avatar(chat: &str, photo: Option<&tl::enums::UserProfilePhoto>) -> Option<MediaRef> {
    let tl::enums::UserProfilePhoto::Photo(photo) = photo? else {
        return None;
    };
    Some(avatar_ref(chat, photo.photo_id))
}

fn chat_avatar(chat: &str, photo: &tl::enums::ChatPhoto) -> Option<MediaRef> {
    let tl::enums::ChatPhoto::Photo(photo) = photo else {
        return None;
    };
    Some(avatar_ref(chat, photo.photo_id))
}

fn avatar_ref(chat: &str, photo_id: i64) -> MediaRef {
    let pointer = MediaPointer {
        kind: PointerKind::Avatar,
        chat: chat.to_string(),
        msg: 0,
        id: photo_id,
        access_hash: 0,
        file_reference: String::new(),
        dc_id: None,
        thumb: String::new(),
        size: None,
    };
    MediaRef {
        remote_ref: encode_pointer(&pointer),
        mime: "image/jpeg".to_string(),
        size_bytes: None,
        width: None,
        height: None,
        duration_ms: None,
        file_name: Some(format!("avatar-{photo_id}.jpg")),
    }
}

/// What a [`MediaPointer`] points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PointerKind {
    /// A photo message.
    Photo,
    /// A document message (video, audio, voice, sticker, file).
    Document,
    /// A chat or user profile picture.
    Avatar,
}

/// The decoded form of `MediaRef::remote_ref`.
///
/// It carries everything needed to rebuild a file location, plus the message it came from so an
/// expired `file_reference` can be refreshed by re-fetching that message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct MediaPointer {
    /// What is being downloaded.
    pub kind: PointerKind,
    /// Connector id of the chat the media lives in.
    pub chat: String,
    /// Message id, or `0` for avatars.
    pub msg: i32,
    /// Photo or document id.
    pub id: i64,
    /// Access hash of the photo or document.
    pub access_hash: i64,
    /// Base64 (standard, padded) `file_reference`.
    pub file_reference: String,
    /// Datacenter hosting the file, when known.
    pub dc_id: Option<i32>,
    /// Thumbnail type for photos.
    pub thumb: String,
    /// Size in bytes when the network reported it.
    pub size: Option<u64>,
}

/// Renders a media pointer as the opaque `remote_ref` the host stores.
pub(crate) fn encode_pointer(pointer: &MediaPointer) -> String {
    serde_json::to_string(pointer).expect("a media pointer is always serializable")
}

/// Parses a `remote_ref` produced by [`encode_pointer`].
pub(crate) fn decode_pointer(reference: &str) -> Result<MediaPointer, ConnectorError> {
    serde_json::from_str(reference).map_err(|error| ConnectorError::InvalidInput {
        message: format!("invalid media reference: {error}"),
    })
}

/// A file location ready for `Client::iter_download`.
#[derive(Debug, Clone)]
pub(crate) struct MediaLocation {
    /// Where Telegram stores the file.
    pub location: tl::enums::InputFileLocation,
    /// Size in bytes, when known.
    pub size: Option<usize>,
    /// Datacenter hosting the file.
    pub dc_id: Option<i32>,
}

impl Downloadable for MediaLocation {
    fn to_raw_input_location(&self) -> Option<tl::enums::InputFileLocation> {
        Some(self.location.clone())
    }

    fn size(&self) -> Option<usize> {
        self.size
    }

    fn dc_id(&self) -> Option<i32> {
        self.dc_id
    }
}

/// The downloadable location of a message’s media, if it has any.
pub(crate) fn location_for(media: &tl::enums::MessageMedia) -> Option<MediaLocation> {
    if let tl::enums::MessageMedia::Photo(photo) = media {
        let tl::enums::Photo::Photo(photo) = photo.photo.as_ref()? else {
            return None;
        };
        let (thumb, _, _, size) = largest_photo_size(&photo.sizes);
        return Some(MediaLocation {
            location: tl::types::InputPhotoFileLocation {
                id: photo.id,
                access_hash: photo.access_hash,
                file_reference: photo.file_reference.clone(),
                thumb_size: thumb,
            }
            .into(),
            size,
            dc_id: Some(photo.dc_id),
        });
    }
    if let tl::enums::MessageMedia::Document(document) = media {
        let tl::enums::Document::Document(document) = document.document.as_ref()? else {
            return None;
        };
        return Some(MediaLocation {
            location: tl::types::InputDocumentFileLocation {
                id: document.id,
                access_hash: document.access_hash,
                file_reference: document.file_reference.clone(),
                thumb_size: String::new(),
            }
            .into(),
            size: usize::try_from(document.size).ok(),
            dc_id: Some(document.dc_id),
        });
    }
    None
}

/// The location of a stored [`MediaPointer`], without touching the network.
pub(crate) fn pointer_location(
    pointer: &MediaPointer,
    peer: PeerRef,
) -> Result<MediaLocation, ConnectorError> {
    if pointer.kind == PointerKind::Avatar {
        return Ok(MediaLocation {
            location: tl::types::InputPeerPhotoFileLocation {
                big: true,
                peer: peer.into(),
                photo_id: pointer.id,
            }
            .into(),
            size: None,
            dc_id: None,
        });
    }

    let file_reference =
        BASE64
            .decode(&pointer.file_reference)
            .map_err(|error| ConnectorError::InvalidInput {
                message: format!("invalid media reference: {error}"),
            })?;
    let size = pointer.size.and_then(|bytes| usize::try_from(bytes).ok());
    let location = if pointer.kind == PointerKind::Photo {
        tl::types::InputPhotoFileLocation {
            id: pointer.id,
            access_hash: pointer.access_hash,
            file_reference,
            thumb_size: pointer.thumb.clone(),
        }
        .into()
    } else {
        tl::types::InputDocumentFileLocation {
            id: pointer.id,
            access_hash: pointer.access_hash,
            file_reference,
            thumb_size: String::new(),
        }
        .into()
    };
    Ok(MediaLocation {
        location,
        size,
        dc_id: pointer.dc_id,
    })
}

/// MIME type guessed from a file name, for uploads that arrive without one.
pub(crate) fn mime_from_name(name: &str) -> String {
    let extension = name
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let mime = match extension.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "heic" => "image/heic",
        "mp4" => "video/mp4",
        "mov" => "video/quicktime",
        "webm" => "video/webm",
        "mkv" => "video/x-matroska",
        "ogg" | "oga" | "opus" => "audio/ogg",
        "mp3" => "audio/mpeg",
        "m4a" => "audio/mp4",
        "wav" => "audio/wav",
        "flac" => "audio/flac",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "txt" => "text/plain",
        _ => "application/octet-stream",
    };
    mime.to_string()
}

/// Duration of an Ogg file, read from the granule position of its last page.
///
/// Telegram’s audio attribute needs a whole number of seconds and an Ogg voice note does not
/// announce one anywhere else. Opus in Ogg always counts granules at 48 kHz. Returns `None` when
/// the data does not look like Ogg, so the caller can send the file without audio metadata instead
/// of lying about its length.
pub(crate) fn ogg_duration(data: &[u8]) -> Option<Duration> {
    const MAX_TAIL: usize = 64 * 1024;
    const PAGE_HEADER: usize = 14;

    let window = &data[data.len().saturating_sub(MAX_TAIL)..];
    let mut last_granule = None;
    let mut index = 0;
    while index + PAGE_HEADER <= window.len() {
        if &window[index..index + 4] != b"OggS" {
            index += 1;
            continue;
        }
        let mut granule = [0u8; 8];
        granule.copy_from_slice(&window[index + 6..index + PAGE_HEADER]);
        let granule = u64::from_le_bytes(granule);
        // `-1` marks a page whose granule position is not set (e.g. a continued header page).
        if granule != u64::MAX {
            last_granule = Some(granule);
        }
        index += PAGE_HEADER;
    }
    let granule = last_granule?;
    Some(Duration::from_millis(
        granule.saturating_mul(1_000) / 48_000,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user_peer(id: i64) -> tl::enums::Peer {
        tl::types::PeerUser { user_id: id }.into()
    }

    fn base_message(id: i32, out: bool) -> tl::types::Message {
        tl::types::Message {
            out,
            mentioned: false,
            media_unread: false,
            silent: false,
            post: false,
            from_scheduled: false,
            legacy: false,
            edit_hide: false,
            pinned: false,
            noforwards: false,
            video_processing_pending: false,
            paid_suggested_post_stars: false,
            invert_media: false,
            id,
            from_id: Some(user_peer(7)),
            from_boosts_applied: None,
            from_rank: None,
            peer_id: user_peer(7),
            saved_peer_id: None,
            fwd_from: None,
            via_bot_id: None,
            via_business_bot_id: None,
            guestchat_via_from: None,
            reply_to: None,
            date: 1_700_000_000,
            message: String::new(),
            media: None,
            reply_markup: None,
            entities: None,
            views: None,
            forwards: None,
            replies: None,
            edit_date: None,
            post_author: None,
            grouped_id: None,
            reactions: None,
            restriction_reason: None,
            ttl_period: None,
            quick_reply_shortcut_id: None,
            effect: None,
            factcheck: None,
            report_delivery_until_date: None,
            paid_message_stars: None,
            paid_suggested_post_ton: false,
            suggested_post: None,
            schedule_repeat_period: None,
            summary_from_language: None,
            rich_message: None,
            offline: false,
        }
    }

    fn text_message(id: i32, text: &str) -> tl::enums::Message {
        tl::enums::Message::Message(tl::types::Message {
            message: text.to_string(),
            ..base_message(id, false)
        })
    }

    fn photo(id: i64) -> tl::enums::Photo {
        tl::enums::Photo::Photo(tl::types::Photo {
            has_stickers: false,
            id,
            access_hash: 9_001,
            file_reference: vec![1, 2, 3],
            date: 1_700_000_000,
            sizes: vec![
                tl::enums::PhotoSize::PhotoStrippedSize(tl::types::PhotoStrippedSize {
                    r#type: "i".to_string(),
                    bytes: vec![0; 8],
                }),
                tl::enums::PhotoSize::Size(tl::types::PhotoSize {
                    r#type: "y".to_string(),
                    w: 1280,
                    h: 720,
                    size: 424_242,
                }),
            ],
            video_sizes: None,
            dc_id: 2,
        })
    }

    fn photo_media(id: i64) -> tl::enums::MessageMedia {
        tl::types::MessageMediaPhoto {
            spoiler: false,
            live_photo: false,
            photo: Some(photo(id)),
            ttl_seconds: None,
            video: None,
        }
        .into()
    }

    fn document(
        attributes: Vec<tl::enums::DocumentAttribute>,
        mime_type: &str,
    ) -> tl::enums::Document {
        tl::enums::Document::Document(tl::types::Document {
            id: 777,
            access_hash: 4242,
            file_reference: vec![4, 5, 6],
            date: 1_700_000_000,
            mime_type: mime_type.to_string(),
            size: 55_000,
            thumbs: None,
            video_thumbs: None,
            dc_id: 4,
            attributes,
        })
    }

    fn document_media(
        document: tl::enums::Document,
        voice: bool,
        video: bool,
    ) -> tl::enums::MessageMedia {
        tl::types::MessageMediaDocument {
            nopremium: false,
            spoiler: false,
            video,
            round: false,
            voice,
            document: Some(document),
            alt_documents: None,
            video_cover: None,
            video_timestamp: None,
            ttl_seconds: None,
        }
        .into()
    }

    fn with_media(id: i32, media: tl::enums::MessageMedia) -> tl::enums::Message {
        tl::enums::Message::Message(tl::types::Message {
            media: Some(media),
            ..base_message(id, false)
        })
    }

    #[test]
    fn peer_ids_round_trip() {
        for (kind, bare_id) in [
            (PeerKindTag::User, 12_345),
            (PeerKindTag::Chat, 987),
            (PeerKindTag::Channel, 1_234_567_890),
        ] {
            let encoded = encode_peer(kind, bare_id);
            assert_eq!(decode_peer(&encoded), Some((kind, bare_id)), "{encoded}");
        }
        assert_eq!(encode_peer(PeerKindTag::Channel, 5), "ch5");
        assert_eq!(encode_peer(PeerKindTag::Chat, 5), "c5");
    }

    #[test]
    fn garbage_peer_ids_are_rejected() {
        for garbage in [
            "",
            "u",
            "c",
            "ch",
            "x1",
            "u-1",
            "u0",
            "u1.5",
            "uabc",
            "u 1",
            "U1",
            "c99999999999999",
        ] {
            assert_eq!(decode_peer(garbage), None, "{garbage:?} should not decode");
        }
    }

    #[test]
    fn peer_refs_carry_the_access_hash() {
        let reference = peer_ref("ch42", -99).expect("valid id");

        assert_eq!(reference.id, PeerId::channel_unchecked(42));
        assert_eq!(reference.auth.hash(), -99);
        assert!(peer_ref("nonsense", 0).is_none());
    }

    #[test]
    fn text_messages_carry_body_sender_and_timestamp() {
        let raw = text_message(31, "hello there");

        let message = message_from_raw(&raw, "u7", None, None).expect("text messages map");

        assert_eq!(message.remote_msg_id, "31");
        assert_eq!(message.remote_chat_id, "u7");
        assert_eq!(message.sender_remote_id, "u7");
        assert_eq!(message.kind, MessageKind::Text);
        assert_eq!(message.body.as_deref(), Some("hello there"));
        assert_eq!(message.formatted, None);
        assert_eq!(message.sent_at, 1_700_000_000_000);
        assert!(!message.is_own);
        assert_eq!(message.txn_id, None);
    }

    #[test]
    fn outgoing_messages_without_a_sender_use_the_account_id() {
        let raw = tl::enums::Message::Message(tl::types::Message {
            out: true,
            from_id: None,
            message: "mine".to_string(),
            ..base_message(5, true)
        });

        let message = message_from_raw(&raw, "u7", Some(99), None).expect("mapped");

        assert_eq!(message.sender_remote_id, "u99");
        assert!(message.is_own);
    }

    #[test]
    fn formatted_bodies_appear_only_with_entities() {
        let (body, formatted) = text_and_formatted("no markup", None);
        assert_eq!(body.as_deref(), Some("no markup"));
        assert_eq!(formatted, None);

        let entities = vec![tl::enums::MessageEntity::Bold(
            tl::types::MessageEntityBold {
                offset: 0,
                length: 4,
            },
        )];
        let (body, formatted) = text_and_formatted("bold rest", Some(&entities));
        assert_eq!(body.as_deref(), Some("bold rest"));
        assert_eq!(formatted.as_deref(), Some("<b>bold</b> rest"));
    }

    #[test]
    fn photo_messages_expose_the_largest_rendition() {
        let raw = with_media(8, photo_media(555));

        let message = message_from_raw(&raw, "u7", None, None).expect("mapped");
        let media = message.media.expect("photo has media");

        assert_eq!(message.kind, MessageKind::Image);
        assert_eq!(media.mime, "image/jpeg");
        assert_eq!(media.size_bytes, Some(424_242));
        assert_eq!(media.width, Some(1280));
        assert_eq!(media.height, Some(720));

        let pointer = decode_pointer(&media.remote_ref).expect("pointer decodes");
        assert_eq!(pointer.kind, PointerKind::Photo);
        assert_eq!(pointer.chat, "u7");
        assert_eq!(pointer.msg, 8);
        assert_eq!(pointer.id, 555);
        assert_eq!(pointer.thumb, "y");
        assert_eq!(pointer.dc_id, Some(2));
        assert_eq!(
            BASE64.decode(pointer.file_reference).unwrap(),
            vec![1, 2, 3]
        );
    }

    #[test]
    fn voice_notes_are_classified_from_the_audio_attribute() {
        let audio = tl::enums::DocumentAttribute::Audio(tl::types::DocumentAttributeAudio {
            voice: true,
            duration: 12,
            title: None,
            performer: None,
            waveform: None,
        });
        let document = document(vec![audio], "audio/ogg");
        let raw = with_media(9, document_media(document, true, false));

        let message = message_from_raw(&raw, "u7", None, None).expect("mapped");
        let media = message.media.expect("voice note has media");

        assert_eq!(message.kind, MessageKind::Voice);
        assert_eq!(media.mime, "audio/ogg");
        assert_eq!(media.duration_ms, Some(12_000));
        assert_eq!(media.size_bytes, Some(55_000));
    }

    #[test]
    fn video_documents_are_classified_from_the_mime_type() {
        let video = tl::enums::DocumentAttribute::Video(tl::types::DocumentAttributeVideo {
            round_message: false,
            supports_streaming: true,
            nosound: false,
            duration: 4.5,
            w: 640,
            h: 360,
            preload_prefix_size: None,
            video_start_ts: None,
            video_codec: None,
        });
        let document = document(vec![video], "video/mp4");
        let raw = with_media(10, document_media(document, false, true));

        let message = message_from_raw(&raw, "u7", None, None).expect("mapped");
        let media = message.media.expect("video has media");

        assert_eq!(message.kind, MessageKind::Video);
        assert_eq!(media.width, Some(640));
        assert_eq!(media.height, Some(360));
        assert_eq!(media.duration_ms, Some(4_500));
    }

    #[test]
    fn stickers_win_over_their_document_container() {
        let sticker = tl::enums::DocumentAttribute::Sticker(tl::types::DocumentAttributeSticker {
            mask: false,
            alt: "🙂".to_string(),
            stickerset: tl::enums::InputStickerSet::Empty,
            mask_coords: None,
        });
        let document = document(vec![sticker], "application/x-tgsticker");
        let raw = with_media(11, document_media(document, false, false));

        let message = message_from_raw(&raw, "u7", None, None).expect("mapped");

        assert_eq!(message.kind, MessageKind::Sticker);
        assert!(message.media.is_some());
    }

    #[test]
    fn other_documents_are_files_with_their_name() {
        let filename =
            tl::enums::DocumentAttribute::Filename(tl::types::DocumentAttributeFilename {
                file_name: "report.pdf".to_string(),
            });
        let document = document(vec![filename], "application/pdf");
        let raw = with_media(12, document_media(document, false, false));

        let message = message_from_raw(&raw, "u7", None, None).expect("mapped");
        let media = message.media.expect("file has media");

        assert_eq!(message.kind, MessageKind::File);
        assert_eq!(media.file_name.as_deref(), Some("report.pdf"));
    }

    #[test]
    fn locations_become_a_geo_body_without_media() {
        let geo = tl::enums::MessageMedia::Geo(tl::types::MessageMediaGeo {
            geo: tl::enums::GeoPoint::Point(tl::types::GeoPoint {
                long: 13.405,
                lat: 52.52,
                access_hash: 0,
                accuracy_radius: None,
            }),
        });
        let raw = with_media(13, geo);

        let message = message_from_raw(&raw, "u7", None, None).expect("mapped");

        assert_eq!(message.kind, MessageKind::Location);
        assert_eq!(message.body.as_deref(), Some("geo:52.52,13.405"));
        assert!(message.media.is_none());
    }

    #[test]
    fn service_messages_describe_the_action() {
        let service = tl::enums::Message::Service(tl::types::MessageService {
            out: false,
            mentioned: false,
            media_unread: false,
            reactions_are_possible: false,
            silent: false,
            post: false,
            legacy: false,
            id: 44,
            from_id: Some(user_peer(7)),
            peer_id: tl::types::PeerChat { chat_id: 55 }.into(),
            saved_peer_id: None,
            reply_to: None,
            date: 1_700_000_100,
            action: tl::types::MessageActionChatCreate {
                title: "Studio".to_string(),
                users: vec![7, 8],
            }
            .into(),
            reactions: None,
            ttl_period: None,
        });

        let message = message_from_raw(&service, "c55", None, None).expect("service messages map");

        assert_eq!(message.kind, MessageKind::System);
        assert_eq!(
            message.body.as_deref(),
            Some("created the group \"Studio\"")
        );
        assert_eq!(message.sender_remote_id, "u7");
        assert_eq!(message.sent_at, 1_700_000_100_000);
    }

    #[test]
    fn channel_service_messages_use_the_pinned_phrasing() {
        let action = tl::enums::MessageAction::PinMessage;

        assert_eq!(service_body(&action), "pinned a message");
    }

    #[test]
    fn cursors_round_trip_and_reject_garbage() {
        let cursor = ChatCursor {
            folder: 1,
            date: 1_700_000_000,
            id: 4242,
            peer: Some("ch77".to_string()),
        };

        let encoded = encode_cursor(&cursor);

        assert_eq!(decode_cursor(&encoded).expect("decodes"), cursor);
        assert!(matches!(
            decode_cursor("not a cursor"),
            Err(ConnectorError::InvalidInput { .. })
        ));
    }

    #[test]
    fn login_code_channels_carry_the_expected_length() {
        let app =
            tl::enums::auth::SentCodeType::App(tl::types::auth::SentCodeTypeApp { length: 5 });
        let sms =
            tl::enums::auth::SentCodeType::Sms(tl::types::auth::SentCodeTypeSms { length: 6 });
        let email =
            tl::enums::auth::SentCodeType::EmailCode(tl::types::auth::SentCodeTypeEmailCode {
                apple_signin_allowed: false,
                google_signin_allowed: false,
                email_pattern: "a***@example.org".to_string(),
                length: 6,
                reset_available_period: None,
                reset_pending_date: None,
            });

        assert_eq!(code_channel(&app), (CodeChannel::App, Some(5)));
        assert_eq!(code_channel(&sms), (CodeChannel::Sms, Some(6)));
        assert_eq!(code_channel(&email), (CodeChannel::Email, Some(6)));
    }

    #[test]
    fn mime_types_come_from_known_extensions() {
        assert_eq!(mime_from_name("photo.PNG"), "image/png");
        assert_eq!(mime_from_name("clip.mp4"), "video/mp4");
        assert_eq!(mime_from_name("note.opus"), "audio/ogg");
        assert_eq!(mime_from_name("README"), "application/octet-stream");
        assert_eq!(mime_from_name("x.unknown"), "application/octet-stream");
    }

    fn ogg_page(granule: u64) -> Vec<u8> {
        let mut page = Vec::new();
        page.extend_from_slice(b"OggS");
        page.push(0); // version
        page.push(0); // header type
        page.extend_from_slice(&granule.to_le_bytes());
        page.extend_from_slice(&[0u8; 8]); // serial, sequence, checksum, segments
        page
    }

    #[test]
    fn ogg_duration_uses_the_last_granule_at_48_khz() {
        let mut data = ogg_page(u64::MAX);
        data.extend_from_slice(&ogg_page(48_000 * 3));

        assert_eq!(ogg_duration(&data), Some(Duration::from_secs(3)));
    }

    #[test]
    fn ogg_duration_is_absent_for_other_data() {
        assert_eq!(ogg_duration(b"not an ogg file"), None);
        assert_eq!(ogg_duration(&[]), None);
    }

    #[test]
    fn message_ids_must_be_numbers() {
        assert_eq!(parse_msg_id("42").unwrap(), 42);
        assert!(matches!(
            parse_msg_id("abc"),
            Err(ConnectorError::InvalidInput { .. })
        ));
        assert!(matches!(
            parse_msg_id(""),
            Err(ConnectorError::InvalidInput { .. })
        ));
    }
}
