//! Device-to-device pairing over the LAN, with no server in the path (ADR 0020, slice A).
//!
//! Flow, from the device that already has the configuration (`offer`) to the new one (`accept`):
//!
//! 1. `offer` binds a TCP port and shows an 8-character code plus a QR payload string.
//! 2. `accept` connects, both sides exchange X25519 ephemeral keys and 32-byte nonces.
//! 3. The session key is `HKDF-SHA256(shared_secret, salt = offer_nonce || accept_nonce,
//!    info = "alloy-pairing-v1")`. The accepting side proves it was given the code by opening a
//!    gate sealed under a key derived from the code, so a wrong code fails authentication rather
//!    than being string-compared. The code is a rendezvous and TTL gate, never the key source.
//! 4. Both sides derive the SAS: six decimal digits from `SHA-256(transcript)`, where the
//!    transcript is the domain string, the pairing id, both public keys and both nonces. Each
//!    person compares the digits on the two screens. This, not the code, is the MITM defence.
//! 5. Only when both sides accept is the bundle sealed with ChaCha20-Poly1305 and sent. A rejected
//!    or mismatched SAS means no bundle byte leaves the offering device.
//!
//! Every sealed message carries its type as AAD and a direction-tagged counter nonce, so a replay,
//! a reordering or a relabel fails to open. Frames are length-prefixed and bounded before
//! allocation. The offer accepts exactly one connection: one code, one attempt, 90 seconds.

use std::net::SocketAddr;
use std::time::Duration;

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use hkdf::Hkdf;
use rand::RngCore;
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{Instant, timeout};
use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::Zeroizing;

use crate::config_bundle::{BundleError, ConfigBundle, MAX_BUNDLE_JSON_BYTES};

/// Protocol version. Both the QR payload tag and the hello frames name it.
pub const PAIRING_VERSION: u8 = 1;

/// How long a code stays valid, from `bind` to the end of the handshake.
pub const CODE_TTL: Duration = Duration::from_secs(90);

/// Decimal digits in the short authentication string the two people compare.
pub const SAS_DIGITS: usize = 6;

/// Code alphabet without look-alikes: no `0`, `O`, `1`, `I`, `L`, `U`.
pub const CODE_ALPHABET: &[u8; 30] = b"23456789ABCDEFGHJKMNPQRSTVWXYZ";

/// Pairing ids and nonces are displayable as hex in the QR payload.
const PAIRING_ID_BYTES: usize = 16;

const HELLO_OFFER_LEN: usize = 1 + PAIRING_ID_BYTES + 32 + 32;
const HELLO_ACCEPT_LEN: usize = 1 + 32 + 32;
const MAX_HELLO_FRAME: usize = 128;
const MAX_SEALED_FRAME: usize = MAX_BUNDLE_JSON_BYTES + 64;

const HKDF_INFO: &[u8] = b"alloy-pairing-v1";
const CODE_INFO: &[u8] = b"alloy-pairing-v1/code";
const CODE_AAD: &[u8] = b"alloy-pairing-v1/code";

const MSG_CODE_PROOF: u8 = 1;
const MSG_ACCEPT: u8 = 2;
const MSG_BUNDLE: u8 = 3;

const QR_TAG: &str = "alloy-pairing-v1";

#[derive(Debug, Error)]
pub enum PairingError {
    #[error("pairing I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("pairing protocol error: {0}")]
    Protocol(String),
    #[error("pairing code expired after {0:?}")]
    CodeExpired(Duration),
    #[error("pairing code did not match, or this is not the device showing it")]
    CodeMismatch,
    #[error("pairing declined: {0}")]
    Declined(&'static str),
    #[error("pairing bundle: {0}")]
    Bundle(#[from] BundleError),
    #[error("pairing payload is malformed: {0}")]
    MalformedPayload(String),
}

/// What the UI renders as a QR code. Plain text so this crate needs no QR dependency: pairing id,
/// reachable endpoint, the offer's ephemeral nonce and an optional LAN hint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingTicket {
    pub pairing_id: String,
    pub endpoint: String,
    pub nonce: [u8; 32],
    pub lan_hint: Option<String>,
}

impl PairingTicket {
    /// `alloy-pairing-v1|<id hex>|<endpoint>|<nonce hex>|<hint or empty>`.
    pub fn to_qr_string(&self) -> String {
        format!(
            "{QR_TAG}|{}|{}|{}|{}",
            self.pairing_id,
            self.endpoint,
            to_hex(&self.nonce),
            self.lan_hint.as_deref().unwrap_or("")
        )
    }

