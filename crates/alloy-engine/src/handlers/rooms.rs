//! Chat commands: the room list, chat details, local flags, tags, notes, drafts, search and the
//! shared-media browser.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};

use alloy_protocol::{
    AccountId, DraftDto, DraftSummary, EngineCommand, EngineResponse, EventId, GlobalSearchHit,
    MediaSource, RoomDetails, RoomId, RoomMediaItem, RoomMediaKind, RoomMemberDto, RoomSearchHit,
    RoomTagDef, TagColour, UserId, UserProfileDto,
};
use alloy_store::{
    AccountState as StoreAccountState, ChatFlagsUpdate, ChatKind as StoreChatKind, ChatRecord,
    MessageKind as StoreMessageKind, MessageRecord, OutboxOp, RoomMediaQuery,
};
use uuid::Uuid;

use crate::accounts::start_stored_account;
use crate::context::EngineContext;
use crate::convert;
use crate::error::{EngineError, map_store};
use crate::files;
use crate::projection;
use crate::router::{CommandGroup, GroupHandler, command_name};
use crate::support;

const MAX_ROOM_QUERY: u32 = 500;

/// Handler for chat commands (`CommandGroup::Rooms`).
pub struct RoomsHandler {
    /// Serializes tag-catalogue read-modify-write cycles.
    tags_lock: Mutex<()>,
    /// Serializes draft-file read-modify-write cycles.
    drafts_lock: Mutex<()>,
}

impl RoomsHandler {
    pub fn new() -> Self {
        Self {
            tags_lock: Mutex::new(()),
            drafts_lock: Mutex::new(()),
        }
    }
}

impl Default for RoomsHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl GroupHandler for RoomsHandler {
    fn group(&self) -> CommandGroup {
        CommandGroup::Rooms
    }

