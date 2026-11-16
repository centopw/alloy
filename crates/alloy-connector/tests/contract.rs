//! Contract tests for `alloy-connector`.
//!
//! Run with `cargo test -p alloy-connector --features testing`.

use std::collections::BTreeMap;

use alloy_connector::fake::{
    FAKE_CHAT_CHANNEL, FAKE_CHAT_DIRECT, FAKE_CHAT_FORUM, FAKE_CHAT_GROUP, FAKE_CODE,
    FAKE_TOPIC_GENERAL, FAKE_TOPIC_RELEASE_NOTES, FAKE_USER_ID, FakeConnector, MAX_PINNED_CHATS,
};
use alloy_connector::{
    ChallengeKind, ChatKind, CodeChannel, ConnectionState, Connector, ConnectorError,
    ConnectorEvent, Frame, LoginAlternative, LoginFailure, LoginInput, LoginStep, MediaRef,
    MessageKind, Network, OutgoingMessage, PROTOCOL_VERSION, RemoteChat, RemoteMessage, RemoteUser,
    SessionBlob, decode_frame, encode_frame,
};
use tokio::sync::mpsc::Receiver;

fn direct_chat() -> RemoteChat {
    RemoteChat {
        remote_chat_id: "chat-1".to_string(),
        kind: ChatKind::Direct,
        title: "Ada".to_string(),
        avatar: None,
        last_activity_at: 1_700_000_000_000,
        unread_count: 2,
        is_pinned: false,
        pin_order: None,
        is_archived: false,
        is_muted: false,
        parent_remote_chat_id: None,
        is_forum: false,
    }
}

fn text_message() -> RemoteMessage {
    RemoteMessage {
        remote_chat_id: "chat-1".to_string(),
        remote_msg_id: "msg-1".to_string(),
        txn_id: Some("txn-1".to_string()),
        sender_remote_id: "user-1".to_string(),
        sender: Some(RemoteUser {
            remote_user_id: "user-1".to_string(),
            display_name: "Ada".to_string(),
            avatar: None,
        }),
        is_own: true,
        sent_at: 1_700_000_000_000,
        edited_at: None,
        kind: MessageKind::Image,
        body: Some("look".to_string()),
        formatted: Some("<b>look</b>".to_string()),
        reply_to_remote_msg_id: None,
        media: Some(MediaRef {
            remote_ref: "remote-ref".to_string(),
            mime: "image/png".to_string(),
            size_bytes: Some(1024),
            width: Some(64),
            height: Some(32),
            duration_ms: None,
            file_name: Some("look.png".to_string()),
        }),
        pinned: false,
    }
}

fn round_trip(frame: &Frame) {
    let line = encode_frame(frame);
    assert!(line.ends_with('\n'), "frames are newline terminated");
    assert_eq!(line.matches('\n').count(), 1, "frames are one line");

    let decoded = decode_frame(&line).expect("encoded frame decodes");
    assert_eq!(&decoded, frame);
    let decoded_without_newline = decode_frame(line.trim_end()).expect("frame decodes without \\n");
    assert_eq!(&decoded_without_newline, frame);
}

async fn next_event(events: &mut Receiver<ConnectorEvent>) -> ConnectorEvent {
    events.recv().await.expect("connector event")
}

async fn logged_in() -> FakeConnector {
    let connector = FakeConnector::new();
    assert_eq!(connector.begin_login().await, LoginStep::PhoneNumber);
    let step = connector
        .submit_login(LoginInput::PhoneNumber {
            phone: "+15550100".to_string(),
        })
        .await;
    assert!(matches!(step, LoginStep::Code { .. }));
    let step = connector
        .submit_login(LoginInput::Code {
            code: FAKE_CODE.to_string(),
        })
        .await;
    assert!(matches!(step, LoginStep::Done { .. }));
    connector
}

fn outgoing(txn_id: &str) -> OutgoingMessage {
    OutgoingMessage {
        txn_id: txn_id.to_string(),
        remote_chat_id: FAKE_CHAT_DIRECT.to_string(),
        body: Some("hello from the test".to_string()),
        formatted: None,
        reply_to: None,
        attachment: None,
    }
}

#[test]
fn network_names_match_the_shared_contract() {
    assert_eq!(
        serde_json::to_string(&Network::Telegram).unwrap(),
        "\"telegram\""
    );
    assert_eq!(
        serde_json::to_string(&Network::Messenger).unwrap(),
        "\"messenger\""
    );
    assert_eq!(serde_json::to_string(&Network::Fake).unwrap(), "\"fake\"");
    assert_eq!(
        serde_json::from_str::<Network>("\"telegram\"").unwrap(),
        Network::Telegram
    );
    assert_eq!(Network::Telegram.as_str(), "telegram");
    assert!(serde_json::from_str::<Network>("\"Telegram\"").is_err());
}

#[test]
fn session_blob_is_base64_on_the_wire_and_redacted_in_debug() {
    let blob = SessionBlob::new(vec![0, 1, 2, 255]);
    assert_eq!(serde_json::to_string(&blob).unwrap(), "\"AAEC/w==\"");

    let decoded: SessionBlob = serde_json::from_str("\"AAEC/w==\"").unwrap();
    assert_eq!(decoded, blob);

    let debug = format!("{blob:?}");
    assert!(debug.contains("4 bytes"), "debug reports the size: {debug}");
    assert!(!debug.contains("AAEC"), "debug leaks the bytes: {debug}");
}

