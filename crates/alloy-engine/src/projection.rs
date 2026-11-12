//! Store-to-UI projections: the room list and one timeline per open chat.
//!
//! Projections are derived from the database on demand and refreshed through [`ChangeBus`], a
//! broadcast of store mutations. Nothing polls: every write that matters sends a [`Change`], and
//! the subscription tasks re-query and emit a minimal [`VectorDiffDto`] against what they last
//! sent.

use std::collections::HashMap;
use std::sync::Arc;

use alloy_protocol::{
    AccountId, EngineEvent, EventId, MediaSource, NotificationCount, ReactionSummary, RoomId,
    RoomSummary, SubscriptionId, TimelineItemContent, TimelineItemDto, TimelineItemPreview, UserId,
    VectorDiffDto,
};
use alloy_store::{
    AccountRecord, ChatKind, ChatRecord, MediaRecord, MessageKind as StoreMessageKind,
    MessageRecord, ParticipantRecord, ReactionRecord, ReadMarkerRecord,
};
use tokio::sync::{Mutex, broadcast};
use uuid::Uuid;

use crate::context::EngineContext;
use crate::convert;
use crate::error::{EngineError, map_store};

/// How many chats one room-list query may read per archive state.
const MAX_ROOM_LIST: u32 = 500;
/// Messages loaded when a timeline opens.
const PAGE_MESSAGES: u32 = 50;
/// Extra messages a live update reads, so a new message never pushes a loaded page out of view.
const LIVE_SLACK: u32 = 50;
const DAY_MILLIS: i64 = 86_400_000;

/// A store mutation a projection must observe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// Any chat row changed: title, counters, flags, preview.
    Chats,
    /// A message of `chat_id` was added, changed or removed.
    Timeline(String),
}

/// Broadcast fan-out of store changes.
#[derive(Clone, Debug)]
pub struct ChangeBus {
    sender: broadcast::Sender<Change>,
}

impl ChangeBus {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(512);
        Self { sender }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Change> {
        self.sender.subscribe()
    }

    pub fn chats(&self) {
        let _ = self.sender.send(Change::Chats);
    }

    pub fn timeline(&self, chat_id: &str) {
        let _ = self.sender.send(Change::Timeline(chat_id.to_string()));
    }
}

impl Default for ChangeBus {
    fn default() -> Self {
        Self::new()
    }
}

/// One open timeline: what the UI last received plus where to send the next diff.
pub struct TimelineWindow {
    pub subscription_id: SubscriptionId,
    pub chat_id: String,
    pub items: Mutex<Vec<TimelineItemDto>>,
}

/// The open timeline windows, one per subscribed chat.
#[derive(Clone, Default)]
pub struct TimelineRegistry {
    windows: Arc<Mutex<HashMap<String, Arc<TimelineWindow>>>>,
}

impl TimelineRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn get(&self, chat_id: &str) -> Option<Arc<TimelineWindow>> {
        self.windows.lock().await.get(chat_id).cloned()
    }

    pub async fn insert(&self, window: Arc<TimelineWindow>) {
        self.windows
            .lock()
            .await
            .insert(window.chat_id.clone(), window);
    }

    pub async fn remove(&self, chat_id: &str) -> Option<Arc<TimelineWindow>> {
        self.windows.lock().await.remove(chat_id)
    }

    /// Drops the window owning `subscription_id`, for `Unsubscribe`.
    pub async fn remove_by_subscription(
        &self,
        subscription_id: &SubscriptionId,
    ) -> Option<Arc<TimelineWindow>> {
        let mut windows = self.windows.lock().await;
        let chat_id = windows
            .iter()
            .find(|(_, window)| window.subscription_id == *subscription_id)
            .map(|(chat_id, _)| chat_id.clone());
        match chat_id {
            Some(chat_id) => windows.remove(&chat_id),
            None => None,
        }
    }
}

