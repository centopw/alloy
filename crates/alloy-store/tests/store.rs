//! Behavioural tests for the M0 store contract (issue #127).
//!
//! Every test runs against a real SQLite database: either in memory or a `tempfile` file when the
//! test needs to reopen or tamper with the file between opens.

use alloy_store::{
    AccountState, CURRENT_SCHEMA_VERSION, ChatFlagsUpdate, ChatKind, ChatUpsert, MessageKind,
    Network, NewAccount, NewMessage, NewOutboxItem, OutboxOp, OutboxState, SendState, Store,
    StoreError,
};
use serde_json::json;
use tempfile::TempDir;

const KEY: [u8; 32] = [9; 32];

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

fn text_message(chat_id: &str, sent_at: i64) -> NewMessage {
    NewMessage {
        chat_id: chat_id.to_owned(),
        message_id: None,
        remote_msg_id: None,
        txn_id: None,
        sender_remote_id: "remote-user-1".to_owned(),
        is_own: false,
        sent_at,
        edited_at: None,
        kind: MessageKind::Text,
        body: Some("hello".to_owned()),
        formatted: None,
        reply_to_message_id: None,
        media_id: None,
        send_state: None,
        extra: None,
    }
}

#[tokio::test]
async fn migrate_reaches_current_version_and_is_idempotent_on_reopen() {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("alloy.db");

    let store = Store::open(&path, KEY).expect("first open");
    assert_eq!(
        store.schema_version().await.expect("schema version"),
        CURRENT_SCHEMA_VERSION
    );
    account_and_chat(&store).await;
    drop(store);

    let store = Store::open(&path, KEY).expect("reopen");
    assert_eq!(
        store.schema_version().await.expect("schema version"),
        CURRENT_SCHEMA_VERSION
    );
}

#[tokio::test]
async fn tampered_migration_checksum_is_rejected() {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("alloy.db");
    drop(Store::open(&path, KEY).expect("create"));

    let conn = rusqlite::Connection::open(&path).expect("raw open");
    conn.execute(
        "UPDATE schema_migrations SET checksum = 'tampered' WHERE version = 1",
        [],
    )
    .expect("tamper");
    drop(conn);

    match Store::open(&path, KEY) {
        Err(StoreError::MigrationChecksumMismatch { version }) => assert_eq!(version, 1),
        Err(other) => panic!("unexpected error: {other:?}"),
        Ok(_) => panic!("expected the checksum mismatch to be reported"),
    }
}

#[tokio::test]
async fn newer_database_version_is_rejected() {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("alloy.db");
    drop(Store::open(&path, KEY).expect("create"));

    let conn = rusqlite::Connection::open(&path).expect("raw open");
    conn.execute(
        "INSERT INTO schema_migrations (version, name, checksum, applied_at)
         VALUES (999, 'future', 'unknown', 0)",
        [],
    )
    .expect("insert future migration");
    drop(conn);

    match Store::open(&path, KEY) {
        Err(StoreError::SchemaTooNew { db, binary }) => {
            assert_eq!(db, 999);
            assert_eq!(binary, CURRENT_SCHEMA_VERSION);
        }
        Err(other) => panic!("unexpected error: {other:?}"),
        Ok(_) => panic!("expected SchemaTooNew"),
    }
}

#[tokio::test]
async fn deleting_an_account_cascades_everything_it_owns() {
    let store = Store::open_in_memory(KEY).expect("open");
    let (account_id, chat_id) = account_and_chat(&store).await;
    let message = store
        .upsert_message(text_message(&chat_id, 1_000))
        .await
        .expect("message");
    store
        .put_secret(&account_id, b"session")
        .await
        .expect("secret");
    store
        .enqueue_outbox(NewOutboxItem {
            txn_id: Some("txn-cascade".to_owned()),
            account_id: account_id.clone(),
            chat_id: chat_id.clone(),
            op: OutboxOp::Send,
            payload: json!({ "text": "queued" }),
            next_attempt_at: 1_000,
            created_at: 1_000,
        })
        .await
        .expect("outbox");
    assert!(!store.events_since(0, 100).await.expect("events").is_empty());

    store
        .delete_account(&account_id)
        .await
        .expect("delete account");

    assert!(
        store
            .get_account(&account_id)
            .await
            .expect("account")
            .is_none()
    );
    assert!(store.get_chat(&chat_id).await.expect("chat").is_none());
    assert!(
        store
            .page_messages(&chat_id, None, 10)
            .await
            .expect("messages")
            .is_empty()
    );
    assert!(
        store
            .get_secret(&account_id)
            .await
            .expect("secret")
            .is_none()
    );
    assert!(
        store
            .claim_due_outbox(i64::MAX, 10)
            .await
            .expect("outbox")
            .is_empty()
    );
    assert!(store.events_since(0, 100).await.expect("events").is_empty());
    assert_eq!(message.message_id.len(), 36);
}

