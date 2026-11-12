//! Behavioural tests for the engine-facing store surface added in schema v2 (issue #129).
//!
//! Every test drives a real SQLite database: the send pipeline, the media cache, the participant
//! and read-marker projections, the media browser and the safe search query.

use alloy_store::{
    AccountState, ChatKind, ChatUpsert, EventRecord, MessageKind, MessageRecord, Network,
    NewAccount, NewMedia, NewMessage, NewOutboxItem, OutboxOp, OutboxState, RoomMediaQuery,
    SendState, Store, StoreError,
};
use serde_json::json;

const KEY: [u8; 32] = [4; 32];

async fn open() -> Store {
    Store::open_in_memory(KEY).expect("open in-memory store")
}

async fn account_and_chat(store: &Store) -> (String, String) {
    let account = store
        .insert_account(NewAccount {
            account_id: None,
            network: Network::Telegram,
            remote_user_id: Some("remote-user-1".to_owned()),
            display_name: Some("Tester".to_owned()),
            state: AccountState::Active,
        })
        .await
        .expect("insert account");
    let chat_id = store
        .upsert_chat(ChatUpsert {
            account_id: account.account_id.clone(),
            remote_chat_id: "remote-chat-1".to_owned(),
            kind: ChatKind::Direct,
            title: Some("Chat".to_owned()),
            avatar_media_id: None,
            last_message_id: None,
            last_activity_at: Some(1_000),
        })
        .await
        .expect("upsert chat");
    (account.account_id, chat_id)
}

fn message(
    chat_id: &str,
    message_id: &str,
    sent_at: i64,
    kind: MessageKind,
    body: Option<&str>,
) -> NewMessage {
    NewMessage {
        chat_id: chat_id.to_owned(),
        message_id: Some(message_id.to_owned()),
        remote_msg_id: Some(format!("remote-{message_id}")),
        txn_id: None,
        sender_remote_id: "u1".to_owned(),
        is_own: false,
        sent_at,
        edited_at: None,
        kind,
        body: body.map(str::to_owned),
        formatted: None,
        reply_to_message_id: None,
        media_id: None,
        send_state: None,
        extra: None,
    }
}

fn ids(messages: Vec<MessageRecord>) -> Vec<String> {
    messages
        .into_iter()
        .map(|message| message.message_id)
        .collect()
}

async fn events_of_kind(store: &Store, kind: &str) -> Vec<EventRecord> {
    store
        .events_since(0, 1_000)
        .await
        .expect("read events")
        .into_iter()
        .filter(|event| event.kind == kind)
        .collect()
}