    pub fn parse(text: &str) -> Result<Self, PairingError> {
        let mut parts = text.trim().splitn(5, '|');
        let tag = parts.next().unwrap_or_default();
        if tag != QR_TAG {
            return Err(PairingError::MalformedPayload(format!(
                "expected a {QR_TAG} payload, got {tag:?}"
            )));
        }
        let pairing_id = parts
            .next()
            .ok_or_else(|| PairingError::MalformedPayload("missing pairing id".into()))?;
        let endpoint = parts
            .next()
            .filter(|e| !e.is_empty())
            .ok_or_else(|| PairingError::MalformedPayload("missing endpoint".into()))?;
        let nonce_hex = parts
            .next()
            .ok_or_else(|| PairingError::MalformedPayload("missing nonce".into()))?;
        let hint = parts.next().unwrap_or_default();
        if hint.contains('|') {
            return Err(PairingError::MalformedPayload(
                "LAN hint contains the payload separator".into(),
            ));
        }
        let nonce = from_hex_array::<32>(nonce_hex)?;
        let id = from_hex_array::<PAIRING_ID_BYTES>(pairing_id)?;
        Ok(Self {
            pairing_id: to_hex(&id),
            endpoint: endpoint.to_string(),
            nonce,
            lan_hint: (!hint.is_empty()).then(|| hint.to_string()),
        })
    }
}

/// The offering side: holds the bundle, listens for exactly one peer and shows code, QR and SAS.
pub struct PairingOffer {
    listener: TcpListener,
    bundle: ConfigBundle,
    secret: StaticSecret,
    public: PublicKey,
    nonce: [u8; 32],
    pairing_id: [u8; PAIRING_ID_BYTES],
    code: String,
    ticket: PairingTicket,
    ttl: Duration,
    deadline: Instant,
}

impl PairingOffer {
    /// Bind `bind_addr` and return a offer that is valid for [`CODE_TTL`]. `advertise` is the
    /// endpoint a second machine should dial when `bind_addr` is wildcard; it defaults to the
    /// bound address.
    pub async fn bind(
        bundle: ConfigBundle,
        bind_addr: SocketAddr,
        advertise: Option<String>,
    ) -> Result<Self, PairingError> {
        Self::bind_with_ttl(bundle, bind_addr, advertise, CODE_TTL).await
    }

    /// Same as [`PairingOffer::bind`] with an explicit TTL, for tests and for a UI that offers a
    /// shorter window.
    pub async fn bind_with_ttl(
        bundle: ConfigBundle,
        bind_addr: SocketAddr,
        advertise: Option<String>,
        ttl: Duration,
    ) -> Result<Self, PairingError> {
        bundle.validate()?;
        let listener = TcpListener::bind(bind_addr).await?;
        let local = listener.local_addr()?;
        let secret = StaticSecret::from(random_bytes::<32>());
        let public = PublicKey::from(&secret);
        let nonce = random_bytes::<32>();
        let pairing_id = random_bytes::<PAIRING_ID_BYTES>();
        let code = random_code();
        let ticket = PairingTicket {
            pairing_id: to_hex(&pairing_id),
            endpoint: advertise.unwrap_or_else(|| local.to_string()),
            nonce,
            lan_hint: None,
        };
        Ok(Self {
            listener,
            bundle,
            secret,
            public,
            nonce,
            pairing_id,
            code,
            ticket,
            ttl,
            deadline: Instant::now() + ttl,
        })
    }

    pub fn ticket(&self) -> &PairingTicket {
        &self.ticket
    }

    pub fn code(&self) -> &str {
        &self.code
    }