#[tokio::test]
async fn secrets_round_trip_and_wrong_key_is_a_decrypt_error() {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("alloy.db");

    let store = Store::open(&path, KEY).expect("open");
    let (account_id, _chat_id) = account_and_chat(&store).await;
    store
        .put_secret(&account_id, b"opaque-connector-session")
        .await
        .expect("put secret");
    assert_eq!(
        store
            .get_secret(&account_id)
            .await
            .expect("get secret")
            .as_deref(),
        Some(&b"opaque-connector-session"[..])
    );
    drop(store);

    let store = Store::open(&path, [1; 32]).expect("open with wrong key");
    match store.get_secret(&account_id).await {
        Err(StoreError::Decrypt {
            account_id: reported,
        }) => assert_eq!(reported, account_id),
        Err(other) => panic!("unexpected error: {other:?}"),
        Ok(_) => panic!("expected a decrypt error"),
    }
}

#[tokio::test]
async fn remote_echo_reconciles_the_queued_message_by_txn_id() {
    let store = Store::open_in_memory(KEY).expect("open");
    let (_account_id, chat_id) = account_and_chat(&store).await;

    let mut local = text_message(&chat_id, 1_000);
    local.txn_id = Some("txn-1".to_owned());
    local.sender_remote_id = "me".to_owned();
    local.is_own = true;
    local.body = Some("outgoing".to_owned());
    local.send_state = Some(SendState::Queued);
    let queued = store.upsert_message(local).await.expect("queued message");

    let mut echo = text_message(&chat_id, 1_050);
    echo.txn_id = Some("txn-1".to_owned());
    echo.remote_msg_id = Some("remote-1".to_owned());
    echo.sender_remote_id = "me".to_owned();
    echo.is_own = true;
    echo.body = Some("outgoing".to_owned());
    echo.send_state = Some(SendState::Sent);
    let reconciled = store.upsert_message(echo).await.expect("echo");

    assert_eq!(queued.message_id, reconciled.message_id);
    assert_eq!(reconciled.remote_msg_id.as_deref(), Some("remote-1"));
    assert_eq!(reconciled.send_state, Some(SendState::Sent));
    assert_eq!(reconciled.sent_at, 1_050);

    let page = store.page_messages(&chat_id, None, 50).await.expect("page");
    assert_eq!(page.len(), 1, "echo must not duplicate the queued row");
}

#[tokio::test]
async fn keyset_pagination_orders_newest_first_without_gaps() {
    let store = Store::open_in_memory(KEY).expect("open");
    let (_account_id, chat_id) = account_and_chat(&store).await;

    for (id, sent_at) in [
        ("m1", 1_000),
        ("m2", 2_000),
        ("m3", 2_000),
        ("m4", 4_000),
        ("m5", 5_000),
    ] {
        let mut message = text_message(&chat_id, sent_at);
        message.message_id = Some(id.to_owned());
        store.upsert_message(message).await.expect("seed message");
    }

    let first = store
        .page_messages(&chat_id, None, 2)
        .await
        .expect("page 1");
    assert_eq!(
        first
            .iter()
            .map(|m| m.message_id.as_str())
            .collect::<Vec<_>>(),
        vec!["m5", "m4"]
    );

    let cursor = first.last().map(|m| (m.sent_at, m.message_id.clone()));
    let second = store
        .page_messages(&chat_id, cursor, 2)
        .await
        .expect("page 2");
    assert_eq!(
        second
            .iter()
            .map(|m| m.message_id.as_str())
            .collect::<Vec<_>>(),
        vec!["m3", "m2"]
    );

    let cursor = second.last().map(|m| (m.sent_at, m.message_id.clone()));
    let third = store
        .page_messages(&chat_id, cursor, 2)
        .await
        .expect("page 3");
    assert_eq!(
        third
            .iter()
            .map(|m| m.message_id.as_str())
            .collect::<Vec<_>>(),
        vec!["m1"]
    );

    let cursor = third.last().map(|m| (m.sent_at, m.message_id.clone()));
    assert!(
        store
            .page_messages(&chat_id, cursor, 2)
            .await
            .expect("page 4")
            .is_empty()
    );
}

