//! A sidecar test double: it serves [`alloy_connector::fake::FakeConnector`] over the NDJSON
//! connector protocol so the host's process supervision can be tested without Messenger or a
//! network.
//!
//! Requests are dispatched concurrently, exactly like a real sidecar that talks to a network, and
//! every frame is written through one mutex so lines never interleave.
//!
//! Test flags, all optional environment variables:
//!
//! - `ALLOY_FAKE_SIDECAR_BAD_VERSION=1` — announce protocol version 999 in `hello`.
//! - `ALLOY_FAKE_SIDECAR_CRASH_AFTER=<n>` — exit without answering the `n + 1`th request, which
//!   simulates a crash with a call in flight.
//! - `ALLOY_FAKE_SIDECAR_PID_FILE=<path>` — write the process id there at startup.
//! - `ALLOY_FAKE_SIDECAR_PID_LOG=<path>` — append the process id there at startup, so a test can
//!   count how many processes the host spawned.
//! - `ALLOY_FAKE_SIDECAR_GARBAGE_BEFORE=<n>` — write one undecodable line to stdout just before
//!   answering the `n`th request, so the host's tolerance of a stray line can be tested between
//!   two frames.
//! - `ALLOY_FAKE_SIDECAR_OFFLINE_ATTEMPT=<n>` and `ALLOY_FAKE_SIDECAR_OFFLINE_RETRY_MS=<ms>` —
//!   after `start`, emit an offline report carrying attempt `n` and a next-retry delay of `ms`, so
//!   the host's decoding of a counted retry and of the connector's own delay can be tested. Setting
//!   either one injects the report; the unset one stays `null` on the wire.

use std::env;
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::Arc;

use alloy_connector::fake::FakeConnector;
use alloy_connector::{
    ConnectionState, Connector, ConnectorError, ConnectorEvent, Frame, LoginInput, MediaRef,
    OutgoingMessage, PROTOCOL_VERSION, SessionBlob, decode_frame, encode_frame,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter, Stdout};
use tokio::sync::{
    Mutex,
    mpsc::{Sender, channel},
};

const BAD_VERSION_ENV: &str = "ALLOY_FAKE_SIDECAR_BAD_VERSION";
const CRASH_AFTER_ENV: &str = "ALLOY_FAKE_SIDECAR_CRASH_AFTER";
const PID_FILE_ENV: &str = "ALLOY_FAKE_SIDECAR_PID_FILE";
const PID_LOG_ENV: &str = "ALLOY_FAKE_SIDECAR_PID_LOG";
const GARBAGE_BEFORE_ENV: &str = "ALLOY_FAKE_SIDECAR_GARBAGE_BEFORE";
const OFFLINE_ATTEMPT_ENV: &str = "ALLOY_FAKE_SIDECAR_OFFLINE_ATTEMPT";
const OFFLINE_RETRY_ENV: &str = "ALLOY_FAKE_SIDECAR_OFFLINE_RETRY_MS";

/// Reason the injected offline report carries; the test only pins the attempt and the delay.
const INJECTED_OFFLINE_REASON: &str = "simulated outage";

/// Events the fake connector produces while one request runs. The host drains its own receiver
/// promptly; this only has to be large enough that a burst of `start` events cannot block.
const EVENT_BUFFER: usize = 1024;

/// Exit code used when `ALLOY_FAKE_SIDECAR_CRASH_AFTER` fires.
const CRASH_EXIT_CODE: i32 = 17;

