//! End-to-end sync against the real server in `server/` (ADR 0020, engine slice).
//!
//! Every test here spawns `node server/src/main.js` with its own data directory and token and
//! drives the engine over real HTTP. The TLS proxy is a real `https` listener on loopback: the
//! pin is taken from a real certificate, not a fixture. Node-dependent tests print a skip line
//! and return when node is absent.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use alloy_engine::factory::{ConnectorFactory, DefaultConnectorFactory};
use alloy_engine::{AppEngine, Engine, EngineContext, EngineError};
use alloy_protocol::{EngineCommand, EngineResponse, SyncMode, SyncState, SyncStatusReport};
use alloy_storage::{InMemorySecretStore, SecretStore};
use alloy_store::{AccountState, ChatFlagsUpdate, ChatKind, ChatUpsert, Network, NewAccount};
use alloy_sync::SignedBatch;
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::time::{sleep, timeout};

const WAIT: Duration = Duration::from_secs(20);
const TOKEN: &str = "test-token-0123456789";

// === Fixtures ===

fn vault() -> Arc<dyn SecretStore> {
    Arc::new(InMemorySecretStore::new())
}

async fn open_engine(data_dir: &Path, secrets: Arc<dyn SecretStore>) -> AppEngine {
    let factory: Arc<dyn ConnectorFactory> = Arc::new(DefaultConnectorFactory::new(
        secrets.clone(),
        None,
        data_dir.to_path_buf(),
    ));
    let ctx = EngineContext::open(data_dir.to_path_buf(), secrets, factory)
        .await
        .expect("open the engine");
    AppEngine::new(ctx)
}

async fn copy_master(from: &Arc<dyn SecretStore>, to: &Arc<dyn SecretStore>) {
    let master = from
        .get_secret("alloy.sync.master_key.v1")
        .await
        .expect("the first device has a master key after ConfigureSync");
    to.set_secret("alloy.sync.master_key.v1", &master)
        .await
        .expect("store the master key on the second device");
}

async fn seed_chat(engine: &AppEngine, tag: Option<&str>) -> String {
    let store = &engine.context().store;
    let account = store
        .insert_account(NewAccount {
            account_id: None,
            network: Network::Telegram,
            remote_user_id: Some("1001".into()),
            display_name: Some("Ada".into()),
            state: AccountState::Active,
        })
        .await
        .expect("insert account");
    let chat_id = store
        .upsert_chat(ChatUpsert {
            account_id: account.account_id.clone(),
            remote_chat_id: "42".into(),
            kind: ChatKind::Direct,
            title: Some("Ada".into()),
            avatar_media_id: None,
            last_message_id: None,
            last_activity_at: None,
            parent_remote_chat_id: None,
            is_forum: false,
        })
        .await
        .expect("insert chat");
    if let Some(tag) = tag {
        store
            .update_chat_flags(
                &chat_id,
                ChatFlagsUpdate {
                    tags: Some(vec![tag.to_string()]),
                    tags_updated_at: Some(1_700_000_000_000),
                    ..ChatFlagsUpdate::default()
                },
            )
            .await
            .expect("tag the chat");
    }
    chat_id
}

fn write_settings(dir: &Path, value: &Value) {
    std::fs::write(
        dir.join("settings.json"),
        serde_json::to_vec_pretty(value).expect("encode settings"),
    )
    .expect("write settings");
}

/// Updates the settings layer without dropping the device-local `sync` section.
fn merge_settings(dir: &Path, layer: &Value) {
    let mut current = read_settings(dir);
    if let (Some(current), Some(layer)) = (current.as_object_mut(), layer.as_object()) {
        for (key, value) in layer {
            current.insert(key.clone(), value.clone());
        }
    }
    write_settings(dir, &current);
}

fn read_settings(dir: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(dir.join("settings.json")).expect("read settings"))
        .expect("parse settings")
}