#[tokio::test]
async fn media_dedupes_per_account_and_clears_cached_paths() {
    let store = open().await;
    let (account_id, _chat_id) = account_and_chat(&store).await;

    let first = store
        .insert_media(NewMedia {
            account_id: account_id.clone(),
            remote_ref: "tg:file-1".to_owned(),
            mime: Some("image/jpeg".to_owned()),
            size_bytes: None,
            width: Some(800),
            height: Some(600),
            duration_ms: None,
            file_name: None,
        })
        .await
        .expect("insert first media");
    let same = store
        .insert_media(NewMedia {
            account_id: account_id.clone(),
            remote_ref: "tg:file-1".to_owned(),
            mime: Some("image/png".to_owned()),
            size_bytes: Some(4_096),
            width: None,
            height: None,
            duration_ms: None,
            file_name: Some("cat.png".to_owned()),
        })
        .await
        .expect("re-insert the same network file");
    assert_eq!(first, same, "the network file keeps one media id");

    let record = store
        .get_media(&first)
        .await
        .expect("read media")
        .expect("media row exists");
    assert_eq!(record.mime.as_deref(), Some("image/png"));
    assert_eq!(record.size_bytes, Some(4_096));
    assert_eq!(record.file_name.as_deref(), Some("cat.png"));
    assert_eq!(record.width, Some(800), "unknown fields keep the old value");
    assert_eq!(record.height, Some(600));

    let other = store
        .insert_media(NewMedia {
            account_id: account_id.clone(),
            remote_ref: "tg:file-2".to_owned(),
            mime: None,
            size_bytes: None,
            width: None,
            height: None,
            duration_ms: None,
            file_name: None,
        })
        .await
        .expect("insert second media");
    assert_ne!(other, first);

    store
        .set_media_local_path(
            &first,
            Some("/cache/cat.png".to_owned()),
            Some("sha256-value".to_owned()),
            Some(9_000),
        )
        .await
        .expect("record the downloaded file");
    let cached = store.get_media(&first).await.expect("read").expect("row");
    assert_eq!(cached.local_path.as_deref(), Some("/cache/cat.png"));
    assert_eq!(cached.fetched_at, Some(9_000));

    let cleared = store
        .clear_media_local_paths(Some(account_id.as_str()))
        .await
        .expect("clear paths");
    assert_eq!(cleared, vec!["/cache/cat.png".to_owned()]);
    let cleared_row = store.get_media(&first).await.expect("read").expect("row");
    assert!(cleared_row.local_path.is_none());
    assert!(cleared_row.sha256.is_none());
    assert!(cleared_row.fetched_at.is_none());
    assert!(
        store
            .clear_media_local_paths(None)
            .await
            .expect("second clear")
            .is_empty()
    );
    assert!(matches!(
        store
            .set_media_local_path("missing", None, None, None)
            .await,
        Err(StoreError::NotFound("media"))
    ));
}

#[tokio::test]
async fn participants_upsert_partial_updates_and_list_stably() {
    let store = open().await;
    let (_account_id, chat_id) = account_and_chat(&store).await;

    let created = store
        .upsert_participant(
            &chat_id,
            "u1",
            Some("Ana".to_owned()),
            None,
            Some("admin".to_owned()),
        )
        .await
        .expect("insert participant");
    assert_eq!(created.display_name.as_deref(), Some("Ana"));

    let updated = store
        .upsert_participant(
            &chat_id,
            "u1",
            None,
            Some("avatar-media".to_owned()),
            Some("owner".to_owned()),
        )
        .await
        .expect("update participant");
    assert_eq!(
        updated.display_name.as_deref(),
        Some("Ana"),
        "a partial update keeps the known name"
    );
    assert_eq!(updated.avatar_media_id.as_deref(), Some("avatar-media"));
    assert_eq!(updated.role.as_deref(), Some("owner"));

    store
        .upsert_participant(&chat_id, "u2", Some("Bob".to_owned()), None, None)
        .await
        .expect("insert second participant");

    let listed = store
        .list_participants(&chat_id)
        .await
        .expect("list participants");
    assert_eq!(
        listed
            .iter()
            .map(|participant| participant.remote_user_id.as_str())
            .collect::<Vec<_>>(),
        vec!["u1", "u2"]
    );
    assert_eq!(listed.len(), 2);

    let found = store
        .get_participant(&chat_id, "u2")
        .await
        .expect("get participant")
        .expect("participant exists");
    assert_eq!(found.display_name.as_deref(), Some("Bob"));
    assert!(
        store
            .get_participant(&chat_id, "u9")
            .await
            .expect("missing participant")
            .is_none()
    );
    assert!(
        store
            .list_participants("no-such-chat")
            .await
            .expect("empty list")
            .is_empty()
    );
    assert!(matches!(
        store
            .upsert_participant("no-such-chat", "u1", None, None, None)
            .await,
        Err(StoreError::NotFound("chat"))
    ));
}