    pub fn qr_payload(&self) -> String {
        self.ticket.to_qr_string()
    }

    /// The address actually bound, for the loopback demo.
    pub fn local_addr(&self) -> Result<SocketAddr, PairingError> {
        Ok(self.listener.local_addr()?)
    }

    /// Wait for one peer, run the handshake and return the SAS to show. The code stops being valid
    /// at the deadline; nothing is retried after a failed attempt, because the offer is consumed.
    pub async fn wait_for_peer(self) -> Result<OfferedHandshake, PairingError> {
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(PairingError::CodeExpired(self.ttl));
        }
        let (mut stream, _peer) = match timeout(remaining, self.listener.accept()).await {
            Err(_) => return Err(PairingError::CodeExpired(self.ttl)),
            Ok(accepted) => accepted?,
        };
        stream.set_nodelay(true)?;

        let mut hello = Vec::with_capacity(HELLO_OFFER_LEN);
        hello.push(PAIRING_VERSION);
        hello.extend_from_slice(&self.pairing_id);
        hello.extend_from_slice(self.public.as_bytes());
        hello.extend_from_slice(&self.nonce);
        write_frame(&mut stream, &hello).await?;

        let ack = read_frame(&mut stream, MAX_HELLO_FRAME).await?;
        let accept_public = parse_accept_hello(&ack)?;

        if Instant::now() >= self.deadline {
            return Err(PairingError::CodeExpired(self.ttl));
        }
        let shared = self.secret.diffie_hellman(&accept_public.public);
        if !shared.was_contributory() {
            return Err(PairingError::Protocol(
                "peer sent a degenerate X25519 public key".into(),
            ));
        }
        let transcript = transcript(
            &self.pairing_id,
            self.public.as_bytes(),
            &self.nonce,
            accept_public.public.as_bytes(),
            &accept_public.nonce,
        );
        let salt = salt(&self.nonce, &accept_public.nonce);
        let key = expand(shared.as_bytes(), &salt, HKDF_INFO)?;
        let mut channel = Channel::new(stream, key, 0);

        let proof = channel.recv(MSG_CODE_PROOF).await?;
        let gate = expand(self.code.as_bytes(), &salt, CODE_INFO)?;
        let opened = open_code_proof(&gate, &proof)?;
        if opened.as_slice() != transcript.as_slice() {
            return Err(PairingError::CodeMismatch);
        }

        Ok(OfferedHandshake {
            sas: sas(&transcript),
            bundle: self.bundle,
            channel,
        })
    }
}

/// The offering side after the handshake, holding the SAS the user must confirm.
pub struct OfferedHandshake {
    sas: String,
    bundle: ConfigBundle,
    channel: Channel,
}

impl OfferedHandshake {
    pub fn sas(&self) -> &str {
        &self.sas
    }

    /// Send the user's decision. The bundle leaves only when both this side and the peer accepted;
    /// otherwise both sides return [`PairingError::Declined`].
    pub async fn finish(mut self, accepted: bool) -> Result<(), PairingError> {
        self.channel.send(MSG_ACCEPT, &[u8::from(accepted)]).await?;
        let reply = self.channel.recv(MSG_ACCEPT).await?;
        let peer_accepted = parse_accept(&reply)?;
        if !accepted {
            return Err(PairingError::Declined("this device rejected the SAS"));
        }
        if !peer_accepted {
            return Err(PairingError::Declined("the other device rejected the SAS"));
        }
        let bytes = self.bundle.to_json()?;
        self.channel.send(MSG_BUNDLE, &bytes).await?;
        Ok(())
    }
}

/// The accepting side after the handshake, holding the SAS the user must confirm.
pub struct PendingPairing {
    sas: String,
    channel: Channel,
}

impl PendingPairing {
    pub fn sas(&self) -> &str {
        &self.sas
    }

