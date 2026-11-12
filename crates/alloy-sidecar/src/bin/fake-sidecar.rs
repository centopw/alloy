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

use std::env;
use std::path::PathBuf;
use std::sync::Arc;

use alloy_connector::fake::FakeConnector;
use alloy_connector::{
    Connector, ConnectorError, ConnectorEvent, Frame, LoginInput, MediaRef, OutgoingMessage,
    PROTOCOL_VERSION, SessionBlob, decode_frame, encode_frame,
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

    let connector = Arc::new(FakeConnector::new());
    let protocol_version = match env::var(BAD_VERSION_ENV).as_deref() {
        Ok("1") => 999,
        _ => PROTOCOL_VERSION,
    };
    let crash_after: Option<u64> = env::var(CRASH_AFTER_ENV)
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
            let response = match dispatch(&connector, &method, params, &events).await {
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

/// Runs one connector call and returns its JSON result.
async fn dispatch(
    connector: &FakeConnector,
    method: &str,
    params: Value,
    events: &Sender<ConnectorEvent>,
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
struct FetchMediaParams {
    media: MediaRef,
    dest_path: String,
}