#[tokio::test]
async fn read_markers_only_move_forward() {
    let store = open().await;
    let (_account_id, chat_id) = account_and_chat(&store).await;

    let first = store
        .set_read_marker(&chat_id, "u1", "m2", 200)
        .await
        .expect("set marker");
    assert_eq!(first.up_to_message_id, "m2");

    let stale = store
        .set_read_marker(&chat_id, "u1", "m1", 100)
        .await
        .expect("stale marker");
    assert_eq!(stale.up_to_message_id, "m2");
    assert_eq!(stale.read_at, 200);

    store
        .set_read_marker(&chat_id, "u2", "m1", 150)
        .await
        .expect("second participant");

    let markers = store
        .list_read_markers(&chat_id)
        .await
        .expect("list markers");
    assert_eq!(markers.len(), 2, "one marker per participant");
    assert_eq!(markers[0].remote_user_id, "u1");
    assert_eq!(markers[1].remote_user_id, "u2");

    assert_eq!(
        events_of_kind(&store, "chat.read_marker").await.len(),
        2,
        "the ignored stale update emits no event"
    );
}

#[tokio::test]
async fn room_media_filters_by_kind_pages_and_skips_deleted_messages() {
    let store = open().await;
    let (_account_id, chat_id) = account_and_chat(&store).await;

    for (id, sent_at, kind, body) in [
        ("i1", 100, MessageKind::Image, None),
        ("v1", 200, MessageKind::Video, None),
        ("f1", 300, MessageKind::File, Some("report.pdf")),
        ("a1", 400, MessageKind::Audio, None),
        ("voice1", 450, MessageKind::Voice, None),
        (
            "link1",
            500,
            MessageKind::Text,
            Some("see https://example.com/x"),
        ),
        ("text1", 550, MessageKind::Text, Some("nothing to see")),
        ("gone", 600, MessageKind::Image, None),
    ] {
        store
            .upsert_message(message(&chat_id, id, sent_at, kind, body))
            .await
            .expect("seed message");
    }
    store
        .mark_deleted("gone", 700)
        .await
        .expect("tombstone deleted image");

    let media = store
        .room_media(&chat_id, RoomMediaQuery::Media, None, 10)
        .await
        .expect("media listing");
    assert_eq!(ids(media), vec!["v1", "i1"]);

    let files = store
        .room_media(&chat_id, RoomMediaQuery::Files, None, 10)
        .await
        .expect("files listing");
    assert_eq!(ids(files), vec!["voice1", "a1", "f1"]);

    let links = store
        .room_media(&chat_id, RoomMediaQuery::Links, None, 10)
        .await
        .expect("links listing");
    assert_eq!(ids(links), vec!["link1"]);

    let first_page = store
        .room_media(&chat_id, RoomMediaQuery::Media, None, 1)
        .await
        .expect("first page");
    assert_eq!(ids(first_page.clone()), vec!["v1"]);
    let cursor = first_page
        .first()
        .map(|m| (m.sent_at, m.message_id.clone()));
    let second_page = store
        .room_media(&chat_id, RoomMediaQuery::Media, cursor, 1)
        .await
        .expect("second page");
    assert_eq!(ids(second_page), vec!["i1"]);
}

