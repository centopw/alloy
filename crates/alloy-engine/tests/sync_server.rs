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
use alloy_protocol::{
    EngineCommand, EngineEvent, EngineResponse, PairingPhase, PairingRole, SyncMode, SyncState,
    SyncStatusReport,
};
use alloy_storage::{InMemorySecretStore, SecretStore};
use alloy_store::{AccountState, ChatFlagsUpdate, ChatKind, ChatUpsert, Network, NewAccount};
use alloy_sync::device::{DeviceSecretKey, DeviceWrapError, open_master};
use alloy_sync::{BlobAddr, BlobError, BlobKeyRing, Layer, MasterKey, SignedBatch, open};
use base64::Engine as _;
use ed25519_dalek::SigningKey;
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::broadcast;
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

/// A loopback proxy that refuses every `DELETE` with `500` and forwards everything else. A server
/// can lose the delete endpoint; the client must keep the rotation and say what is left.
struct DeleteRefusingProxy {
    child: Child,
    port: u16,
    _dir: TempDir,
}

impl Drop for DeleteRefusingProxy {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

async fn start_delete_refusing_proxy(target_port: u16) -> DeleteRefusingProxy {
    let dir = TempDir::new().expect("proxy dir");
    let script = dir.path().join("no-delete.mjs");
    std::fs::write(
        &script,
        r#"
import http from 'node:http';
const port = Number(process.env.PROXY_PORT);
const target = Number(process.env.TARGET_PORT);
const server = http.createServer((req, res) => {
  const chunks = [];
  req.on('data', (chunk) => chunks.push(chunk));
  req.on('end', () => {
    if (req.method === 'DELETE') {
      res.writeHead(500);
      res.end();
      return;
    }
    const upstream = http.request(
      { host: '127.0.0.1', port: target, path: req.url, method: req.method, headers: req.headers },
      (answer) => {
        res.writeHead(answer.statusCode, answer.headers);
        answer.pipe(res);
      },
    );
    upstream.end(Buffer.concat(chunks));
  });
});
server.listen(port, '127.0.0.1', () => console.log(`listening ${server.address().port}`));
"#,
    )
    .expect("write the delete-refusing proxy");
    let (child, port) = spawn_node_script(
        &script,
        &[
            ("TARGET_PORT", target_port.to_string()),
            ("PROXY_PORT", "0".to_string()),
        ],
    )
    .await;
    DeleteRefusingProxy {
        child,
        port,
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

fn head_epoch(bytes: &[u8]) -> u32 {
    SignedBatch::from_bytes(bytes)
        .expect("the server head is a signed batch")
        .header
        .epoch
}

// === Revocation helpers ===

struct Revoked {
    epoch: u32,
    re_sealed: Option<bool>,
    blobs_deleted: Option<u32>,
    blobs_remaining: Option<u32>,
    recovery_stale: Option<bool>,
    error: Option<String>,
}

async fn revoke(engine: &AppEngine, device_id: &str) -> Revoked {
    match engine
        .handle(EngineCommand::RevokeDevice {
            device_id: device_id.to_string(),
        })
        .await
        .expect("revoke")
    {
        EngineResponse::DeviceRevoked {
            epoch,
            re_sealed,
            blobs_deleted,
            blobs_remaining,
            recovery_stale,
            error,
        } => Revoked {
            epoch,
            re_sealed,
            blobs_deleted,
            blobs_remaining,
            recovery_stale,
            error,
        },
        other => panic!("unexpected response: {other:?}"),
    }
}

fn read_json_file(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).expect("read json file")).expect("parse json file")
}

/// The ids this device published under `epoch`, as `(id, layer key, seq)`.
fn indexed_blobs(dir: &Path, epoch: u32) -> Vec<(String, String, u64)> {
    let index = read_json_file(&dir.join("sync-blobs.json"));
    index["epochs"][epoch.to_string()]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|blob| {
            (
                blob["id"].as_str().expect("indexed id").to_string(),
                blob["layer"].as_str().expect("indexed layer").to_string(),
                blob["seq"].as_u64().expect("indexed seq"),
            )
        })
        .collect()
}

/// Adds a peer entry to `settings.json`, as an earlier enrolment would have.
fn add_roster_entry(dir: &Path, device_id: &str) {
    let mut settings = read_settings(dir);
    settings["sync"]["devices"]
        .as_array_mut()
        .expect("roster array")
        .push(json!({
            "device_id": device_id,
            "label": "Enrolled peer",
            "enrolled_ms": 1_700_000_000_000i64,
            "epoch": 0,
        }));
    write_settings(dir, &settings);
}

/// Adds a peer through a current enrolment: the entry carries the peer's X25519 public key, so a
/// revocation can seal the rotated master to it.
fn add_roster_peer(dir: &Path, device_id: &str, x25519_pub: &str) {
    let mut settings = read_settings(dir);
    settings["sync"]["devices"]
        .as_array_mut()
        .expect("roster array")
        .push(json!({
            "device_id": device_id,
            "label": "Enrolled peer",
            "enrolled_ms": 1_700_000_000_000i64,
            "epoch": 0,
            "x25519_pub": x25519_pub,
        }));
    write_settings(dir, &settings);
}