    async fn handle(
        &self,
        ctx: &EngineContext,
        cmd: EngineCommand,
    ) -> Result<EngineResponse, EngineError> {
        let name = command_name(&cmd);
        match cmd {
            EngineCommand::SubscribeRoomList { filter } => {
                let subscription_id = projection::subscribe_room_list(ctx, filter).await?;
                Ok(EngineResponse::RoomListSubscribed { subscription_id })
            }
            EngineCommand::Unsubscribe { subscription_id } => {
                ctx.subscriptions.cancel(&subscription_id).await;
                ctx.timelines.remove_by_subscription(&subscription_id).await;
                Ok(EngineResponse::Unsubscribed)
            }
            EngineCommand::GetRoomMembers { room_id } => {
                let chat = support::chat_record(ctx, &room_id).await?;
                let members = ctx
                    .store
                    .list_participants(&chat.chat_id)
                    .await
                    .map_err(map_store)?
                    .into_iter()
                    .map(|participant| RoomMemberDto {
                        user_id: UserId(participant.remote_user_id),
                        display_name: participant.display_name,
                        avatar: participant
                            .avatar_media_id
                            .map(|media_id| MediaSource { media_id }),
                        role: participant.role,
                    })
                    .collect();
                Ok(EngineResponse::RoomMembers { members })
            }
            EngineCommand::GetRoomDetails { room_id } => Ok(EngineResponse::RoomDetailsRetrieved {
                details: room_details(ctx, room_id).await?,
            }),
            EngineCommand::GetUserProfile {
                account_id,
                user_id,
            } => Ok(EngineResponse::UserProfile {
                profile: user_profile(ctx, &account_id, &user_id).await?,
            }),
            EngineCommand::GetRoomNote { room_id } => {
                let chat = support::chat_record(ctx, &room_id).await?;
                Ok(EngineResponse::RoomNote { note: chat.note })
            }
            EngineCommand::SetRoomNote { room_id, note } => {
                let chat = support::chat_record(ctx, &room_id).await?;
                let trimmed = note.trim().to_string();
                let note = if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed)
                };
                ctx.store
                    .update_chat_flags(
                        &chat.chat_id,
                        ChatFlagsUpdate {
                            note: Some(note),
                            ..ChatFlagsUpdate::default()
                        },
                    )
                    .await
                    .map_err(map_store)?;
                ctx.bus.chats();
                Ok(EngineResponse::RoomNoteSet)
            }
            EngineCommand::GetTagCatalog => Ok(EngineResponse::TagCatalog {
                tags: read_catalog(ctx)?,
            }),
            EngineCommand::CreateRoomTag { name, colour } => {
                create_room_tag(&self.tags_lock, ctx, name, colour)
            }
            EngineCommand::RenameRoomTag { id, name, colour } => {
                rename_room_tag(&self.tags_lock, ctx, id, name, colour)
            }
            EngineCommand::ReorderRoomTags { ids } => reorder_room_tags(&self.tags_lock, ctx, ids),
            EngineCommand::SetRoomTag {
                room_id,
                tag_id,
                enabled,
            } => set_room_tag(&self.tags_lock, ctx, &room_id, tag_id, enabled).await,
            EngineCommand::DeleteRoomTag { tag_id } => {
                delete_room_tag(&self.tags_lock, ctx, tag_id).await
            }
            EngineCommand::MarkRoomRead { room_id } => mark_room_read(ctx, &room_id).await,
            EngineCommand::SetRoomPinned { room_id, pinned } => {
                update_flags(
                    ctx,
                    &room_id,
                    ChatFlagsUpdate {
                        is_pinned: Some(pinned),
                        ..ChatFlagsUpdate::default()
                    },
                )
                .await
            }
            EngineCommand::SetRoomArchived { room_id, archived } => {
                update_flags(
                    ctx,
                    &room_id,
                    ChatFlagsUpdate {
                        is_archived: Some(archived),
                        ..ChatFlagsUpdate::default()
                    },
                )
                .await
            }
            EngineCommand::SetRoomMarkedUnread { room_id, unread } => {
                update_flags(
                    ctx,
                    &room_id,
                    ChatFlagsUpdate {
                        is_marked_unread: Some(unread),
                        ..ChatFlagsUpdate::default()
                    },
                )
                .await
            }
            EngineCommand::SetRoomMuted {
                room_id,
                muted_until,
            } => {
                let stored = muted_until.map(|value| i64::try_from(value).unwrap_or(i64::MAX));
                update_flags(
                    ctx,
                    &room_id,
                    ChatFlagsUpdate {
                        muted_until: Some(stored),
                        ..ChatFlagsUpdate::default()
                    },
                )
                .await
            }
            EngineCommand::GetRoomMedia {
                room_id,
                kind,
                before,
                limit,
            } => room_media(ctx, &room_id, kind, before, limit).await,
            EngineCommand::SearchRoomMessages {
                room_id,
                query,
                limit,
            } => search_room(ctx, &room_id, query, limit).await,
            EngineCommand::SearchAllMessages { query, limit } => {
                search_all(ctx, query, limit).await
            }
            EngineCommand::ClearMediaCache => clear_media_cache(ctx).await,
            EngineCommand::RefreshSync => refresh_sync(ctx).await,
            EngineCommand::SaveDraft {
                room_id,
                text,
                reply_to,
            } => save_draft(&self.drafts_lock, ctx, &room_id, text, reply_to).await,
            EngineCommand::GetDraft { room_id } => {
                let chat = support::chat_record(ctx, &room_id).await?;
                let drafts = read_drafts(ctx)?;
                Ok(EngineResponse::Draft {
                    draft: drafts.get(&chat.chat_id).cloned(),
                })
            }
            EngineCommand::ListDrafts => {
                let drafts = read_drafts(ctx)?;
                let mut summaries = Vec::with_capacity(drafts.len());
                for (room, draft) in drafts {
                    if draft.text.trim().is_empty() {
                        continue;
                    }
                    if ctx
                        .store
                        .get_chat(&room)
                        .await
                        .map_err(map_store)?
                        .is_none()
                    {
                        continue;
                    }
                    summaries.push(DraftSummary {
                        room_id: RoomId(room),
                        text: files::truncate_chars(&draft.text, 80),
                    });
                }
                Ok(EngineResponse::Drafts { drafts: summaries })
            }
            _ => Err(EngineError::invalid_argument(format!(
                "Command '{}' does not belong to the Rooms group",
                name
            ))),
        }
    }
}