#[test]
fn every_frame_variant_round_trips() {
    let chat = direct_chat();
    let message = text_message();
    let capabilities = FakeConnector::new().capabilities();

    let frames = vec![
        Frame::Hello {
            protocol_version: PROTOCOL_VERSION,
            network: Network::Messenger,
            capabilities: capabilities.clone(),
        },
        Frame::Request {
            id: 7,
            method: "backfill".to_string(),
            params: serde_json::json!({
                "remote_chat_id": "chat-1",
                "before": null,
                "limit": 50
            }),
        },
        Frame::Response {
            id: 7,
            result: Some(serde_json::json!({"items": [], "next_cursor": null})),
            error: None,
        },
        Frame::Response {
            id: 8,
            result: None,
            error: Some(ConnectorError::RateLimited {
                retry_after_ms: 1500,
            }),
        },
        Frame::Event {
            event: ConnectorEvent::Connection {
                state: ConnectionState::Connecting,
            },
        },
        Frame::Event {
            event: ConnectorEvent::Connection {
                state: ConnectionState::Online,
            },
        },
        Frame::Event {
            event: ConnectorEvent::Connection {
                state: ConnectionState::Offline {
                    reason: "tls handshake failed".to_string(),
                    attempt: None,
                    next_retry_millis: None,
                },
            },
        },
        Frame::Event {
            event: ConnectorEvent::Connection {
                state: ConnectionState::Offline {
                    reason: "connection lost".to_string(),
                    attempt: Some(3),
                    next_retry_millis: Some(8_000),
                },
            },
        },
        Frame::Event {
            event: ConnectorEvent::Connection {
                state: ConnectionState::NeedsRelogin,
            },
        },
        Frame::Event {
            event: ConnectorEvent::ChatUpserted { chat: chat.clone() },
        },
        Frame::Event {
            event: ConnectorEvent::ChatRemoved {
                remote_chat_id: chat.remote_chat_id.clone(),
            },
        },
        Frame::Event {
            event: ConnectorEvent::Message {
                message: message.clone(),
            },
        },
        Frame::Event {
            event: ConnectorEvent::MessageEdited {
                remote_chat_id: chat.remote_chat_id.clone(),
                remote_msg_id: message.remote_msg_id.clone(),
                body: Some("edited".to_string()),
                formatted: None,
                edited_at: 1_700_000_100_000,
            },
        },
        Frame::Event {
            event: ConnectorEvent::MessageDeleted {
                remote_chat_id: chat.remote_chat_id.clone(),
                remote_msg_id: message.remote_msg_id.clone(),
            },
        },
        Frame::Event {
            event: ConnectorEvent::Reaction {
                remote_chat_id: chat.remote_chat_id.clone(),
                remote_msg_id: message.remote_msg_id.clone(),
                sender_remote_id: "user-1".to_string(),
                emoji: "👍".to_string(),
                added: true,
            },
        },
        Frame::Event {
            event: ConnectorEvent::ReadMarker {
                remote_chat_id: chat.remote_chat_id.clone(),
                remote_user_id: "user-1".to_string(),
                up_to_remote_msg_id: message.remote_msg_id.clone(),
            },
        },
        Frame::Event {
            event: ConnectorEvent::Typing {
                remote_chat_id: chat.remote_chat_id.clone(),
                remote_user_id: "user-1".to_string(),
                typing: true,
            },
        },
        Frame::Event {
            event: ConnectorEvent::SessionUpdated {
                session: SessionBlob::new(vec![9, 8, 7]),
            },
        },
    ];

    for frame in &frames {
        round_trip(frame);
    }
}

#[test]
fn a_sidecar_offline_frame_carries_the_attempt_and_the_delay() {
    // The exact line `sidecars/alloy-meta` writes; `protocol_test.go` pins the same bytes from the
    // Go side, so a rename on either side of the wire fails here or there.
    let line = r#"{"type":"event","event":{"type":"connection","state":{"type":"offline","reason":"network unreachable","attempt":3,"next_retry_millis":8000}}}"#;

    match decode_frame(line).unwrap() {
        Frame::Event {
            event:
                ConnectorEvent::Connection {
                    state:
                        ConnectionState::Offline {
                            reason,
                            attempt,
                            next_retry_millis,
                        },
                },
        } => {
            assert_eq!(reason, "network unreachable");
            assert_eq!(attempt, Some(3));
            assert_eq!(next_retry_millis, Some(8_000));
        }
        other => panic!("expected an offline event, got {other:?}"),
    }
}

