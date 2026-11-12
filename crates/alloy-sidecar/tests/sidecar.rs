//! Behavioural tests for the sidecar host.
//!
//! The sidecar under test is `fake-sidecar`, which serves the deterministic fake connector over
//! the real protocol, so these tests exercise process launch, the handshake, request correlation,
//! event forwarding and supervision end to end.
//!
//! Run with `cargo test -p alloy-sidecar --features fake-sidecar`.

use std::path::Path;
use std::time::Duration;

use alloy_connector::fake::{FAKE_CHAT_DIRECT, FAKE_CODE, FAKE_USER_ID};
use alloy_connector::{
    ConnectionState, Connector, ConnectorError, ConnectorEvent, LoginInput, LoginStep, Network,
    OutgoingMessage, RemoteMessage,
};
use alloy_sidecar::{SidecarConfig, SidecarConnector};
use tokio::sync::mpsc::{Receiver, channel};

/// Long enough for a restart (1 s backoff plus a fresh handshake) on a loaded machine.
const EVENT_TIMEOUT: Duration = Duration::from_secs(15);

fn config(directory: &Path, env: &[(&str, &str)]) -> SidecarConfig {
    let mut config = SidecarConfig::new(env!("CARGO_BIN_EXE_fake-sidecar"), directory);
    config.env = env
        .iter()
        .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
        .collect();
    config.request_timeout = Duration::from_secs(10);
    config.hello_timeout = Duration::from_secs(10);
    config
}

fn outgoing(txn_id: &str, body: &str) -> OutgoingMessage {
    OutgoingMessage {
        txn_id: txn_id.to_string(),
        remote_chat_id: FAKE_CHAT_DIRECT.to_string(),
        body: Some(body.to_string()),
        formatted: None,
        reply_to: None,
        attachment: None,
    }
}

async fn login(connector: &SidecarConnector) {
    assert_eq!(connector.begin_login().await, LoginStep::PhoneNumber);

    let step = connector
        .submit_login(LoginInput::PhoneNumber {
            phone: "+15550100".to_string(),
        })
        .await;
    assert!(
        matches!(
            step,
            LoginStep::Code {
                length: Some(5),
                ..
            }
        ),
        "the phone number is answered with a code step, got {step:?}"
    );

    let step = connector
        .submit_login(LoginInput::Code {
            code: FAKE_CODE.to_string(),
        })
        .await;
    match step {
        LoginStep::Done {
            remote_user_id,
            display_name,
            session,
        } => {
            assert_eq!(remote_user_id, FAKE_USER_ID);
            assert_eq!(display_name, "Fake User");
            assert!(
                session.byte_len() > 0,
                "a completed login hands back a session"
            );
        }
        other => panic!("expected a completed login, got {other:?}"),
    }
}

async fn next_event(events: &mut Receiver<ConnectorEvent>) -> ConnectorEvent {
    tokio::time::timeout(EVENT_TIMEOUT, events.recv())
        .await
        .expect("a connector event arrives within the timeout")
        .expect("the event channel stays open")
}

async fn wait_for_online(events: &mut Receiver<ConnectorEvent>) {
    loop {
        if let ConnectorEvent::Connection {
            state: ConnectionState::Online,
        } = next_event(events).await
        {
            return;
        }
    }
}

async fn wait_for_message(events: &mut Receiver<ConnectorEvent>, txn_id: &str) -> RemoteMessage {
    loop {
        let wanted = match next_event(events).await {
            ConnectorEvent::Message { message } => {
                (message.txn_id.as_deref() == Some(txn_id)).then_some(message)
            }
            _ => None,
        };
        if let Some(message) = wanted {
            return message;
        }
    }
}

#[tokio::test]
async fn handshake_reports_network_and_capabilities() {
    let directory = tempfile::tempdir().expect("temp dir");
    let connector = SidecarConnector::spawn(config(directory.path(), &[]))
        .await
        .expect("the fake sidecar completes the handshake");

    assert_eq!(connector.network(), Network::Fake);

    let capabilities = connector.capabilities();
    assert!(capabilities.edit);
    assert_eq!(capabilities.edit_window_secs, None);
    assert!(capabilities.delete_for_everyone);
    assert!(capabilities.reactions);
    assert_eq!(capabilities.max_reactions_per_user, Some(3));
    assert!(capabilities.typing);
    assert!(capabilities.replies);
    assert!(!capabilities.threads);
    assert!(!capabilities.polls);
    assert_eq!(capabilities.max_upload_bytes, Some(50 * 1024 * 1024));
    assert!(!capabilities.formatted_text);

    connector.stop().await.expect("stop");
}

#[tokio::test]
async fn login_runs_through_the_sidecar() {
    let directory = tempfile::tempdir().expect("temp dir");
    let connector = SidecarConnector::spawn(config(directory.path(), &[]))
        .await
        .expect("spawn");

    login(&connector).await;

    let (sender, mut events) = channel(64);
    connector.start(sender).await.expect("start");
    wait_for_online(&mut events).await;

    let page = connector.list_chats(None, 10).await.expect("list_chats");
    assert_eq!(page.items.len(), 3, "the fake seeds three chats");
    assert!(
        page.items
            .iter()
            .any(|chat| chat.remote_chat_id == FAKE_CHAT_DIRECT),
        "the seeded direct chat is listed"
    );
    assert_eq!(page.next_cursor, None, "one page covers the seeded chats");

    connector.stop().await.expect("stop");
}