async fn room_details(ctx: &EngineContext, room_id: RoomId) -> Result<RoomDetails, EngineError> {
    let chat = support::chat_record(ctx, &room_id).await?;
    let account = support::account_record(ctx, &AccountId(chat.account_id.clone())).await?;
    let members = ctx
        .store
        .list_participants(&chat.chat_id)
        .await
        .map_err(map_store)?;
    let capabilities = match ctx.accounts.get(&chat.account_id).await {
        Some(runtime) => convert::capabilities_dto(&runtime.capabilities),
        None => convert::no_capabilities(),
    };
    Ok(RoomDetails {
        room_id,
        account_id: AccountId(chat.account_id.clone()),
        network: convert::network_store_to_protocol(account.network),
        name: chat.title.clone(),
        avatar: chat
            .avatar_media_id
            .clone()
            .map(|media_id| MediaSource { media_id }),
        kind: convert::chat_kind_store_to_protocol(chat.kind),
        is_direct: chat.kind == StoreChatKind::Direct,
        member_count: u64::try_from(members.len()).unwrap_or(0),
        note: chat.note,
        capabilities,
    })
}

async fn user_profile(
    ctx: &EngineContext,
    account_id: &AccountId,
    user_id: &UserId,
) -> Result<UserProfileDto, EngineError> {
    let account = support::account_record(ctx, account_id).await?;
    let mut display_name = None;
    let mut avatar = None;
    let mut dm_room_id = None;

    for chat in all_chats(ctx).await? {
        if chat.account_id != account_id.0 {
            continue;
        }
        let Some(participant) = ctx
            .store
            .get_participant(&chat.chat_id, &user_id.0)
            .await
            .map_err(map_store)?
        else {
            continue;
        };
        if display_name.is_none() {
            display_name = participant.display_name;
        }
        if avatar.is_none() {
            avatar = participant
                .avatar_media_id
                .map(|media_id| MediaSource { media_id });
        }
        if dm_room_id.is_none() && chat.kind == StoreChatKind::Direct {
            dm_room_id = Some(RoomId(chat.chat_id.clone()));
        }
    }

    if display_name.is_none() && avatar.is_none() && dm_room_id.is_none() {
        return Err(EngineError::not_found(format!("user '{}'", user_id.0)));
    }

    Ok(UserProfileDto {
        user_id: user_id.clone(),
        account_id: account_id.clone(),
        display_name,
        avatar,
        is_me: account.remote_user_id.as_deref() == Some(user_id.0.as_str()),
        dm_room_id,
    })
}

async fn mark_room_read(
    ctx: &EngineContext,
    room_id: &RoomId,
) -> Result<EngineResponse, EngineError> {
    let chat = support::chat_record(ctx, room_id).await?;
    ctx.store
        .set_unread(&chat.chat_id, 0, 0)
        .await
        .map_err(map_store)?;
    if chat.is_marked_unread {
        ctx.store
            .update_chat_flags(
                &chat.chat_id,
                ChatFlagsUpdate {
                    is_marked_unread: Some(false),
                    ..ChatFlagsUpdate::default()
                },
            )
            .await
            .map_err(map_store)?;
    }

    // Tell the network too: the newest message that has a network id is the receipt anchor.
    let anchor = ctx
        .store
        .page_messages(&chat.chat_id, None, 1)
        .await
        .map_err(map_store)?
        .into_iter()
        .next()
        .filter(|message| message.remote_msg_id.is_some());
    if let Some(newest) = anchor {
        support::enqueue_op(
            ctx,
            &chat.account_id,
            &chat.chat_id,
            OutboxOp::Read,
            serde_json::json!({ "event_id": newest.message_id }),
        )
        .await?;
    }

    ctx.bus.chats();
    Ok(EngineResponse::RoomFlagsUpdated)
}