#[test]
fn hello_carries_network_and_capabilities() {
    let connector = FakeConnector::new();
    let frame = Frame::Hello {
        protocol_version: PROTOCOL_VERSION,
        network: connector.network(),
        capabilities: connector.capabilities(),
    };

    match decode_frame(&encode_frame(&frame)).unwrap() {
        Frame::Hello {
            protocol_version,
            network,
            capabilities,
        } => {
            assert_eq!(protocol_version, PROTOCOL_VERSION);
            assert_eq!(network, Network::Fake);
            assert_eq!(capabilities, connector.capabilities());
            assert_eq!(capabilities.max_reactions_per_user, Some(3));
            assert_eq!(capabilities.max_upload_bytes, Some(50 * 1024 * 1024));
            assert!(capabilities.edit && capabilities.replies && capabilities.typing);
            assert!(!capabilities.threads && !capabilities.polls);
        }
        other => panic!("expected a hello frame, got {other:?}"),
    }
}

#[test]
fn malformed_frames_are_protocol_errors() {
    for line in [
        "",
        "   \n",
        "not json",
        "{}",
        r#"{"type":"request","id":1}"#,
        "42",
    ] {
        let error = match decode_frame(line) {
            Ok(frame) => panic!("expected an error for {line:?}, got {frame:?}"),
            Err(error) => error,
        };
        assert!(
            matches!(&error, ConnectorError::Protocol { .. }),
            "unexpected error for {line:?}: {error:?}"
        );
        assert!(!error.is_retryable());
    }
}

#[test]
fn login_alternatives_round_trip() {
    let step = LoginStep::Credentials {
        username_label: "Email".to_string(),
        alternatives: vec![LoginAlternative::Cookies],
    };
    let value = serde_json::to_value(&step).unwrap();
    assert_eq!(
        value,
        serde_json::json!({
            "type": "credentials",
            "username_label": "Email",
            "alternatives": ["cookies"],
        })
    );
    assert_eq!(serde_json::from_value::<LoginStep>(value).unwrap(), step);

    let input = LoginInput::Switch {
        to: LoginAlternative::Cookies,
    };
    let value = serde_json::to_value(&input).unwrap();
    assert_eq!(
        value,
        serde_json::json!({"type": "switch", "to": "cookies"})
    );
    assert_eq!(serde_json::from_value::<LoginInput>(value).unwrap(), input);
}

#[test]
fn challenge_step_frame_decodes() {
    // The sidecar writes a challenge as its own step: the kind is what the UI leads with, the
    // message is the network's own explanation, and the step keeps the attempt open.
    for (wire_kind, kind) in [
        ("checkpoint", ChallengeKind::Checkpoint),
        ("two_factor", ChallengeKind::TwoFactor),
        ("consent", ChallengeKind::Consent),
    ] {
        let frame =
            format!(r#"{{"type":"challenge","kind":"{wire_kind}","message":"confirm it"}}"#);
        let step: LoginStep = serde_json::from_str(&frame).expect("challenge frame decodes");
        assert_eq!(
            step,
            LoginStep::Challenge {
                kind,
                message: "confirm it".to_string(),
            }
        );
        // A challenge is answered, not terminal: the id stays valid until the user cancels.
        assert!(!step.is_terminal());
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&frame).unwrap(),
            serde_json::to_value(&step).unwrap()
        );
    }

    // A challenge with no kind, or an unknown one, is a contract violation rather than a silent
    // fallback to a locked account.
    for frame in [
        r#"{"type":"challenge","message":"confirm it"}"#,
        r#"{"type":"challenge","kind":"captcha","message":"confirm it"}"#,
    ] {
        assert!(serde_json::from_str::<LoginStep>(frame).is_err());
    }
}

#[test]
fn login_input_debug_redacts_secrets() {
    let inputs = [
        LoginInput::Code {
            code: "987654".to_string(),
        },
        LoginInput::Password {
            password: "hunter2-pw".to_string(),
        },
        LoginInput::Credentials {
            username: "ada".to_string(),
            password: "hunter2-pw".to_string(),
        },
        LoginInput::Cookies {
            values: BTreeMap::from([
                ("c_user".to_string(), "cookie-secret".to_string()),
                ("xs".to_string(), "cookie-secret".to_string()),
            ]),
        },
    ];

    for input in &inputs {
        let debug = format!("{input:?}");
        assert!(!debug.contains("987654"), "code leaked: {debug}");
        assert!(!debug.contains("hunter2-pw"), "password leaked: {debug}");
        assert!(!debug.contains("cookie-secret"), "cookie leaked: {debug}");
        assert!(debug.contains("<redacted>"), "no redaction marker: {debug}");
    }

    let cookies = format!("{:?}", inputs[3]);
    assert!(
        cookies.contains("c_user"),
        "cookie names stay visible: {cookies}"
    );
    assert!(
        cookies.contains("xs"),
        "cookie names stay visible: {cookies}"
    );
}