#[tokio::test]
async fn send_returns_ack_and_echo_with_the_same_txn_id() {
    let directory = tempfile::tempdir().expect("temp dir");
    let connector = SidecarConnector::spawn(config(directory.path(), &[]))
        .await
        .expect("spawn");
    login(&connector).await;

    let (sender, mut events) = channel(64);
    connector.start(sender).await.expect("start");
    wait_for_online(&mut events).await;

    let ack = connector
        .send(outgoing("txn-sidecar-1", "hello from the host"))
        .await
        .expect("send");
    assert_eq!(ack.txn_id, "txn-sidecar-1");
    assert!(!ack.remote_msg_id.is_empty());
    assert!(ack.sent_at > 0);

    let echo = wait_for_message(&mut events, "txn-sidecar-1").await;
    assert_eq!(echo.remote_chat_id, FAKE_CHAT_DIRECT);
    assert_eq!(echo.remote_msg_id, ack.remote_msg_id);
    assert_eq!(echo.sent_at, ack.sent_at);
    assert_eq!(echo.body.as_deref(), Some("hello from the host"));
    assert!(echo.is_own);

    connector.stop().await.expect("stop");
}

#[tokio::test]
async fn version_mismatch_is_refused() {
    let directory = tempfile::tempdir().expect("temp dir");
    let error = SidecarConnector::spawn(config(
        directory.path(),
        &[("ALLOY_FAKE_SIDECAR_BAD_VERSION", "1")],
    ))
    .await
    .err()
    .expect("a sidecar announcing another protocol version is refused");

    match error {
        ConnectorError::Protocol { message } => assert!(
            message.contains("999") && message.contains("version"),
            "the refusal names the announced version, got: {message}"
        ),
        other => panic!("expected a protocol error, got {other:?}"),
    }
}

#[tokio::test]
async fn crash_mid_session_fails_pending_and_resumes_after_respawn() {
    let directory = tempfile::tempdir().expect("temp dir");
    let connector = SidecarConnector::spawn(config(
        directory.path(),
        &[("ALLOY_FAKE_SIDECAR_CRASH_AFTER", "4")],
    ))
    .await
    .expect("spawn");
    login(&connector).await;

    let (sender, mut events) = channel(128);
    connector.start(sender).await.expect("start");
    wait_for_online(&mut events).await;

    // `begin_login`, both `submit_login` calls and `start` are requests 1 to 4; the sidecar exits
    // on request 5 without answering it, so this send is in flight when the process dies.
    let error = connector
        .send(outgoing("txn-crash", "lost to the crash"))
        .await
        .expect_err("the in-flight send fails");
    match error {
        ConnectorError::Network { message } => assert!(
            message.contains("sidecar exited"),
            "the request fails with the exit status, got: {message}"
        ),
        other => panic!("expected a network error, got {other:?}"),
    }

    let mut saw_offline = false;
    loop {
        match next_event(&mut events).await {
            ConnectorEvent::Connection {
                state: ConnectionState::Offline { .. },
            } => saw_offline = true,
            ConnectorEvent::Connection {
                state: ConnectionState::Online,
            } => break,
            _ => {}
        }
    }
    assert!(
        saw_offline,
        "the host reports the sidecar offline before it resumes"
    );

    // The host replayed `restore` and `start` on the new process, so the account is still live.
    let page = connector
        .list_chats(None, 10)
        .await
        .expect("the replayed session lists chats");
    assert_eq!(page.items.len(), 3);

    let ack = connector
        .send(outgoing("txn-after-restart", "still here"))
        .await
        .expect("send after the restart");
    assert_eq!(ack.txn_id, "txn-after-restart");
    let echo = wait_for_message(&mut events, "txn-after-restart").await;
    assert_eq!(echo.remote_msg_id, ack.remote_msg_id);

    connector.stop().await.expect("stop");
}

#[tokio::test]
async fn stop_terminates_the_sidecar() {
    let directory = tempfile::tempdir().expect("temp dir");
    let pid_file = directory.path().join("sidecar.pid");
    let pid_path = pid_file.to_str().expect("a UTF-8 temp path").to_string();

    let connector = SidecarConnector::spawn(config(
        directory.path(),
        &[("ALLOY_FAKE_SIDECAR_PID_FILE", pid_path.as_str())],
    ))
    .await
    .expect("spawn");
    login(&connector).await;

    let (sender, mut events) = channel(32);
    connector.start(sender).await.expect("start");
    wait_for_online(&mut events).await;

    #[cfg(unix)]
    {
        assert!(
            process_alive(recorded_pid(&pid_file)),
            "the sidecar process runs before stop"
        );
    }

    connector.stop().await.expect("stop");

    #[cfg(unix)]
    {
        assert!(
            !process_alive(recorded_pid(&pid_file)),
            "stop terminates the sidecar process"
        );
    }

    let error = connector
        .list_chats(None, 10)
        .await
        .expect_err("there is no transport after stop");
    assert!(
        matches!(error, ConnectorError::Network { .. }),
        "unexpected error after stop: {error:?}"
    );

    connector.stop().await.expect("stop is idempotent");
}

#[cfg(unix)]
fn recorded_pid(pid_file: &Path) -> u32 {
    std::fs::read_to_string(pid_file)
        .expect("the sidecar recorded its process id")
        .trim()
        .parse()
        .expect("a numeric process id")
}

#[cfg(unix)]
fn process_alive(pid: u32) -> bool {
    std::process::Command::new("ps")
        .arg("-p")
        .arg(pid.to_string())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}