type Writer = Arc<Mutex<BufWriter<Stdout>>>;

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("fake-sidecar: {error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    if let Ok(path) = env::var(PID_FILE_ENV) {
        std::fs::write(path, std::process::id().to_string())?;
    }
    if let Ok(path) = env::var(PID_LOG_ENV) {
        let mut log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        writeln!(log, "{}", std::process::id())?;
    }

    let connector = Arc::new(FakeConnector::new());
    let protocol_version = match env::var(BAD_VERSION_ENV).as_deref() {
        Ok("1") => 999,
        _ => PROTOCOL_VERSION,
    };
    let crash_after: Option<u64> = env::var(CRASH_AFTER_ENV)
        .ok()
        .and_then(|value| value.trim().parse().ok());
    let offline_attempt: Option<u32> = env::var(OFFLINE_ATTEMPT_ENV)
        .ok()
        .and_then(|value| value.trim().parse().ok());
    let offline_retry_millis: Option<u64> = env::var(OFFLINE_RETRY_ENV)
        .ok()
        .and_then(|value| value.trim().parse().ok());
    let garbage_before: Option<u64> = env::var(GARBAGE_BEFORE_ENV)
        .ok()
        .and_then(|value| value.trim().parse().ok());

    let writer: Writer = Arc::new(Mutex::new(BufWriter::new(tokio::io::stdout())));
    write_frame(
        &writer,
        &Frame::Hello {
            protocol_version,
            network: connector.network(),
            capabilities: connector.capabilities(),
        },
    )
    .await?;

    let (event_sender, mut event_receiver) = channel::<ConnectorEvent>(EVENT_BUFFER);
    {
        let writer = Arc::clone(&writer);
        tokio::spawn(async move {
            while let Some(event) = event_receiver.recv().await {
                if write_frame(&writer, &Frame::Event { event }).await.is_err() {
                    return;
                }
            }
        });
    }

    let mut requests: u64 = 0;
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    while let Some(line) = lines.next_line().await? {
        let frame = match decode_frame(&line) {
            Ok(frame) => frame,
            Err(error) => {
                eprintln!("fake-sidecar: {error}");
                continue;
            }
        };
        let Frame::Request { id, method, params } = frame else {
            eprintln!("fake-sidecar: unexpected frame from the host");
            continue;
        };

        requests += 1;
        if crash_after.is_some_and(|limit| requests > limit) {
            eprintln!("fake-sidecar: simulated crash on request {requests}");
            std::process::exit(CRASH_EXIT_CODE);
        }

        let connector = Arc::clone(&connector);
        let events = event_sender.clone();
        let writer = Arc::clone(&writer);
        tokio::spawn(async move {
            let dispatched = dispatch(
                &connector,
                &method,
                params,
                &events,
                offline_attempt,
                offline_retry_millis,
            )
            .await;
            let response = match dispatched {
                Ok(result) => Frame::Response {
                    id,
                    result: Some(result),
                    error: None,
                },
                Err(error) => Frame::Response {
                    id,
                    result: None,
                    error: Some(error),
                },
            };
            // One line the host cannot decode, written where a frame belongs: the response after
            // it must still arrive on the same transport.
            if garbage_before == Some(requests)
                && let Err(error) = write_raw(&writer, "fake-sidecar: not a frame").await
            {
                eprintln!("fake-sidecar: cannot write a stray line: {error}");
            }
            if let Err(error) = write_frame(&writer, &response).await {
                eprintln!("fake-sidecar: cannot write a response: {error}");
            }
        });
    }

    Ok(())
}

/// Writes one frame under the shared writer lock.
async fn write_frame(writer: &Writer, frame: &Frame) -> std::io::Result<()> {
    let line = encode_frame(frame);
    let mut writer = writer.lock().await;
    writer.write_all(line.as_bytes()).await?;
    writer.flush().await
}