#[tokio::test]
async fn search_treats_fts_syntax_characters_literally() {
    let store = open().await;
    let (account_id, chat_id) = account_and_chat(&store).await;
    let other_chat = store
        .upsert_chat(ChatUpsert {
            account_id: account_id.clone(),
            remote_chat_id: "remote-chat-2".to_owned(),
            kind: ChatKind::Direct,
            title: Some("Other".to_owned()),
            avatar_media_id: None,
            last_message_id: None,
            last_activity_at: Some(2_000),
        })
        .await
        .expect("second chat");

    let stored = store
        .upsert_message(message(
            &chat_id,
            "noise",
            1_000,
            MessageKind::Text,
            Some("he said \"hi\" (twice) - ok *"),
        ))
        .await
        .expect("seed message");
    store
        .upsert_message(message(
            &other_chat,
            "elsewhere",
            1_100,
            MessageKind::Text,
            Some("banana bread"),
        ))
        .await
        .expect("other chat message");

    for query in [
        "\"hi\"",
        "(twice)",
        "\"hi\" (twice) - ok",
        "\"",
        "*",
        "(",
        "-",
        "- ok",
        "NEAR(",
    ] {
        store
            .search_messages_in_chat(&chat_id, query, 10)
            .await
            .unwrap_or_else(|err| panic!("query {query:?} must not fail: {err}"));
        store
            .search_messages(query, None, 10)
            .await
            .unwrap_or_else(|err| panic!("account query {query:?} must not fail: {err}"));
    }

    let quoted = store
        .search_messages_in_chat(&chat_id, "\"hi\"", 10)
        .await
        .expect("quoted literal");
    assert_eq!(ids(quoted), vec![stored.message_id.clone()]);

    let phrase = store
        .search_messages_in_chat(&chat_id, "\"hi\" (twice) - ok", 10)
        .await
        .expect("mixed punctuation");
    assert_eq!(ids(phrase), vec![stored.message_id.clone()]);

    let punctuation_only = store
        .search_messages_in_chat(&chat_id, "(", 10)
        .await
        .expect("bracket only");
    assert!(punctuation_only.is_empty());

    assert!(
        store
            .search_messages_in_chat(&chat_id, "banana", 10)
            .await
            .expect("chat scoped search")
            .is_empty(),
        "another chat's body must not leak into the result"
    );
    assert_eq!(
        store
            .search_messages("banana", Some(account_id), 10)
            .await
            .expect("account search")
            .len(),
        1
    );
    assert!(
        store
            .search_messages_in_chat(&chat_id, "   ", 10)
            .await
            .expect("blank query")
            .is_empty()
    );
}

#[tokio::test]
async fn reset_in_flight_outbox_recovers_claimed_rows() {
    let store = open().await;
    let (account_id, chat_id) = account_and_chat(&store).await;

    for (txn_id, created_at) in [("txn-a", 100), ("txn-b", 200)] {
        store
            .enqueue_outbox(NewOutboxItem {
                txn_id: Some(txn_id.to_owned()),
                account_id: account_id.clone(),
                chat_id: chat_id.clone(),
                op: OutboxOp::Send,
                payload: json!({ "text": txn_id }),
                next_attempt_at: 0,
                created_at,
            })
            .await
            .expect("enqueue");
    }

    let claimed = store.claim_due_outbox(1_000, 10).await.expect("claim");
    assert_eq!(claimed.len(), 2);
    assert!(
        claimed
            .iter()
            .all(|item| item.state == OutboxState::InFlight)
    );

    assert_eq!(
        store
            .reset_in_flight_outbox()
            .await
            .expect("crash recovery"),
        2
    );
    let recovered = store
        .pending_outbox_for_chat(&chat_id)
        .await
        .expect("pending queue");
    assert_eq!(
        recovered
            .iter()
            .map(|item| item.txn_id.as_str())
            .collect::<Vec<_>>(),
        vec!["txn-a", "txn-b"],
        "recovered rows keep their order and retry"
    );
    assert!(
        recovered
            .iter()
            .all(|item| item.state == OutboxState::Pending)
    );

    store
        .complete_outbox("txn-a")
        .await
        .expect("complete first");
    let unfinished = store
        .pending_outbox_for_chat(&chat_id)
        .await
        .expect("queue after completion");
    assert_eq!(
        unfinished
            .iter()
            .map(|item| item.txn_id.as_str())
            .collect::<Vec<_>>(),
        vec!["txn-b"],
        "completed rows leave the pending queue"
    );

    store.delete_outbox("txn-b").await.expect("cancel send");
    assert!(
        store
            .pending_outbox_for_chat(&chat_id)
            .await
            .expect("queue after cancel")
            .is_empty()
    );
    store
        .delete_outbox("txn-b")
        .await
        .expect("cancelling an absent row is a no-op");
    assert_eq!(store.reset_in_flight_outbox().await.expect("idle reset"), 0);
}

