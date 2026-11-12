//! Live tests against a real Telegram account.
//!
//! Every test here is `#[ignore]`d: they log in with real credentials, which CI never has. To run
//! them:
//!
//! ```sh
//! ALLOY_TELEGRAM_API_ID=12345 \
//! ALLOY_TELEGRAM_API_HASH=0123456789abcdef0123456789abcdef \
//! ALLOY_TELEGRAM_PHONE=+15550100 \
//! ALLOY_TELEGRAM_CODE=12345 \
//! cargo test -p alloy-telegram --test live -- --ignored --nocapture
//! ```
//!
//! The code is read once per run, so a second attempt needs a fresh `ALLOY_TELEGRAM_CODE`. The
//! tests never send messages: they only read what the account already has.

use alloy_connector::{Connector, LoginInput, LoginStep, Network};
use alloy_telegram::{TelegramConfig, TelegramConnector};

fn connector() -> TelegramConnector {
    let config = TelegramConfig::from_env()
        .expect("ALLOY_TELEGRAM_API_ID and ALLOY_TELEGRAM_API_HASH must be set");
    TelegramConnector::new(config)
}

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} must be set to run this test"))
}

#[tokio::test]
#[ignore = "logs in with a real Telegram account"]
async fn phone_login_restores_and_lists_chats() {
    let login = connector();
    assert_eq!(login.network(), Network::Telegram);
    let capabilities = login.capabilities();
    assert!(capabilities.edit, "telegram edits messages");
    assert!(!capabilities.polls, "telegram polls are not implemented");
    assert!(capabilities.typing, "telegram must not suppress typing");

    assert_eq!(login.begin_login().await, LoginStep::PhoneNumber);

    let step = login
        .submit_login(LoginInput::PhoneNumber {
            phone: env("ALLOY_TELEGRAM_PHONE"),
        })
        .await;
    assert!(
        matches!(step, LoginStep::Code { .. }),
        "expected a code step, got {step:?}"
    );

    let step = login
        .submit_login(LoginInput::Code {
            code: env("ALLOY_TELEGRAM_CODE"),
        })
        .await;
    let session = match step {
        LoginStep::Done {
            remote_user_id,
            session,
            ..
        } => {
            assert!(
                remote_user_id.starts_with('u'),
                "unexpected user id {remote_user_id}"
            );
            session
        }
        other => panic!("expected the login to finish, got {other:?}"),
    };

    // A second connector must rebuild the same account from the blob alone.
    let restored = connector();
    let account = restored.restore(session).await.expect("restore");
    assert!(
        !account.display_name.is_empty(),
        "the account needs a display name"
    );

    let (events, mut receiver) = tokio::sync::mpsc::channel(64);
    restored.start(events).await.expect("start");
    let first = receiver.recv().await.expect("a connection event");
    assert!(
        matches!(first, alloy_connector::ConnectorEvent::Connection { .. }),
        "unexpected first event {first:?}"
    );

    let page = restored.list_chats(None, 5).await.expect("list chats");
    assert!(page.items.len() <= 5, "the limit must be respected");

    if let Some(chat) = page.items.first() {
        let history = restored
            .backfill(&chat.remote_chat_id, None, 5)
            .await
            .expect("backfill");
        assert!(history.items.len() <= 5, "the limit must be respected");
        if let Some(oldest) = history.items.last() {
            let before = restored
                .backfill(&chat.remote_chat_id, Some(oldest.remote_msg_id.clone()), 5)
                .await
                .expect("backfill before");
            assert!(
                before
                    .items
                    .iter()
                    .all(|message| message.remote_msg_id != oldest.remote_msg_id),
                "`before` is exclusive"
            );
        }
    }

    restored.stop().await.expect("stop");
}
