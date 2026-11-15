//! Deterministic in-memory connector behind the `testing` feature.
//!
//! It implements the whole [`Connector`] contract without a network: a two-step login
//! (phone number → code), three seeded chats plus a forum with two topics, seeded history, real
//! message state for send/edit/delete, reaction tracking, and event emission. The contract tests
//! drive it, and from M1 on the desktop and Android end-to-end suites use it through the real
//! engine.
//!
//! Determinism makes it useful as a fixture: ids and timestamps are assigned from a counter and a
//! clock that starts at a fixed value, so two runs observe the same events in the same order.
//!
//! # Login
//!
//! `begin_login` returns [`LoginStep::PhoneNumber`]. Any non-empty number is accepted and answered
//! with [`LoginStep::Code`] (`via: app`, five characters). The only accepted code is [`FAKE_CODE`].
//! A wrong code returns [`LoginStep::Failed`] with
//! [`LoginFailure::InvalidInput`](crate::LoginFailure::InvalidInput) naming the `code` field; a
//! failed step is terminal, so the host restarts with `begin_login`. QR, password, cookie and
//! credential inputs are not modelled: they return a `Failed` step with
//! [`LoginFailure::NotSupported`](crate::LoginFailure::NotSupported).
//!
//! # Session blobs
//!
//! A successful login issues the blob `alloy-fake-session:v1:<remote_user_id>`. [`restore`] accepts
//! exactly that shape on any instance, which is what lets an E2E test persist the blob, drop the
//! connector and restore it later.
//!
//! [`restore`]: crate::Connector::restore

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use async_trait::async_trait;
use tokio::sync::Mutex;
use tokio::sync::mpsc::Sender;

use crate::capabilities::Capabilities;
use crate::connector::{Connector, OutgoingMessage, Page, SendAck};
use crate::error::ConnectorError;
use crate::events::{ConnectionState, ConnectorEvent};
use crate::login::{CodeChannel, LoginFailure, LoginInput, LoginStep};
use crate::types::{
    ChatKind, MediaRef, MessageKind, Network, RemoteChat, RemoteMember, RemoteMessage, RemoteUser,
    SessionBlob,
};

/// The only login code the fake accepts.
pub const FAKE_CODE: &str = "12345";
/// Remote user id of the account the fake logs in as.
pub const FAKE_USER_ID: &str = "fake-self";
/// Display name of the account the fake logs in as.
pub const FAKE_DISPLAY_NAME: &str = "Fake User";
/// Prefix of every session blob the fake issues.
pub const FAKE_SESSION_PREFIX: &str = "alloy-fake-session:v1:";

/// Seeded direct chat.
pub const FAKE_CHAT_DIRECT: &str = "fake-chat-direct";
/// Seeded group chat.
pub const FAKE_CHAT_GROUP: &str = "fake-chat-group";
/// Seeded channel with no history.
pub const FAKE_CHAT_CHANNEL: &str = "fake-chat-channel";
/// Seeded forum: a group whose topics are separate conversations. It has no messages of its own.
pub const FAKE_CHAT_FORUM: &str = "fake-chat-forum";
/// The General topic of [`FAKE_CHAT_FORUM`].
pub const FAKE_TOPIC_GENERAL: &str = "fake-chat-forum/t1";
/// The "Release notes" topic of [`FAKE_CHAT_FORUM`].
pub const FAKE_TOPIC_RELEASE_NOTES: &str = "fake-chat-forum/t2";

const MAX_REACTIONS_PER_USER: u8 = 3;
/// The fake's pin limit, so the engine's handling of a refused list can be tested.
pub const MAX_PINNED_CHATS: usize = 5;

type ReactionKey = (String, String);
type ReactionSet = BTreeSet<(String, String)>;

/// A deterministic connector that speaks [`Network::Fake`].
pub struct FakeConnector {
    inner: Mutex<Inner>,
}