fn hex_of(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn decode_b64(text: &str) -> Vec<u8> {
    base64::engine::general_purpose::STANDARD
        .decode(text.trim())
        .expect("the secret is base64")
}

/// This device's roster id and wrap target, creating both keys on first use exactly as the engine
/// does. `ListDevices` is the creation point, so call it first.
async fn device_identity(engine: &AppEngine, secrets: &Arc<dyn SecretStore>) -> (String, String) {
    let _ = roster_ids(engine).await;
    let seed: [u8; 32] = decode_b64(
        &secrets
            .get_secret("alloy.device.ed25519_key.v1")
            .await
            .expect("the device has an Ed25519 key after ListDevices"),
    )
    .try_into()
    .expect("the Ed25519 seed is 32 bytes");
    let id = hex_of(SigningKey::from_bytes(&seed).verifying_key().as_bytes());
    let key = device_secret(secrets).await;
    (id, hex_of(&key.public_key()))
}

async fn device_secret(secrets: &Arc<dyn SecretStore>) -> DeviceSecretKey {
    let bytes: [u8; 32] = decode_b64(
        &secrets
            .get_secret("alloy.device.x25519_key.v1")
            .await
            .expect("the device has an X25519 key after ListDevices"),
    )
    .try_into()
    .expect("the X25519 key is 32 bytes");
    DeviceSecretKey::from_bytes(bytes)
}

async fn verifying_key(secrets: &Arc<dyn SecretStore>) -> ed25519_dalek::VerifyingKey {
    let seed: [u8; 32] = decode_b64(
        &secrets
            .get_secret("alloy.device.ed25519_key.v1")
            .await
            .expect("the device has an Ed25519 key"),
    )
    .try_into()
    .expect("the Ed25519 seed is 32 bytes");
    SigningKey::from_bytes(&seed).verifying_key()
}

/// Writes both sides of an enrolment: each roster names the other device with its wrap target.
async fn enrol(
    a: &AppEngine,
    a_dir: &Path,
    a_secrets: &Arc<dyn SecretStore>,
    b: &AppEngine,
    b_dir: &Path,
    b_secrets: &Arc<dyn SecretStore>,
) -> (String, String) {
    let (a_id, a_pub) = device_identity(a, a_secrets).await;
    let (b_id, b_pub) = device_identity(b, b_secrets).await;
    add_roster_peer(a_dir, &b_id, &b_pub);
    add_roster_peer(b_dir, &a_id, &a_pub);
    (a_id, b_id)
}

/// The wrap blobs this device published under `epoch`.
fn indexed_wraps(dir: &Path, epoch: u32) -> Vec<String> {
    indexed_blobs(dir, epoch)
        .into_iter()
        .filter(|(_, layer, _)| layer == "device-key")
        .map(|(id, _, _)| id)
        .collect()
}

async fn blob_status(base: &str, id: &str) -> reqwest::StatusCode {
    reqwest::Client::new()
        .get(format!("{base}/v1/blob/{id}"))
        .bearer_auth(TOKEN)
        .send()
        .await
        .expect("get blob")
        .status()
}

async fn blob_bytes(base: &str, id: &str) -> Vec<u8> {
    let response = reqwest::Client::new()
        .get(format!("{base}/v1/blob/{id}"))
        .bearer_auth(TOKEN)
        .send()
        .await
        .expect("get blob");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    response.bytes().await.expect("blob body").to_vec()
}

async fn stored_master(secrets: &Arc<dyn SecretStore>) -> MasterKey {
    let text = secrets
        .get_secret("alloy.sync.master_key.v1")
        .await
        .expect("this device has a master key");
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(text.trim())
        .expect("the master key is base64");
    MasterKey::from_bytes(bytes.try_into().expect("the master key is 32 bytes"))
}

// === Recovery helpers ===

async fn create_phrase(engine: &AppEngine) -> Vec<String> {
    match engine
        .handle(EngineCommand::CreateRecoveryPhrase)
        .await
        .expect("create the recovery phrase")
    {
        EngineResponse::RecoveryPhraseCreated { words } => words,
        other => panic!("unexpected response: {other:?}"),
    }
}

async fn recover(
    engine: &AppEngine,
    words: &[String],
    url: &str,
    token: &str,
) -> Result<
    (
        Vec<alloy_protocol::SyncLayerStatus>,
        Vec<alloy_protocol::DeviceDto>,
    ),
    EngineError,
> {
    match engine
        .handle(EngineCommand::RecoverFromPhrase {
            words: words.to_vec(),
            url: url.to_string(),
            token: token.to_string(),
            accept_new_pin: false,
        })
        .await?
    {
        EngineResponse::Recovered { layers, devices } => Ok((layers, devices)),
        other => panic!("unexpected response: {other:?}"),
    }
}

/// The device ids in this device's roster, and the one marked current.
async fn roster_ids(engine: &AppEngine) -> (Vec<String>, Option<String>) {
    match engine
        .handle(EngineCommand::ListDevices)
        .await
        .expect("list devices")
    {
        EngineResponse::DevicesList { devices } => {
            let current = devices
                .iter()
                .find(|device| device.current)
                .map(|device| device.device_id.clone());
            (
                devices.into_iter().map(|device| device.device_id).collect(),
                current,
            )
        }
        other => panic!("unexpected response: {other:?}"),
    }
}

/// Every file under `dir` with its size, sorted: a before/after comparison proves that a failed
/// recovery wrote not one byte.
fn dir_snapshot(dir: &Path) -> Vec<(String, u64)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(path) = stack.pop() {
        for entry in std::fs::read_dir(&path).expect("read the data dir") {
            let entry = entry.expect("dir entry");
            let meta = entry.metadata().expect("entry metadata");
            let relative = entry
                .path()
                .strip_prefix(dir)
                .expect("entry is under the data dir")
                .display()
                .to_string();
            if meta.is_dir() {
                stack.push(entry.path());
            } else {
                out.push((relative, meta.len()));
            }
        }
    }
    out.sort();
    out
}

async fn read_escrow(base: &str) -> Option<(Vec<u8>, String)> {
    let response = reqwest::Client::new()
        .get(format!("{base}/v1/escrow"))
        .bearer_auth(TOKEN)
        .send()
        .await
        .expect("get escrow");
    match response.status() {
        reqwest::StatusCode::OK => {
            let etag = response
                .headers()
                .get(reqwest::header::ETAG)
                .and_then(|value| value.to_str().ok())
                .expect("escrow etag")
                .to_string();
            Some((response.bytes().await.expect("escrow body").to_vec(), etag))
        }
        reqwest::StatusCode::NOT_FOUND => None,
        other => panic!("unexpected escrow status {other}"),
    }
}

async fn write_escrow_create(base: &str, bytes: &[u8]) {
    let response = reqwest::Client::new()
        .put(format!("{base}/v1/escrow"))
        .bearer_auth(TOKEN)
        .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
        .header(reqwest::header::IF_NONE_MATCH, "*")
        .body(bytes.to_vec())
        .send()
        .await
        .expect("put escrow");
    assert_eq!(response.status(), reqwest::StatusCode::NO_CONTENT);
}

/// A loopback proxy that, on the first `PUT /v1/escrow`, lands a competing CAS write upstream
/// before forwarding the client's write. The client must lose that one, re-read and win the next.
struct EscrowRaceProxy {
    child: Child,
    port: u16,
    marker: PathBuf,
    _dir: TempDir,
}