#[tokio::test]
async fn fake_login_happy_path_issues_a_restorable_session() {
    let connector = FakeConnector::new();
    assert_eq!(connector.begin_login().await, LoginStep::PhoneNumber);

    let step = connector
        .submit_login(LoginInput::PhoneNumber {
            phone: "+15550100".to_string(),
        })
        .await;
    assert!(!step.is_terminal());
    match &step {
        LoginStep::Code { via, length } => {
            assert_eq!(*via, CodeChannel::App);
            assert_eq!(*length, Some(FAKE_CODE.len() as u8));
        }
        other => panic!("expected a code prompt, got {other:?}"),
    }

    let step = connector
        .submit_login(LoginInput::Code {
            code: FAKE_CODE.to_string(),
        })
        .await;
    let session = match step {
        LoginStep::Done {
            remote_user_id,
            display_name,
            session,
        } => {
            assert_eq!(remote_user_id, FAKE_USER_ID);
            assert_eq!(display_name, "Fake User");
            assert!(session.byte_len() > 0);
            session
        }
        other => panic!("expected login to finish, got {other:?}"),
    };

    let user = connector.restore(session.clone()).await.unwrap();
    assert_eq!(user.remote_user_id, FAKE_USER_ID);

    // A blob issued by one instance restores on a fresh one, which is what an app restart does.
    let reopened = FakeConnector::new();
    let restored = reopened.restore(session).await.unwrap();
    assert_eq!(restored.remote_user_id, FAKE_USER_ID);

    let garbage = reopened
        .restore(SessionBlob::new(b"garbage".to_vec()))
        .await;
    assert!(matches!(garbage, Err(ConnectorError::Unauthorized)));
}

#[tokio::test]
async fn fake_login_wrong_code_fails_the_attempt() {
    let connector = FakeConnector::new();
    connector.begin_login().await;
    let step = connector
        .submit_login(LoginInput::PhoneNumber {
            phone: "+15550100".to_string(),
        })
        .await;
    assert!(matches!(step, LoginStep::Code { .. }));

    let step = connector
        .submit_login(LoginInput::Code {
            code: "00000".to_string(),
        })
        .await;
    assert!(step.is_terminal());
    assert_eq!(
        step,
        LoginStep::Failed {
            reason: LoginFailure::InvalidInput {
                field: Some("code".to_string()),
                message: "incorrect code".to_string(),
            },
        }
    );

    // A failed step ends the attempt: the host must start over.
    let step = connector
        .submit_login(LoginInput::Code {
            code: FAKE_CODE.to_string(),
        })
        .await;
    assert!(matches!(
        &step,
        LoginStep::Failed {
            reason: LoginFailure::InvalidInput {
                field: Some(field),
                ..
            },
        } if field == "step"
    ));
}

#[tokio::test]
async fn send_emits_ack_and_echo_with_the_same_txn_id() {
    let connector = logged_in().await;
    let (sender, mut events) = tokio::sync::mpsc::channel(16);
    connector.start(sender).await.unwrap();

    assert_eq!(
        next_event(&mut events).await,
        ConnectorEvent::Connection {
            state: ConnectionState::Connecting,
        }
    );
    assert_eq!(
        next_event(&mut events).await,
        ConnectorEvent::Connection {
            state: ConnectionState::Online,
        }
    );

    let ack = connector.send(outgoing("txn-1")).await.unwrap();
    assert_eq!(ack.txn_id, "txn-1");

    match next_event(&mut events).await {
        ConnectorEvent::Message { message } => {
            assert_eq!(message.txn_id.as_deref(), Some("txn-1"));
            assert_eq!(message.remote_msg_id, ack.remote_msg_id);
            assert_eq!(message.sent_at, ack.sent_at);
            assert!(message.is_own);
            assert_eq!(message.body.as_deref(), Some("hello from the test"));
        }
        other => panic!("expected the message echo, got {other:?}"),
    }
    assert!(events.try_recv().is_err(), "no further events expected");

    // The echo is also the newest message in the chat history.
    let history = connector.backfill(FAKE_CHAT_DIRECT, None, 1).await.unwrap();
    assert_eq!(history.items.len(), 1);
    assert_eq!(history.items[0].remote_msg_id, ack.remote_msg_id);
}

#[tokio::test]
async fn backfill_pagination_covers_every_seeded_message_exactly_once() {
    let connector = logged_in().await;

    let chats = connector.list_chats(None, 10).await.unwrap();
    assert_eq!(chats.items.len(), 6);
    assert!(chats.next_cursor.is_none());
    assert!(
        chats
            .items
            .iter()
            .any(|chat| chat.remote_chat_id == FAKE_CHAT_FORUM && chat.is_forum)
    );

    for chat in &chats.items {
        let full = connector
            .backfill(&chat.remote_chat_id, None, 100)
            .await
            .unwrap();
        assert!(full.next_cursor.is_none());

        let mut paged = Vec::new();
        let mut cursor = None;
        loop {
            let page = connector
                .backfill(&chat.remote_chat_id, cursor.clone(), 2)
                .await
                .unwrap();
            assert!(page.items.len() <= 2);
            paged.extend(page.items);
            match page.next_cursor {
                Some(next) => cursor = Some(next),
                None => break,
            }
        }

        assert_eq!(paged, full.items);

        let mut ids: Vec<&str> = paged
            .iter()
            .map(|message| message.remote_msg_id.as_str())
            .collect();
        let total = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), total, "a message was paged twice");

        // Backfill is newest first.
        for pair in paged.windows(2) {
            assert!(pair[0].sent_at > pair[1].sent_at);
        }
    }

    let direct = connector
        .backfill(FAKE_CHAT_DIRECT, None, 100)
        .await
        .unwrap();
    assert_eq!(direct.items.len(), 5);
    let group = connector
        .backfill(FAKE_CHAT_GROUP, None, 100)
        .await
        .unwrap();
    assert_eq!(group.items.len(), 3);
    let channel = connector
        .backfill(FAKE_CHAT_CHANNEL, None, 100)
        .await
        .unwrap();
    assert!(channel.items.is_empty());
}