    /// Send the user's decision and, when both sides accepted, receive and validate the bundle.
    pub async fn finish(mut self, accepted: bool) -> Result<ConfigBundle, PairingError> {
        self.channel.send(MSG_ACCEPT, &[u8::from(accepted)]).await?;
        let reply = self.channel.recv(MSG_ACCEPT).await?;
        let peer_accepted = parse_accept(&reply)?;
        if !accepted {
            return Err(PairingError::Declined("this device rejected the SAS"));
        }
        if !peer_accepted {
            return Err(PairingError::Declined("the other device rejected the SAS"));
        }
        let bytes = self.channel.recv(MSG_BUNDLE).await?;
        Ok(ConfigBundle::from_json(&bytes)?)
    }
}

/// Connect to an offering device and complete the handshake. The code is the one the offer shows.
/// A wrong code is not detectable here (the offering side holds the expected value), so it shows
/// up as a failed [`PendingPairing::finish`] on both ends.
pub async fn connect(
    endpoint: &str,
    ticket: &PairingTicket,
    code: &str,
) -> Result<PendingPairing, PairingError> {
    let expected_id = from_hex_array::<PAIRING_ID_BYTES>(&ticket.pairing_id)?;
    let mut stream = TcpStream::connect(endpoint).await?;
    stream.set_nodelay(true)?;

    let hello = read_frame(&mut stream, MAX_HELLO_FRAME).await?;
    let (version, pairing_id, offer_public, offer_nonce) = parse_offer_hello(&hello)?;
    if version != PAIRING_VERSION {
        return Err(PairingError::Protocol(format!(
            "peer speaks pairing version {version}, this build speaks {PAIRING_VERSION}"
        )));
    }
    if pairing_id != expected_id {
        return Err(PairingError::Protocol(
            "pairing id does not match the payload".into(),
        ));
    }
    if offer_nonce != ticket.nonce {
        return Err(PairingError::Protocol(
            "offer nonce does not match the payload".into(),
        ));
    }

    let secret = StaticSecret::from(random_bytes::<32>());
    let public = PublicKey::from(&secret);
    let accept_nonce = random_bytes::<32>();
    let mut ack = Vec::with_capacity(HELLO_ACCEPT_LEN);
    ack.push(PAIRING_VERSION);
    ack.extend_from_slice(public.as_bytes());
    ack.extend_from_slice(&accept_nonce);
    write_frame(&mut stream, &ack).await?;

    let shared = secret.diffie_hellman(&offer_public);
    if !shared.was_contributory() {
        return Err(PairingError::Protocol(
            "peer sent a degenerate X25519 public key".into(),
        ));
    }
    let transcript = transcript(
        &pairing_id,
        offer_public.as_bytes(),
        &offer_nonce,
        public.as_bytes(),
        &accept_nonce,
    );
    let salt = salt(&offer_nonce, &accept_nonce);
    let key = expand(shared.as_bytes(), &salt, HKDF_INFO)?;
    let gate = expand(code.as_bytes(), &salt, CODE_INFO)?;
    let mut channel = Channel::new(stream, key, 1);
    let proof = seal_code_proof(&gate, &transcript)?;
    channel.send(MSG_CODE_PROOF, &proof).await?;

    Ok(PendingPairing {
        sas: sas(&transcript),
        channel,
    })
}

/// Sealed, direction-tagged channel. The session key is the same in both directions, but the nonce
/// starts with the direction, so the two counters never meet.
struct Channel {
    stream: TcpStream,
    key: Zeroizing<[u8; 32]>,
    send_dir: u8,
    send_counter: u64,
    recv_counter: u64,
}

impl Channel {
    fn new(stream: TcpStream, key: Zeroizing<[u8; 32]>, send_dir: u8) -> Self {
        Self {
            stream,
            key,
            send_dir,
            send_counter: 0,
            recv_counter: 0,
        }
    }

    async fn send(&mut self, msg_type: u8, plaintext: &[u8]) -> Result<(), PairingError> {
        let cipher = ChaCha20Poly1305::new(&Key::from(*self.key));
        let nonce = frame_nonce(self.send_dir, self.send_counter);
        let sealed = cipher
            .encrypt(
                &Nonce::from(nonce),
                Payload {
                    msg: plaintext,
                    aad: &[msg_type],
                },
            )
            .map_err(|_| PairingError::Protocol("sealing a frame failed".into()))?;
        write_frame(&mut self.stream, &sealed).await?;
        self.send_counter += 1;
        Ok(())
    }