impl Drop for EscrowRaceProxy {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

impl EscrowRaceProxy {
    fn raced(&self) -> bool {
        self.marker.exists()
    }
}

async fn start_escrow_race_proxy(target_port: u16) -> EscrowRaceProxy {
    let dir = TempDir::new().expect("proxy dir");
    let marker = dir.path().join("raced.log");
    let script = dir.path().join("escrow-race.mjs");
    std::fs::write(
        &script,
        r#"
import http from 'node:http';
import fs from 'node:fs';
const port = Number(process.env.PROXY_PORT);
const target = Number(process.env.TARGET_PORT);
const marker = process.env.RACE_FILE;
const token = process.env.TOKEN;
let raced = false;

function upstream(method, path, headers, body) {
  return new Promise((resolve, reject) => {
    const req = http.request(
      { host: '127.0.0.1', port: target, path, method, headers },
      (res) => {
        const chunks = [];
        res.on('data', (chunk) => chunks.push(chunk));
        res.on('end', () =>
          resolve({ status: res.statusCode, headers: res.headers, body: Buffer.concat(chunks) }),
        );
      },
    );
    req.on('error', reject);
    req.end(body);
  });
}

const server = http.createServer((req, res) => {
  const chunks = [];
  req.on('data', (chunk) => chunks.push(chunk));
  req.on('end', async () => {
    const body = Buffer.concat(chunks);
    if (req.method === 'PUT' && req.url === '/v1/escrow' && !raced) {
      raced = true;
      const auth = {
        authorization: `Bearer ${token}`,
        'content-type': 'application/octet-stream',
      };
      const current = await upstream('GET', '/v1/escrow', auth);
      if (current.status === 200) {
        await upstream(
          'PUT',
          '/v1/escrow',
          { ...auth, 'if-match': current.headers.etag },
          Buffer.from('competitor-escrow'),
        );
        fs.appendFileSync(marker, 'raced\n');
      }
    }
    const answer = await upstream(req.method, req.url, req.headers, body);
    res.writeHead(answer.status, answer.headers);
    res.end(answer.body);
  });
});
server.listen(port, '127.0.0.1', () => console.log(`listening ${server.address().port}`));
"#,
    )
    .expect("write the escrow race proxy");
    let (child, port) = spawn_node_script(
        &script,
        &[
            ("TARGET_PORT", target_port.to_string()),
            ("PROXY_PORT", "0".to_string()),
            ("RACE_FILE", marker.display().to_string()),
            ("TOKEN", TOKEN.to_string()),
        ],
    )
    .await;
    EscrowRaceProxy {
        child,
        port,
        marker,
        _dir: dir,
    }
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

#[tokio::test]
async fn the_phrase_alone_rebuilds_settings_tags_and_the_roster() {
    if !node_available().await {
        eprintln!("skipping: node is not on PATH, so the real sync server cannot be spawned");
        return;
    }
    let server = start_server().await;
    let first_dir = TempDir::new().expect("first data dir");
    let first_vault = vault();
    let first = open_engine(first_dir.path(), first_vault.clone()).await;
    seed_chat(&first, Some("work")).await;
    write_settings(
        first_dir.path(),
        &json!({"density": "compact", "accent": "blue", "window": {"width": 900}}),
    );
    configure(&first, &server.base(), TOKEN, false)
        .await
        .expect("the first device pins and syncs");
    let first_master = first_vault
        .get_secret("alloy.sync.master_key.v1")
        .await
        .expect("the first device has a master key");
    let words = create_phrase(&first).await;

    // The escrow is on the server, and it is ciphertext: the phrase and the master key are not
    // in it.
    let (escrow, _etag) = read_escrow(&server.base())
        .await
        .expect("creating a phrase publishes the escrow");
    let wire = String::from_utf8_lossy(&escrow);
    assert!(!wire.contains(&words.join(" ")), "the phrase never travels");
    assert!(
        !wire.contains(&first_master),
        "the master key never travels"
    );
    let parsed: Value = serde_json::from_slice(&escrow).expect("the escrow is a JSON blob");
    assert_eq!(parsed["kdf"], json!("argon2id"));
    assert_eq!(parsed["version"], json!(1));
    assert!(
        !parsed["ciphertext_b64"]
            .as_str()
            .unwrap_or_default()
            .is_empty()
    );

    // A completely fresh data directory with the phrase, the URL and the token: nothing else.
    let second_dir = TempDir::new().expect("second data dir");
    let second_vault = vault();
    let second = open_engine(second_dir.path(), second_vault.clone()).await;
    let second_chat = seed_chat(&second, None).await;
    let (layers, devices) = recover(&second, &words, &server.base(), TOKEN)
        .await
        .expect("the phrase alone recovers the device");

    let settings = read_settings(second_dir.path());
    assert_eq!(settings["density"], json!("compact"));
    assert_eq!(settings["accent"], json!("blue"));
    assert_eq!(
        settings["window"],
        Value::Null,
        "device-local settings never cross the server"
    );
    assert_eq!(
        chat_tags(&second, &second_chat).await,
        vec!["work".to_string()]
    );
    assert_eq!(
        second_vault
            .get_secret("alloy.sync.master_key.v1")
            .await
            .expect("the recovered master key is stored"),
        first_master,
        "recovery adopts exactly the master key the escrow wrapped"
    );
    assert!(
        layers
            .iter()
            .any(|layer| layer.key == "settings" && layer.enabled),
        "the recovered layers are reported as enabled"
    );
    assert!(
        layers
            .iter()
            .any(|layer| layer.key == "messages" && !layer.enabled)
    );

    let (first_ids, first_current) = roster_ids(&first).await;
    let (second_ids, second_current) = roster_ids(&second).await;
    assert_eq!(
        first_ids.len(),
        1,
        "the first device is alone in its roster"
    );
    assert_eq!(
        second_ids.len(),
        2,
        "the recovered roster names both devices"
    );
    assert!(
        second_ids.contains(&first_current.clone().expect("the first device is current")),
        "the roster names the device whose signed head was followed"
    );
    assert!(
        second_ids.contains(&second_current.clone().expect("one entry is current")),
        "the roster names the recovering device"
    );
    assert_eq!(
        devices.len(),
        2,
        "the response reports the recovered roster"
    );

    // Recovery leaves a working configuration: a later sync is a no-op, not an error.
    let (uploaded, _, _, _) = sync(&second).await.expect("the recovered device syncs");
    assert_eq!(uploaded, 0, "nothing changed, so nothing is uploaded");
}

#[tokio::test]
async fn a_wrong_phrase_writes_nothing_on_a_fresh_device() {
    if !node_available().await {
        eprintln!("skipping: node is not on PATH, so the real sync server cannot be spawned");
        return;
    }
    let server = start_server().await;
    let first_dir = TempDir::new().expect("first data dir");
    let first = open_engine(first_dir.path(), vault()).await;
    seed_chat(&first, Some("work")).await;
    configure(&first, &server.base(), TOKEN, false)
        .await
        .expect("configure");
    let words = create_phrase(&first).await;

    let mut wrong = words.clone();
    let replacement = if wrong[7] == "abandon" {
        "ability"
    } else {
        "abandon"
    };
    wrong[7] = replacement.to_string();

    let fresh_dir = TempDir::new().expect("fresh data dir");
    let fresh_vault = vault();
    let fresh = open_engine(fresh_dir.path(), fresh_vault.clone()).await;
    let before = dir_snapshot(fresh_dir.path());

    let error = recover(&fresh, &wrong, &server.base(), TOKEN)
        .await
        .expect_err("a wrong phrase must not recover");
    assert_eq!(error.code, "recovery_rejected");
    assert!(
        !error.message.contains(replacement),
        "the message names no word: {}",
        error.message
    );
    assert!(
        !error.message.contains(&words[7]),
        "the message names no word: {}",
        error.message
    );

    assert_eq!(
        dir_snapshot(fresh_dir.path()),
        before,
        "a wrong phrase writes nothing, not even a settings file or a head cursor"
    );
    assert!(!fresh_dir.path().join("settings.json").exists());
    assert!(!fresh_dir.path().join("sync-heads.json").exists());
    assert!(
        fresh_vault
            .get_secret("alloy.sync.master_key.v1")
            .await
            .is_err(),
        "a wrong phrase must not create a master key"
    );
    assert!(
        fresh_vault
            .get_secret("alloy.sync.server_token.v1")
            .await
            .is_err(),
        "a wrong phrase must not store the token"
    );
}

#[tokio::test]
async fn a_second_fresh_device_recovers_with_the_same_phrase() {
    if !node_available().await {
        eprintln!("skipping: node is not on PATH, so the real sync server cannot be spawned");
        return;
    }
    let server = start_server().await;
    let first_dir = TempDir::new().expect("first data dir");
    let first = open_engine(first_dir.path(), vault()).await;
    seed_chat(&first, Some("work")).await;
    write_settings(first_dir.path(), &json!({"density": "compact"}));
    configure(&first, &server.base(), TOKEN, false)
        .await
        .expect("configure");
    let words = create_phrase(&first).await;

    for round in 0..2 {
        let dir = TempDir::new().expect("fresh data dir");
        let engine = open_engine(dir.path(), vault()).await;
        seed_chat(&engine, None).await;
        let (_, devices) = recover(&engine, &words, &server.base(), TOKEN)
            .await
            .unwrap_or_else(|error| panic!("recovery {round} failed: {error:?}"));
        assert_eq!(read_settings(dir.path())["density"], json!("compact"));
        assert_eq!(devices.len(), 2, "recovery {round} names both devices");
    }
}

#[tokio::test]
async fn the_escrow_upload_survives_a_competing_cas() {
    if !node_available().await {
        eprintln!("skipping: node is not on PATH, so the real sync server cannot be spawned");
        return;
    }
    let server = start_server().await;
    let proxy = start_escrow_race_proxy(server.port).await;
    let url = format!("http://127.0.0.1:{}", proxy.port);
    // An existing (foreign) escrow makes the first engine write a matching CAS, not a create.
    write_escrow_create(&server.base(), b"seed-escrow").await;

    let dir = TempDir::new().expect("data dir");
    let engine = open_engine(dir.path(), vault()).await;
    seed_chat(&engine, Some("work")).await;
    configure(&engine, &url, TOKEN, false)
        .await
        .expect("configure through the proxy");
    let words = create_phrase(&engine).await;

    assert!(
        proxy.raced(),
        "the proxy must have landed a competing write"
    );
    let (escrow, _) = read_escrow(&server.base())
        .await
        .expect("the retried write published the escrow");
    assert_ne!(
        escrow, b"competitor-escrow",
        "the loser of the first CAS re-read and retried instead of giving up"
    );
    let parsed: Value = serde_json::from_slice(&escrow).expect("the escrow is a JSON blob");
    assert_eq!(parsed["kdf"], json!("argon2id"));

    // The published blob is the one this device can open with the phrase it just returned.
    let fresh_dir = TempDir::new().expect("fresh data dir");
    let fresh = open_engine(fresh_dir.path(), vault()).await;
    recover(&fresh, &words, &server.base(), TOKEN)
        .await
        .expect("the retried escrow is the one the phrase opens");
}

#[tokio::test]
async fn revocation_rotates_the_key_and_deletes_the_old_ciphertext() {
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
    seed_chat(&second, None).await;
    write_settings(first_dir.path(), &json!({"density": "compact"}));

    configure(&first, &server.base(), TOKEN, false)
        .await
        .expect("the first device publishes seq 1");
    let (_, revoked_id) = roster_ids(&second).await;
    let revoked_id = revoked_id.expect("the second device names itself");
    copy_master(&first_vault, &second_vault).await;
    configure(&second, &server.base(), TOKEN, false)
        .await
        .expect("the second device pulls seq 1");
    add_roster_entry(first_dir.path(), &revoked_id);

    // Every id the first device published under the old epoch is readable before the revoke.
    let old = indexed_blobs(first_dir.path(), 0);
    assert_eq!(old.len(), 5, "one id per enabled layer");
    for (id, _, _) in &old {
        assert_eq!(
            blob_status(&server.base(), id).await,
            reqwest::StatusCode::OK
        );
    }

    let revoked = revoke(&first, &revoked_id).await;
    assert_eq!(revoked.epoch, 1);
    assert_eq!(revoked.re_sealed, Some(true));
    assert_eq!(revoked.blobs_deleted, Some(old.len() as u32));
    assert_eq!(revoked.blobs_remaining, Some(0));
    assert_eq!(revoked.recovery_stale, Some(true));
    assert!(revoked.error.is_none(), "{:?}", revoked.error);

    // A real 404: the old ciphertext is gone from the real server, not merely orphaned.
    for (id, _, _) in &old {
        let status = blob_status(&server.base(), id).await;
        eprintln!("revoked epoch's blob {id} -> {status}");
        assert_eq!(
            status,
            reqwest::StatusCode::NOT_FOUND,
            "revoked blob {id} is still served"
        );
    }

    let new = indexed_blobs(first_dir.path(), 1);
    assert_eq!(new.len(), 5, "every enabled layer was re-sealed");
    for (id, _, _) in &new {
        assert_eq!(
            blob_status(&server.base(), id).await,
            reqwest::StatusCode::OK
        );
    }

    // The revoked device holds the old master key. It cannot name the new epoch's key at all,
    // and guessing the epoch gives a key that fails authentication.
    let settings = new
        .iter()
        .find(|(_, layer, _)| layer == "settings")
        .expect("the settings layer was re-sealed");
    let blob = blob_bytes(&server.base(), &settings.0).await;
    let addr = BlobAddr::new(Layer::Config, "settings", settings.2, 1);
    let old_master = stored_master(&second_vault).await;
    let error = open(&BlobKeyRing::new(&old_master, [0u32]), &addr, &blob)
        .expect_err("the old key set does not contain the new epoch");
    assert_eq!(error, BlobError::UnknownEpoch { epoch: 1 });
    let error = open(&BlobKeyRing::new(&old_master, [0u32, 1]), &addr, &blob)
        .expect_err("the old master cannot derive the rotated key");
    assert_eq!(error, BlobError::AuthenticationFailed);

    // Following the head fails the same way: nothing sealed after the revoke opens.
    let error = sync(&second)
        .await
        .expect_err("the revoked device cannot read the new epoch");
    assert_eq!(error.code, "sync_blocked");
    assert_eq!(error.reason.as_deref(), Some("content"));

    let (ids, current) = roster_ids(&first).await;
    assert!(!ids.contains(&revoked_id), "the revoked entry is gone");
    assert!(current.is_some(), "this device stays in its own roster");
}

#[tokio::test]
async fn a_remaining_device_converges_on_the_re_sealed_layers() {
    if !node_available().await {
        eprintln!("skipping: node is not on PATH, so the real sync server cannot be spawned");
        return;
    }
    let server = start_server().await;
    let first_dir = TempDir::new().expect("first data dir");
    let revoked_dir = TempDir::new().expect("revoked data dir");
    let remaining_dir = TempDir::new().expect("remaining data dir");
    let first_vault = vault();
    let revoked_vault = vault();
    let remaining_vault = vault();
    let first = open_engine(first_dir.path(), first_vault.clone()).await;
    let revoked = open_engine(revoked_dir.path(), revoked_vault.clone()).await;
    let remaining = open_engine(remaining_dir.path(), remaining_vault.clone()).await;
    seed_chat(&first, Some("work")).await;
    seed_chat(&revoked, None).await;
    let remaining_chat = seed_chat(&remaining, None).await;
    write_settings(first_dir.path(), &json!({"density": "compact"}));

    configure(&first, &server.base(), TOKEN, false)
        .await
        .expect("the first device publishes seq 1");
    let (_, revoked_id) = roster_ids(&revoked).await;
    let revoked_id = revoked_id.expect("the revoked device names itself");
    // Both peers start with the pre-rotation master, as enrolment would have left them.
    copy_master(&first_vault, &revoked_vault).await;
    copy_master(&first_vault, &remaining_vault).await;
    configure(&revoked, &server.base(), TOKEN, false)
        .await
        .expect("the revoked device pulls seq 1");
    configure(&remaining, &server.base(), TOKEN, false)
        .await
        .expect("the remaining device pulls seq 1");
    add_roster_entry(first_dir.path(), &revoked_id);

    let outcome = revoke(&first, &revoked_id).await;
    assert_eq!(outcome.epoch, 1);
    assert_eq!(outcome.re_sealed, Some(true));

    // With the pre-rotation key the remaining device cannot open the re-sealed head.
    let error = sync(&remaining)
        .await
        .expect_err("the old key does not open the new epoch");
    assert_eq!(error.code, "sync_blocked");
    assert_eq!(error.reason.as_deref(), Some("content"));

    // A surviving device that obtained the rotated key reads the re-sealed layers and follows
    // the head at the new epoch from then on.
    copy_master(&first_vault, &remaining_vault).await;
    merge_settings(first_dir.path(), &json!({"accent": "green"}));
    sync(&first).await.expect("the surviving device publishes");
    sync(&remaining)
        .await
        .expect("the remaining device converges");
    assert_eq!(
        read_settings(remaining_dir.path())["accent"],
        json!("green")
    );
    assert_eq!(
        chat_tags(&remaining, &remaining_chat).await,
        vec!["work".to_string()]
    );
    eprintln!(
        "remaining device converged at epoch {}: accent={} tags={:?}",
        read_settings(remaining_dir.path())["sync"]["epoch"],
        read_settings(remaining_dir.path())["accent"],
        chat_tags(&remaining, &remaining_chat).await
    );
    assert_eq!(
        read_settings(remaining_dir.path())["sync"]["epoch"],
        json!(1)
    );

    sync(&remaining)
        .await
        .expect("the remaining device publishes");
    let (head, _) = read_head(&server.base()).await;
    assert_eq!(head_epoch(&head), 1, "publishing follows the adopted epoch");
}

#[tokio::test]
async fn a_server_that_refuses_delete_keeps_the_revocation_and_reports_leftovers() {
    if !node_available().await {
        eprintln!("skipping: node is not on PATH, so the real sync server cannot be spawned");
        return;
    }
    let server = start_server().await;
    let proxy = start_delete_refusing_proxy(server.port).await;
    let url = format!("http://127.0.0.1:{}", proxy.port);
    let first_dir = TempDir::new().expect("first data dir");
    let revoked_dir = TempDir::new().expect("revoked data dir");
    let first_vault = vault();
    let revoked_vault = vault();
    let first = open_engine(first_dir.path(), first_vault.clone()).await;
    let revoked = open_engine(revoked_dir.path(), revoked_vault.clone()).await;
    seed_chat(&first, Some("work")).await;
    seed_chat(&revoked, None).await;
    write_settings(first_dir.path(), &json!({"density": "compact"}));

    configure(&first, &url, TOKEN, false)
        .await
        .expect("the first device publishes through the proxy");
    let (_, revoked_id) = roster_ids(&revoked).await;
    let revoked_id = revoked_id.expect("the revoked device names itself");
    copy_master(&first_vault, &revoked_vault).await;
    configure(&revoked, &server.base(), TOKEN, false)
        .await
        .expect("the revoked device pulls");
    add_roster_entry(first_dir.path(), &revoked_id);

    let old = indexed_blobs(first_dir.path(), 0);
    assert_eq!(old.len(), 5);

    let outcome = revoke(&first, &revoked_id).await;
    assert_eq!(outcome.epoch, 1, "the revocation stands");
    assert_eq!(outcome.re_sealed, Some(true), "the new epoch is published");
    assert_eq!(outcome.blobs_deleted, Some(0));
    assert_eq!(outcome.blobs_remaining, Some(old.len() as u32));
    let error = outcome
        .error
        .expect("an incomplete sweep is reported, not hidden");
    assert!(
        error.contains("left on the server"),
        "the report names the leftovers: {error}"
    );

    // The delete really failed: the old ciphertext is still there, which is exactly what the
    // response said.
    for (id, _, _) in &old {
        assert_eq!(
            blob_status(&server.base(), id).await,
            reqwest::StatusCode::OK,
            "the refused delete must leave the blob in place"
        );
    }
    let (head, _) = read_head(&server.base()).await;
    assert_eq!(head_epoch(&head), 1);
    let (ids, _) = roster_ids(&first).await;
    assert!(!ids.contains(&revoked_id));

    // The epoch, not the deletion, is the boundary.
    let error = sync(&revoked)
        .await
        .expect_err("the revoked device cannot read the new epoch");
    assert_eq!(error.code, "sync_blocked");
    assert_eq!(error.reason.as_deref(), Some("content"));
}

// === Device-key distribution across a rotation ===

/// The hole this slice closes: revocation rotates the master, and every survivor must be able to
/// follow it without the user doing anything. Three devices enrolled; one revoked; the two
/// survivors sync once and hold the same master bytes as the revoker.
#[tokio::test]
async fn both_survivors_adopt_the_rotated_key_without_user_action() {
    if !node_available().await {
        eprintln!("skipping: node is not on PATH, so the real sync server cannot be spawned");
        return;
    }
    let server = start_server().await;
    let a_dir = TempDir::new().expect("a dir");
    let b_dir = TempDir::new().expect("b dir");
    let c_dir = TempDir::new().expect("c dir");
    let a_vault = vault();
    let b_vault = vault();
    let c_vault = vault();
    let a = open_engine(a_dir.path(), a_vault.clone()).await;
    let b = open_engine(b_dir.path(), b_vault.clone()).await;
    let c = open_engine(c_dir.path(), c_vault.clone()).await;
    let a_chat = seed_chat(&a, Some("work")).await;
    let b_chat = seed_chat(&b, None).await;
    seed_chat(&c, None).await;
    write_settings(a_dir.path(), &json!({"accent": "green"}));

    configure(&a, &server.base(), TOKEN, false)
        .await
        .expect("the revoker publishes seq 1");
    let (a_id, b_id) = enrol(&a, a_dir.path(), &a_vault, &b, b_dir.path(), &b_vault).await;
    let (_, c_id) = enrol(&a, a_dir.path(), &a_vault, &c, c_dir.path(), &c_vault).await;
    copy_master(&a_vault, &b_vault).await;
    copy_master(&a_vault, &c_vault).await;
    configure(&b, &server.base(), TOKEN, false)
        .await
        .expect("b pulls epoch 0");
    configure(&c, &server.base(), TOKEN, false)
        .await
        .expect("c pulls epoch 0");
    assert_ne!(a_id, b_id);

    let outcome = revoke(&a, &c_id).await;
    assert_eq!(outcome.epoch, 1);
    assert_eq!(outcome.re_sealed, Some(true));
    assert_eq!(outcome.blobs_remaining, Some(0));
    assert_eq!(outcome.recovery_stale, Some(true));
    assert!(outcome.error.is_none(), "{:?}", outcome.error);

    // One wrap per surviving device, addressed to it, booked under the new epoch.
    let wraps = indexed_wraps(a_dir.path(), 1);
    assert_eq!(wraps.len(), 1, "only b survives: {wraps:?}");
    for id in &wraps {
        assert_eq!(
            blob_status(&server.base(), id).await,
            reqwest::StatusCode::OK
        );
    }

    // B does nothing but sync. It finds its wrap, adopts the rotated master and pulls the layers.
    let (_, _, head_hex, epoch) = sync(&b).await.expect("b adopts the rotation on its own");
    assert_eq!(epoch, 1);
    assert_eq!(head_epoch(&read_head(&server.base()).await.0), epoch);
    assert!(!head_hex.is_empty());
    assert_eq!(
        stored_master(&b_vault).await.expose_secret(),
        stored_master(&a_vault).await.expose_secret(),
        "both devices hold the same rotated master bytes"
    );
    assert_eq!(
        read_settings(b_dir.path())["accent"],
        json!("green"),
        "the survivor reads the re-sealed settings"
    );

    // B's local copy of the shared chat carries the revoker's tag after the import.
    assert_eq!(
        chat_tags(&b, &b_chat).await,
        vec!["work".to_string()],
        "the survivor imported the re-sealed tags"
    );
    assert_eq!(chat_tags(&a, &a_chat).await, vec!["work".to_string()]);

    // B publishes on top of the adopted epoch; A follows the head without touching the key.
    sync(&b).await.expect("b publishes at the new epoch");
    sync(&a).await.expect("a converges at the new epoch");
    assert_eq!(
        stored_master(&a_vault).await.expose_secret(),
        stored_master(&b_vault).await.expose_secret()
    );
    assert_eq!(read_settings(b_dir.path())["sync"]["epoch"], json!(1));
}

/// The revoked device keeps the pre-rotation master. It finds no wrap addressed to it, so the
/// next sync is the same content failure as before, and the surviving device's ciphertext does
/// not open with the revoked device's key.
#[tokio::test]
async fn the_revoked_device_cannot_reach_the_rotated_key() {
    if !node_available().await {
        eprintln!("skipping: node is not on PATH, so the real sync server cannot be spawned");
        return;
    }
    let server = start_server().await;
    let a_dir = TempDir::new().expect("a dir");
    let b_dir = TempDir::new().expect("b dir");
    let c_dir = TempDir::new().expect("c dir");
    let a_vault = vault();
    let b_vault = vault();
    let c_vault = vault();
    let a = open_engine(a_dir.path(), a_vault.clone()).await;
    let b = open_engine(b_dir.path(), b_vault.clone()).await;
    let c = open_engine(c_dir.path(), c_vault.clone()).await;
    seed_chat(&a, Some("work")).await;
    seed_chat(&b, None).await;
    seed_chat(&c, None).await;
    write_settings(a_dir.path(), &json!({"accent": "green"}));

    configure(&a, &server.base(), TOKEN, false)
        .await
        .expect("configure a");
    let _ = enrol(&a, a_dir.path(), &a_vault, &b, b_dir.path(), &b_vault).await;
    let (_, c_id) = enrol(&a, a_dir.path(), &a_vault, &c, c_dir.path(), &c_vault).await;
    copy_master(&a_vault, &b_vault).await;
    copy_master(&a_vault, &c_vault).await;
    configure(&b, &server.base(), TOKEN, false)
        .await
        .expect("configure b");
    configure(&c, &server.base(), TOKEN, false)
        .await
        .expect("configure c");

    let revoked = revoke(&a, &c_id).await;
    assert_eq!(revoked.epoch, 1);
    assert!(revoked.error.is_none(), "{:?}", revoked.error);
    assert!(
        !indexed_wraps(a_dir.path(), 1).is_empty(),
        "the survivor's wrap exists"
    );

    let error = sync(&c)
        .await
        .expect_err("the revoked device cannot read the new epoch");
    assert_eq!(error.code, "sync_blocked");
    assert_eq!(error.reason.as_deref(), Some("content"));
    assert_ne!(
        stored_master(&c_vault).await.expose_secret(),
        stored_master(&a_vault).await.expose_secret(),
        "the revoked device still holds the old master"
    );

    // The surviving device's wrap is on the server; it does not open with the revoked device's
    // key and id, even with the right epoch and the signing device's key.
    let wrap_id = indexed_wraps(a_dir.path(), 1)
        .into_iter()
        .next()
        .expect("one wrap");
    let wrap = blob_bytes(&server.base(), &wrap_id).await;
    let (c_own_id, _) = device_identity(&c, &c_vault).await;
    let error = open_master(
        &device_secret(&c_vault).await,
        &c_own_id,
        1,
        &verifying_key(&a_vault).await,
        &wrap,
    )
    .expect_err("a wrap addressed to another device does not open");
    assert_eq!(error, DeviceWrapError::WrongRecipient);
}

/// A device that was never enrolled still holds the pre-rotation master (an old pairing). Its id
/// is not on the revoker's roster, so no wrap is addressed to it; replaying a survivor's wrap
/// under its own name fails, and its sync stays blocked.
#[tokio::test]
async fn a_wrap_replayed_for_a_device_outside_the_roster_is_refused() {
    if !node_available().await {
        eprintln!("skipping: node is not on PATH, so the real sync server cannot be spawned");
        return;
    }
    let server = start_server().await;
    let a_dir = TempDir::new().expect("a dir");
    let b_dir = TempDir::new().expect("b dir");
    let outsider_dir = TempDir::new().expect("outsider dir");
    let a_vault = vault();
    let b_vault = vault();
    let outsider_vault = vault();
    let a = open_engine(a_dir.path(), a_vault.clone()).await;
    let b = open_engine(b_dir.path(), b_vault.clone()).await;
    let outsider = open_engine(outsider_dir.path(), outsider_vault.clone()).await;
    seed_chat(&a, Some("work")).await;
    seed_chat(&b, None).await;
    seed_chat(&outsider, None).await;

    configure(&a, &server.base(), TOKEN, false)
        .await
        .expect("configure a");
    let _ = enrol(&a, a_dir.path(), &a_vault, &b, b_dir.path(), &b_vault).await;
    copy_master(&a_vault, &b_vault).await;
    copy_master(&a_vault, &outsider_vault).await;
    configure(&b, &server.base(), TOKEN, false)
        .await
        .expect("configure b");
    configure(&outsider, &server.base(), TOKEN, false)
        .await
        .expect("the outsider pulls epoch 0");

    // The outsider is not on the revoker's roster. Enrol a third device and revoke it, so the
    // only wrap produced is for b.
    let c_dir = TempDir::new().expect("c dir");
    let c_vault = vault();
    let c = open_engine(c_dir.path(), c_vault.clone()).await;
    seed_chat(&c, None).await;
    let (_, c_id) = enrol(&a, a_dir.path(), &a_vault, &c, c_dir.path(), &c_vault).await;
    copy_master(&a_vault, &c_vault).await;
    configure(&c, &server.base(), TOKEN, false)
        .await
        .expect("configure c");
    let revoked = revoke(&a, &c_id).await;
    assert_eq!(revoked.epoch, 1, "revocation of c");
    let wraps = indexed_wraps(a_dir.path(), 1);
    assert_eq!(wraps.len(), 1, "one wrap, for b");
    let wrap = blob_bytes(&server.base(), &wraps[0]).await;

    // Replaying that ciphertext against the outsider's key and id does not open it.
    let (outsider_id, _) = device_identity(&outsider, &outsider_vault).await;
    let error = open_master(
        &device_secret(&outsider_vault).await,
        &outsider_id,
        1,
        &verifying_key(&a_vault).await,
        &wrap,
    )
    .expect_err("the ciphertext is bound to the enrolled survivor");
    assert_eq!(error, DeviceWrapError::WrongRecipient);

    let error = sync(&outsider)
        .await
        .expect_err("a device outside the roster cannot follow the rotation");
    assert_eq!(error.code, "sync_blocked");
    assert_eq!(error.reason.as_deref(), Some("content"));
}

/// Two rotations in a row: a survivor that missed the first one never reaches the second, and the
/// first rotation's wrap is stale by epoch, not merely old.
#[tokio::test]
async fn a_second_rotation_makes_the_first_wraps_stale() {
    if !node_available().await {
        eprintln!("skipping: node is not on PATH, so the real sync server cannot be spawned");
        return;
    }
    let server = start_server().await;
    let a_dir = TempDir::new().expect("a dir");
    let b_dir = TempDir::new().expect("b dir");
    let c_dir = TempDir::new().expect("c dir");
    let d_dir = TempDir::new().expect("d dir");
    let a_vault = vault();
    let b_vault = vault();
    let c_vault = vault();
    let d_vault = vault();
    let a = open_engine(a_dir.path(), a_vault.clone()).await;
    let b = open_engine(b_dir.path(), b_vault.clone()).await;
    let c = open_engine(c_dir.path(), c_vault.clone()).await;
    let d = open_engine(d_dir.path(), d_vault.clone()).await;
    seed_chat(&a, Some("work")).await;
    for engine in [&b, &c, &d] {
        seed_chat(engine, None).await;
    }

    configure(&a, &server.base(), TOKEN, false)
        .await
        .expect("configure a");
    let _ = enrol(&a, a_dir.path(), &a_vault, &b, b_dir.path(), &b_vault).await;
    let (b_id, _) = device_identity(&b, &b_vault).await;
    let (_, c_id) = enrol(&a, a_dir.path(), &a_vault, &c, c_dir.path(), &c_vault).await;
    let (_, d_id) = enrol(&a, a_dir.path(), &a_vault, &d, d_dir.path(), &d_vault).await;
    for (engine, secrets) in [(&b, &b_vault), (&c, &c_vault), (&d, &d_vault)] {
        copy_master(&a_vault, secrets).await;
        configure(engine, &server.base(), TOKEN, false)
            .await
            .expect("configure a peer");
    }

    // First rotation: b and d get a copy at epoch 1. b does not sync.
    let first = revoke(&a, &c_id).await;
    assert_eq!(first.epoch, 1);
    assert_eq!(indexed_wraps(a_dir.path(), 1).len(), 2, "b and d");
    let b_secret = device_secret(&b_vault).await;
    let a_verifying = verifying_key(&a_vault).await;
    let mut stale: Option<Vec<u8>> = None;
    for id in indexed_wraps(a_dir.path(), 1) {
        let bytes = blob_bytes(&server.base(), &id).await;
        if open_master(&b_secret, &b_id, 1, &a_verifying, &bytes).is_ok() {
            stale = Some(bytes);
        }
    }
    let stale = stale.expect("b's epoch-1 wrap was on the server");

    // Second rotation: d is revoked; b is the only survivor.
    let second = revoke(&a, &d_id).await;
    assert_eq!(second.epoch, 2);
    assert_eq!(indexed_wraps(a_dir.path(), 2).len(), 1, "b only");

    // b is still at epoch 0. The second head's parent is a link b never verified, so the chain
    // refuses it before any key is tried; the epoch-2 wrap is also named under a master b never
    // received. Either way the run blocks and b keeps no rotated key.
    let error = sync(&b)
        .await
        .expect_err("a device that missed a rotation cannot follow the next one");
    assert_eq!(error.code, "sync_blocked");
    assert!(
        matches!(error.reason.as_deref(), Some("fork") | Some("content")),
        "unexpected reason: {:?}",
        error.reason
    );
    assert_ne!(
        stored_master(&b_vault).await.expose_secret(),
        stored_master(&a_vault).await.expose_secret()
    );

    // The captured first-rotation copy is stale: it installs epoch 1, and the head demands 2.
    let error = open_master(&b_secret, &b_id, 2, &a_verifying, &stale)
        .expect_err("an old rotation's wrap is not accepted");
    assert_eq!(
        error,
        DeviceWrapError::WrongEpoch {
            found: 1,
            expected: 2
        }
    );
}

/// The rotation invalidates the recovery phrase, and the engine says so instead of pretending it
/// re-sealed the escrow. Creating a new phrase is the documented remedy and clears the report.
#[tokio::test]
async fn a_rotated_master_reports_the_recovery_phrase_stale_and_a_new_phrase_clears_it() {
    if !node_available().await {
        eprintln!("skipping: node is not on PATH, so the real sync server cannot be spawned");
        return;
    }
    let server = start_server().await;
    let a_dir = TempDir::new().expect("a dir");
    let b_dir = TempDir::new().expect("b dir");
    let a_vault = vault();
    let b_vault = vault();
    let a = open_engine(a_dir.path(), a_vault.clone()).await;
    let b = open_engine(b_dir.path(), b_vault.clone()).await;
    seed_chat(&a, Some("work")).await;
    seed_chat(&b, None).await;
    configure(&a, &server.base(), TOKEN, false)
        .await
        .expect("configure a");
    let _ = create_phrase(&a).await;
    assert_eq!(status(&a).await.recovery_stale, Some(false));

    let (_, b_id) = enrol(&a, a_dir.path(), &a_vault, &b, b_dir.path(), &b_vault).await;
    copy_master(&a_vault, &b_vault).await;
    configure(&b, &server.base(), TOKEN, false)
        .await
        .expect("configure b");

    let outcome = revoke(&a, &b_id).await;
    assert_eq!(outcome.epoch, 1);
    assert_eq!(outcome.recovery_stale, Some(true));
    assert_eq!(status(&a).await.recovery_stale, Some(true));

    let _ = create_phrase(&a).await;
    assert_eq!(
        status(&a).await.recovery_stale,
        Some(false),
        "a phrase created after the rotation wraps the current master"
    );
}

// === Pairing + server journey ===

async fn begin_pairing(engine: &AppEngine) -> (String, String, String) {
    match engine
        .handle(EngineCommand::BeginPairing {
            role: PairingRole::Offer,
        })
        .await
        .expect("begin pairing")
    {
        EngineResponse::PairingStarted {
            pairing_id,
            ticket,
            code,
            ..
        } => (pairing_id, ticket, code),
        other => panic!("unexpected response: {other:?}"),
    }
}

async fn accept_ticket(engine: &AppEngine, ticket: &str, code: &str) -> String {
    match engine
        .handle(EngineCommand::AcceptPairingTicket {
            ticket: ticket.to_string(),
            code: code.to_string(),
        })
        .await
        .expect("accept ticket")
    {
        EngineResponse::PairingStarted { pairing_id, .. } => pairing_id,
        other => panic!("unexpected response: {other:?}"),
    }
}

async fn wait_pairing(
    events: &mut broadcast::Receiver<EngineEvent>,
    pairing_id: &str,
    phase: PairingPhase,
) {
    let deadline = tokio::time::Instant::now() + WAIT;
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        let event = timeout(remaining, events.recv())
            .await
            .unwrap_or_else(|_| panic!("no {phase:?} event for pairing {pairing_id}"))
            .expect("event channel open");
        if let EngineEvent::PairingChanged {
            pairing_id: id,
            state,
            ..
        } = event
            && id == pairing_id
            && state == phase
        {
            return;
        }
    }
}

async fn confirm(engine: &AppEngine, pairing_id: &str) {
    engine
        .handle(EngineCommand::ConfirmPairing {
            pairing_id: pairing_id.to_string(),
            accept: true,
        })
        .await
        .expect("confirm pairing");
}

/// The flagship journey: pair over the LAN, then use the delivered master key to pull the
/// layers this account published to a real sync server before the pairing.
#[tokio::test]
async fn a_paired_device_pulls_the_servers_layers_with_the_delivered_master() {
    if !node_available().await {
        eprintln!("skipping: node is not on PATH, so the real sync server cannot be spawned");
        return;
    }
    let server = start_server().await;
    let offer_dir = TempDir::new().expect("offer dir");
    let offer_secrets = vault();
    let offer = open_engine(offer_dir.path(), offer_secrets.clone()).await;
    let offer_chat = seed_chat(&offer, Some("work")).await;
    write_settings(offer_dir.path(), &json!({"density": "compact"}));
    configure(&offer, &server.base(), TOKEN, false)
        .await
        .expect("configure the offering device");
    sync(&offer)
        .await
        .expect("the offering device publishes its layers");

    let accept_dir = TempDir::new().expect("accept dir");
    let accept_secrets = vault();
    let receiver = open_engine(accept_dir.path(), accept_secrets.clone()).await;
    let chat_id = seed_chat(&receiver, None).await;

    let mut offer_events = offer.sink().subscribe();
    let mut accept_events = receiver.sink().subscribe();
    let (offer_id, ticket, code) = begin_pairing(&offer).await;
    let accept_id = accept_ticket(&receiver, &ticket, &code).await;
    wait_pairing(&mut offer_events, &offer_id, PairingPhase::SasReady).await;
    wait_pairing(&mut accept_events, &accept_id, PairingPhase::SasReady).await;
    confirm(&offer, &offer_id).await;
    confirm(&receiver, &accept_id).await;
    wait_pairing(&mut offer_events, &offer_id, PairingPhase::Done).await;
    wait_pairing(&mut accept_events, &accept_id, PairingPhase::Done).await;

    let offer_master = stored_master(&offer_secrets).await;
    let delivered = stored_master(&accept_secrets).await;
    assert_eq!(
        offer_master.expose_secret(),
        delivered.expose_secret(),
        "the paired device holds the offering device's master bytes"
    );

    // Change the account after the pairing, so a successful pull is the only way the receiver
    // sees the new settings and tag.
    std::thread::sleep(Duration::from_millis(5));
    merge_settings(offer_dir.path(), &json!({"density": "cozy"}));
    offer
        .context()
        .store
        .update_chat_flags(
            &offer_chat,
            ChatFlagsUpdate {
                tags: Some(vec!["later".to_string()]),
                tags_updated_at: Some(1_800_000_000_000),
                ..ChatFlagsUpdate::default()
            },
        )
        .await
        .expect("retag the offering device's chat");
    sync(&offer)
        .await
        .expect("the offering device publishes a second batch");

    configure(&receiver, &server.base(), TOKEN, false)
        .await
        .expect("the paired device joins the same server");
    let (_, downloaded, _, epoch) = sync(&receiver).await.expect("the paired device pulls");

    assert!(
        downloaded > 0,
        "the paired device downloaded the server's layers"
    );
    assert_eq!(epoch, 0, "the delivered epoch matches the head");
    assert_eq!(
        read_settings(accept_dir.path())["density"],
        json!("cozy"),
        "a layer the pairing never carried came from the server"
    );
    let tags = chat_tags(&receiver, &chat_id).await;
    assert_eq!(
        tags,
        vec!["later".to_string()],
        "the newer server tag overrode the paired one"
    );
}