async fn update_flags(
    ctx: &EngineContext,
    room_id: &RoomId,
    update: ChatFlagsUpdate,
) -> Result<EngineResponse, EngineError> {
    let chat = support::chat_record(ctx, room_id).await?;
    ctx.store
        .update_chat_flags(&chat.chat_id, update)
        .await
        .map_err(map_store)?;
    ctx.bus.chats();
    Ok(EngineResponse::RoomFlagsUpdated)
}

// === Tag catalogue ===

fn tags_path(ctx: &EngineContext) -> PathBuf {
    ctx.data_dir.join("tags.json")
}

fn read_catalog(ctx: &EngineContext) -> Result<Vec<RoomTagDef>, EngineError> {
    Ok(files::read_json::<Vec<RoomTagDef>>(&tags_path(ctx))?.unwrap_or_default())
}

fn write_catalog(ctx: &EngineContext, tags: &[RoomTagDef]) -> Result<(), EngineError> {
    files::write_json(&tags_path(ctx), &tags)
}

fn create_room_tag(
    lock: &Mutex<()>,
    ctx: &EngineContext,
    name: String,
    colour: TagColour,
) -> Result<EngineResponse, EngineError> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(EngineError::invalid_argument("the tag name is empty"));
    }
    if name.chars().count() > 64 {
        return Err(EngineError::invalid_argument(
            "the tag name is longer than 64 characters",
        ));
    }

    let _guard = lock.lock().unwrap_or_else(PoisonError::into_inner);
    let mut tags = read_catalog(ctx)?;
    if tags.iter().any(|tag| tag.name.eq_ignore_ascii_case(&name)) {
        return Err(EngineError::invalid_argument(
            "a tag with that name already exists",
        ));
    }
    let tag = RoomTagDef {
        id: Uuid::new_v4().to_string(),
        name,
        colour,
    };
    tags.push(tag.clone());
    write_catalog(ctx, &tags)?;
    Ok(EngineResponse::RoomTagCreated { tag })
}

fn rename_room_tag(
    lock: &Mutex<()>,
    ctx: &EngineContext,
    id: String,
    name: Option<String>,
    colour: Option<TagColour>,
) -> Result<EngineResponse, EngineError> {
    if name.is_none() && colour.is_none() {
        return Err(EngineError::invalid_argument(
            "a rename needs a new name or colour",
        ));
    }

    let _guard = lock.lock().unwrap_or_else(PoisonError::into_inner);
    let mut tags = read_catalog(ctx)?;
    if let Some(name) = &name {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(EngineError::invalid_argument("the tag name is empty"));
        }
        if tags
            .iter()
            .any(|other| other.id != id && other.name.eq_ignore_ascii_case(trimmed))
        {
            return Err(EngineError::invalid_argument(
                "a tag with that name already exists",
            ));
        }
    }
    let tag = tags
        .iter_mut()
        .find(|tag| tag.id == id)
        .ok_or_else(|| EngineError::not_found(format!("tag '{id}'")))?;
    if let Some(name) = name {
        tag.name = name.trim().to_string();
    }
    if let Some(colour) = colour {
        tag.colour = colour;
    }
    write_catalog(ctx, &tags)?;
    Ok(EngineResponse::RoomTagsUpdated)
}

fn reorder_room_tags(
    lock: &Mutex<()>,
    ctx: &EngineContext,
    ids: Vec<String>,
) -> Result<EngineResponse, EngineError> {
    let _guard = lock.lock().unwrap_or_else(PoisonError::into_inner);
    let mut tags = read_catalog(ctx)?;
    let complete =
        ids.len() == tags.len() && ids.iter().all(|id| tags.iter().any(|tag| &tag.id == id));
    if !complete {
        return Err(EngineError::invalid_argument(
            "the new order must list every tag exactly once",
        ));
    }
    tags.sort_by_key(|tag| {
        ids.iter()
            .position(|id| id == &tag.id)
            .unwrap_or(usize::MAX)
    });
    write_catalog(ctx, &tags)?;
    Ok(EngineResponse::RoomTagsUpdated)
}