#[tokio::test]
async fn set_send_state_reconciles_the_outgoing_message() {
    let store = open().await;
    let (_account_id, chat_id) = account_and_chat(&store).await;

    let mut local = message(
        &chat_id,
        "local-1",
        1_000,
        MessageKind::Text,
        Some("outgoing"),
    );
    local.txn_id = Some("txn-1".to_owned());
    local.sender_remote_id = "me".to_owned();
    local.is_own = true;
    local.send_state = Some(SendState::Queued);
    let stored = store.upsert_message(local).await.expect("queued message");

    store
        .set_send_state(
            &stored.message_id,
            SendState::Sent,
            Some("remote-9".to_owned()),
            Some(1_500),
        )
        .await
        .expect("send ack");

    let reconciled = store
        .get_message(&stored.message_id)
        .await
        .expect("read message")
        .expect("message exists");
    assert_eq!(reconciled.send_state, Some(SendState::Sent));
    assert_eq!(reconciled.remote_msg_id.as_deref(), Some("remote-9"));
    assert_eq!(reconciled.sent_at, 1_500, "the network timestamp wins");
    assert_eq!(reconciled.txn_id.as_deref(), Some("txn-1"));

    let by_remote = store
        .find_message_by_remote(&chat_id, "remote-9")
        .await
        .expect("lookup by remote id")
        .expect("message exists");
    assert_eq!(by_remote.message_id, stored.message_id);
    let by_txn = store
        .find_message_by_txn("txn-1")
        .await
        .expect("lookup by txn id")
        .expect("message exists");
    assert_eq!(by_txn.message_id, stored.message_id);
    assert!(
        store
            .find_message_by_remote(&chat_id, "remote-missing")
            .await
            .expect("missing remote id")
            .is_none()
    );

    store
        .set_send_state(&stored.message_id, SendState::Failed, None, None)
        .await
        .expect("fail");
    let failed = store
        .get_message(&stored.message_id)
        .await
        .expect("read failed")
        .expect("message exists");
    assert_eq!(failed.send_state, Some(SendState::Failed));
    assert_eq!(
        failed.remote_msg_id.as_deref(),
        Some("remote-9"),
        "absent values keep the stored remote id"
    );
    assert_eq!(events_of_kind(&store, "message.updated").await.len(), 2);

    assert!(matches!(
        store
            .set_send_state("missing", SendState::Sent, None, None)
            .await,
        Err(StoreError::NotFound("message"))
    ));
}

#[tokio::test]
async fn delete_local_message_removes_the_row_and_its_reactions() {
    let store = open().await;
    let (_account_id, chat_id) = account_and_chat(&store).await;

    let stored = store
        .upsert_message(message(
            &chat_id,
            "cancelled",
            1_000,
            MessageKind::Text,
            Some("cancel me"),
        ))
        .await
        .expect("queued message");
    store
        .set_reaction(&stored.message_id, "u1", "ok", 1_100)
        .await
        .expect("reaction");
    assert_eq!(
        store
            .list_reactions(std::slice::from_ref(&stored.message_id))
            .await
            .expect("reactions")
            .len(),
        1
    );

    store
        .delete_local_message(&stored.message_id)
        .await
        .expect("hard delete");

    assert!(
        store
            .get_message(&stored.message_id)
            .await
            .expect("read deleted")
            .is_none()
    );
    assert!(
        store
            .list_reactions(std::slice::from_ref(&stored.message_id))
            .await
            .expect("reactions after delete")
            .is_empty(),
        "the foreign key cascades the reactions away"
    );
    assert!(
        store
            .search_messages("cancel", None, 10)
            .await
            .expect("search after delete")
            .is_empty(),
        "the full-text index forgets the deleted body"
    );
    let last = store
        .events_since(0, 1_000)
        .await
        .expect("events")
        .pop()
        .expect("at least one event");
    assert_eq!(last.kind, "message.removed");
    assert_eq!(last.payload["message_id"], stored.message_id);

    assert!(matches!(
        store.delete_local_message(&stored.message_id).await,
        Err(StoreError::NotFound("message"))
    ));
}