    async fn recv(&mut self, msg_type: u8) -> Result<Vec<u8>, PairingError> {
        let sealed = read_frame(&mut self.stream, MAX_SEALED_FRAME).await?;
        let cipher = ChaCha20Poly1305::new(&Key::from(*self.key));
        let nonce = frame_nonce(self.send_dir ^ 1, self.recv_counter);
        let opened = cipher
            .decrypt(
                &Nonce::from(nonce),
                Payload {
                    msg: &sealed,
                    aad: &[msg_type],
                },
            )
            .map_err(|_| {
                PairingError::Protocol(
                    "a sealed frame failed to open (wrong type, order or key)".into(),
                )
            })?;
        self.recv_counter += 1;
        Ok(opened)
    }
}

struct AcceptHello {
    public: PublicKey,
    nonce: [u8; 32],
}

fn parse_offer_hello(
    hello: &[u8],
) -> Result<(u8, [u8; PAIRING_ID_BYTES], PublicKey, [u8; 32]), PairingError> {
    if hello.len() != HELLO_OFFER_LEN {
        return Err(PairingError::Protocol(format!(
            "offer hello is {} bytes, expected {HELLO_OFFER_LEN}",
            hello.len()
        )));
    }
    let version = hello[0];
    let mut id = [0u8; PAIRING_ID_BYTES];
    id.copy_from_slice(&hello[1..1 + PAIRING_ID_BYTES]);
    let mut public = [0u8; 32];
    public.copy_from_slice(&hello[1 + PAIRING_ID_BYTES..1 + PAIRING_ID_BYTES + 32]);
    let mut nonce = [0u8; 32];
    nonce.copy_from_slice(&hello[1 + PAIRING_ID_BYTES + 32..]);
    Ok((version, id, PublicKey::from(public), nonce))
}

fn parse_accept_hello(hello: &[u8]) -> Result<AcceptHello, PairingError> {
    if hello.len() != HELLO_ACCEPT_LEN {
        return Err(PairingError::Protocol(format!(
            "accept hello is {} bytes, expected {HELLO_ACCEPT_LEN}",
            hello.len()
        )));
    }
    if hello[0] != PAIRING_VERSION {
        return Err(PairingError::Protocol(format!(
            "peer speaks pairing version {}, this build speaks {PAIRING_VERSION}",
            hello[0]
        )));
    }
    let mut public = [0u8; 32];
    public.copy_from_slice(&hello[1..33]);
    let mut nonce = [0u8; 32];
    nonce.copy_from_slice(&hello[33..]);
    Ok(AcceptHello {
        public: PublicKey::from(public),
        nonce,
    })
}

fn parse_accept(reply: &[u8]) -> Result<bool, PairingError> {
    match reply {
        [0] => Ok(false),
        [1] => Ok(true),
        other => Err(PairingError::Protocol(format!(
            "accept reply is {} bytes, expected one flag",
            other.len()
        ))),
    }
}

fn transcript(
    pairing_id: &[u8; PAIRING_ID_BYTES],
    offer_public: &[u8; 32],
    offer_nonce: &[u8; 32],
    accept_public: &[u8; 32],
    accept_nonce: &[u8; 32],
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(HKDF_INFO);
    hasher.update(pairing_id);
    hasher.update(offer_public);
    hasher.update(offer_nonce);
    hasher.update(accept_public);
    hasher.update(accept_nonce);
    hasher.finalize().into()
}

/// Six decimal digits of the transcript hash: what the two people compare.
fn sas(transcript: &[u8; 32]) -> String {
    let value = u32::from_be_bytes([transcript[0], transcript[1], transcript[2], transcript[3]]);
    format!("{:06}", value % 1_000_000)
}

fn salt(offer_nonce: &[u8; 32], accept_nonce: &[u8; 32]) -> [u8; 64] {
    let mut salt = [0u8; 64];
    salt[..32].copy_from_slice(offer_nonce);
    salt[32..].copy_from_slice(accept_nonce);
    salt
}