impl FakeConnector {
    /// Creates a connector with the seeded chats and history, logged out.
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Inner::seeded()),
        }
    }

    /// Injects an incoming message from another participant, as a live update would.
    ///
    /// Appends it to the chat's history, bumps the chat's unread count and activity timestamp, and
    /// emits [`ConnectorEvent::Message`] followed by [`ConnectorEvent::ChatUpserted`] when the
    /// connector has been started. Returns the stored message, including its assigned id and
    /// timestamp. Tests use it to exercise receive paths without a network.
    pub async fn inject_incoming(
        &self,
        remote_chat_id: &str,
        sender_remote_id: &str,
        body: impl Into<String>,
    ) -> Result<RemoteMessage, ConnectorError> {
        let mut inner = self.inner.lock().await;
        let chat_index = inner.chat_index(remote_chat_id)?;

        let seq = inner.next_seq;
        inner.next_seq += 1;
        let sent_at = inner.clock_ms;
        inner.clock_ms += 1_000;

        let message = RemoteMessage {
            remote_chat_id: remote_chat_id.to_string(),
            remote_msg_id: format!("fake-msg-sent-{seq}"),
            txn_id: None,
            sender_remote_id: sender_remote_id.to_string(),
            sender: fake_profile(sender_remote_id),
            is_own: false,
            sent_at,
            edited_at: None,
            kind: MessageKind::Text,
            body: Some(body.into()),
            formatted: None,
            reply_to_remote_msg_id: None,
            media: None,
            pinned: false,
        };

        inner
            .messages
            .entry(remote_chat_id.to_string())
            .or_default()
            .push(message.clone());
        inner.chats[chat_index].last_activity_at = sent_at;
        inner.chats[chat_index].unread_count += 1;
        let chat = inner.chats[chat_index].clone();
        let events = inner.events.clone();
        drop(inner);

        if let Some(events) = events {
            emit(
                &events,
                ConnectorEvent::Message {
                    message: message.clone(),
                },
            )
            .await?;
            emit(&events, ConnectorEvent::ChatUpserted { chat }).await?;
        }

        Ok(message)
    }

    /// Sends an arbitrary event through the running event channel, as the network would deliver it.
    ///
    /// Tests use it to simulate events a real network sends in a shape the fake does not produce
    /// itself, such as Telegram naming a forum's deletes and pins by the forum id alone. Fails when
    /// the connector has not been started, since there is no channel to send through.
    pub async fn emit_event(&self, event: ConnectorEvent) -> Result<(), ConnectorError> {
        let events = self.inner.lock().await.events.clone();
        match events {
            Some(events) => emit(&events, event).await,
            None => Err(ConnectorError::Protocol {
                message: "connector is not started".to_string(),
            }),
        }
    }

    /// Returns the read marker last stored by [`Connector::mark_read`] for a chat.
    ///
    /// The trait has no read side for local markers, so this accessor exists for tests that assert
    /// the receipt was actually recorded rather than only validated.
    pub async fn read_marker(&self, remote_chat_id: &str) -> Option<String> {
        let inner = self.inner.lock().await;
        inner.read_markers.get(remote_chat_id).cloned()
    }

    fn session_for(remote_user_id: &str) -> SessionBlob {
        SessionBlob::new(format!("{FAKE_SESSION_PREFIX}{remote_user_id}").into_bytes())
    }
}