/// Computes the diffs that turn `current` into `next`, identified by `id`.
///
/// A simple ordered walk: removals from the tail, then inserts/sets/replacements front to back, so
/// every diff keeps the client's vector identical to the engine's own view.
pub fn diff_values<T, F>(current: &[T], next: Vec<T>, id: F) -> Vec<VectorDiffDto<T>>
where
    T: Clone + PartialEq,
    F: Fn(&T) -> String,
{
    if current.is_empty() {
        return if next.is_empty() {
            Vec::new()
        } else {
            vec![VectorDiffDto::Reset { values: next }]
        };
    }

    let mut current = current.to_vec();
    let mut diffs = Vec::new();

    // Items the next state no longer has disappear from the tail first, so indices shift only
    // towards the front.
    let next_ids: Vec<String> = next.iter().map(&id).collect();
    let mut index = current.len();
    while index > 0 {
        index -= 1;
        if !next_ids.contains(&id(&current[index])) {
            current.remove(index);
            diffs.push(VectorDiffDto::Remove { index });
        }
    }

    let mut position = 0;
    while position < next.len() {
        if position >= current.len() {
            let values = next[position..].to_vec();
            current.extend(values.iter().cloned());
            diffs.push(VectorDiffDto::Append { values });
            break;
        }
        if id(&current[position]) != id(&next[position]) {
            current.insert(position, next[position].clone());
            diffs.push(VectorDiffDto::Insert {
                index: position,
                value: next[position].clone(),
            });
        } else if current[position] != next[position] {
            current[position] = next[position].clone();
            diffs.push(VectorDiffDto::Set {
                index: position,
                value: next[position].clone(),
            });
        }
        position += 1;
    }

    while current.len() > next.len() {
        let index = current.len() - 1;
        current.remove(index);
        diffs.push(VectorDiffDto::Remove { index });
    }

    diffs
}

// === Room list ===

/// Subscribes the room list: an immediate `Reset`, then minimal diffs on every chat change.
pub async fn subscribe_room_list(
    ctx: &EngineContext,
    filter: Option<String>,
) -> Result<SubscriptionId, EngineError> {
    let subscription_id = SubscriptionId(Uuid::new_v4().to_string());
    let items = query_room_list(ctx, filter.as_deref()).await?;
    ctx.sink.emit(EngineEvent::RoomListDiff {
        subscription_id: subscription_id.clone(),
        diff: Box::new(VectorDiffDto::Reset {
            values: items.clone(),
        }),
    });

    let mut changes = ctx.bus.subscribe();
    let task_ctx = ctx.clone();
    let task_id = subscription_id.clone();
    let handle = tokio::spawn(async move {
        let mut current = items;
        loop {
            match changes.recv().await {
                Ok(Change::Chats) => {}
                Ok(Change::Timeline(_)) => continue,
                // A lagged receiver missed changes, so the next query already returns the truth.
                Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(broadcast::error::RecvError::Closed) => break,
            }
            match query_room_list(&task_ctx, filter.as_deref()).await {
                Ok(fresh) => {
                    for diff in diff_values(&current, fresh.clone(), |summary: &RoomSummary| {
                        summary.id.0.clone()
                    }) {
                        task_ctx.sink.emit(EngineEvent::RoomListDiff {
                            subscription_id: task_id.clone(),
                            diff: Box::new(diff),
                        });
                    }
                    current = fresh;
                }
                Err(err) => tracing::warn!(error = %err.message, "room list projection failed"),
            }
        }
    });

    ctx.subscriptions
        .register(subscription_id.clone(), handle.abort_handle())
        .await;
    Ok(subscription_id)
}

