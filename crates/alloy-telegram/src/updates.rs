//! Live updates: one task that owns grammers’ update stream and turns it into contract events.
//!
//! grammers’ message box already resolves update gaps (it asks Telegram for the missing range), so
//! this task only has to map what it sees. It runs for as long as the client pool lives, independent
//! of whether the host is listening: with no sink the events are dropped, which is what “events are
//! never replayed” means — the host gets history from [`crate::mapping`]-backed backfill instead.
//!
//! On a transport error the task reports [`ConnectionState::Offline`] and retries with backoff;
//! grammers re-establishes the connection underneath. On a dead session it reports
//! [`ConnectionState::NeedsRelogin`] and stops, because no retry can succeed.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use alloy_connector::{ConnectionState, ConnectorEvent};
use grammers_client::client::UpdateStream;
use grammers_client::message::Message as GrammersMessage;
use grammers_client::update::Update;
use grammers_tl_types as tl;
use tokio::time::sleep;
use tracing::{debug, warn};

use crate::connector::Shared;
use crate::error;
use crate::mapping::{self, PeerKindTag};

/// First delay before retrying a failed update read.
const BACKOFF_START: Duration = Duration::from_secs(1);
/// Longest delay between update-read retries.
const BACKOFF_MAX: Duration = Duration::from_secs(30);

/// Runs until the session dies or the pool is dropped.
///
/// `Connector::start` reports the transition to `Online`, because it is the call that makes the
/// stream observable by the host. This task only reports a *recovery* to `Online` after an
/// `Offline`, so the host never sees two identical transitions in a row.
pub(crate) async fn run(shared: Arc<Shared>, mut stream: UpdateStream) {
    let mut backoff = BACKOFF_START;
    let mut recovering = false;
    loop {
        match stream.next().await {
            Ok(update) => {
                backoff = BACKOFF_START;
                if recovering {
                    recovering = false;
                    shared
                        .emit(ConnectorEvent::Connection {
                            state: ConnectionState::Online,
                        })
                        .await;
                }
                handle(&shared, update).await;
                if shared.checkpoint_due() {
                    checkpoint(&shared, &stream).await;
                }
            }
            Err(error) => {
                if error::is_dead_session(&error) {
                    debug!(error = %error, "telegram session is dead");
                    shared
                        .emit(ConnectorEvent::Connection {
                            state: ConnectionState::NeedsRelogin,
                        })
                        .await;
                    return;
                }
                let reason = error.to_string();
                warn!(error = %reason, "telegram update stream failed; retrying");
                recovering = true;
                shared
                    .emit(ConnectorEvent::Connection {
                        state: ConnectionState::Offline { reason },
                    })
                    .await;
                sleep(backoff).await;
                backoff = (backoff * 2).min(BACKOFF_MAX);
            }
        }
    }
}

/// Persists the update-state checkpoint so a restart resumes near the same point.
async fn checkpoint(shared: &Shared, stream: &UpdateStream) {
    match stream.sync_update_state().await {
        Ok(()) => {
            shared.mark_checkpoint();
            if let Err(error) = shared.emit_session().await {
                debug!(error = %error, "cannot emit the telegram session checkpoint");
            }
        }
        Err(error) => warn!(error = %error, "cannot persist the telegram update state"),
    }
}

async fn handle(shared: &Shared, update: Update) {
    match update {
        Update::NewMessage(message) => {
            let message: &GrammersMessage = &message;
            remember(shared, message).await;
            let Some(chat_id) = mapping::message_chat(&message.raw) else {
                debug!(id = message.id(), "telegram message without a peer");
                return;
            };
            let txn_id = shared.txn_of(message.id());
            let Some(mapped) =
                mapping::message_from_raw(&message.raw, &chat_id, shared.self_bare_id(), txn_id)
            else {
                return;
            };
            shared
                .emit(ConnectorEvent::Message { message: mapped })
                .await;
        }
        Update::MessageEdited(message) => {
            let message: &GrammersMessage = &message;
            remember(shared, message).await;
            let Some(chat_id) = mapping::message_chat(&message.raw) else {
                return;
            };
            let (body, formatted) = match &message.raw {
                tl::enums::Message::Message(raw) => {
                    mapping::text_and_formatted(&raw.message, raw.entities.as_deref())
                }
                _ => (None, None),
            };
            let edited_at = mapping::raw_date(&message.raw)
                .map(mapping::seconds_to_ms)
                .unwrap_or_else(mapping::now_ms);
            shared
                .emit(ConnectorEvent::MessageEdited {
                    remote_chat_id: chat_id,
                    remote_msg_id: mapping::message_id(&message.raw).to_string(),
                    body,
                    formatted,
                    edited_at,
                })
                .await;
        }
        Update::MessageDeleted(deletion) => {
            let channel_chat = deletion
                .channel_id()
                .map(|id| mapping::encode_peer(PeerKindTag::Channel, id));
            for id in deletion.into_messages() {
                let chat_id = channel_chat.clone().or_else(|| shared.chat_of_message(id));
                match chat_id {
                    Some(remote_chat_id) => {
                        shared.forget_message(id);
                        shared
                            .emit(ConnectorEvent::MessageDeleted {
                                remote_chat_id,
                                remote_msg_id: id.to_string(),
                            })
                            .await;
                    }
                    None => debug!(msg_id = id, "cannot attribute a deleted telegram message"),
                }
            }
        }
        Update::Raw(raw) => handle_raw(shared, &raw.raw).await,
        // Bot-only updates (callbacks, inline queries) have no meaning for a personal account.
        _ => {}
    }
}