async fn set_room_tag(
    lock: &Mutex<()>,
    ctx: &EngineContext,
    room_id: &RoomId,
    tag_id: String,
    enabled: bool,
) -> Result<EngineResponse, EngineError> {
    {
        let _guard = lock.lock().unwrap_or_else(PoisonError::into_inner);
        let tags = read_catalog(ctx)?;
        if !tags.iter().any(|tag| tag.id == tag_id) {
            return Err(EngineError::not_found(format!("tag '{tag_id}'")));
        }
    }

    let chat = support::chat_record(ctx, room_id).await?;
    let mut local = chat.local_tags.clone();
    if enabled && !local.contains(&tag_id) {
        local.push(tag_id);
    } else if !enabled {
        local.retain(|id| id != &tag_id);
    }
    local.sort();
    ctx.store
        .update_chat_flags(
            &chat.chat_id,
            ChatFlagsUpdate {
                tags: Some(local),
                ..ChatFlagsUpdate::default()
            },
        )
        .await
        .map_err(map_store)?;
    ctx.bus.chats();
    Ok(EngineResponse::RoomTagsUpdated)
}

async fn delete_room_tag(
    lock: &Mutex<()>,
    ctx: &EngineContext,
    tag_id: String,
) -> Result<EngineResponse, EngineError> {
    {
        let _guard = lock.lock().unwrap_or_else(PoisonError::into_inner);
        let mut tags = read_catalog(ctx)?;
        let before = tags.len();
        tags.retain(|tag| tag.id != tag_id);
        if tags.len() == before {
            return Err(EngineError::not_found(format!("tag '{tag_id}'")));
        }
        write_catalog(ctx, &tags)?;
    }

    for chat in all_chats(ctx).await? {
        if chat.local_tags.contains(&tag_id) {
            let mut local = chat.local_tags.clone();
            local.retain(|id| id != &tag_id);
            ctx.store
                .update_chat_flags(
                    &chat.chat_id,
                    ChatFlagsUpdate {
                        tags: Some(local),
                        ..ChatFlagsUpdate::default()
                    },
                )
                .await
                .map_err(map_store)?;
        }
    }
    ctx.bus.chats();
    Ok(EngineResponse::RoomTagsUpdated)
}

// === Drafts ===

fn drafts_path(ctx: &EngineContext) -> PathBuf {
    ctx.data_dir.join("drafts.json")
}

fn read_drafts(ctx: &EngineContext) -> Result<BTreeMap<String, DraftDto>, EngineError> {
    Ok(files::read_json::<BTreeMap<String, DraftDto>>(&drafts_path(ctx))?.unwrap_or_default())
}

async fn save_draft(
    lock: &Mutex<()>,
    ctx: &EngineContext,
    room_id: &RoomId,
    text: String,
    reply_to: Option<EventId>,
) -> Result<EngineResponse, EngineError> {
    let chat = support::chat_record(ctx, room_id).await?;
    if let Some(reply_to) = &reply_to {
        support::message_in_chat(ctx, &chat.chat_id, reply_to).await?;
    }

    let empty = text.trim().is_empty();
    {
        let _guard = lock.lock().unwrap_or_else(PoisonError::into_inner);
        let mut drafts = read_drafts(ctx)?;
        if empty {
            drafts.remove(&chat.chat_id);
        } else {
            drafts.insert(
                chat.chat_id.clone(),
                DraftDto {
                    text: text.clone(),
                    reply_to,
                },
            );
        }
        files::write_json(&drafts_path(ctx), &drafts)?;
    }

    let preview = if empty {
        None
    } else {
        Some(files::truncate_chars(&text, 80))
    };
    ctx.store
        .update_chat_flags(
            &chat.chat_id,
            ChatFlagsUpdate {
                draft: Some(preview),
                ..ChatFlagsUpdate::default()
            },
        )
        .await
        .map_err(map_store)?;
    ctx.bus.chats();
    Ok(EngineResponse::DraftSaved)
}

// === Shared media and search ===