#[tokio::test]
async fn editing_the_body_reindexes_search() {
    let store = open().await;
    let (_account_id, chat_id) = account_and_chat(&store).await;

    let stored = store
        .upsert_message(message(
            &chat_id,
            "edited",
            1_000,
            MessageKind::Text,
            Some("first wording"),
        ))
        .await
        .expect("message");

    store
        .edit_message_body(
            &stored.message_id,
            Some("second wording".to_owned()),
            Some("<b>second</b>".to_owned()),
            2_000,
        )
        .await
        .expect("edit");

    let edited = store
        .get_message(&stored.message_id)
        .await
        .expect("read")
        .expect("message exists");
    assert_eq!(edited.body.as_deref(), Some("second wording"));
    assert_eq!(edited.formatted.as_deref(), Some("<b>second</b>"));
    assert_eq!(edited.edited_at, Some(2_000));
    assert!(
        store
            .search_messages("first", None, 10)
            .await
            .expect("old wording")
            .is_empty()
    );
    assert_eq!(
        store
            .search_messages("second", None, 10)
            .await
            .expect("new wording")
            .len(),
        1
    );
    assert!(matches!(
        store
            .edit_message_body("missing", Some("x".to_owned()), None, 2_000)
            .await,
        Err(StoreError::NotFound("message"))
    ));
}

#[tokio::test]
async fn chat_metadata_and_unread_counters_follow_the_network() {
    let store = open().await;
    let (account_id, chat_id) = account_and_chat(&store).await;

    let found = store
        .find_chat(&account_id, "remote-chat-1")
        .await
        .expect("find chat")
        .expect("chat exists");
    assert_eq!(found.chat_id, chat_id);
    assert!(
        store
            .find_chat(&account_id, "remote-unknown")
            .await
            .expect("missing chat")
            .is_none()
    );

    let renamed = store
        .update_chat_meta(
            &chat_id,
            Some("Renamed".to_owned()),
            None,
            None,
            Some(5_000),
        )
        .await
        .expect("rename");
    assert_eq!(renamed.title.as_deref(), Some("Renamed"));
    assert_eq!(renamed.last_activity_at, Some(5_000));

    let older = store
        .update_chat_meta(&chat_id, None, None, None, Some(4_000))
        .await
        .expect("older activity");
    assert_eq!(
        older.title.as_deref(),
        Some("Renamed"),
        "None keeps the title"
    );
    assert_eq!(
        older.last_activity_at,
        Some(5_000),
        "older activity never rewinds the chat"
    );

    let marked = store.set_unread(&chat_id, 3, 1).await.expect("set unread");
    assert_eq!((marked.unread_count, marked.mention_count), (3, 1));
    let bumped = store
        .increment_unread(&chat_id, true)
        .await
        .expect("mention");
    assert_eq!((bumped.unread_count, bumped.mention_count), (4, 2));
    let plain = store
        .increment_unread(&chat_id, false)
        .await
        .expect("plain message");
    assert_eq!((plain.unread_count, plain.mention_count), (5, 2));

    let events = store.events_since(0, 1_000).await.expect("events");
    let last_seq = events.last().expect("event").seq;
    assert_eq!(
        store.latest_event_seq().await.expect("latest seq"),
        last_seq
    );
    assert!(
        events.iter().any(|event| event.kind == "chat.updated"),
        "counter changes are visible on the change feed"
    );

    assert!(matches!(
        store
            .update_chat_meta("missing", None, None, None, None)
            .await,
        Err(StoreError::NotFound("chat"))
    ));
    assert!(matches!(
        store.set_unread("missing", 0, 0).await,
        Err(StoreError::NotFound("chat"))
    ));
    assert!(matches!(
        store.increment_unread("missing", false).await,
        Err(StoreError::NotFound("chat"))
    ));
}
