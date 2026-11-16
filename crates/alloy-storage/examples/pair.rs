//! Headless proof of ADR 0020 slice A: two terminals (or two machines on one LAN) exchange a
//! configuration bundle with no server in the path.
//!
//! ```text
//! # terminal 1
//! cargo run -p alloy-storage --example pair -- offer
//! # terminal 2, with the endpoint, pairing id and code printed above
//! cargo run -p alloy-storage --example pair -- accept \
//!     --qr 'alloy-pairing-v1|…' --code ABCDEFGH --state /tmp/alloy-accept.json
//! ```
//!
//! Both sides print the same six-digit SAS; type `y` on both. The offer sends the bundle, the
//! accept side imports it into `--state` (a JSON `ConfigState`) and prints what changed. `--yes`
//! accepts the SAS without prompting, for scripted runs.

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use alloy_storage::config_bundle::{
    BackgroundAsset, ChatKey, ConfigBundle, ConfigState, ImportReport, TagSet, ThemeSelection,
};
use alloy_storage::pairing::{self, DeviceProver, PairingOffer, PairingTicket};
use ed25519_dalek::{Signer, SigningKey};
use serde_json::json;

/// What the engine supplies in the app: this device's Ed25519 key and a label. The handshake only
/// carries the bytes; whether the peer's proof verifies is the caller's decision, which this demo
/// deliberately skips so it can run without a roster.
struct DemoProver {
    key: SigningKey,
    identity: Vec<u8>,
}

impl DemoProver {
    fn new(label: &str, seed: u8) -> Self {
        let key = SigningKey::from_bytes(&[seed; 32]);
        let mut identity = key.verifying_key().as_bytes().to_vec();
        identity.extend_from_slice(label.as_bytes());
        Self { key, identity }
    }
}

impl DeviceProver for DemoProver {
    fn identity(&self) -> &[u8] {
        &self.identity
    }