/// Every chat of every account, archived included, ordered pinned first then by last activity.
pub async fn query_room_list(
    ctx: &EngineContext,
    filter: Option<&str>,
) -> Result<Vec<RoomSummary>, EngineError> {
    let records = ctx.store.list_accounts().await.map_err(map_store)?;
    let accounts: HashMap<String, AccountRecord> = records
        .into_iter()
        .map(|record| (record.account_id.clone(), record))
        .collect();

    let mut chats = ctx
        .store
        .list_chats(None, false, MAX_ROOM_LIST)
        .await
        .map_err(map_store)?;
    chats.extend(
        ctx.store
            .list_chats(None, true, MAX_ROOM_LIST)
            .await
            .map_err(map_store)?,
    );
    chats.sort_by(|left, right| {
        right
            .is_pinned
            .cmp(&left.is_pinned)
            .then_with(|| {
                right
                    .last_activity_at
                    .unwrap_or(0)
                    .cmp(&left.last_activity_at.unwrap_or(0))
            })
            .then_with(|| left.chat_id.cmp(&right.chat_id))
    });

    let needle = filter
        .map(|value| value.trim().to_lowercase())
        .filter(|value| !value.is_empty());

    let mut summaries = Vec::with_capacity(chats.len());
    for chat in chats {
        let Some(account) = accounts.get(&chat.account_id) else {
            continue;
        };
        if let Some(needle) = &needle {
            let title = chat.title.as_deref().unwrap_or_default().to_lowercase();
            if !title.contains(needle.as_str()) {
                continue;
            }
        }
        summaries.push(room_summary(ctx, &chat, account).await?);
    }
    Ok(summaries)
}

async fn room_summary(
    ctx: &EngineContext,
    chat: &ChatRecord,
    account: &AccountRecord,
) -> Result<RoomSummary, EngineError> {
    let latest = match &chat.last_message_id {
        Some(message_id) => ctx.store.get_message(message_id).await.map_err(map_store)?,
        None => None,
    };
    let latest_event = latest.map(|message| TimelineItemPreview {
        sender: UserId(message.sender_remote_id.clone()),
        timestamp_millis: u64::try_from(message.sent_at).unwrap_or(0),
        body: message
            .body
            .clone()
            .unwrap_or_else(|| preview_label(message.kind)),
    });

    Ok(RoomSummary {
        id: RoomId(chat.chat_id.clone()),
        account_id: AccountId(chat.account_id.clone()),
        network: convert::network_store_to_protocol(account.network),
        name: chat.title.clone(),
        avatar: chat
            .avatar_media_id
            .clone()
            .map(|media_id| MediaSource { media_id }),
        kind: convert::chat_kind_store_to_protocol(chat.kind),
        is_direct: chat.kind == ChatKind::Direct,
        unread_notifications: NotificationCount {
            highlight_count: u64::try_from(chat.mention_count).unwrap_or(0),
            notification_count: u64::try_from(chat.unread_count).unwrap_or(0),
        },
        latest_event,
        pinned: chat.is_pinned,
        archived: chat.is_archived,
        muted_until: muted_until(chat),
        marked_unread: chat.is_marked_unread,
        tags: chat.local_tags.clone(),
        draft: chat.draft.clone(),
    })
}

fn muted_until(chat: &ChatRecord) -> Option<u64> {
    if !chat.is_muted {
        return None;
    }
    match chat.muted_until {
        // `i64::MAX` is the store's "muted forever"; the protocol spells it `u64::MAX`.
        Some(value) if value >= i64::MAX - 1 => Some(u64::MAX),
        Some(value) => Some(u64::try_from(value).unwrap_or(0)),
        None => Some(u64::MAX),
    }
}

fn preview_label(kind: StoreMessageKind) -> String {
    match kind {
        StoreMessageKind::Text => "Message",
        StoreMessageKind::Image => "Photo",
        StoreMessageKind::Video => "Video",
        StoreMessageKind::Audio => "Audio",
        StoreMessageKind::Voice => "Voice message",
        StoreMessageKind::File => "File",
        StoreMessageKind::Sticker => "Sticker",
        StoreMessageKind::Location => "Location",
        StoreMessageKind::System => "System message",
    }
    .to_string()
}

// === Timeline ===