impl Default for FakeConnector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Connector for FakeConnector {
    fn network(&self) -> Network {
        Network::Fake
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            edit: true,
            edit_window_secs: None,
            delete_for_everyone: true,
            reactions: true,
            max_reactions_per_user: Some(MAX_REACTIONS_PER_USER),
            replies: true,
            threads: false,
            typing: true,
            read_receipts: true,
            delivered_receipts: false,
            voice: true,
            polls: false,
            location: false,
            max_upload_bytes: Some(50 * 1024 * 1024),
            formatted_text: false,
            pin_chats: true,
            pin_messages: true,
            topics: true,
        }
    }

    async fn begin_login(&self) -> LoginStep {
        let mut inner = self.inner.lock().await;
        inner.login = LoginState::Idle;
        LoginStep::PhoneNumber
    }

    async fn submit_login(&self, input: LoginInput) -> LoginStep {
        let mut inner = self.inner.lock().await;
        let state = std::mem::replace(&mut inner.login, LoginState::Idle);

        match (state, input) {
            (_, LoginInput::PhoneNumber { phone }) => {
                if phone.trim().is_empty() {
                    return failed(Some("phone"), "a phone number is required");
                }
                inner.login = LoginState::AwaitingCode;
                LoginStep::Code {
                    via: CodeChannel::App,
                    length: Some(FAKE_CODE.len() as u8),
                }
            }
            (LoginState::AwaitingCode, LoginInput::Code { code }) => {
                if code.trim() != FAKE_CODE {
                    return failed(Some("code"), "incorrect code");
                }
                let user = fake_user();
                inner.login = LoginState::LoggedIn { user: user.clone() };
                LoginStep::Done {
                    remote_user_id: user.remote_user_id,
                    display_name: user.display_name,
                    session: Self::session_for(FAKE_USER_ID),
                }
            }
            (_, LoginInput::Code { .. }) => {
                failed(Some("step"), "code submitted before a phone number")
            }
            (_, LoginInput::QrRefresh)
            | (_, LoginInput::Switch { .. })
            | (_, LoginInput::Password { .. })
            | (_, LoginInput::Cookies { .. })
            | (_, LoginInput::Credentials { .. }) => LoginStep::Failed {
                reason: LoginFailure::NotSupported,
            },
        }
    }

    async fn restore(&self, session: SessionBlob) -> Result<RemoteUser, ConnectorError> {
        let text =
            std::str::from_utf8(session.as_bytes()).map_err(|_| ConnectorError::Unauthorized)?;
        let remote_user_id = text
            .strip_prefix(FAKE_SESSION_PREFIX)
            .filter(|id| !id.is_empty())
            .ok_or(ConnectorError::Unauthorized)?;
        let user = RemoteUser {
            remote_user_id: remote_user_id.to_string(),
            display_name: FAKE_DISPLAY_NAME.to_string(),
            avatar: None,
        };
        let mut inner = self.inner.lock().await;
        inner.login = LoginState::LoggedIn { user: user.clone() };
        Ok(user)
    }

    async fn start(&self, events: Sender<ConnectorEvent>) -> Result<(), ConnectorError> {
        {
            let mut inner = self.inner.lock().await;
            logged_in_user(&inner)?;
            inner.events = Some(events.clone());
        }

        emit(
            &events,
            ConnectorEvent::Connection {
                state: ConnectionState::Connecting,
            },
        )
        .await?;
        emit(
            &events,
            ConnectorEvent::Connection {
                state: ConnectionState::Online,
            },
        )
        .await
    }

    async fn stop(&self) -> Result<(), ConnectorError> {
        let mut inner = self.inner.lock().await;
        inner.events = None;
        Ok(())
    }

    async fn list_chats(
        &self,
        cursor: Option<String>,
        limit: usize,
    ) -> Result<Page<RemoteChat>, ConnectorError> {
        let inner = self.inner.lock().await;
        logged_in_user(&inner)?;
        validate_limit(limit)?;

        let offset = match cursor {
            None => 0,
            Some(cursor) => cursor
                .parse::<usize>()
                .map_err(|_| ConnectorError::InvalidInput {
                    message: format!("invalid chat cursor: {cursor}"),
                })?,
        };

        let mut chats = inner.chats.clone();
        chats.sort_by(|a, b| {
            b.last_activity_at
                .cmp(&a.last_activity_at)
                .then_with(|| a.remote_chat_id.cmp(&b.remote_chat_id))
        });

        let items: Vec<RemoteChat> = chats.iter().skip(offset).take(limit).cloned().collect();
        let consumed = offset + items.len();
        let next_cursor = (consumed < chats.len()).then(|| consumed.to_string());
        Ok(Page { items, next_cursor })
    }

    async fn list_members(
        &self,
        remote_chat_id: &str,
    ) -> Result<Vec<RemoteMember>, ConnectorError> {
        let inner = self.inner.lock().await;
        let me = logged_in_user(&inner)?.clone();
        inner.chat_index(remote_chat_id)?;
        if remote_chat_id != FAKE_CHAT_GROUP {
            return Ok(Vec::new());
        }
        Ok(vec![
            RemoteMember {
                user: fake_profile("fake-user-charles").expect("a seeded profile"),
                role: Some("owner".to_string()),
            },
            RemoteMember {
                user: me,
                role: None,
            },
        ])
    }

    async fn list_topics(&self, remote_chat_id: &str) -> Result<Vec<RemoteChat>, ConnectorError> {
        let inner = self.inner.lock().await;
        logged_in_user(&inner)?;
        let forum = &inner.chats[inner.chat_index(remote_chat_id)?];
        if !forum.is_forum {
            return Err(ConnectorError::InvalidInput {
                message: format!("not a forum: {remote_chat_id}"),
            });
        }
        let mut topics: Vec<RemoteChat> = inner
            .chats
            .iter()
            .filter(|chat| chat.parent_remote_chat_id.as_deref() == Some(remote_chat_id))
            .cloned()
            .collect();
        topics.sort_by(|a, b| {
            b.last_activity_at
                .cmp(&a.last_activity_at)
                .then_with(|| a.remote_chat_id.cmp(&b.remote_chat_id))
        });
        Ok(topics)
    }

    async fn backfill(
        &self,
        remote_chat_id: &str,
        before: Option<String>,
        limit: usize,
    ) -> Result<Page<RemoteMessage>, ConnectorError> {
        let inner = self.inner.lock().await;
        logged_in_user(&inner)?;
        validate_limit(limit)?;

        let messages =
            inner
                .messages
                .get(remote_chat_id)
                .ok_or_else(|| ConnectorError::InvalidInput {
                    message: format!("unknown chat: {remote_chat_id}"),
                })?;

        let end = match before {
            None => messages.len(),
            Some(anchor) => messages
                .iter()
                .position(|message| message.remote_msg_id == anchor)
                .ok_or_else(|| ConnectorError::InvalidInput {
                    message: format!("unknown message: {anchor}"),
                })?,
        };
        let start = end.saturating_sub(limit);
        let items: Vec<RemoteMessage> = messages[start..end].iter().rev().cloned().collect();
        let next_cursor = (start > 0).then(|| messages[start].remote_msg_id.clone());

        Ok(Page { items, next_cursor })
    }

    async fn send(&self, message: OutgoingMessage) -> Result<SendAck, ConnectorError> {
        let mut inner = self.inner.lock().await;
        let sender_remote_id = logged_in_user(&inner)?.remote_user_id.clone();

        if message.txn_id.trim().is_empty() {
            return Err(ConnectorError::InvalidInput {
                message: "txn_id must not be empty".to_string(),
            });
        }
        if message.body.is_none() && message.formatted.is_none() && message.attachment.is_none() {
            return Err(ConnectorError::InvalidInput {
                message: "a message needs a body or an attachment".to_string(),
            });
        }
        let chat_index = inner.chat_index(&message.remote_chat_id)?;

        let seq = inner.next_seq;
        inner.next_seq += 1;
        let sent_at = inner.clock_ms;
        inner.clock_ms += 1_000;
        let remote_msg_id = format!("fake-msg-sent-{seq}");

        let (kind, media) = match &message.attachment {
            Some(attachment) => {
                let mime = attachment
                    .mime
                    .clone()
                    .unwrap_or_else(|| "application/octet-stream".to_string());
                let kind = if mime.starts_with("image/") {
                    MessageKind::Image
                } else if mime.starts_with("video/") {
                    MessageKind::Video
                } else if mime.starts_with("audio/") {
                    MessageKind::Audio
                } else {
                    MessageKind::File
                };
                (
                    kind,
                    Some(MediaRef {
                        remote_ref: format!("fake-media:{remote_msg_id}"),
                        mime,
                        size_bytes: None,
                        width: None,
                        height: None,
                        duration_ms: None,
                        file_name: attachment.file_name.clone(),
                    }),
                )
            }
            None => (MessageKind::Text, None),
        };

        let chat_messages = inner
            .messages
            .entry(message.remote_chat_id.clone())
            .or_default();
        if let Some(reply_to) = &message.reply_to {
            let known = chat_messages
                .iter()
                .any(|stored| stored.remote_msg_id == *reply_to);
            if !known {
                return Err(ConnectorError::InvalidInput {
                    message: format!("unknown reply target: {reply_to}"),
                });
            }
        }

        let echo = RemoteMessage {
            remote_chat_id: message.remote_chat_id.clone(),
            remote_msg_id: remote_msg_id.clone(),
            txn_id: Some(message.txn_id.clone()),
            sender_remote_id,
            sender: None,
            is_own: true,
            sent_at,
            edited_at: None,
            kind,
            body: message.body,
            formatted: message.formatted,
            reply_to_remote_msg_id: message.reply_to,
            media,
            pinned: false,
        };
        chat_messages.push(echo.clone());
        inner.chats[chat_index].last_activity_at = sent_at;
        let events = inner.events.clone();
        drop(inner);

        if let Some(events) = events {
            emit(&events, ConnectorEvent::Message { message: echo }).await?;
        }

        Ok(SendAck {
            txn_id: message.txn_id,
            remote_msg_id,
            sent_at,
        })
    }

    async fn edit(
        &self,
        remote_chat_id: &str,
        remote_msg_id: &str,
        body: String,
        formatted: Option<String>,
    ) -> Result<(), ConnectorError> {
        if body.trim().is_empty() && formatted.is_none() {
            return Err(ConnectorError::InvalidInput {
                message: "an edited message needs a body or formatted body".to_string(),
            });
        }

        let mut inner = self.inner.lock().await;
        logged_in_user(&inner)?;
        let edited_at = inner.clock_ms;
        inner.clock_ms += 1_000;

        {
            let messages = inner.messages.get_mut(remote_chat_id).ok_or_else(|| {
                ConnectorError::InvalidInput {
                    message: format!("unknown chat: {remote_chat_id}"),
                }
            })?;
            let message = messages
                .iter_mut()
                .find(|message| message.remote_msg_id == remote_msg_id)
                .ok_or_else(|| ConnectorError::InvalidInput {
                    message: format!("unknown message: {remote_msg_id}"),
                })?;
            if !message.is_own {
                return Err(ConnectorError::InvalidInput {
                    message: "cannot edit another participant's message".to_string(),
                });
            }
            message.body = Some(body.clone());
            message.formatted = formatted.clone();
            message.edited_at = Some(edited_at);
        }

        let events = inner.events.clone();
        drop(inner);
        if let Some(events) = events {
            emit(
                &events,
                ConnectorEvent::MessageEdited {
                    remote_chat_id: remote_chat_id.to_string(),
                    remote_msg_id: remote_msg_id.to_string(),
                    body: Some(body),
                    formatted,
                    edited_at,
                },
            )
            .await?;
        }
        Ok(())
    }

    async fn delete(
        &self,
        remote_chat_id: &str,
        remote_msg_id: &str,
        for_everyone: bool,
    ) -> Result<(), ConnectorError> {
        let mut inner = self.inner.lock().await;
        logged_in_user(&inner)?;

        {
            let messages = inner.messages.get_mut(remote_chat_id).ok_or_else(|| {
                ConnectorError::InvalidInput {
                    message: format!("unknown chat: {remote_chat_id}"),
                }
            })?;
            let index = messages
                .iter()
                .position(|message| message.remote_msg_id == remote_msg_id)
                .ok_or_else(|| ConnectorError::InvalidInput {
                    message: format!("unknown message: {remote_msg_id}"),
                })?;
            if for_everyone && !messages[index].is_own {
                return Err(ConnectorError::InvalidInput {
                    message: "cannot delete another participant's message for everyone".to_string(),
                });
            }
            messages.remove(index);
        }

        let events = inner.events.clone();
        drop(inner);
        if let Some(events) = events {
            emit(
                &events,
                ConnectorEvent::MessageDeleted {
                    remote_chat_id: remote_chat_id.to_string(),
                    remote_msg_id: remote_msg_id.to_string(),
                },
            )
            .await?;
        }
        Ok(())
    }

    async fn react(
        &self,
        remote_chat_id: &str,
        remote_msg_id: &str,
        emoji: &str,
    ) -> Result<(), ConnectorError> {
        let mut inner = self.inner.lock().await;
        let user_id = logged_in_user(&inner)?.remote_user_id.clone();
        if emoji.trim().is_empty() {
            return Err(ConnectorError::InvalidInput {
                message: "emoji must not be empty".to_string(),
            });
        }
        inner.require_message(remote_chat_id, remote_msg_id)?;

        let key = (remote_chat_id.to_string(), remote_msg_id.to_string());
        let reactions = inner.reactions.entry(key).or_default();
        if reactions
            .iter()
            .any(|(user, reaction)| user == &user_id && reaction == emoji)
        {
            return Ok(());
        }
        let used = reactions
            .iter()
            .filter(|(user, _)| user == &user_id)
            .count();
        if used >= usize::from(MAX_REACTIONS_PER_USER) {
            return Err(ConnectorError::InvalidInput {
                message: "reaction limit reached".to_string(),
            });
        }
        reactions.insert((user_id.clone(), emoji.to_string()));

        let events = inner.events.clone();
        drop(inner);
        if let Some(events) = events {
            emit(
                &events,
                ConnectorEvent::Reaction {
                    remote_chat_id: remote_chat_id.to_string(),
                    remote_msg_id: remote_msg_id.to_string(),
                    sender_remote_id: user_id,
                    emoji: emoji.to_string(),
                    added: true,
                },
            )
            .await?;
        }
        Ok(())
    }

    async fn unreact(
        &self,
        remote_chat_id: &str,
        remote_msg_id: &str,
        emoji: &str,
    ) -> Result<(), ConnectorError> {
        let mut inner = self.inner.lock().await;
        let user_id = logged_in_user(&inner)?.remote_user_id.clone();
        inner.require_message(remote_chat_id, remote_msg_id)?;

        let key = (remote_chat_id.to_string(), remote_msg_id.to_string());
        let removed = inner
            .reactions
            .get_mut(&key)
            .is_some_and(|reactions| reactions.remove(&(user_id.clone(), emoji.to_string())));
        if !removed {
            return Ok(());
        }

        let events = inner.events.clone();
        drop(inner);
        if let Some(events) = events {
            emit(
                &events,
                ConnectorEvent::Reaction {
                    remote_chat_id: remote_chat_id.to_string(),
                    remote_msg_id: remote_msg_id.to_string(),
                    sender_remote_id: user_id,
                    emoji: emoji.to_string(),
                    added: false,
                },
            )
            .await?;
        }
        Ok(())
    }

    async fn mark_read(
        &self,
        remote_chat_id: &str,
        up_to_remote_msg_id: &str,
    ) -> Result<(), ConnectorError> {
        let mut inner = self.inner.lock().await;
        logged_in_user(&inner)?;
        inner.require_message(remote_chat_id, up_to_remote_msg_id)?;
        inner
            .read_markers
            .insert(remote_chat_id.to_string(), up_to_remote_msg_id.to_string());
        Ok(())
    }

    async fn set_typing(&self, remote_chat_id: &str, _typing: bool) -> Result<(), ConnectorError> {
        let inner = self.inner.lock().await;
        logged_in_user(&inner)?;
        inner.chat_index(remote_chat_id)?;
        Ok(())
    }

    async fn set_chat_pinned(
        &self,
        remote_chat_id: &str,
        pinned: bool,
    ) -> Result<(), ConnectorError> {
        let mut inner = self.inner.lock().await;
        logged_in_user(&inner)?;
        let index = inner.chat_index(remote_chat_id)?;
        if inner.chats[index].is_pinned == pinned {
            return Ok(());
        }
        if pinned {
            // The newly pinned chat goes to the top; the others move down one.
            for chat in inner.chats.iter_mut().filter(|chat| chat.is_pinned) {
                chat.pin_order = chat.pin_order.map(|order| order + 1);
            }
            inner.chats[index].is_pinned = true;
            inner.chats[index].pin_order = Some(0);
        } else {
            let gone = inner.chats[index].pin_order;
            inner.chats[index].is_pinned = false;
            inner.chats[index].pin_order = None;
            if let Some(gone) = gone {
                for chat in inner.chats.iter_mut().filter(|chat| chat.is_pinned) {
                    chat.pin_order = chat
                        .pin_order
                        .map(|order| if order > gone { order - 1 } else { order });
                }
            }
        }
        let events = inner.events.clone();
        drop(inner);
        if let Some(events) = events {
            emit(
                &events,
                ConnectorEvent::ChatPinned {
                    remote_chat_id: remote_chat_id.to_string(),
                    pinned,
                },
            )
            .await?;
        }
        Ok(())
    }

    async fn reorder_pinned_chats(&self, remote_chat_ids: &[String]) -> Result<(), ConnectorError> {
        let mut inner = self.inner.lock().await;
        logged_in_user(&inner)?;
        for id in remote_chat_ids {
            inner.chat_index(id)?;
        }
        if remote_chat_ids.len() > MAX_PINNED_CHATS {
            return Err(ConnectorError::InvalidInput {
                message: format!("at most {MAX_PINNED_CHATS} chats can be pinned"),
            });
        }
        for chat in &mut inner.chats {
            let position = remote_chat_ids
                .iter()
                .position(|id| *id == chat.remote_chat_id);
            chat.is_pinned = position.is_some();
            chat.pin_order = position.map(|value| value as u32);
        }
        let events = inner.events.clone();
        drop(inner);
        if let Some(events) = events {
            emit(
                &events,
                ConnectorEvent::PinnedChatsReordered {
                    remote_chat_ids: remote_chat_ids.to_vec(),
                },
            )
            .await?;
        }
        Ok(())
    }

    async fn set_message_pinned(
        &self,
        remote_chat_id: &str,
        remote_msg_id: &str,
        pinned: bool,
    ) -> Result<(), ConnectorError> {
        let mut inner = self.inner.lock().await;
        logged_in_user(&inner)?;
        inner.require_message(remote_chat_id, remote_msg_id)?;
        let message = inner
            .messages
            .get_mut(remote_chat_id)
            .and_then(|messages| {
                messages
                    .iter_mut()
                    .find(|message| message.remote_msg_id == remote_msg_id)
            })
            .expect("require_message checked the message exists");
        if message.pinned == pinned {
            return Ok(());
        }
        message.pinned = pinned;
        let events = inner.events.clone();
        drop(inner);
        if let Some(events) = events {
            emit(
                &events,
                ConnectorEvent::MessagePinned {
                    remote_chat_id: remote_chat_id.to_string(),
                    remote_msg_id: remote_msg_id.to_string(),
                    pinned,
                },
            )
            .await?;
        }
        Ok(())
    }

    async fn fetch_media(&self, media: &MediaRef, dest_path: &Path) -> Result<u64, ConnectorError> {
        {
            let inner = self.inner.lock().await;
            logged_in_user(&inner)?;
        }

        let remote_ref = &media.remote_ref;
        let mime = &media.mime;
        let bytes = format!("fake-media:{remote_ref}:{mime}").into_bytes();
        if let Err(error) = std::fs::write(dest_path, &bytes) {
            return Err(ConnectorError::InvalidInput {
                message: format!("cannot write media to {}: {error}", dest_path.display()),
            });
        }
        Ok(bytes.len() as u64)
    }

    async fn logout(&self) -> Result<(), ConnectorError> {
        let mut inner = self.inner.lock().await;
        inner.login = LoginState::Idle;
        inner.events = None;
        Ok(())
    }
}