async fn room_media(
    ctx: &EngineContext,
    room_id: &RoomId,
    kind: RoomMediaKind,
    before: Option<String>,
    limit: u32,
) -> Result<EngineResponse, EngineError> {
    let chat = support::chat_record(ctx, room_id).await?;
    let cursor = match before.as_deref() {
        Some(raw) => Some(parse_cursor(raw)?),
        None => None,
    };
    let query = match kind {
        RoomMediaKind::Media => RoomMediaQuery::Media,
        RoomMediaKind::Files => RoomMediaQuery::Files,
        RoomMediaKind::Links => RoomMediaQuery::Links,
    };
    let limit = limit.clamp(1, 200);
    let records = ctx
        .store
        .room_media(&chat.chat_id, query, cursor, limit)
        .await
        .map_err(map_store)?;
    let next = records
        .last()
        .map(|record| format!("{}:{}", record.sent_at, record.message_id));
    let names = participant_names(ctx, &chat.chat_id).await?;

    let mut items = Vec::with_capacity(records.len());
    for record in records {
        items.push(room_media_item(ctx, &record, &names).await?);
    }
    Ok(EngineResponse::RoomMedia { items, next })
}

fn parse_cursor(raw: &str) -> Result<(i64, String), EngineError> {
    let (sent_at, message_id) = raw
        .split_once(':')
        .ok_or_else(|| EngineError::invalid_argument(format!("invalid media cursor '{raw}'")))?;
    let sent_at = sent_at
        .parse::<i64>()
        .map_err(|_| EngineError::invalid_argument(format!("invalid media cursor '{raw}'")))?;
    if message_id.is_empty() {
        return Err(EngineError::invalid_argument(format!(
            "invalid media cursor '{raw}'"
        )));
    }
    Ok((sent_at, message_id.to_string()))
}

async fn room_media_item(
    ctx: &EngineContext,
    record: &MessageRecord,
    names: &HashMap<String, String>,
) -> Result<RoomMediaItem, EngineError> {
    let media = match &record.media_id {
        Some(media_id) => ctx.store.get_media(media_id).await.map_err(map_store)?,
        None => None,
    };
    let kind = match record.kind {
        StoreMessageKind::Image => "image",
        StoreMessageKind::Video => "video",
        StoreMessageKind::Audio => "audio",
        StoreMessageKind::Voice => "voice",
        StoreMessageKind::File => "file",
        _ => "link",
    };
    let urls = if kind == "link" {
        links_in(record.body.as_deref().unwrap_or_default())
    } else {
        Vec::new()
    };
    Ok(RoomMediaItem {
        event_id: EventId(record.message_id.clone()),
        sender: UserId(record.sender_remote_id.clone()),
        sender_display_name: names.get(&record.sender_remote_id).cloned(),
        timestamp_millis: u64::try_from(record.sent_at).unwrap_or(0),
        kind: kind.to_string(),
        body: record
            .body
            .clone()
            .or_else(|| media.as_ref().and_then(|media| media.file_name.clone()))
            .unwrap_or_default(),
        source: media.as_ref().map(|media| MediaSource {
            media_id: media.media_id.clone(),
        }),
        mime_type: media.as_ref().and_then(|media| media.mime.clone()),
        size_bytes: media
            .as_ref()
            .and_then(|media| media.size_bytes)
            .and_then(|value| u64::try_from(value).ok()),
        width: media
            .as_ref()
            .and_then(|media| media.width)
            .and_then(|value| u32::try_from(value).ok()),
        height: media
            .as_ref()
            .and_then(|media| media.height)
            .and_then(|value| u32::try_from(value).ok()),
        urls,
    })
}

fn links_in(body: &str) -> Vec<String> {
    body.split_whitespace()
        .filter(|word| word.starts_with("http://") || word.starts_with("https://"))
        .map(|word| {
            word.trim_end_matches(|character: char| {
                matches!(character, ',' | '.' | ')' | ';' | '"' | '\'')
            })
            .to_string()
        })
        .filter(|word| word.len() > "https://".len())
        .collect()
}

async fn participant_names(
    ctx: &EngineContext,
    chat_id: &str,
) -> Result<HashMap<String, String>, EngineError> {
    Ok(ctx
        .store
        .list_participants(chat_id)
        .await
        .map_err(map_store)?
        .into_iter()
        .filter_map(|participant| match participant.display_name {
            Some(name) => Some((participant.remote_user_id, name)),
            None => None,
        })
        .collect())
}