#[tokio::test]
async fn inject_incoming_emits_message_and_chat_update() {
    let connector = logged_in().await;
    let (sender, mut events) = tokio::sync::mpsc::channel(16);
    connector.start(sender).await.unwrap();
    next_event(&mut events).await;
    next_event(&mut events).await;

    let message = connector
        .inject_incoming(FAKE_CHAT_GROUP, "fake-user-charles", "ping")
        .await
        .unwrap();
    assert!(!message.is_own);
    assert_eq!(message.body.as_deref(), Some("ping"));

    match next_event(&mut events).await {
        ConnectorEvent::Message { message: event } => {
            assert_eq!(event.remote_msg_id, message.remote_msg_id);
        }
        other => panic!("expected the injected message, got {other:?}"),
    }
    match next_event(&mut events).await {
        ConnectorEvent::ChatUpserted { chat } => {
            assert_eq!(chat.remote_chat_id, FAKE_CHAT_GROUP);
            assert_eq!(chat.unread_count, 1);
            assert_eq!(chat.last_activity_at, message.sent_at);
        }
        other => panic!("expected the chat update, got {other:?}"),
    }
}

#[tokio::test]
async fn messages_carry_the_sender_profile_the_network_delivered() {
    let connector = logged_in().await;

    // History and live updates both name a sender the network sent a profile for.
    let history = connector.backfill(FAKE_CHAT_GROUP, None, 1).await.unwrap();
    let sender = history.items[0].sender.as_ref().expect("a sender profile");
    assert_eq!(sender.remote_user_id, history.items[0].sender_remote_id);
    assert_eq!(sender.display_name, "Charles Babbage");
    let live = connector
        .inject_incoming(FAKE_CHAT_GROUP, "fake-user-ada", "ping")
        .await
        .unwrap();
    assert_eq!(live.sender.unwrap().display_name, "Ada Lovelace");

    // A sender without one arrives as a bare id.
    let bare = connector
        .inject_incoming(FAKE_CHAT_GROUP, "fake-user-unknown", "ping")
        .await
        .unwrap();
    assert_eq!(bare.sender, None);
}

#[test]
fn a_message_frame_without_a_sender_profile_decodes() {
    let Frame::Event {
        event: ConnectorEvent::Message { message },
    } = decode_frame(
        r#"{"type":"event","event":{"type":"message","message":{"remote_chat_id":"chat-1","remote_msg_id":"msg-1","txn_id":null,"sender_remote_id":"user-1","is_own":false,"sent_at":1,"edited_at":null,"kind":"text","body":"hi","formatted":null,"reply_to_remote_msg_id":null,"media":null}}}"#,
    )
    .unwrap() else {
        panic!("expected a message event")
    };
    assert_eq!(message.sender, None);
}

#[tokio::test]
async fn list_members_reports_group_members_with_roles() {
    let connector = logged_in().await;
    let members = connector.list_members(FAKE_CHAT_GROUP).await.unwrap();
    let roles: Vec<(&str, Option<&str>)> = members
        .iter()
        .map(|member| (member.user.remote_user_id.as_str(), member.role.as_deref()))
        .collect();
    assert_eq!(
        roles,
        [("fake-user-charles", Some("owner")), (FAKE_USER_ID, None)]
    );

    assert!(
        connector
            .list_members(FAKE_CHAT_DIRECT)
            .await
            .unwrap()
            .is_empty()
    );
    let unknown = connector.list_members("chat-does-not-exist").await;
    assert!(matches!(unknown, Err(ConnectorError::InvalidInput { .. })));
}

#[tokio::test]
async fn pinning_a_chat_puts_it_on_top_and_reports_it() {
    let connector = logged_in().await;
    assert!(connector.capabilities().pin_chats);
    let (sender, mut events) = tokio::sync::mpsc::channel(16);
    connector.start(sender).await.unwrap();
    next_event(&mut events).await;
    next_event(&mut events).await;

    // The direct chat is seeded pinned at the top; pinning the group pushes it down.
    connector
        .set_chat_pinned(FAKE_CHAT_GROUP, true)
        .await
        .unwrap();
    match next_event(&mut events).await {
        ConnectorEvent::ChatPinned {
            remote_chat_id,
            pinned,
        } => {
            assert_eq!(remote_chat_id, FAKE_CHAT_GROUP);
            assert!(pinned);
        }
        other => panic!("expected ChatPinned, got {other:?}"),
    }
    let orders = pin_orders(&connector).await;
    assert_eq!(orders[FAKE_CHAT_GROUP], Some(0));
    assert_eq!(orders[FAKE_CHAT_DIRECT], Some(1));
    assert_eq!(orders[FAKE_CHAT_CHANNEL], None);

    // Pinning again is idempotent and silent; unpinning closes the gap.
    connector
        .set_chat_pinned(FAKE_CHAT_GROUP, true)
        .await
        .unwrap();
    connector
        .set_chat_pinned(FAKE_CHAT_GROUP, false)
        .await
        .unwrap();
    match next_event(&mut events).await {
        ConnectorEvent::ChatPinned { pinned, .. } => assert!(!pinned),
        other => panic!("expected ChatPinned, got {other:?}"),
    }
    let orders = pin_orders(&connector).await;
    assert_eq!(orders[FAKE_CHAT_GROUP], None);
    assert_eq!(orders[FAKE_CHAT_DIRECT], Some(0));

    let unknown = connector.set_chat_pinned("chat-does-not-exist", true).await;
    assert!(matches!(unknown, Err(ConnectorError::InvalidInput { .. })));
}