enum LoginState {
    Idle,
    AwaitingCode,
    LoggedIn { user: RemoteUser },
}

struct Inner {
    login: LoginState,
    events: Option<Sender<ConnectorEvent>>,
    chats: Vec<RemoteChat>,
    messages: BTreeMap<String, Vec<RemoteMessage>>,
    reactions: BTreeMap<ReactionKey, ReactionSet>,
    read_markers: BTreeMap<String, String>,
    next_seq: u64,
    clock_ms: i64,
}

impl Inner {
    fn seeded() -> Self {
        let chats = vec![
            RemoteChat {
                remote_chat_id: FAKE_CHAT_DIRECT.to_string(),
                kind: ChatKind::Direct,
                title: "Ada Lovelace".to_string(),
                avatar: None,
                last_activity_at: 1_700_000_300_000,
                unread_count: 2,
                is_pinned: true,
                pin_order: Some(0),
                is_archived: false,
                is_muted: false,
                parent_remote_chat_id: None,
                is_forum: false,
            },
            RemoteChat {
                remote_chat_id: FAKE_CHAT_GROUP.to_string(),
                kind: ChatKind::Group,
                title: "Analytical Engine".to_string(),
                avatar: None,
                last_activity_at: 1_700_000_200_000,
                unread_count: 0,
                is_pinned: false,
                pin_order: None,
                is_archived: false,
                is_muted: true,
                parent_remote_chat_id: None,
                is_forum: false,
            },
            RemoteChat {
                remote_chat_id: FAKE_CHAT_CHANNEL.to_string(),
                kind: ChatKind::Channel,
                title: "Alloy News".to_string(),
                avatar: None,
                last_activity_at: 1_700_000_100_000,
                unread_count: 5,
                is_pinned: false,
                pin_order: None,
                is_archived: true,
                is_muted: false,
                parent_remote_chat_id: None,
                is_forum: false,
            },
            RemoteChat {
                remote_chat_id: FAKE_CHAT_FORUM.to_string(),
                kind: ChatKind::Group,
                title: "Alloy Forum".to_string(),
                avatar: None,
                last_activity_at: 1_700_000_150_000,
                unread_count: 0,
                is_pinned: false,
                pin_order: None,
                is_archived: false,
                is_muted: false,
                parent_remote_chat_id: None,
                is_forum: true,
            },
            RemoteChat {
                remote_chat_id: FAKE_TOPIC_GENERAL.to_string(),
                kind: ChatKind::Group,
                title: "General".to_string(),
                avatar: None,
                last_activity_at: 1_700_000_120_000,
                unread_count: 0,
                is_pinned: false,
                pin_order: None,
                is_archived: false,
                is_muted: false,
                parent_remote_chat_id: Some(FAKE_CHAT_FORUM.to_string()),
                is_forum: false,
            },
            RemoteChat {
                remote_chat_id: FAKE_TOPIC_RELEASE_NOTES.to_string(),
                kind: ChatKind::Group,
                title: "Release notes".to_string(),
                avatar: None,
                last_activity_at: 1_700_000_150_000,
                unread_count: 0,
                is_pinned: false,
                pin_order: None,
                is_archived: false,
                is_muted: false,
                parent_remote_chat_id: Some(FAKE_CHAT_FORUM.to_string()),
                is_forum: false,
            },
        ];

        let mut messages = BTreeMap::new();
        messages.insert(
            FAKE_CHAT_DIRECT.to_string(),
            seed_messages(
                FAKE_CHAT_DIRECT,
                "direct",
                "fake-user-ada",
                &[
                    "Are you free for a walk?",
                    "The engine sketch is ready.",
                    "I pushed the notes.",
                    "Thanks!",
                    "Talk tomorrow.",
                ],
            ),
        );
        messages.insert(
            FAKE_CHAT_GROUP.to_string(),
            seed_messages(
                FAKE_CHAT_GROUP,
                "group",
                "fake-user-charles",
                &["Meeting moved to 10:00.", "Works for me.", "Same here."],
            ),
        );
        messages.insert(FAKE_CHAT_CHANNEL.to_string(), Vec::new());
        messages.insert(FAKE_CHAT_FORUM.to_string(), Vec::new());
        messages.insert(
            FAKE_TOPIC_GENERAL.to_string(),
            seed_messages(
                FAKE_TOPIC_GENERAL,
                "topic-general",
                "fake-user-ada",
                &[
                    "Welcome to the forum.",
                    "Keep each topic to its subject.",
                    "Thanks for organizing.",
                ],
            ),
        );
        messages.insert(
            FAKE_TOPIC_RELEASE_NOTES.to_string(),
            seed_messages(
                FAKE_TOPIC_RELEASE_NOTES,
                "topic-release",
                "fake-user-charles",
                &["The release notes draft is up.", "Added the forum section."],
            ),
        );

        Self {
            login: LoginState::Idle,
            events: None,
            chats,
            messages,
            reactions: BTreeMap::new(),
            read_markers: BTreeMap::new(),
            next_seq: 1,
            clock_ms: 1_700_000_400_000,
        }
    }