async fn handle_raw(shared: &Shared, update: &tl::enums::Update) {
    match update {
        tl::enums::Update::UserTyping(typing) => {
            let chat = mapping::encode_peer(PeerKindTag::User, typing.user_id);
            shared
                .emit(ConnectorEvent::Typing {
                    remote_user_id: chat.clone(),
                    remote_chat_id: chat,
                    typing: is_typing(&typing.action),
                })
                .await;
        }
        tl::enums::Update::ChatUserTyping(typing) => {
            let Some(user) = mapping::tl_peer(&typing.from_id) else {
                return;
            };
            shared
                .emit(ConnectorEvent::Typing {
                    remote_chat_id: mapping::encode_peer(PeerKindTag::Chat, typing.chat_id),
                    remote_user_id: mapping::encode_peer(user.0, user.1),
                    typing: is_typing(&typing.action),
                })
                .await;
        }
        tl::enums::Update::ChannelUserTyping(typing) => {
            let Some(user) = mapping::tl_peer(&typing.from_id) else {
                return;
            };
            shared
                .emit(ConnectorEvent::Typing {
                    remote_chat_id: mapping::encode_peer(PeerKindTag::Channel, typing.channel_id),
                    remote_user_id: mapping::encode_peer(user.0, user.1),
                    typing: is_typing(&typing.action),
                })
                .await;
        }
        tl::enums::Update::ReadHistoryInbox(read) => {
            // Only direct chats say who read: the peer is the only other participant there.
            let Some((kind, bare_id)) = mapping::tl_peer(&read.peer) else {
                return;
            };
            if kind != PeerKindTag::User {
                debug!("telegram read receipt without a reader identity");
                return;
            }
            let chat = mapping::encode_peer(kind, bare_id);
            shared
                .emit(ConnectorEvent::ReadMarker {
                    remote_chat_id: chat.clone(),
                    remote_user_id: chat,
                    up_to_remote_msg_id: read.max_id.to_string(),
                })
                .await;
        }
        tl::enums::Update::ReadHistoryOutbox(read) => {
            // This account read the chat on another device.
            let Some((kind, bare_id)) = mapping::tl_peer(&read.peer) else {
                return;
            };
            let Some(myself) = shared.self_profile() else {
                return;
            };
            shared
                .emit(ConnectorEvent::ReadMarker {
                    remote_chat_id: mapping::encode_peer(kind, bare_id),
                    remote_user_id: myself.remote_user_id,
                    up_to_remote_msg_id: read.max_id.to_string(),
                })
                .await;
        }
        tl::enums::Update::MessageReactions(changed) => {
            let Some((kind, bare_id)) = mapping::tl_peer(&changed.peer) else {
                return;
            };
            let chat = mapping::encode_peer(kind, bare_id);
            // Single-constructor boxed types convert infallibly, so no variant has to be named.
            let reactions: tl::types::MessageReactions = changed.reactions.clone().into();
            let Some(recent) = reactions.recent_reactions else {
                return;
            };
            for entry in recent {
                let entry: tl::types::MessagePeerReaction = entry.into();
                let tl::enums::Reaction::Emoji(reaction) = &entry.reaction else {
                    continue;
                };
                let Some((kind, bare_id)) = mapping::tl_peer(&entry.peer_id) else {
                    continue;
                };
                shared
                    .emit(ConnectorEvent::Reaction {
                        remote_chat_id: chat.clone(),
                        remote_msg_id: changed.msg_id.to_string(),
                        sender_remote_id: mapping::encode_peer(kind, bare_id),
                        emoji: reaction.emoticon.clone(),
                        added: true,
                    })
                    .await;
            }
        }
        // The outstanding QR token changed because another client asked for one. The host refreshes
        // by submitting `QrRefresh`, which always re-exports, so nothing is forwarded here.
        tl::enums::Update::LoginToken => debug!("telegram login token changed"),
        _ => {}
    }
}