#[tokio::test]
async fn reordering_pinned_chats_replaces_the_list() {
    let connector = logged_in().await;
    let (sender, mut events) = tokio::sync::mpsc::channel(16);
    connector.start(sender).await.unwrap();
    next_event(&mut events).await;
    next_event(&mut events).await;

    let wanted = vec![FAKE_CHAT_CHANNEL.to_string(), FAKE_CHAT_GROUP.to_string()];
    connector.reorder_pinned_chats(&wanted).await.unwrap();
    match next_event(&mut events).await {
        ConnectorEvent::PinnedChatsReordered { remote_chat_ids } => {
            assert_eq!(remote_chat_ids, wanted);
        }
        other => panic!("expected PinnedChatsReordered, got {other:?}"),
    }
    // The chat that was pinned and is not named any more is unpinned.
    let orders = pin_orders(&connector).await;
    assert_eq!(orders[FAKE_CHAT_CHANNEL], Some(0));
    assert_eq!(orders[FAKE_CHAT_GROUP], Some(1));
    assert_eq!(orders[FAKE_CHAT_DIRECT], None);

    let too_many: Vec<String> = (0..=MAX_PINNED_CHATS)
        .map(|_| FAKE_CHAT_GROUP.to_string())
        .collect();
    let refused = connector.reorder_pinned_chats(&too_many).await;
    assert!(matches!(refused, Err(ConnectorError::InvalidInput { .. })));
    let unknown = connector
        .reorder_pinned_chats(&["chat-does-not-exist".to_string()])
        .await;
    assert!(matches!(unknown, Err(ConnectorError::InvalidInput { .. })));
}

#[tokio::test]
async fn pinning_a_message_is_reported_and_shows_in_history() {
    let connector = logged_in().await;
    assert!(connector.capabilities().pin_messages);
    let (sender, mut events) = tokio::sync::mpsc::channel(16);
    connector.start(sender).await.unwrap();
    next_event(&mut events).await;
    next_event(&mut events).await;

    let page = connector.backfill(FAKE_CHAT_GROUP, None, 1).await.unwrap();
    let target = page.items[0].remote_msg_id.clone();
    assert!(!page.items[0].pinned);

    connector
        .set_message_pinned(FAKE_CHAT_GROUP, &target, true)
        .await
        .unwrap();
    match next_event(&mut events).await {
        ConnectorEvent::MessagePinned {
            remote_chat_id,
            remote_msg_id,
            pinned,
        } => {
            assert_eq!(remote_chat_id, FAKE_CHAT_GROUP);
            assert_eq!(remote_msg_id, target);
            assert!(pinned);
        }
        other => panic!("expected MessagePinned, got {other:?}"),
    }
    let page = connector.backfill(FAKE_CHAT_GROUP, None, 1).await.unwrap();
    assert!(page.items[0].pinned);

    // Idempotent: pinning a pinned message emits nothing more.
    connector
        .set_message_pinned(FAKE_CHAT_GROUP, &target, true)
        .await
        .unwrap();
    connector
        .set_message_pinned(FAKE_CHAT_GROUP, &target, false)
        .await
        .unwrap();
    match next_event(&mut events).await {
        ConnectorEvent::MessagePinned { pinned, .. } => assert!(!pinned),
        other => panic!("expected MessagePinned, got {other:?}"),
    }
    let unknown = connector
        .set_message_pinned(FAKE_CHAT_GROUP, "msg-does-not-exist", true)
        .await;
    assert!(matches!(unknown, Err(ConnectorError::InvalidInput { .. })));
}

#[test]
fn chat_and_message_frames_without_pin_fields_decode() {
    let Frame::Event {
        event: ConnectorEvent::ChatUpserted { chat },
    } = decode_frame(
        r#"{"type":"event","event":{"type":"chat_upserted","chat":{"remote_chat_id":"chat-1","kind":"direct","title":"Ada","avatar":null,"last_activity_at":1,"unread_count":0,"is_pinned":true,"is_archived":false,"is_muted":false}}}"#,
    )
    .unwrap() else {
        panic!("expected a chat event")
    };
    assert!(chat.is_pinned);
    assert_eq!(chat.pin_order, None);
    let Frame::Event {
        event: ConnectorEvent::MessagePinned { pinned, .. },
    } = decode_frame(
        r#"{"type":"event","event":{"type":"message_pinned","remote_chat_id":"chat-1","remote_msg_id":"msg-1","pinned":true}}"#,
    )
    .unwrap() else {
        panic!("expected a message_pinned event")
    };
    assert!(pinned);
}