/// Writes one raw line under the shared writer lock, for bytes that are not a frame.
async fn write_raw(writer: &Writer, line: &str) -> std::io::Result<()> {
    let mut writer = writer.lock().await;
    writer.write_all(line.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await
}

/// Runs one connector call and returns its JSON result.
async fn dispatch(
    connector: &FakeConnector,
    method: &str,
    params: Value,
    events: &Sender<ConnectorEvent>,
    offline_attempt: Option<u32>,
    offline_retry_millis: Option<u64>,
) -> Result<Value, ConnectorError> {
    match method {
        "network" => encode_result(connector.network()),
        "capabilities" => encode_result(connector.capabilities()),
        "begin_login" => encode_result(connector.begin_login().await),
        "submit_login" => {
            let params = read_params::<SubmitLoginParams>(params)?;
            encode_result(connector.submit_login(params.input).await)
        }
        "restore" => {
            let params = read_params::<RestoreParams>(params)?;
            encode_result(connector.restore(params.session).await?)
        }
        "start" => {
            connector.start(events.clone()).await?;
            if offline_attempt.is_some() || offline_retry_millis.is_some() {
                // Injected only when the test asks for it: it exercises the host's path for an
                // offline report that names its retry attempt and the delay it will wait.
                events
                    .send(ConnectorEvent::Connection {
                        state: ConnectionState::Offline {
                            reason: INJECTED_OFFLINE_REASON.to_string(),
                            attempt: offline_attempt,
                            next_retry_millis: offline_retry_millis,
                        },
                    })
                    .await
                    .map_err(|_| ConnectorError::Network {
                        message: "the host stopped reading events".to_string(),
                    })?;
            }
            Ok(Value::Null)
        }
        "stop" => {
            connector.stop().await?;
            Ok(Value::Null)
        }
        "list_chats" => {
            let params = read_params::<ListChatsParams>(params)?;
            encode_result(connector.list_chats(params.cursor, params.limit).await?)
        }
        "list_members" => {
            let params = read_params::<ListMembersParams>(params)?;
            encode_result(connector.list_members(&params.remote_chat_id).await?)
        }
        "backfill" => {
            let params = read_params::<BackfillParams>(params)?;
            encode_result(
                connector
                    .backfill(&params.remote_chat_id, params.before, params.limit)
                    .await?,
            )
        }
        "send" => {
            let params = read_params::<SendParams>(params)?;
            encode_result(connector.send(params.message).await?)
        }
        "edit" => {
            let params = read_params::<EditParams>(params)?;
            connector
                .edit(
                    &params.remote_chat_id,
                    &params.remote_msg_id,
                    params.body,
                    params.formatted,
                )
                .await?;
            Ok(Value::Null)
        }
        "delete" => {
            let params = read_params::<DeleteParams>(params)?;
            connector
                .delete(
                    &params.remote_chat_id,
                    &params.remote_msg_id,
                    params.for_everyone,
                )
                .await?;
            Ok(Value::Null)
        }
        "react" => {
            let params = read_params::<ReactionParams>(params)?;
            connector
                .react(&params.remote_chat_id, &params.remote_msg_id, &params.emoji)
                .await?;
            Ok(Value::Null)
        }
        "unreact" => {
            let params = read_params::<ReactionParams>(params)?;
            connector
                .unreact(&params.remote_chat_id, &params.remote_msg_id, &params.emoji)
                .await?;
            Ok(Value::Null)
        }
        "mark_read" => {
            let params = read_params::<MarkReadParams>(params)?;
            connector
                .mark_read(&params.remote_chat_id, &params.up_to_remote_msg_id)
                .await?;
            Ok(Value::Null)
        }
        "set_typing" => {
            let params = read_params::<TypingParams>(params)?;
            connector
                .set_typing(&params.remote_chat_id, params.typing)
                .await?;
            Ok(Value::Null)
        }
        "set_chat_pinned" => {
            let params = read_params::<ChatPinnedParams>(params)?;
            connector
                .set_chat_pinned(&params.remote_chat_id, params.pinned)
                .await?;
            Ok(Value::Null)
        }
        "reorder_pinned_chats" => {
            let params = read_params::<ReorderPinnedChatsParams>(params)?;
            connector
                .reorder_pinned_chats(&params.remote_chat_ids)
                .await?;
            Ok(Value::Null)
        }
        "set_message_pinned" => {
            let params = read_params::<MessagePinnedParams>(params)?;
            connector
                .set_message_pinned(&params.remote_chat_id, &params.remote_msg_id, params.pinned)
                .await?;
            Ok(Value::Null)
        }
        "fetch_media" => {
            let params = read_params::<FetchMediaParams>(params)?;
            let dest_path = PathBuf::from(&params.dest_path);
            encode_result(connector.fetch_media(&params.media, &dest_path).await?)
        }
        "logout" => {
            connector.logout().await?;
            Ok(Value::Null)
        }
        other => Err(ConnectorError::NotSupported {
            feature: other.to_string(),
        }),
    }
}

fn encode_result<T: Serialize>(result: T) -> Result<Value, ConnectorError> {
    serde_json::to_value(result).map_err(|error| ConnectorError::Protocol {
        message: format!("cannot encode the result: {error}"),
    })
}

fn read_params<T: serde::de::DeserializeOwned>(params: Value) -> Result<T, ConnectorError> {
    serde_json::from_value(params).map_err(|error| ConnectorError::InvalidInput {
        message: format!("invalid method parameters: {error}"),
    })
}

#[derive(Deserialize)]
struct SubmitLoginParams {
    input: LoginInput,
}

#[derive(Deserialize)]
struct RestoreParams {
    session: SessionBlob,
}

#[derive(Deserialize)]
struct ListChatsParams {
    cursor: Option<String>,
    limit: usize,
}

#[derive(Deserialize)]
struct ListMembersParams {
    remote_chat_id: String,
}

#[derive(Deserialize)]
struct BackfillParams {
    remote_chat_id: String,
    before: Option<String>,
    limit: usize,
}

#[derive(Deserialize)]
struct SendParams {
    message: OutgoingMessage,
}

#[derive(Deserialize)]
struct EditParams {
    remote_chat_id: String,
    remote_msg_id: String,
    body: String,
    formatted: Option<String>,
}

#[derive(Deserialize)]
struct DeleteParams {
    remote_chat_id: String,
    remote_msg_id: String,
    for_everyone: bool,
}

#[derive(Deserialize)]
struct ReactionParams {
    remote_chat_id: String,
    remote_msg_id: String,
    emoji: String,
}

#[derive(Deserialize)]
struct MarkReadParams {
    remote_chat_id: String,
    up_to_remote_msg_id: String,
}

#[derive(Deserialize)]
struct TypingParams {
    remote_chat_id: String,
    typing: bool,
}

#[derive(Deserialize)]
struct ChatPinnedParams {
    remote_chat_id: String,
    pinned: bool,
}

#[derive(Deserialize)]
struct ReorderPinnedChatsParams {
    remote_chat_ids: Vec<String>,
}

#[derive(Deserialize)]
struct MessagePinnedParams {
    remote_chat_id: String,
    remote_msg_id: String,
    pinned: bool,
}

#[derive(Deserialize)]
struct FetchMediaParams {
    media: MediaRef,
    dest_path: String,
}