    fn chat_index(&self, remote_chat_id: &str) -> Result<usize, ConnectorError> {
        self.chats
            .iter()
            .position(|chat| chat.remote_chat_id == remote_chat_id)
            .ok_or_else(|| ConnectorError::InvalidInput {
                message: format!("unknown chat: {remote_chat_id}"),
            })
    }

    fn require_message(
        &self,
        remote_chat_id: &str,
        remote_msg_id: &str,
    ) -> Result<(), ConnectorError> {
        let messages =
            self.messages
                .get(remote_chat_id)
                .ok_or_else(|| ConnectorError::InvalidInput {
                    message: format!("unknown chat: {remote_chat_id}"),
                })?;
        if messages
            .iter()
            .any(|message| message.remote_msg_id == remote_msg_id)
        {
            Ok(())
        } else {
            Err(ConnectorError::InvalidInput {
                message: format!("unknown message: {remote_msg_id}"),
            })
        }
    }
}

fn fake_user() -> RemoteUser {
    RemoteUser {
        remote_user_id: FAKE_USER_ID.to_string(),
        display_name: FAKE_DISPLAY_NAME.to_string(),
        avatar: None,
    }
}

/// The profile the fake network delivers with a message from one of its seeded people. Any
/// other sender arrives as a bare id, as a network that sends no profile would deliver it.
fn fake_profile(remote_user_id: &str) -> Option<RemoteUser> {
    let display_name = match remote_user_id {
        "fake-user-ada" => "Ada Lovelace",
        "fake-user-charles" => "Charles Babbage",
        _ => return None,
    };
    Some(RemoteUser {
        remote_user_id: remote_user_id.to_string(),
        display_name: display_name.to_string(),
        avatar: None,
    })
}