/// Pin order of every seeded chat by remote id, as `list_chats` reports it.
async fn pin_orders(connector: &FakeConnector) -> BTreeMap<String, Option<u32>> {
    let page = connector.list_chats(None, 10).await.unwrap();
    page.items
        .into_iter()
        .map(|chat| {
            (
                chat.remote_chat_id,
                chat.is_pinned.then_some(chat.pin_order).flatten(),
            )
        })
        .collect()
}

#[tokio::test]
async fn mark_read_records_the_marker() {
    let connector = logged_in().await;
    let history = connector.backfill(FAKE_CHAT_DIRECT, None, 1).await.unwrap();
    let newest = history.items[0].remote_msg_id.clone();

    connector
        .mark_read(FAKE_CHAT_DIRECT, &newest)
        .await
        .unwrap();
    assert_eq!(
        connector.read_marker(FAKE_CHAT_DIRECT).await.as_deref(),
        Some(newest.as_str())
    );

    let unknown = connector
        .mark_read(FAKE_CHAT_DIRECT, "msg-does-not-exist")
        .await;
    assert!(matches!(unknown, Err(ConnectorError::InvalidInput { .. })));
}

#[tokio::test]
async fn unauthenticated_calls_are_unauthorized() {
    let connector = FakeConnector::new();

    assert!(matches!(
        connector.list_chats(None, 10).await,
        Err(ConnectorError::Unauthorized)
    ));
    assert!(matches!(
        connector.backfill(FAKE_CHAT_DIRECT, None, 1).await,
        Err(ConnectorError::Unauthorized)
    ));
    assert!(matches!(
        connector.send(outgoing("txn-1")).await,
        Err(ConnectorError::Unauthorized)
    ));

    let (sender, _events) = tokio::sync::mpsc::channel(1);
    assert!(matches!(
        connector.start(sender).await,
        Err(ConnectorError::Unauthorized)
    ));
}

#[tokio::test]
async fn logout_revokes_local_credentials() {
    let connector = logged_in().await;
    connector.logout().await.unwrap();
    assert!(matches!(
        connector.list_chats(None, 10).await,
        Err(ConnectorError::Unauthorized)
    ));
}

#[test]
fn connector_errors_serialize_and_classify_retryability() {
    let rate_limited = serde_json::to_value(ConnectorError::RateLimited {
        retry_after_ms: 1500,
    })
    .unwrap();
    assert_eq!(
        rate_limited,
        serde_json::json!({"type": "rate_limited", "retry_after_ms": 1500})
    );

    let unauthorized = serde_json::to_value(ConnectorError::Unauthorized).unwrap();
    assert_eq!(unauthorized, serde_json::json!({"type": "unauthorized"}));

    assert!(ConnectorError::RateLimited { retry_after_ms: 10 }.is_retryable());
    assert!(
        ConnectorError::Network {
            message: "offline".to_string()
        }
        .is_retryable()
    );
    assert!(!ConnectorError::Unauthorized.is_retryable());
    assert!(
        !ConnectorError::NotSupported {
            feature: "polls".to_string()
        }
        .is_retryable()
    );
    assert!(
        !ConnectorError::Protocol {
            message: "bad frame".to_string()
        }
        .is_retryable()
    );
    assert!(
        !ConnectorError::InvalidInput {
            message: "bad".to_string()
        }
        .is_retryable()
    );
}

#[test]
fn connector_is_object_safe() {
    let connector: Box<dyn Connector> = Box::new(FakeConnector::new());
    assert_eq!(connector.network(), Network::Fake);
    assert!(connector.capabilities().replies);
}

#[tokio::test]
async fn limit_zero_is_invalid_input() {
    let connector = logged_in().await;
    assert!(matches!(
        connector.list_chats(None, 0).await,
        Err(ConnectorError::InvalidInput { .. })
    ));
    assert!(matches!(
        connector.backfill(FAKE_CHAT_DIRECT, None, 0).await,
        Err(ConnectorError::InvalidInput { .. })
    ));
}

#[tokio::test]
async fn react_emits_events_and_enforces_the_per_user_limit() {
    let connector = logged_in().await;
    let (sender, mut events) = tokio::sync::mpsc::channel(16);
    connector.start(sender).await.unwrap();
    next_event(&mut events).await;
    next_event(&mut events).await;

    let newest = connector
        .backfill(FAKE_CHAT_DIRECT, None, 1)
        .await
        .unwrap()
        .items[0]
        .remote_msg_id
        .clone();

    connector
        .react(FAKE_CHAT_DIRECT, &newest, "👍")
        .await
        .unwrap();
    match next_event(&mut events).await {
        ConnectorEvent::Reaction { emoji, added, .. } => {
            assert_eq!(emoji, "👍");
            assert!(added);
        }
        other => panic!("expected a reaction, got {other:?}"),
    }

    // Re-adding the same reaction is idempotent and does not emit.
    connector
        .react(FAKE_CHAT_DIRECT, &newest, "👍")
        .await
        .unwrap();
    assert!(events.try_recv().is_err());

    connector
        .react(FAKE_CHAT_DIRECT, &newest, "🔥")
        .await
        .unwrap();
    next_event(&mut events).await;
    connector
        .react(FAKE_CHAT_DIRECT, &newest, "🎉")
        .await
        .unwrap();
    next_event(&mut events).await;

    let over_limit = connector.react(FAKE_CHAT_DIRECT, &newest, "🚀").await;
    assert!(matches!(
        over_limit,
        Err(ConnectorError::InvalidInput { .. })
    ));

    connector
        .unreact(FAKE_CHAT_DIRECT, &newest, "👍")
        .await
        .unwrap();
    match next_event(&mut events).await {
        ConnectorEvent::Reaction { added, .. } => assert!(!added),
        other => panic!("expected a reaction removal, got {other:?}"),
    }
}