async fn configure(
    engine: &AppEngine,
    url: &str,
    token: &str,
    accept_new_pin: bool,
) -> Result<(Option<String>, SyncStatusReport), EngineError> {
    match engine
        .handle(EngineCommand::ConfigureSync {
            url: url.to_string(),
            token: token.to_string(),
            mode: SyncMode::Prefs,
            accept_new_pin,
        })
        .await?
    {
        EngineResponse::SyncConfigured { pin, status } => Ok((pin, status)),
        other => panic!("unexpected response: {other:?}"),
    }
}

async fn sync(engine: &AppEngine) -> Result<(u64, u64, String, u32), EngineError> {
    match engine.handle(EngineCommand::SyncNow).await? {
        EngineResponse::SyncCompleted {
            uploaded,
            downloaded,
            head_hex,
            epoch,
        } => Ok((uploaded, downloaded, head_hex, epoch)),
        other => panic!("unexpected response: {other:?}"),
    }
}

async fn status(engine: &AppEngine) -> SyncStatusReport {
    match engine
        .handle(EngineCommand::SyncStatus)
        .await
        .expect("status")
    {
        EngineResponse::SyncStatus { status } => status,
        other => panic!("unexpected response: {other:?}"),
    }
}

async fn chat_tags(engine: &AppEngine, chat_id: &str) -> Vec<String> {
    engine
        .context()
        .store
        .get_chat(chat_id)
        .await
        .expect("read chat")
        .expect("chat exists")
        .local_tags
}

async fn node_available() -> bool {
    Command::new("node")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .is_ok_and(|status| status.success())
}

// === The real server ===

struct SyncServer {
    child: Child,
    port: u16,
    _dir: TempDir,
}