    fn sign(&self, transcript: &[u8; 32]) -> [u8; 64] {
        self.key.sign(transcript).to_bytes()
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    if let Err(err) = run().await {
        eprintln!("pair: {err}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let command = args.next().unwrap_or_default();
    let options = Options::parse(args)?;
    match command.as_str() {
        "offer" => offer(&options).await,
        "accept" => accept(&options).await,
        "" | "-h" | "--help" => {
            eprintln!(
                "usage:\n  pair offer [--bind ADDR] [--advertise HOST:PORT] [--label NAME] \
                 [--state FILE] [--yes]\n  pair accept --qr PAYLOAD --code CODE [--state FILE] \
                 [--yes]"
            );
            Ok(())
        }
        other => Err(format!("unknown subcommand {other:?}; expected offer or accept").into()),
    }
}

async fn offer(options: &Options) -> Result<(), Box<dyn std::error::Error>> {
    let label = options.get("label").unwrap_or("desk");
    let state = match options.get("state") {
        Some(path) => load_state(Path::new(path))?,
        None => demo_state(now_ms()),
    };
    let bundle = state.export(label, now_ms());
    let summary = bundle_summary(&bundle);

    let bind: SocketAddr = options
        .get("bind")
        .unwrap_or("127.0.0.1:0")
        .parse()
        .map_err(|e| format!("--bind is not an address: {e}"))?;
    let offer = PairingOffer::bind(
        bundle,
        bind,
        options.get("advertise").map(str::to_string),
        Arc::new(DemoProver::new(label, 1)),
    )
    .await?;
    let local = offer.local_addr()?;
    let advertised = advertised_endpoint(local, options.get("advertise"));

    println!("pairing code: {}", offer.code());
    println!("pairing id:   {}", offer.ticket().pairing_id);
    println!("endpoint:     {advertised}");
    println!("qr payload:   {}", offer.qr_payload());
    println!("bundle:       {summary}");
    println!("waiting for the other device (90 s)...");

    let handshake = offer.wait_for_peer().await?;
    println!("\nshort authentication string: {}", handshake.sas());
    println!("compare those six digits with the other screen");
    let accepted = confirm(options.yes, "send the bundle?")?;
    match handshake.finish(accepted).await {
        Ok(peer) => println!(
            "bundle sent; the peer proved identity {} ({} bytes). Nothing was uploaded anywhere.",
            hex_prefix(&peer.identity),
            peer.identity.len()
        ),
        Err(pairing::PairingError::Declined(_)) if !accepted => {
            println!("declined locally; the bundle stayed on this device.")
        }
        Err(err) => return Err(err.into()),
    }
    Ok(())
}

async fn accept(options: &Options) -> Result<(), Box<dyn std::error::Error>> {
    let qr = options
        .get("qr")
        .ok_or("accept needs --qr with the payload the other device shows")?;
    let code = options.get("code").ok_or("accept needs --code")?;
    let ticket = PairingTicket::parse(qr)?;

    let prover = DemoProver::new("accept", 2);
    let pending = pairing::connect(&ticket.endpoint, &ticket, code, &prover).await?;
    println!("short authentication string: {}", pending.sas());
    println!("compare those six digits with the other screen");
    let accepted = confirm(options.yes, "accept the bundle?")?;
    let paired = pending.finish(accepted).await?;
    println!(
        "peer proved identity {} over the transcript",
        hex_prefix(&paired.peer.identity)
    );
    let bundle = paired.bundle;

    let path = options.get("state").map(PathBuf::from);
    let mut state = match &path {
        Some(path) if path.exists() => load_state(path)?,
        _ => ConfigState::default(),
    };
    println!("received:     {}", bundle_summary(&bundle));
    let report = bundle.import_into(&mut state)?;
    println!("imported:     {}", report_summary(&report));
    println!("state now:    {}", state_summary(&state));
    if let Some(path) = path {
        std::fs::write(&path, serde_json::to_vec_pretty(&state)?)?;
        println!("wrote state:  {}", path.display());
    }
    Ok(())
}

/// The first eight bytes of an identity as hex, enough for the demo to show it moved.
fn hex_prefix(bytes: &[u8]) -> String {
    bytes
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn load_state(path: &Path) -> Result<ConfigState, Box<dyn std::error::Error>> {
    let bytes = std::fs::read(path)?;
    Ok(serde_json::from_slice(&bytes)?)
}

fn confirm(auto_yes: bool, prompt: &str) -> Result<bool, Box<dyn std::error::Error>> {
    if auto_yes {
        return Ok(true);
    }
    use std::io::Write;
    print!("{prompt} [y/N] ");
    std::io::stdout().flush()?;
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    Ok(matches!(line.trim(), "y" | "Y" | "yes" | "Yes" | "YES"))
}

/// The address a second machine should dial: the bound address unless it is wildcard, in which
/// case a route lookup (no packet is sent; `connect` on UDP only selects an interface) gives the
/// LAN address.
fn advertised_endpoint(local: SocketAddr, override_endpoint: Option<&str>) -> String {
    if let Some(value) = override_endpoint {
        return value.to_string();
    }
    if !local.ip().is_unspecified() {
        return local.to_string();
    }
    let lan = std::net::UdpSocket::bind("0.0.0.0:0")
        .and_then(|probe| probe.connect("192.168.0.1:9").map(|()| probe))
        .and_then(|probe| probe.local_addr())
        .map(|addr| SocketAddr::new(addr.ip(), local.port()));
    match lan {
        Ok(addr) => addr.to_string(),
        Err(_) => format!("{local} (pass --advertise with this machine's LAN address)"),
    }
}

fn bundle_summary(bundle: &ConfigBundle) -> String {
    let mut parts = Vec::new();
    if !bundle.settings.is_null() {
        parts.push("settings".to_string());
    }
    if !bundle.catalogue.is_null() {
        parts.push("catalogue".to_string());
    }
    parts.push(format!("{} tagged chats", bundle.tags.len()));
    parts.push(format!("{} backgrounds", bundle.backgrounds.len()));
    if let Some(theme) = &bundle.theme {
        parts.push(format!("theme {:?}", theme.id));
    }
    parts.push(format!("from {:?}", bundle.device_label));
    parts.join(", ")
}

fn report_summary(report: &ImportReport) -> String {
    format!(
        "settings={} catalogue={} tags={} backgrounds={} theme={}",
        report.settings, report.catalogue, report.tags, report.backgrounds, report.theme
    )
}

fn state_summary(state: &ConfigState) -> String {
    format!(
        "{} tagged chats, {} backgrounds, theme {:?}",
        state.tags.len(),
        state.backgrounds.len(),
        state.theme.as_ref().map(|theme| theme.id.as_str())
    )
}

fn demo_state(now: i64) -> ConfigState {
    let mut state = ConfigState {
        settings: json!({"density": "comfortable", "send_on_enter": true, "accent": "#7aa2f7"}),
        settings_updated_at_ms: now,
        catalogue: json!({"categories": ["Work", "Family", "Later"]}),
        catalogue_updated_at_ms: now,
        theme: Some(ThemeSelection {
            id: "midnight".into(),
            updated_at_ms: now,
            deleted: false,
            theme: Some(json!({"background": "#111318", "foreground": "#d7dbe7"})),
        }),
        ..ConfigState::default()
    };
    state.tags.insert(
        ChatKey::new("tg:alice", "1001"),
        TagSet::new(["work".to_string(), "urgent".to_string()], now),
    );
    state.backgrounds.insert(
        "backgrounds/demo.png".into(),
        BackgroundAsset {
            relative_path: "backgrounds/demo.png".into(),
            updated_at_ms: now,
            deleted: false,
            bytes: b"\x89PNG\r\n\x1a\nnot a real image, enough for a demo".to_vec(),
        },
    );
    state
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

struct Options {
    values: BTreeMap<String, String>,
    yes: bool,
}

impl Options {
    fn parse(args: impl Iterator<Item = String>) -> Result<Self, Box<dyn std::error::Error>> {
        let mut values = BTreeMap::new();
        let mut yes = false;
        let mut args = args.peekable();
        while let Some(arg) = args.next() {
            if arg == "--yes" {
                yes = true;
                continue;
            }
            let Some(name) = arg.strip_prefix("--") else {
                return Err(format!("unexpected argument {arg:?}").into());
            };
            let value = args
                .next()
                .ok_or_else(|| format!("--{name} needs a value"))?;
            values.insert(name.to_string(), value);
        }
        Ok(Self { values, yes })
    }

    fn get(&self, name: &str) -> Option<&str> {
        self.values.get(name).map(String::as_str)
    }
}