/// Subscribes one chat's timeline: an immediate `Reset`, then live diffs.
pub async fn subscribe_timeline(
    ctx: &EngineContext,
    room_id: &RoomId,
) -> Result<SubscriptionId, EngineError> {
    let chat = ctx
        .store
        .get_chat(&room_id.0)
        .await
        .map_err(map_store)?
        .ok_or_else(|| EngineError::not_found(format!("chat '{}'", room_id.0)))?;

    if let Some(existing) = ctx.timelines.remove(&chat.chat_id).await {
        ctx.subscriptions.cancel(&existing.subscription_id).await;
    }

    let items = query_timeline(ctx, &chat.chat_id, PAGE_MESSAGES).await?;
    let subscription_id = SubscriptionId(Uuid::new_v4().to_string());
    ctx.sink.emit(EngineEvent::TimelineDiff {
        subscription_id: subscription_id.clone(),
        room_id: room_id.clone(),
        diff: Box::new(VectorDiffDto::Reset {
            values: items.clone(),
        }),
    });

    let window = Arc::new(TimelineWindow {
        subscription_id: subscription_id.clone(),
        chat_id: chat.chat_id.clone(),
        items: Mutex::new(items),
    });
    let task = tokio::spawn(run_timeline(ctx.clone(), room_id.clone(), window.clone()));
    ctx.subscriptions
        .register_for_room(
            subscription_id.clone(),
            room_id.clone(),
            task.abort_handle(),
        )
        .await;
    ctx.timelines.insert(window).await;
    Ok(subscription_id)
}

async fn run_timeline(ctx: EngineContext, room_id: RoomId, window: Arc<TimelineWindow>) {
    let mut changes = ctx.bus.subscribe();
    loop {
        match changes.recv().await {
            Ok(Change::Timeline(chat_id)) if chat_id == window.chat_id => {}
            Ok(_) => continue,
            Err(broadcast::error::RecvError::Lagged(_)) => {}
            Err(broadcast::error::RecvError::Closed) => break,
        }

        let mut items = window.items.lock().await;
        let loaded = items
            .iter()
            .filter(|item| !matches!(&item.content, TimelineItemContent::DayDivider { .. }))
            .count();
        let limit = u32::try_from(loaded)
            .unwrap_or(u32::MAX)
            .saturating_add(LIVE_SLACK)
            .max(PAGE_MESSAGES);
        match query_timeline(&ctx, &window.chat_id, limit).await {
            Ok(fresh) => {
                for diff in diff_values(&items, fresh.clone(), |item: &TimelineItemDto| {
                    item.internal_id.clone()
                }) {
                    ctx.sink.emit(EngineEvent::TimelineDiff {
                        subscription_id: window.subscription_id.clone(),
                        room_id: room_id.clone(),
                        diff: Box::new(diff),
                    });
                }
                *items = fresh;
            }
            Err(err) => {
                tracing::warn!(chat = %window.chat_id, error = %err.message, "timeline projection failed");
            }
        }
    }
}

/// Loads the newest `limit` messages of a chat, oldest first, with day dividers.
pub async fn query_timeline(
    ctx: &EngineContext,
    chat_id: &str,
    limit: u32,
) -> Result<Vec<TimelineItemDto>, EngineError> {
    let chat = ctx
        .store
        .get_chat(chat_id)
        .await
        .map_err(map_store)?
        .ok_or_else(|| EngineError::not_found(format!("chat '{chat_id}'")))?;
    let account = ctx
        .store
        .get_account(&chat.account_id)
        .await
        .map_err(map_store)?;
    let records = ctx
        .store
        .page_messages(chat_id, None, limit.max(1))
        .await
        .map_err(map_store)?;
    map_messages(ctx, chat_id, records, account.as_ref()).await
}