impl SyncServer {
    fn base(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
}

impl Drop for SyncServer {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

/// A free loopback port, released before the server binds it.
fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind a free port");
    listener.local_addr().expect("local addr").port()
}

async fn start_server() -> SyncServer {
    start_server_with(TOKEN).await
}

async fn start_server_with(tokens: &str) -> SyncServer {
    let dir = TempDir::new().expect("server data dir");
    let port = free_port();
    let mut child = Command::new("node")
        .arg("../../server/src/main.js")
        .env("SYNC_DATA_DIR", dir.path())
        .env("SYNC_TOKENS", tokens)
        .env("PORT", port.to_string())
        .env("HOST", "127.0.0.1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("spawn the real sync server");
    let stdout = child.stdout.take().expect("server stdout");
    let mut lines = BufReader::new(stdout).lines();
    let line = timeout(WAIT, lines.next_line())
        .await
        .expect("the sync server did not start in time")
        .expect("read the server's startup line")
        .expect("the sync server printed no startup line");
    assert!(
        line.contains(&port.to_string()),
        "the server did not report port {port}: {line:?}"
    );
    SyncServer {
        child,
        port,
        _dir: dir,
    }
}

/// A loopback proxy that delays the `PUT /v1/head` by `delay_ms`, widening the read/write window
/// so a second writer wins between a client's GET and its CAS.
struct DelayProxy {
    child: Child,
    port: u16,
    /// Appended with `409` every time the delayed head write lost the CAS.
    conflicts: PathBuf,
    _dir: TempDir,
}

impl Drop for DelayProxy {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

async fn start_delay_proxy(target_port: u16, delay_ms: u64) -> DelayProxy {
    let dir = TempDir::new().expect("proxy dir");
    let conflicts = dir.path().join("conflicts.log");
    let script = dir.path().join("delay.mjs");
    std::fs::write(
        &script,
        r#"
import http from 'node:http';
import fs from 'node:fs';
const port = Number(process.env.PROXY_PORT);
const target = Number(process.env.TARGET_PORT);
const delay = Number(process.env.DELAY_MS);
const conflictFile = process.env.CONFLICT_FILE;
const server = http.createServer((req, res) => {
  const chunks = [];
  req.on('data', (chunk) => chunks.push(chunk));
  req.on('end', () => {
    const body = Buffer.concat(chunks);
    const send = () => {
      const upstream = http.request(
        { host: '127.0.0.1', port: target, path: req.url, method: req.method, headers: req.headers },
        (answer) => {
          if (
            conflictFile &&
            req.method === 'PUT' &&
            req.url === '/v1/head' &&
            answer.statusCode === 409
          ) {
            fs.appendFileSync(conflictFile, '409\n');
          }
          res.writeHead(answer.statusCode, answer.headers);
          answer.pipe(res);
        },
      );
      upstream.end(body);
    };
    if (req.method === 'PUT' && req.url === '/v1/head') setTimeout(send, delay);
    else send();
  });
});
server.listen(port, '127.0.0.1', () => console.log(`listening ${server.address().port}`));
"#,
    )
    .expect("write the delay proxy");
    let (child, port) = spawn_node_script(
        &script,
        &[
            ("TARGET_PORT", target_port.to_string()),
            ("PROXY_PORT", "0".to_string()),
            ("DELAY_MS", delay_ms.to_string()),
            ("CONFLICT_FILE", conflicts.display().to_string()),
        ],
    )
    .await;
    DelayProxy {
        child,
        port,
        conflicts,
        _dir: dir,
    }
}

/// A real TLS listener with a self-signed certificate, forwarding to the plain-HTTP server.
struct TlsProxy {
    child: Child,
    port: u16,
    _dir: TempDir,
}

impl Drop for TlsProxy {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

impl TlsProxy {
    /// Kills the listener and waits for the port to be released before a replacement binds it.
    async fn stop(&mut self) {
        let _ = self.child.start_kill();
        let _ = timeout(WAIT, self.child.wait()).await;
    }
}

fn write_certificate(dir: &Path) {
    let status = std::process::Command::new("openssl")
        .args([
            "req", "-x509", "-newkey", "rsa:2048", "-sha256", "-days", "2", "-nodes", "-keyout",
        ])
        .arg(dir.join("key.pem"))
        .args(["-out"])
        .arg(dir.join("cert.pem"))
        .args(["-subj", "/CN=localhost"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("openssl must be installed for the TLS test");
    assert!(
        status.success(),
        "openssl could not create a test certificate"
    );
}

async fn start_tls_proxy(target_port: u16, port: Option<u16>, certificate: &Path) -> TlsProxy {
    let dir = TempDir::new().expect("tls proxy dir");
    let script = dir.path().join("tls.mjs");
    std::fs::write(
        &script,
        r#"
import https from 'node:https';
import http from 'node:http';
import fs from 'node:fs';
const certDir = process.env.CERT_DIR;
const target = Number(process.env.TARGET_PORT);
const port = Number(process.env.PROXY_PORT);
const server = https.createServer(
  {
    key: fs.readFileSync(`${certDir}/key.pem`),
    cert: fs.readFileSync(`${certDir}/cert.pem`),
    minVersion: 'TLSv1.3',
  },
  (req, res) => {
    const upstream = http.request(
      { host: '127.0.0.1', port: target, path: req.url, method: req.method, headers: req.headers },
      (answer) => {
        res.writeHead(answer.statusCode, answer.headers);
        answer.pipe(res);
      },
    );
    req.pipe(upstream);
  },
);
server.listen(port, '127.0.0.1', () => console.log(`listening ${server.address().port}`));
"#,
    )
    .expect("write the tls proxy");
    let (child, port) = spawn_node_script(
        &script,
        &[
            ("TARGET_PORT", target_port.to_string()),
            ("PROXY_PORT", port.unwrap_or(0).to_string()),
            ("CERT_DIR", certificate.display().to_string()),
        ],
    )
    .await;
    TlsProxy {
        child,
        port,
        _dir: dir,
    }
}

async fn spawn_node_script(script: &Path, env: &[(&str, String)]) -> (Child, u16) {
    let mut command = Command::new("node");
    command
        .arg(script)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true);
    for (key, value) in env {
        command.env(key, value);
    }
    let mut child = command.spawn().expect("spawn the node proxy");
    let stdout = child.stdout.take().expect("proxy stdout");
    let mut lines = BufReader::new(stdout).lines();
    let line = timeout(WAIT, lines.next_line())
        .await
        .expect("the proxy did not start in time")
        .expect("read the proxy startup line")
        .expect("the proxy printed no startup line");
    let port = line
        .rsplit(' ')
        .next()
        .and_then(|port| port.trim().parse().ok())
        .unwrap_or_else(|| panic!("cannot parse the proxy port from {line:?}"));
    (child, port)
}

// === Direct server reads for the rollback test ===

async fn read_head(base: &str) -> (Vec<u8>, String) {
    let response = reqwest::Client::new()
        .get(format!("{base}/v1/head"))
        .bearer_auth(TOKEN)
        .send()
        .await
        .expect("get head");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let etag = response
        .headers()
        .get(reqwest::header::ETAG)
        .and_then(|value| value.to_str().ok())
        .expect("head etag")
        .to_string();
    (response.bytes().await.expect("head body").to_vec(), etag)
}

async fn write_head(base: &str, bytes: &[u8], etag: &str) {
    let response = reqwest::Client::new()
        .put(format!("{base}/v1/head"))
        .bearer_auth(TOKEN)
        .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
        .header(reqwest::header::IF_MATCH, etag)
        .body(bytes.to_vec())
        .send()
        .await
        .expect("put head");
    assert_eq!(
        response.status(),
        reqwest::StatusCode::NO_CONTENT,
        "replaying an old head must land"
    );
}

fn head_seq(bytes: &[u8]) -> u64 {
    SignedBatch::from_bytes(bytes)
        .expect("the server head is a signed batch")
        .header
        .seq
}

// === Tests ===

#[tokio::test]
async fn plain_http_for_a_non_loopback_host_is_refused() {
    let dir = TempDir::new().expect("data dir");
    let engine = open_engine(dir.path(), vault()).await;
    let error = configure(&engine, "http://example.com", TOKEN, false)
        .await
        .expect_err("internet http must be refused");
    assert_eq!(error.code, "invalid_argument");
    assert!(error.message.contains("loopback"), "{}", error.message);
}

#[tokio::test]
async fn a_wrong_token_is_a_typed_auth_error() {
    if !node_available().await {
        eprintln!("skipping: node is not on PATH, so the real sync server cannot be spawned");
        return;
    }
    let server = start_server().await;
    let dir = TempDir::new().expect("data dir");
    let engine = open_engine(dir.path(), vault()).await;
    let error = configure(&engine, &server.base(), "not-the-token", false)
        .await
        .expect_err("a wrong token must be refused");
    assert_eq!(error.code, "auth_failed");
    assert_eq!(error.reason.as_deref(), Some("sync_token_invalid"));
    assert!(
        !format!("{error:?}").contains("not-the-token"),
        "the token must never appear in an error"
    );
}

#[tokio::test]
async fn an_exhausted_quota_is_a_typed_rate_limit_error() {
    if !node_available().await {
        eprintln!("skipping: node is not on PATH, so the real sync server cannot be spawned");
        return;
    }
    let server = start_server_with(&format!("{TOKEN}:1024")).await;
    let dir = TempDir::new().expect("data dir");
    let engine = open_engine(dir.path(), vault()).await;
    seed_chat(&engine, Some("work")).await;
    write_settings(dir.path(), &json!({"density": "compact"}));
    let error = configure(&engine, &server.base(), TOKEN, false)
        .await
        .expect_err("a one-kilobyte quota cannot hold a padded blob");
    assert_eq!(error.code, "rate_limited");
}

#[tokio::test]
async fn configure_pins_the_https_endpoint_and_syncs() {
    if !node_available().await {
        eprintln!("skipping: node is not on PATH, so the real sync server cannot be spawned");
        return;
    }
    let server = start_server().await;
    let certs = TempDir::new().expect("certs");
    write_certificate(certs.path());
    let proxy = start_tls_proxy(server.port, None, certs.path()).await;
    let url = format!("https://127.0.0.1:{}", proxy.port);

    let dir = TempDir::new().expect("data dir");
    let engine = open_engine(dir.path(), vault()).await;
    seed_chat(&engine, Some("work")).await;
    write_settings(dir.path(), &json!({"density": "compact"}));

    let (pin, status) = configure(&engine, &url, TOKEN, false)
        .await
        .expect("first configure pins and syncs");
    let pin = pin.expect("an https endpoint is pinned");
    assert_eq!(pin.len(), 64, "SPKI SHA-256 is 64 hex characters");
    assert!(
        pin.chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
    );
    assert_eq!(status.pin.as_deref(), Some(pin.as_str()));
    assert_eq!(status.state, SyncState::Done);
    assert!(status.token_configured);
    assert!(
        status
            .layers
            .iter()
            .any(|layer| layer.key == "settings" && layer.enabled)
    );
    assert!(
        status
            .layers
            .iter()
            .any(|layer| layer.key == "messages" && !layer.enabled)
    );

    // The first run actually wrote the head through the TLS proxy.
    let (head, _) = read_head(&server.base()).await;
    assert_eq!(head_seq(&head), 1);
}

#[tokio::test]
async fn a_changed_certificate_is_refused_with_pin_mismatch() {
    if !node_available().await {
        eprintln!("skipping: node is not on PATH, so the real sync server cannot be spawned");
        return;
    }
    let server = start_server().await;
    let first_certs = TempDir::new().expect("first certs");
    let second_certs = TempDir::new().expect("second certs");
    write_certificate(first_certs.path());
    write_certificate(second_certs.path());

    let mut proxy = start_tls_proxy(server.port, None, first_certs.path()).await;
    let url = format!("https://127.0.0.1:{}", proxy.port);
    let dir = TempDir::new().expect("data dir");
    let engine = open_engine(dir.path(), vault()).await;
    seed_chat(&engine, None).await;
    write_settings(dir.path(), &json!({"density": "compact"}));
    let (stored, _) = configure(&engine, &url, TOKEN, false)
        .await
        .expect("first configure");

    // Same URL, same port, different leaf: the pinned certificate is replaced.
    let port = proxy.port;
    proxy.stop().await;
    drop(proxy);
    let replacement = start_tls_proxy(server.port, Some(port), second_certs.path()).await;
    assert_eq!(replacement.port, port);

    let error = sync(&engine)
        .await
        .expect_err("a changed certificate must stop sync");
    assert_eq!(error.code, "pin_mismatch");
    assert_eq!(error.reason.as_deref(), Some("pin_mismatch"));
    let stored = stored.expect("the first configure pinned the certificate");
    assert!(error.message.contains(&stored), "{}", error.message);
    let presented = error
        .message
        .split("presented ")
        .nth(1)
        .expect("the presented pin is named")
        .to_string();
    assert_ne!(stored, presented);
}

#[tokio::test]
async fn two_devices_converge_on_settings_and_tags() {
    if !node_available().await {
        eprintln!("skipping: node is not on PATH, so the real sync server cannot be spawned");
        return;
    }
    let server = start_server().await;
    let first_dir = TempDir::new().expect("first data dir");
    let second_dir = TempDir::new().expect("second data dir");
    let first_vault = vault();
    let second_vault = vault();
    let first = open_engine(first_dir.path(), first_vault.clone()).await;
    let second = open_engine(second_dir.path(), second_vault.clone()).await;
    seed_chat(&first, Some("work")).await;
    let second_chat = seed_chat(&second, None).await;
    write_settings(
        first_dir.path(),
        &json!({"density": "compact", "accent": "blue", "window": {"width": 900}}),
    );

    configure(&first, &server.base(), TOKEN, false)
        .await
        .expect("the first device pins and syncs");
    // A second batch before the second device exists: a fresh device must catch up from the
    // current signed head, not only from seq 1.
    merge_settings(first_dir.path(), &json!({"accent": "green"}));
    sync(&first)
        .await
        .expect("the first device publishes a second batch");
    let (head, _) = read_head(&server.base()).await;
    assert_eq!(head_seq(&head), 2);
    copy_master(&first_vault, &second_vault).await;
    let (pin, status) = configure(&second, &server.base(), TOKEN, false)
        .await
        .expect("the second device pulls the first device's configuration");

    assert_eq!(pin, None, "loopback http has no certificate to pin");
    assert_eq!(status.state, SyncState::Done);
    let settings = read_settings(second_dir.path());
    assert_eq!(settings["density"], json!("compact"));
    assert_eq!(settings["accent"], json!("green"));
    assert_eq!(
        settings["window"],
        Value::Null,
        "device-local settings never cross the server"
    );
    assert_eq!(
        chat_tags(&second, &second_chat).await,
        vec!["work".to_string()]
    );

    // Same settings and tags on both, and a second pass moves no new content.
    let first_settings = read_settings(first_dir.path());
    assert_eq!(first_settings["density"], settings["density"]);
    assert_eq!(first_settings["accent"], settings["accent"]);
    let (uploaded, _, head_hex, _) = sync(&first).await.expect("a no-op sync succeeds");
    assert_eq!(uploaded, 0, "identical layers are not uploaded again");
    assert_eq!(head_hex.len(), 64);
}

#[tokio::test]
async fn a_competing_head_is_rebased_not_overwritten() {
    if !node_available().await {
        eprintln!("skipping: node is not on PATH, so the real sync server cannot be spawned");
        return;
    }
    let server = start_server().await;
    let proxy = start_delay_proxy(server.port, 700).await;
    let proxy_url = format!("http://127.0.0.1:{}", proxy.port);

    let first_dir = TempDir::new().expect("first data dir");
    let second_dir = TempDir::new().expect("second data dir");
    let first_vault = vault();
    let second_vault = vault();
    let first = Arc::new(open_engine(first_dir.path(), first_vault.clone()).await);
    let second = open_engine(second_dir.path(), second_vault.clone()).await;
    seed_chat(&first, None).await;
    seed_chat(&second, None).await;
    write_settings(first_dir.path(), &json!({"density": "compact"}));

    configure(&first, &proxy_url, TOKEN, false)
        .await
        .expect("first configure publishes seq 1");
    copy_master(&first_vault, &second_vault).await;
    configure(&second, &server.base(), TOKEN, false)
        .await
        .expect("second device pulls seq 1");

    // Both devices now change the settings layer differently, keeping their sync credentials.
    merge_settings(
        first_dir.path(),
        &json!({"density": "spacious", "staff": true}),
    );
    merge_settings(
        second_dir.path(),
        &json!({"density": "cosy", "staff": false}),
    );

    let racing = Arc::clone(&first);
    let handle = tokio::spawn(async move { sync(&racing).await });
    // The first device has read the head and is now blocked on the delayed PUT; the second
    // device wins the CAS first.
    sleep(Duration::from_millis(150)).await;
    sync(&second)
        .await
        .expect("the second writer wins the head");

    let (uploaded, _, head_hex, _) = handle.await.expect("join").expect("the rebase succeeds");
    assert!(uploaded > 0);
    let (head, _) = read_head(&server.base()).await;
    assert_eq!(
        head_seq(&head),
        3,
        "seq 1, the competitor's seq 2, then the rebased seq 3"
    );
    assert_eq!(head_hex.len(), 64);
    let conflicts = std::fs::read_to_string(&proxy.conflicts).expect("conflict log");
    assert!(
        conflicts.contains("409"),
        "the losing head write must have been a real 409, not just a later read"
    );

    // The rebased head kept both writers' data: the second device pulls the newer settings on
    // top of its own and does not lose the first device's change.
    sync(&second)
        .await
        .expect("second device follows the rebase");
    let settings = read_settings(second_dir.path());
    assert_eq!(settings["density"], json!("spacious"));
    assert_eq!(settings["staff"], json!(true));
    let _ = first;
}

#[tokio::test]
async fn a_rolled_back_head_blocks_and_keeps_local_state() {
    if !node_available().await {
        eprintln!("skipping: node is not on PATH, so the real sync server cannot be spawned");
        return;
    }
    let server = start_server().await;
    let first_dir = TempDir::new().expect("first data dir");
    let second_dir = TempDir::new().expect("second data dir");
    let first_vault = vault();
    let second_vault = vault();
    let first = open_engine(first_dir.path(), first_vault.clone()).await;
    let second = open_engine(second_dir.path(), second_vault.clone()).await;
    let first_chat = seed_chat(&first, Some("work")).await;
    seed_chat(&second, None).await;
    write_settings(first_dir.path(), &json!({"density": "compact"}));

    configure(&first, &server.base(), TOKEN, false)
        .await
        .expect("first configure publishes seq 1");
    let (older, _) = read_head(&server.base()).await;
    assert_eq!(head_seq(&older), 1);

    copy_master(&first_vault, &second_vault).await;
    configure(&second, &server.base(), TOKEN, false)
        .await
        .expect("second device pulls");
    merge_settings(
        second_dir.path(),
        &json!({"density": "cosy", "staff": true}),
    );
    sync(&second).await.expect("second device publishes seq 2");

    // The first device sees seq 2, then the server is rolled back to seq 1.
    sync(&first).await.expect("first device follows seq 2");
    let before = read_settings(first_dir.path());
    assert_eq!(before["density"], json!("cosy"), "the newer layer landed");
    let (newer, newer_etag) = read_head(&server.base()).await;
    assert_eq!(head_seq(&newer), 2);
    write_head(&server.base(), &older, &newer_etag).await;

    let error = sync(&first)
        .await
        .expect_err("a rolled-back head blocks sync");
    assert_eq!(error.code, "sync_blocked");
    assert_eq!(error.reason.as_deref(), Some("branch"));

    // Local state is exactly what the last successful run left, and the status says blocked.
    let after = read_settings(first_dir.path());
    assert_eq!(after["density"], before["density"]);
    assert_eq!(after["staff"], before["staff"]);
    assert_eq!(
        chat_tags(&first, &first_chat).await,
        vec!["work".to_string()]
    );
    let status = status(&first).await;
    assert_eq!(status.state, SyncState::Blocked);
    assert_eq!(status.last_error.as_deref(), Some("branch"));
}

#[tokio::test]
async fn the_token_never_lands_in_settings() {
    if !node_available().await {
        eprintln!("skipping: node is not on PATH, so the real sync server cannot be spawned");
        return;
    }
    let server = start_server().await;
    let dir = TempDir::new().expect("data dir");
    let secrets = vault();
    let engine = open_engine(dir.path(), secrets.clone()).await;
    seed_chat(&engine, None).await;
    configure(&engine, &server.base(), TOKEN, false)
        .await
        .expect("configure");

    let settings = std::fs::read_to_string(dir.path().join("settings.json")).expect("settings");
    assert!(
        !settings.contains(TOKEN),
        "the bearer token must not be in settings.json"
    );
    assert_eq!(
        secrets
            .get_secret("alloy.sync.server_token.v1")
            .await
            .expect("stored"),
        TOKEN
    );
    let status = status(&engine).await;
    assert!(status.token_configured);
}