#[tokio::test]
async fn fetch_media_writes_the_requested_file() {
    let connector = logged_in().await;
    let media = MediaRef {
        remote_ref: "remote-ref".to_string(),
        mime: "image/png".to_string(),
        size_bytes: None,
        width: None,
        height: None,
        duration_ms: None,
        file_name: None,
    };
    let path = std::env::temp_dir().join(format!(
        "alloy-connector-fetch-media-{}.bin",
        std::process::id()
    ));

    let written = connector.fetch_media(&media, &path).await.unwrap();
    let contents = std::fs::read_to_string(&path).unwrap();
    assert_eq!(contents, "fake-media:remote-ref:image/png");
    assert_eq!(written, contents.len() as u64);

    std::fs::remove_file(&path).unwrap();
}

#[tokio::test]
async fn list_topics_returns_each_topic_of_a_forum_newest_first() {
    let connector = logged_in().await;
    assert!(connector.capabilities().topics);

    let topics = connector.list_topics(FAKE_CHAT_FORUM).await.unwrap();
    let ids: Vec<&str> = topics
        .iter()
        .map(|chat| chat.remote_chat_id.as_str())
        .collect();
    assert_eq!(ids, [FAKE_TOPIC_RELEASE_NOTES, FAKE_TOPIC_GENERAL]);
    for topic in &topics {
        assert_eq!(topic.kind, ChatKind::Group);
        assert!(!topic.is_forum);
        assert_eq!(
            topic.parent_remote_chat_id.as_deref(),
            Some(FAKE_CHAT_FORUM)
        );
    }
    assert_eq!(topics[0].title, "Release notes");
    assert_eq!(topics[1].title, "General");
}

#[tokio::test]
async fn list_topics_on_a_non_forum_is_invalid_input() {
    let connector = logged_in().await;
    assert!(matches!(
        connector.list_topics(FAKE_CHAT_GROUP).await,
        Err(ConnectorError::InvalidInput { .. })
    ));
    assert!(matches!(
        connector.list_topics(FAKE_TOPIC_GENERAL).await,
        Err(ConnectorError::InvalidInput { .. })
    ));
    assert!(matches!(
        connector.list_topics("chat-does-not-exist").await,
        Err(ConnectorError::InvalidInput { .. })
    ));
}

#[tokio::test]
async fn backfill_on_a_topic_returns_only_that_topics_messages() {
    let connector = logged_in().await;

    let general = connector
        .backfill(FAKE_TOPIC_GENERAL, None, 100)
        .await
        .unwrap();
    assert_eq!(general.items.len(), 3);
    assert!(
        general
            .items
            .iter()
            .all(|message| message.remote_chat_id == FAKE_TOPIC_GENERAL)
    );

    let release = connector
        .backfill(FAKE_TOPIC_RELEASE_NOTES, None, 100)
        .await
        .unwrap();
    assert_eq!(release.items.len(), 2);
    assert!(
        release
            .items
            .iter()
            .all(|message| message.remote_chat_id == FAKE_TOPIC_RELEASE_NOTES)
    );

    let forum = connector
        .backfill(FAKE_CHAT_FORUM, None, 100)
        .await
        .unwrap();
    assert!(forum.items.is_empty());
}

#[tokio::test]
async fn send_to_a_topic_echoes_the_topic_id() {
    let connector = logged_in().await;
    let (sender, mut events) = tokio::sync::mpsc::channel(16);
    connector.start(sender).await.unwrap();
    next_event(&mut events).await;
    next_event(&mut events).await;

    let message = OutgoingMessage {
        remote_chat_id: FAKE_TOPIC_GENERAL.to_string(),
        ..outgoing("txn-topic")
    };
    let ack = connector.send(message).await.unwrap();
    assert_eq!(ack.txn_id, "txn-topic");

    match next_event(&mut events).await {
        ConnectorEvent::Message { message } => {
            assert_eq!(message.remote_chat_id, FAKE_TOPIC_GENERAL);
            assert_eq!(message.remote_msg_id, ack.remote_msg_id);
            assert_eq!(message.txn_id.as_deref(), Some("txn-topic"));
        }
        other => panic!("expected the topic message echo, got {other:?}"),
    }

    let history = connector
        .backfill(FAKE_TOPIC_GENERAL, None, 1)
        .await
        .unwrap();
    assert_eq!(history.items[0].remote_msg_id, ack.remote_msg_id);
}
