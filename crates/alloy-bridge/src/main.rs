//! Loopback WebSocket bridge to the Alloy engine.
//!
//! Lets the web UI (browser, Playwright/puppeteer, future web target) talk to the real engine
//! without Tauri: the same `EngineCommand` / `EngineResponse` / `EngineEvent` JSON as the Tauri
//! IPC commands. It binds to 127.0.0.1 only and is a development/testing tool, not a server.
//! Every connection must present the bridge token as `?token=<token>`. When the client sends an
//! `Origin` header it must be a local UI origin; see [`origin_allowed`].
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
use tokio_tungstenite::tungstenite::handshake::server::{
    ErrorResponse, Request as HsRequest, Response as HsResponse,
};
use tokio_tungstenite::tungstenite::http::StatusCode;

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
    /// Token every client must present as `?token=` in the WebSocket URL. Defaults to
    /// `$ALLOY_BRIDGE_TOKEN`, else a random token printed at startup.
    #[arg(long)]
    token: Option<String>,
}

#[derive(Deserialize)]
struct Request {
    id: u64,
    command: EngineCommand,
}

/// Environment variable carrying the bridge token (used when `--token` is absent).
const TOKEN_ENV: &str = "ALLOY_BRIDGE_TOKEN";

/// Why a WebSocket handshake was refused.
#[derive(Debug, PartialEq, Eq)]
enum Rejection {
    /// The `Origin` header is present but is not a UI origin.
    Origin,
    /// The `token` query parameter is missing or wrong.
    Token,
}

/// Browser origins the UI may connect from: the Tauri webview and local dev servers.
fn origin_allowed(origin: &str) -> bool {
    if matches!(origin, "tauri://localhost" | "http://tauri.localhost") {
        return true;
    }
    ["http://localhost", "http://127.0.0.1"].iter().any(|host| {
        origin == *host
            || origin
                .strip_prefix(*host)
                .and_then(|rest| rest.strip_prefix(':'))
                .is_some_and(|port| !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()))
    })
}

/// Value of the `token` query parameter, if present.
fn query_token(query: Option<&str>) -> Option<&str> {
    query?
        .split('&')
        .find_map(|pair| pair.strip_prefix("token="))
}

/// Compares secrets without returning at the first differing byte.
fn token_matches(given: &str, expected: &str) -> bool {
    let (a, b) = (given.as_bytes(), expected.as_bytes());
    let mut diff = a.len() ^ b.len();
    for (x, y) in a.iter().zip(b) {
        diff |= usize::from(x ^ y);
    }
    diff == 0
}

/// Decides whether a handshake may proceed. `origin` is `None` when the client sent no
/// `Origin` header (non-browser clients); the token is required in every case.
fn check_handshake(
    origin: Option<&str>,
    query: Option<&str>,
    token: &str,
) -> Result<(), Rejection> {
    if origin.is_some_and(|o| !origin_allowed(o)) {
        return Err(Rejection::Origin);
    }
    match query_token(query) {
        Some(given) if !token.is_empty() && token_matches(given, token) => Ok(()),
        _ => Err(Rejection::Token),
    }
}

/// HTTP refusal sent before the WebSocket upgrade completes.
fn refuse(rejection: Rejection) -> ErrorResponse {
    let (status, message) = match rejection {
        Rejection::Origin => (StatusCode::FORBIDDEN, "origin not allowed"),
        Rejection::Token => (StatusCode::UNAUTHORIZED, "missing or invalid bridge token"),
    };
    let mut response = ErrorResponse::new(Some(message.to_owned()));
    *response.status_mut() = status;
    response
}

/// A 256-bit random token: two OS-seeded v4 UUIDs as 64 hex characters.
fn generate_token() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