/// `sendMessageCancelAction` stops an indicator; every other action shows one.
fn is_typing(action: &tl::enums::SendMessageAction) -> bool {
    !matches!(
        action,
        tl::enums::SendMessageAction::SendMessageCancelAction
    )
}

/// Adds the message’s chat and sender peers to the resolvable cache.
async fn remember(shared: &Shared, message: &GrammersMessage) {
    if let Some(peer) = message.peer()
        && let Ok(Some(reference)) = peer.to_ref().await
    {
        shared.remember_peer(reference);
    }
    if let Some(sender) = message.sender()
        && let Ok(Some(reference)) = sender.to_ref().await
    {
        shared.remember_peer(reference);
    }
}

/// Bounded message-id → chat/txn map, so a delete without a chat id and the echo of a local send
/// can still be attributed.
///
/// Telegram’s non-channel deletes carry only message ids, and message ids are per-account there, so
/// a single id → chat entry is unambiguous for that account.
pub(crate) struct MessageIndex {
    entries: HashMap<i32, IndexEntry>,
    order: VecDeque<i32>,
    capacity: usize,
}

struct IndexEntry {
    chat: String,
    txn_id: Option<String>,
}

impl MessageIndex {
    /// Creates an index that forgets the oldest entry once `capacity` ids are remembered.
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            entries: HashMap::with_capacity(capacity),
            order: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    /// Remembers where a message was sent and, for a local send, which transaction it answers.
    pub(crate) fn insert(&mut self, id: i32, chat: String, txn_id: Option<String>) {
        if let Some(entry) = self.entries.get_mut(&id) {
            entry.chat = chat;
            if txn_id.is_some() {
                entry.txn_id = txn_id;
            }
            return;
        }
        while self.order.len() >= self.capacity {
            match self.order.pop_front() {
                Some(oldest) => {
                    self.entries.remove(&oldest);
                }
                None => break,
            }
        }
        self.order.push_back(id);
        self.entries.insert(id, IndexEntry { chat, txn_id });
    }

    /// The chat a remembered message belongs to.
    pub(crate) fn chat(&self, id: i32) -> Option<String> {
        self.entries.get(&id).map(|entry| entry.chat.clone())
    }

    /// The transaction id of a locally sent message, while it is remembered.
    pub(crate) fn txn_id(&self, id: i32) -> Option<String> {
        self.entries.get(&id).and_then(|entry| entry.txn_id.clone())
    }

    /// Forgets a message that was deleted or consumed.
    pub(crate) fn remove(&mut self, id: i32) {
        if self.entries.remove(&id).is_some() {
            self.order.retain(|remembered| *remembered != id);
        }
    }

    /// How many ids are remembered.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index() -> MessageIndex {
        MessageIndex::new(3)
    }

    #[test]
    fn the_index_remembers_chats_and_transactions() {
        let mut index = index();
        index.insert(10, "u1".to_string(), None);
        index.insert(11, "u1".to_string(), Some("txn-a".to_string()));

        assert_eq!(index.chat(10).as_deref(), Some("u1"));
        assert_eq!(index.txn_id(10), None);
        assert_eq!(index.txn_id(11).as_deref(), Some("txn-a"));
        assert_eq!(index.chat(12), None);
    }

    #[test]
    fn the_index_forgets_the_oldest_ids_first() {
        let mut index = index();
        index.insert(1, "u1".to_string(), None);
        index.insert(2, "u1".to_string(), None);
        index.insert(3, "u1".to_string(), None);
        index.insert(4, "u1".to_string(), None);

        assert_eq!(index.len(), 3);
        assert_eq!(index.chat(1), None);
        assert_eq!(index.chat(4).as_deref(), Some("u1"));
    }

    #[test]
    fn reinserting_an_id_does_not_grow_the_index() {
        let mut index = index();
        index.insert(7, "u1".to_string(), None);
        index.insert(7, "u1".to_string(), Some("txn-b".to_string()));

        assert_eq!(index.len(), 1);
        assert_eq!(index.txn_id(7).as_deref(), Some("txn-b"));
    }

    #[test]
    fn removing_an_id_frees_it() {
        let mut index = index();
        index.insert(5, "u1".to_string(), None);
        index.remove(5);

        assert_eq!(index.len(), 0);
        assert_eq!(index.chat(5), None);

        index.insert(6, "u2".to_string(), None);
        assert_eq!(index.chat(6).as_deref(), Some("u2"));
    }

    #[test]
    fn only_the_cancel_action_stops_a_typing_indicator() {
        assert!(!is_typing(&tl::types::SendMessageCancelAction {}.into()));
        assert!(is_typing(&tl::types::SendMessageTypingAction {}.into()));
        assert!(is_typing(
            &tl::types::SendMessageUploadDocumentAction { progress: 40 }.into()
        ));
    }
}