#[tokio::test]
async fn outbox_claims_each_row_once_and_retries_increment_attempts() {
    let store = Store::open_in_memory(KEY).expect("open");
    let (account_id, chat_id) = account_and_chat(&store).await;

    let first_txn = store
        .enqueue_outbox(NewOutboxItem {
            txn_id: Some("txn-a".to_owned()),
            account_id: account_id.clone(),
            chat_id: chat_id.clone(),
            op: OutboxOp::Send,
            payload: json!({ "text": "a" }),
            next_attempt_at: 900,
            created_at: 900,
        })
        .await
        .expect("enqueue a");
    let second_txn = store
        .enqueue_outbox(NewOutboxItem {
            txn_id: Some("txn-b".to_owned()),
            account_id,
            chat_id,
            op: OutboxOp::Read,
            payload: json!({ "up_to_message_id": "m1" }),
            next_attempt_at: 950,
            created_at: 950,
        })
        .await
        .expect("enqueue b");

    let claimed = store.claim_due_outbox(1_000, 10).await.expect("claim");
    assert_eq!(claimed.len(), 2);
    assert_eq!(claimed[0].txn_id, first_txn);
    assert_eq!(claimed[1].txn_id, second_txn);
    assert!(
        claimed
            .iter()
            .all(|item| item.state == OutboxState::InFlight)
    );
    assert!(
        store
            .claim_due_outbox(1_000, 10)
            .await
            .expect("claim again")
            .is_empty(),
        "in-flight rows must not be handed out twice"
    );

    store
        .fail_outbox(&first_txn, "flood wait", 2_000)
        .await
        .expect("fail");
    assert!(
        store
            .claim_due_outbox(1_999, 10)
            .await
            .expect("too early")
            .is_empty()
    );
    let retried = store.claim_due_outbox(2_000, 10).await.expect("retry");
    assert_eq!(retried.len(), 1);
    assert_eq!(retried[0].attempts, 1);
    assert_eq!(retried[0].last_error.as_deref(), Some("flood wait"));

    store.complete_outbox(&second_txn).await.expect("complete");
    assert!(
        store
            .claim_due_outbox(i64::MAX, 10)
            .await
            .expect("after complete")
            .is_empty()
    );
}

#[tokio::test]
async fn events_since_returns_newer_events_in_order() {
    let store = Store::open_in_memory(KEY).expect("open");
    let (_account_id, chat_id) = account_and_chat(&store).await;

    let seen = store.events_since(0, 100).await.expect("initial events");
    assert!(!seen.is_empty());
    let last_seq = seen.last().expect("last event").seq;

    store
        .update_chat_flags(
            &chat_id,
            ChatFlagsUpdate {
                is_pinned: Some(true),
                ..ChatFlagsUpdate::default()
            },
        )
        .await
        .expect("pin");
    store
        .update_chat_flags(
            &chat_id,
            ChatFlagsUpdate {
                note: Some(Some("later".to_owned())),
                ..ChatFlagsUpdate::default()
            },
        )
        .await
        .expect("note");

    let events = store.events_since(last_seq, 100).await.expect("new events");
    assert_eq!(events.len(), 2);
    assert!(events[0].seq > last_seq);
    assert!(events[0].seq < events[1].seq);
    assert_eq!(events[0].kind, "chat.flags");
    assert_eq!(events[1].kind, "chat.flags");
    assert_eq!(events[1].payload["note"], "later");
}