/// Tokens travel unescaped in a URL query, so only URL-safe characters are accepted.
fn token_is_valid(token: &str) -> bool {
    !token.is_empty()
        && token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

async fn serve(stream: TcpStream, engine: Arc<AppEngine>, ctx: EngineContext, token: Arc<str>) {
    // The callback's error type is fixed by tungstenite's `Callback` trait.
    #[allow(clippy::result_large_err)]
    let handshake = move |req: &HsRequest, resp: HsResponse| {
        let origin = req
            .headers()
            .get("origin")
            .map(|v| v.to_str().unwrap_or(""));
        match check_handshake(origin, req.uri().query(), &token) {
            Ok(()) => Ok(resp),
            Err(rejection) => Err(refuse(rejection)),
        }
    };
    let Ok(ws) = tokio_tungstenite::accept_hdr_async(stream, handshake).await else {
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
        Arc::new(KeyringSecretStore::new("chat.alloy.bridge").map_err(std::io::Error::other)?)
    } else {
        Arc::new(InMemorySecretStore::new())
    };
    let ctx = EngineContext::new(args.data_dir, secrets);
    let engine = Arc::new(AppEngine::new(ctx.clone()));

    let token = match args
        .token
        .or_else(|| std::env::var(TOKEN_ENV).ok())
        .filter(|t| !t.is_empty())
    {
        Some(token) => {
            if !token_is_valid(&token) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "bridge token may contain only ASCII letters, digits, '-' and '_'",
                ));
            }
            token
        }
        None => {
            let token = generate_token();
            println!("alloy-bridge token: {token}");
            token
        }
    };
    let token: Arc<str> = Arc::from(token);

    let listener = TcpListener::bind(("127.0.0.1", args.port)).await?;
    println!("alloy-bridge listening on ws://127.0.0.1:{}", args.port);
    loop {
        tokio::select! {
            accepted = listener.accept() => {
                let (stream, _) = accepted?;
                tokio::spawn(serve(stream, engine.clone(), ctx.clone(), token.clone()));
            }
            _ = tokio::signal::ctrl_c() => return Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_origins_are_allowed() {
        for origin in [
            "tauri://localhost",
            "http://tauri.localhost",
            "http://localhost",
            "http://localhost:5176",
            "http://127.0.0.1:1420",
        ] {
            assert!(origin_allowed(origin), "{origin}");
        }
    }

    #[test]
    fn foreign_and_malformed_origins_are_refused() {
        for origin in [
            "https://evil.example",
            "http://localhost.evil.example",
            "http://127.0.0.1.evil.example:5176",
            "http://localhost:",
            "http://localhost:80x",
            "null",
            "",
        ] {
            assert!(!origin_allowed(origin), "{origin}");
        }
    }

    #[test]
    fn token_is_required_on_every_connection() {
        assert_eq!(check_handshake(None, None, "secret"), Err(Rejection::Token));
        assert_eq!(
            check_handshake(None, Some("token=wrong"), "secret"),
            Err(Rejection::Token)
        );
        assert_eq!(
            check_handshake(None, Some("token=secre"), "secret"),
            Err(Rejection::Token)
        );
        assert_eq!(
            check_handshake(None, Some("token=secrets"), "secret"),
            Err(Rejection::Token)
        );
        assert_eq!(
            check_handshake(None, Some("token=secret"), ""),
            Err(Rejection::Token)
        );
        assert_eq!(
            check_handshake(None, Some("token=secret"), "secret"),
            Ok(())
        );
        assert_eq!(
            check_handshake(None, Some("x=1&token=secret"), "secret"),
            Ok(())
        );
    }

    #[test]
    fn foreign_origin_is_refused_even_with_valid_token() {
        assert_eq!(
            check_handshake(Some("https://evil.example"), Some("token=secret"), "secret"),
            Err(Rejection::Origin)
        );
        assert_eq!(
            check_handshake(
                Some("http://localhost:5176"),
                Some("token=secret"),
                "secret"
            ),
            Ok(())
        );
    }

    #[test]
    fn generated_tokens_are_valid_and_unique() {
        let (a, b) = (generate_token(), generate_token());
        assert_eq!(a.len(), 64);
        assert!(token_is_valid(&a));
        assert_ne!(a, b);
        assert!(!token_is_valid("a&b"));
        assert!(!token_is_valid(""));
    }
}