/// Maps store rows (newest first, as the store returns them) to timeline items (oldest first).
pub async fn map_messages(
    ctx: &EngineContext,
    chat_id: &str,
    records: Vec<MessageRecord>,
    account: Option<&AccountRecord>,
) -> Result<Vec<TimelineItemDto>, EngineError> {
    if records.is_empty() {
        return Ok(Vec::new());
    }
    let participants: HashMap<String, ParticipantRecord> = ctx
        .store
        .list_participants(chat_id)
        .await
        .map_err(map_store)?
        .into_iter()
        .map(|participant| (participant.remote_user_id.clone(), participant))
        .collect();
    let ids: Vec<String> = records
        .iter()
        .map(|record| record.message_id.clone())
        .collect();
    let reactions = ctx.store.list_reactions(&ids).await.map_err(map_store)?;
    let mut by_message: HashMap<String, Vec<ReactionRecord>> = HashMap::new();
    for reaction in reactions {
        by_message
            .entry(reaction.message_id.clone())
            .or_default()
            .push(reaction);
    }
    let markers = ctx
        .store
        .list_read_markers(chat_id)
        .await
        .map_err(map_store)?;
    let own = account.and_then(|record| record.remote_user_id.clone());

    let mut items = Vec::with_capacity(records.len() + 1);
    let mut last_day: Option<i64> = None;
    for record in records.into_iter().rev() {
        let day = record.sent_at.div_euclid(DAY_MILLIS);
        if last_day != Some(day) {
            items.push(day_divider(day.saturating_mul(DAY_MILLIS)));
            last_day = Some(day);
        }
        let reactions = by_message
            .get(&record.message_id)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        items.push(
            map_item(
                ctx,
                &record,
                &participants,
                reactions,
                &markers,
                own.as_deref(),
            )
            .await?,
        );
    }
    Ok(items)
}

async fn map_item(
    ctx: &EngineContext,
    record: &MessageRecord,
    participants: &HashMap<String, ParticipantRecord>,
    reactions: &[ReactionRecord],
    markers: &[ReadMarkerRecord],
    own: Option<&str>,
) -> Result<TimelineItemDto, EngineError> {
    let participant = participants.get(&record.sender_remote_id);
    let content = map_content(ctx, record).await?;
    let reply_to = match &record.reply_to_message_id {
        Some(parent_id) => match ctx.store.get_message(parent_id).await.map_err(map_store)? {
            Some(parent) => Some(Box::new(reply_preview(ctx, &parent, participants).await?)),
            None => None,
        },
        None => None,
    };

    let mut summaries: Vec<ReactionSummary> = Vec::new();
    for reaction in reactions {
        let by_me = own == Some(reaction.sender_remote_id.as_str());
        match summaries
            .iter_mut()
            .find(|summary| summary.key == reaction.emoji)
        {
            Some(summary) => {
                summary.count += 1;
                summary
                    .senders
                    .push(UserId(reaction.sender_remote_id.clone()));
                summary.by_me |= by_me;
            }
            None => summaries.push(ReactionSummary {
                key: reaction.emoji.clone(),
                count: 1,
                senders: vec![UserId(reaction.sender_remote_id.clone())],
                by_me,
            }),
        }
    }

    let read_by: Vec<UserId> = markers
        .iter()
        .filter(|marker| marker.up_to_message_id == record.message_id)
        .map(|marker| UserId(marker.remote_user_id.clone()))
        .collect();

    Ok(TimelineItemDto {
        internal_id: record.message_id.clone(),
        event_id: Some(EventId(record.message_id.clone())),
        sender: UserId(record.sender_remote_id.clone()),
        sender_display_name: participant.and_then(|participant| participant.display_name.clone()),
        sender_avatar: participant
            .and_then(|participant| participant.avatar_media_id.clone())
            .map(|media_id| MediaSource { media_id }),
        timestamp_millis: u64::try_from(record.sent_at).unwrap_or(0),
        content,
        send_state: send_state(record),
        is_own: record.is_own,
        reply_to,
        read_by,
        edited: record.edited_at.is_some(),
        reactions: summaries,
        transaction_id: record.txn_id.clone(),
    })
}

/// The quoted parent of a reply: same shape, without its own quote, reactions or receipts.
async fn reply_preview(
    ctx: &EngineContext,
    record: &MessageRecord,
    participants: &HashMap<String, ParticipantRecord>,
) -> Result<TimelineItemDto, EngineError> {
    let participant = participants.get(&record.sender_remote_id);
    Ok(TimelineItemDto {
        internal_id: record.message_id.clone(),
        event_id: Some(EventId(record.message_id.clone())),
        sender: UserId(record.sender_remote_id.clone()),
        sender_display_name: participant.and_then(|participant| participant.display_name.clone()),
        sender_avatar: participant
            .and_then(|participant| participant.avatar_media_id.clone())
            .map(|media_id| MediaSource { media_id }),
        timestamp_millis: u64::try_from(record.sent_at).unwrap_or(0),
        content: map_content(ctx, record).await?,
        send_state: send_state(record),
        is_own: record.is_own,
        reply_to: None,
        read_by: Vec::new(),
        edited: record.edited_at.is_some(),
        reactions: Vec::new(),
        transaction_id: record.txn_id.clone(),
    })
}