fn expand(ikm: &[u8], salt: &[u8], info: &[u8]) -> Result<Zeroizing<[u8; 32]>, PairingError> {
    let hkdf = Hkdf::<Sha256>::new(Some(salt), ikm);
    let mut out = Zeroizing::new([0u8; 32]);
    hkdf.expand(info, &mut out[..])
        .map_err(|_| PairingError::Protocol("HKDF expansion failed".into()))?;
    Ok(out)
}

fn frame_nonce(direction: u8, counter: u64) -> [u8; 12] {
    let mut nonce = [0u8; 12];
    nonce[0] = direction;
    nonce[1..9].copy_from_slice(&counter.to_be_bytes());
    nonce
}

fn seal_code_proof(key: &[u8; 32], transcript: &[u8; 32]) -> Result<Vec<u8>, PairingError> {
    ChaCha20Poly1305::new(&Key::from(*key))
        .encrypt(
            &Nonce::from([2u8; 12]),
            Payload {
                msg: transcript,
                aad: CODE_AAD,
            },
        )
        .map_err(|_| PairingError::Protocol("sealing the code proof failed".into()))
}

fn open_code_proof(key: &[u8; 32], proof: &[u8]) -> Result<Vec<u8>, PairingError> {
    ChaCha20Poly1305::new(&Key::from(*key))
        .decrypt(
            &Nonce::from([2u8; 12]),
            Payload {
                msg: proof,
                aad: CODE_AAD,
            },
        )
        .map_err(|_| PairingError::CodeMismatch)
}

async fn read_frame(stream: &mut TcpStream, limit: usize) -> Result<Vec<u8>, PairingError> {
    let mut len = [0u8; 4];
    stream.read_exact(&mut len).await?;
    let len = u32::from_be_bytes(len) as usize;
    if len == 0 || len > limit {
        return Err(PairingError::Protocol(format!(
            "frame length {len} is outside 1..={limit}"
        )));
    }
    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf).await?;
    Ok(buf)
}

async fn write_frame(stream: &mut TcpStream, payload: &[u8]) -> Result<(), PairingError> {
    let len = u32::try_from(payload.len())
        .map_err(|_| PairingError::Protocol("frame is longer than u32::MAX".into()))?;
    stream.write_all(&len.to_be_bytes()).await?;
    stream.write_all(payload).await?;
    stream.flush().await?;
    Ok(())
}

fn random_bytes<const N: usize>() -> [u8; N] {
    let mut bytes = [0u8; N];
    rand::rng().fill_bytes(&mut bytes);
    bytes
}

fn random_code() -> String {
    let alphabet = CODE_ALPHABET.len();
    // Rejection sampling keeps every code equally likely: 30 does not divide 256.
    let limit = (256 / alphabet) * alphabet;
    let mut rng = rand::rng();
    let mut code = String::with_capacity(8);
    while code.len() < 8 {
        let mut byte = [0u8; 1];
        rng.fill_bytes(&mut byte);
        if (byte[0] as usize) < limit {
            code.push(CODE_ALPHABET[byte[0] as usize % alphabet] as char);
        }
    }
    code
}

fn to_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    out
}

fn from_hex_array<const N: usize>(text: &str) -> Result<[u8; N], PairingError> {
    if text.len() != N * 2 {
        return Err(PairingError::MalformedPayload(format!(
            "expected {} hex characters, got {}",
            N * 2,
            text.len()
        )));
    }
    let mut out = [0u8; N];
    let bytes = text.as_bytes();
    for (i, slot) in out.iter_mut().enumerate() {
        let hi = hex_digit(bytes[i * 2]).ok_or_else(|| {
            PairingError::MalformedPayload(format!("{text:?} is not hexadecimal"))
        })?;
        let lo = hex_digit(bytes[i * 2 + 1]).ok_or_else(|| {
            PairingError::MalformedPayload(format!("{text:?} is not hexadecimal"))
        })?;
        *slot = (hi << 4) | lo;
    }
    Ok(out)
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
