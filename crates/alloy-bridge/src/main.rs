//! Loopback WebSocket bridge to the Alloy engine.
//!
//! Lets the web UI (browser, Playwright/puppeteer, future web target) talk to the real engine
//! without Tauri: the same `EngineCommand` / `EngineResponse` / `EngineEvent` JSON as the Tauri
//! IPC commands. It binds to 127.0.0.1 only and is a development/testing tool, not a server.
//!
//! Frames (text JSON):
//!   client -> bridge: `{"id": 1, "command": <EngineCommand>}`
//!   bridge -> client: `{"id": 1, "ok": <EngineResponse>}` | `{"id": 1, "err": <EngineError>}`
//!                     `{"event": <EngineEvent>}`

use alloy_engine::{AppEngine, Engine, EngineContext};
use alloy_protocol::{EngineCommand, EngineError};
use alloy_storage::{InMemorySecretStore, KeyringSecretStore, SecretStore};
use clap::Parser;
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::Message;

#[derive(Parser)]
#[command(about = "Expose the Alloy engine on ws://127.0.0.1:<port>")]
struct Args {
    /// Port to listen on (loopback only).
    #[arg(long, default_value_t = 8765)]
    port: u16,
    /// Directory for stores, media cache and settings.
    #[arg(long)]
    data_dir: PathBuf,
    /// Keep secrets in the OS keychain instead of memory (sessions then survive restarts).
    #[arg(long)]
    keyring: bool,
}

#[derive(Deserialize)]
struct Request {
    id: u64,
    command: EngineCommand,
}

async fn serve(stream: TcpStream, engine: Arc<AppEngine>, ctx: EngineContext) {
    let Ok(ws) = tokio_tungstenite::accept_async(stream).await else {
        return;
    };
    let (mut tx, mut rx) = ws.split();
    let (out_tx, mut out_rx) = tokio::sync::mpsc::unbounded_channel::<String>();

    let writer = tokio::spawn(async move {
        while let Some(text) = out_rx.recv().await {
            if tx.send(Message::text(text)).await.is_err() {
                break;
            }
        }
    });

    let mut events = ctx.sink.subscribe();
    let ev_out = out_tx.clone();
    let forwarder = tokio::spawn(async move {
        loop {
            match events.recv().await {
                Ok(event) => {
                    let frame = serde_json::json!({ "event": event }).to_string();
                    if ev_out.send(frame).is_err() {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("dropped {n} events (slow client)");
                }
                Err(_) => break,
            }
        }
    });

    while let Some(Ok(msg)) = rx.next().await {
        let Message::Text(text) = msg else { continue };
        let req: Request = match serde_json::from_str(text.as_str()) {
            Ok(r) => r,
            Err(e) => {
                let err = EngineError::invalid_argument(format!("bad request: {e}"));
                let _ = out_tx.send(serde_json::json!({ "id": 0, "err": err }).to_string());
                continue;
            }
        };
        // Commands run concurrently so a slow one never blocks event delivery or other calls.
        let engine = engine.clone();
        let out = out_tx.clone();
        tokio::spawn(async move {
            let frame = match engine.handle(req.command).await {
                Ok(ok) => serde_json::json!({ "id": req.id, "ok": ok }),
                Err(err) => serde_json::json!({ "id": req.id, "err": err }),
            };
            let _ = out.send(frame.to_string());
        });
    }
    forwarder.abort();
    writer.abort();
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();
    let args = Args::parse();
    std::fs::create_dir_all(&args.data_dir)?;

    let secrets: Arc<dyn SecretStore> = if args.keyring {
        Arc::new(KeyringSecretStore::new("chat.alloy.bridge"))
    } else {
        Arc::new(InMemorySecretStore::new())
    };
    let ctx = EngineContext::new(args.data_dir, secrets);
    let engine = Arc::new(AppEngine::new(ctx.clone()));

    let listener = TcpListener::bind(("127.0.0.1", args.port)).await?;
    println!("alloy-bridge listening on ws://127.0.0.1:{}", args.port);
    loop {
        tokio::select! {
            accepted = listener.accept() => {
                let (stream, _) = accepted?;
                tokio::spawn(serve(stream, engine.clone(), ctx.clone()));
            }
            _ = tokio::signal::ctrl_c() => return Ok(()),
        }
    }
}