async fn map_content(
    ctx: &EngineContext,
    record: &MessageRecord,
) -> Result<TimelineItemContent, EngineError> {
    if record.deleted_at.is_some() {
        return Ok(TimelineItemContent::Deleted);
    }
    let body = record.body.clone();
    Ok(match record.kind {
        StoreMessageKind::Text => TimelineItemContent::TextMessage {
            body: body.unwrap_or_default(),
            formatted_body: record.formatted.clone(),
            html_sanitized: false,
        },
        StoreMessageKind::System => TimelineItemContent::System {
            text: body.unwrap_or_default(),
        },
        StoreMessageKind::Location => TimelineItemContent::Location {
            geo_uri: body.unwrap_or_default(),
            description: None,
        },
        _ => {
            let media = match &record.media_id {
                Some(media_id) => ctx.store.get_media(media_id).await.map_err(map_store)?,
                None => None,
            };
            match media {
                Some(media) => media_content(record, &media, body),
                // A media row that is missing (cache cleared, row pruned) still has text or a
                // caption to show; an empty bubble would be worse than the fallback.
                None => TimelineItemContent::TextMessage {
                    body: body.unwrap_or_default(),
                    formatted_body: record.formatted.clone(),
                    html_sanitized: false,
                },
            }
        }
    })
}

fn media_content(
    record: &MessageRecord,
    media: &MediaRecord,
    body: Option<String>,
) -> TimelineItemContent {
    TimelineItemContent::Media {
        source: MediaSource {
            media_id: media.media_id.clone(),
        },
        mime_type: media
            .mime
            .clone()
            .unwrap_or_else(|| "application/octet-stream".to_string()),
        size_bytes: media
            .size_bytes
            .map_or(0, |value| u64::try_from(value).unwrap_or(0)),
        body: body.or_else(|| media.file_name.clone()).unwrap_or_default(),
        media_kind: record.kind.as_str().to_string(),
        width: media.width.and_then(|value| u32::try_from(value).ok()),
        height: media.height.and_then(|value| u32::try_from(value).ok()),
        duration_ms: media
            .duration_ms
            .and_then(|value| u64::try_from(value).ok()),
        voice: record.kind == StoreMessageKind::Voice,
    }
}

fn send_state(record: &MessageRecord) -> alloy_protocol::EventSendState {
    use alloy_store::SendState;

    let transaction_id = record.txn_id.clone().unwrap_or_default();
    match record.send_state {
        Some(SendState::Queued) => alloy_protocol::EventSendState::Queued { transaction_id },
        Some(SendState::Sending) => alloy_protocol::EventSendState::Sending { transaction_id },
        Some(SendState::Failed) => {
            let extra = record.extra.as_ref();
            let error = extra
                .and_then(|value| value.get("send_error"))
                .and_then(|value| value.as_str())
                .unwrap_or("the message could not be sent")
                .to_string();
            let can_retry = extra
                .and_then(|value| value.get("can_retry"))
                .and_then(|value| value.as_bool())
                .unwrap_or(true);
            alloy_protocol::EventSendState::Failed { error, can_retry }
        }
        _ => alloy_protocol::EventSendState::Sent {
            event_id: Some(EventId(record.message_id.clone())),
        },
    }
}