fn logged_in_user(inner: &Inner) -> Result<&RemoteUser, ConnectorError> {
    match &inner.login {
        LoginState::LoggedIn { user } => Ok(user),
        LoginState::Idle | LoginState::AwaitingCode => Err(ConnectorError::Unauthorized),
    }
}

fn validate_limit(limit: usize) -> Result<(), ConnectorError> {
    if limit == 0 {
        Err(ConnectorError::InvalidInput {
            message: "limit must be greater than zero".to_string(),
        })
    } else {
        Ok(())
    }
}

fn failed(field: Option<&str>, message: &str) -> LoginStep {
    LoginStep::Failed {
        reason: LoginFailure::InvalidInput {
            field: field.map(str::to_string),
            message: message.to_string(),
        },
    }
}

async fn emit(
    events: &Sender<ConnectorEvent>,
    event: ConnectorEvent,
) -> Result<(), ConnectorError> {
    events
        .send(event)
        .await
        .map_err(|_| ConnectorError::Protocol {
            message: "connector event receiver was dropped".to_string(),
        })
}

fn seed_messages(
    remote_chat_id: &str,
    slug: &str,
    sender_remote_id: &str,
    bodies: &[&str],
) -> Vec<RemoteMessage> {
    bodies
        .iter()
        .enumerate()
        .map(|(index, body)| RemoteMessage {
            remote_chat_id: remote_chat_id.to_string(),
            remote_msg_id: format!("fake-msg-{slug}-{}", index + 1),
            txn_id: None,
            sender_remote_id: sender_remote_id.to_string(),
            sender: fake_profile(sender_remote_id),
            is_own: false,
            sent_at: 1_700_000_000_000 + (index as i64) * 60_000,
            edited_at: None,
            kind: MessageKind::Text,
            body: Some((*body).to_string()),
            formatted: None,
            reply_to_remote_msg_id: None,
            media: None,
            pinned: false,
        })
        .collect()
}