#[tokio::test]
async fn full_text_search_tracks_body_edits() {
    let store = Store::open_in_memory(KEY).expect("open");
    let (account_id, chat_id) = account_and_chat(&store).await;

    let mut message = text_message(&chat_id, 1_000);
    message.remote_msg_id = Some("remote-fts".to_owned());
    message.body = Some("banana bread recipe".to_owned());
    let stored = store.upsert_message(message).await.expect("insert");

    let found = store
        .search_messages("banana", Some(account_id), 10)
        .await
        .expect("search banana");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].message_id, stored.message_id);

    let mut edit = text_message(&chat_id, 1_000);
    edit.message_id = Some(stored.message_id.clone());
    edit.remote_msg_id = Some("remote-fts".to_owned());
    edit.body = Some("apple pie recipe".to_owned());
    edit.edited_at = Some(2_000);
    store.upsert_message(edit).await.expect("edit");

    assert!(
        store
            .search_messages("banana", None, 10)
            .await
            .expect("old token")
            .is_empty(),
        "the edited body must be removed from the index"
    );
    let found = store
        .search_messages("apple", None, 10)
        .await
        .expect("new token");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].body.as_deref(), Some("apple pie recipe"));
    assert_eq!(found[0].edited_at, Some(2_000));
}

#[tokio::test]
async fn vacuum_keeps_full_text_index_aligned() {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("alloy.db");

    let store = Store::open(&path, KEY).expect("open");
    let (_account_id, chat_id) = account_and_chat(&store).await;

    let mut kept = text_message(&chat_id, 1_000);
    kept.remote_msg_id = Some("remote-kept".to_owned());
    kept.body = Some("alpha banana".to_owned());
    let kept = store.upsert_message(kept).await.expect("kept message");

    let mut removed = text_message(&chat_id, 2_000);
    removed.remote_msg_id = Some("remote-removed".to_owned());
    removed.body = Some("bravo bread".to_owned());
    let removed = store
        .upsert_message(removed)
        .await
        .expect("removed message");

    let mut survivor = text_message(&chat_id, 3_000);
    survivor.remote_msg_id = Some("remote-survivor".to_owned());
    survivor.body = Some("charlie banana".to_owned());
    let survivor = store
        .upsert_message(survivor)
        .await
        .expect("survivor message");

    let mut edit = text_message(&chat_id, 1_000);
    edit.message_id = Some(kept.message_id.clone());
    edit.remote_msg_id = Some("remote-kept".to_owned());
    edit.body = Some("delta apple".to_owned());
    edit.edited_at = Some(1_500);
    store.upsert_message(edit).await.expect("edit kept message");

    drop(store);

    // Drop the middle row so VACUUM has a rowid gap to compact before it rebuilds the table.
    // Without a stable INTEGER key on `messages` the surviving rows would be renumbered and the
    // external-content FTS index would point at the wrong rows.
    let conn = rusqlite::Connection::open(&path).expect("raw open");
    conn.execute(
        "DELETE FROM messages WHERE message_id = ?1",
        [removed.message_id],
    )
    .expect("delete middle message");
    conn.execute_batch("VACUUM").expect("vacuum");
    drop(conn);

    let store = Store::open(&path, KEY).expect("reopen");

    let banana = store
        .search_messages("banana", None, 10)
        .await
        .expect("banana after vacuum");
    assert_eq!(banana.len(), 1, "the edited body must no longer match");
    assert_eq!(banana[0].message_id, survivor.message_id);
    assert_eq!(banana[0].body.as_deref(), Some("charlie banana"));

    let apple = store
        .search_messages("apple", None, 10)
        .await
        .expect("apple after vacuum");
    assert_eq!(apple.len(), 1);
    assert_eq!(apple[0].message_id, kept.message_id);
    assert_eq!(apple[0].body.as_deref(), Some("delta apple"));

    assert!(
        store
            .search_messages("bravo", None, 10)
            .await
            .expect("bravo after vacuum")
            .is_empty(),
        "a hard-deleted message must not resurface through the index"
    );
}