fn day_divider(timestamp_millis: i64) -> TimelineItemDto {
    TimelineItemDto {
        internal_id: format!("day:{timestamp_millis}"),
        event_id: None,
        sender: UserId(String::new()),
        sender_display_name: None,
        sender_avatar: None,
        timestamp_millis: u64::try_from(timestamp_millis).unwrap_or(0),
        content: TimelineItemContent::DayDivider {
            timestamp_millis: u64::try_from(timestamp_millis).unwrap_or(0),
        },
        send_state: alloy_protocol::EventSendState::Sent { event_id: None },
        is_own: false,
        reply_to: None,
        read_by: Vec::new(),
        edited: false,
        reactions: Vec::new(),
        transaction_id: None,
    }
}

/// Prepends older history to an open timeline, backfilling through the connector when the store
/// has nothing older. Returns whether the start of the chat was reached.
pub async fn paginate_backwards(
    ctx: &EngineContext,
    room_id: &RoomId,
    count: usize,
) -> Result<bool, EngineError> {
    let window = ctx
        .timelines
        .get(&room_id.0)
        .await
        .ok_or_else(|| EngineError::unavailable("the timeline is not open"))?;
    let count = u32::try_from(count).unwrap_or(u32::MAX).clamp(1, 200);

    let mut items = window.items.lock().await;
    let chat = ctx
        .store
        .get_chat(&window.chat_id)
        .await
        .map_err(map_store)?
        .ok_or_else(|| EngineError::not_found("chat"))?;

    let oldest = items.iter().find_map(|item| item.event_id.clone());
    let cursor = match &oldest {
        Some(event_id) => ctx
            .store
            .get_message(&event_id.0)
            .await
            .map_err(map_store)?
            .map(|record| (record.sent_at, record.message_id)),
        None => None,
    };

    let mut older = ctx
        .store
        .page_messages(&window.chat_id, cursor.clone(), count)
        .await
        .map_err(map_store)?;

    let anchor = if older.is_empty() {
        oldest.as_ref()
    } else {
        None
    };
    if let Some(event_id) = anchor {
        let remote_anchor = ctx
            .store
            .get_message(&event_id.0)
            .await
            .map_err(map_store)?
            .and_then(|record| record.remote_msg_id);
        if let (Some(remote_anchor), Some(runtime)) =
            (remote_anchor, ctx.accounts.get(&chat.account_id).await)
        {
            match runtime
                .connector
                .backfill(
                    &chat.remote_chat_id,
                    Some(remote_anchor),
                    usize::try_from(count).unwrap_or(1),
                )
                .await
            {
                Ok(page) => {
                    for message in page.items {
                        if let Err(err) =
                            crate::actor::apply_message(ctx, &chat.account_id, message, false).await
                        {
                            tracing::warn!(chat = %chat.chat_id, error = %err.message, "failed to store older history");
                        }
                    }
                    older = ctx
                        .store
                        .page_messages(&window.chat_id, cursor, count)
                        .await
                        .map_err(map_store)?;
                }
                Err(err) => {
                    tracing::warn!(chat = %chat.chat_id, error = %err, "history backfill failed");
                }
            }
        }
    }

    if older.is_empty() {
        return Ok(true);
    }

    let account = ctx
        .store
        .get_account(&chat.account_id)
        .await
        .map_err(map_store)?;
    let prepended = map_messages(ctx, &window.chat_id, older, account.as_ref()).await?;
    let mut combined: Vec<TimelineItemDto> = Vec::with_capacity(prepended.len() + items.len());
    combined.extend(prepended);
    combined.extend(items.iter().cloned());
    // The page boundary can leave two dividers for the same day; keep one.
    combined.dedup_by(|left, right| match (&left.content, &right.content) {
        (
            TimelineItemContent::DayDivider {
                timestamp_millis: left,
            },
            TimelineItemContent::DayDivider {
                timestamp_millis: right,
            },
        ) => left == right,
        _ => false,
    });

    for diff in diff_values(&items, combined.clone(), |item: &TimelineItemDto| {
        item.internal_id.clone()
    }) {
        ctx.sink.emit(EngineEvent::TimelineDiff {
            subscription_id: window.subscription_id.clone(),
            room_id: room_id.clone(),
            diff: Box::new(diff),
        });
    }
    *items = combined;
    Ok(false)
}