async fn search_room(
    ctx: &EngineContext,
    room_id: &RoomId,
    query: String,
    limit: u32,
) -> Result<EngineResponse, EngineError> {
    let chat = support::chat_record(ctx, room_id).await?;
    if query.trim().is_empty() {
        return Ok(EngineResponse::RoomSearchResults { hits: Vec::new() });
    }
    let limit = limit.clamp(1, 200);
    let records = ctx
        .store
        .search_messages_in_chat(&chat.chat_id, &query, limit)
        .await
        .map_err(map_store)?;
    let names = participant_names(ctx, &chat.chat_id).await?;
    let hits = records
        .into_iter()
        .map(|record| RoomSearchHit {
            event_id: EventId(record.message_id),
            sender: UserId(record.sender_remote_id.clone()),
            sender_display_name: names.get(&record.sender_remote_id).cloned(),
            timestamp_millis: u64::try_from(record.sent_at).unwrap_or(0),
            body: record.body.unwrap_or_default(),
        })
        .collect();
    Ok(EngineResponse::RoomSearchResults { hits })
}

async fn search_all(
    ctx: &EngineContext,
    query: String,
    limit: Option<u32>,
) -> Result<EngineResponse, EngineError> {
    if query.trim().is_empty() {
        return Ok(EngineResponse::AllMessagesFound { hits: Vec::new() });
    }
    let limit = limit.unwrap_or(50).clamp(1, 200);
    let records = ctx
        .store
        .search_messages(&query, None, limit)
        .await
        .map_err(map_store)?;

    let mut hits = Vec::with_capacity(records.len());
    for record in records {
        let room_name = ctx
            .store
            .get_chat(&record.chat_id)
            .await
            .map_err(map_store)?
            .and_then(|chat| chat.title);
        let sender_display_name = ctx
            .store
            .get_participant(&record.chat_id, &record.sender_remote_id)
            .await
            .map_err(map_store)?
            .and_then(|participant| participant.display_name);
        hits.push(GlobalSearchHit {
            room_id: RoomId(record.chat_id.clone()),
            room_name,
            event_id: EventId(record.message_id),
            sender: UserId(record.sender_remote_id),
            sender_display_name,
            timestamp_millis: u64::try_from(record.sent_at).unwrap_or(0),
            body: record.body.unwrap_or_default(),
        });
    }
    Ok(EngineResponse::AllMessagesFound { hits })
}

// === Cache and sync ===

async fn clear_media_cache(ctx: &EngineContext) -> Result<EngineResponse, EngineError> {
    let paths = ctx
        .store
        .clear_media_local_paths(None)
        .await
        .map_err(map_store)?;
    for path in paths {
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => {
                tracing::warn!(path = %path, error = %err, "cannot remove a cached media file");
            }
        }
    }
    Ok(EngineResponse::MediaCacheCleared)
}

async fn refresh_sync(ctx: &EngineContext) -> Result<EngineResponse, EngineError> {
    ctx.accounts.stop_all().await;
    // Every worker is stopped, so releasing interrupted rows cannot duplicate a live operation.
    if let Err(err) = ctx.store.reset_in_flight_outbox().await {
        tracing::warn!(error = %err, "cannot recover interrupted outbox rows");
    }
    let records = ctx.store.list_accounts().await.map_err(map_store)?;
    for record in records {
        if record.state != StoreAccountState::Active {
            continue;
        }
        if let Err(err) = start_stored_account(ctx, record).await {
            tracing::warn!(error = %err.message, "account could not be restarted");
        }
    }
    Ok(EngineResponse::SyncRefreshed)
}

/// Every chat of every account, archived included.
async fn all_chats(ctx: &EngineContext) -> Result<Vec<ChatRecord>, EngineError> {
    let mut chats = ctx
        .store
        .list_chats(None, false, MAX_ROOM_QUERY)
        .await
        .map_err(map_store)?;
    chats.extend(
        ctx.store
            .list_chats(None, true, MAX_ROOM_QUERY)
            .await
            .map_err(map_store)?,
    );
    Ok(chats)
}
