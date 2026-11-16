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
//! 6. The accept frame carries each side's device identity and a [`DeviceProver`] proof over the
//!    transcript, sealed with the session key. This crate only moves those bytes: verifying the
//!    proof before enrolling the peer is the caller's job, and the identity it returns in
//!    [`PairedPeer`] is untrusted until then.
//!
//! Every sealed message carries its type as AAD and a direction-tagged counter nonce, so a replay,
//! a reordering or a relabel fails to open. Frames are length-prefixed and bounded before
//! allocation. The offer accepts exactly one connection: one code, one attempt, 90 seconds.

use std::net::SocketAddr;
use std::sync::Arc;
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

/// Protocol version. The hello frames name it; the QR payload tag stays `v1`, because the ticket
/// shape did not change and an old client must still parse a ticket before its version check
/// refuses the hello (see [`PairingError::Protocol`]).
///
/// Version 2 adds the optional master-key envelope to the bundle payload. A version-1 acceptor
/// refuses a version-2 offer at the hello instead of misparsing the payload.
pub const PAIRING_VERSION: u8 = 2;

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

/// Largest master-key envelope this layer carries. The wrap helper in `alloy-sync` produces a
/// few hundred bytes for a 64-hex device id; the bound is generous but finite, so a peer cannot
/// make the frame parser allocate more than this before authenticating anything.
pub const MAX_MASTER_ENVELOPE_BYTES: usize = 1024;

/// Bundle payload header: the payload version and a flags byte.
const PAYLOAD_HEADER_LEN: usize = 2;
/// Epoch and envelope length that precede an envelope in the payload.
const MASTER_PREFIX_LEN: usize = 4 + 4;
/// Bit set in the payload flags when a master-key envelope follows.
const PAYLOAD_FLAG_MASTER: u8 = 1;
/// Bits a version-2 payload may set. Anything else is refused rather than ignored.
const PAYLOAD_FLAGS_KNOWN: u8 = PAYLOAD_FLAG_MASTER;
const AEAD_TAG_LEN: usize = 16;

const MAX_SEALED_FRAME: usize = MAX_BUNDLE_JSON_BYTES
    + PAYLOAD_HEADER_LEN
    + MASTER_PREFIX_LEN
    + MAX_MASTER_ENVELOPE_BYTES
    + AEAD_TAG_LEN;

const HKDF_INFO: &[u8] = b"alloy-pairing-v1";
const CODE_INFO: &[u8] = b"alloy-pairing-v1/code";
const CODE_AAD: &[u8] = b"alloy-pairing-v1/code";

const MSG_CODE_PROOF: u8 = 1;
const MSG_ACCEPT: u8 = 2;
const MSG_BUNDLE: u8 = 3;

/// Longest device identity this layer carries in an accept frame. Verification and the identity's
/// internal shape belong to the caller; only the bound is enforced here, before allocation.
const MAX_IDENTITY_LEN: usize = 128;

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

/// What the caller brings to a pairing: the bytes that identify this device to the peer, and a
/// proof over the handshake transcript.
///
/// This crate only moves the identity and the proof; it never decides whether either is trusted.
/// Verifying the proof over [`PairedPeer::transcript`] before enrolling the peer is the caller's
/// job (in alloy-engine, that is an Ed25519 check against the key inside the identity).
pub trait DeviceProver {
    /// Opaque identity bytes. The engine puts the Ed25519 public key first, then a UTF-8 label.
    fn identity(&self) -> &[u8];
    /// Signature over the 32-byte transcript both sides derived from the handshake.
    fn sign(&self, transcript: &[u8; 32]) -> [u8; 64];
}

/// What the peer sent once both people accepted the SAS: its identity and a proof over the
/// transcript. Nothing in here is trusted yet; the caller verifies the proof before enrolling.
#[derive(Debug, Clone)]
pub struct PairedPeer {
    pub identity: Vec<u8>,
    pub proof: [u8; 64],
    /// The transcript the proof must cover. Both sides derived the same one.
    pub transcript: [u8; 32],
    pub sas: String,
}

/// The accepting side's result: the bundle, the identity the offering device proved, and the
/// optional sealed master-key envelope addressed to this device.
#[derive(Debug, Clone)]
pub struct PairedBundle {
    pub bundle: ConfigBundle,
    pub peer: PairedPeer,
    /// `None` for a version-1 offer (settings only) or an offer that carries no master key.
    pub master: Option<MasterEnvelope>,
}

/// A sealed master-key envelope addressed to the accepting device.
///
/// The bytes are opaque to this crate: the offering side builds them with
/// `alloy_sync::device::wrap_master`, and only the addressed device's `DeviceSecretKey` can open
/// them. `epoch` is the epoch the envelope installs, authenticated by the wrap's own header, and
/// is carried in the clear only so the acceptor knows which epoch to expect before opening.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MasterEnvelope {
    pub epoch: u32,
    pub bytes: Vec<u8>,
}

/// Supplies the optional master-key envelope for one accept, after the acceptor's X25519 key is
/// known.
///
/// The offering side learns the peer's identity only when the accept frame arrives, so the
/// envelope cannot be built at [`PairingOffer::bind`]. The provider receives the unverified
/// [`PairedPeer`]; it must check the proof before wrapping the master key to that identity, and
/// every failure it returns aborts the transfer before the bundle is sent.
pub trait MasterEnvelopeProvider {
    fn envelope(&self, peer: &PairedPeer) -> Result<Option<MasterEnvelope>, PairingError>;
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
    prover: Arc<dyn DeviceProver + Send + Sync>,
    master: Option<Arc<dyn MasterEnvelopeProvider + Send + Sync>>,
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
    /// bound address. `prover` supplies this device's identity and its proof over the transcript.
    /// `master` supplies the optional master-key envelope once the acceptor is known.
    pub async fn bind(
        bundle: ConfigBundle,
        bind_addr: SocketAddr,
        advertise: Option<String>,
        prover: Arc<dyn DeviceProver + Send + Sync>,
        master: Option<Arc<dyn MasterEnvelopeProvider + Send + Sync>>,
    ) -> Result<Self, PairingError> {
        Self::bind_with_ttl(bundle, bind_addr, advertise, CODE_TTL, prover, master).await
    }

    /// Same as [`PairingOffer::bind`] with an explicit TTL, for tests and for a UI that offers a
    /// shorter window.
    pub async fn bind_with_ttl(
        bundle: ConfigBundle,
        bind_addr: SocketAddr,
        advertise: Option<String>,
        ttl: Duration,
        prover: Arc<dyn DeviceProver + Send + Sync>,
        master: Option<Arc<dyn MasterEnvelopeProvider + Send + Sync>>,
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
            prover,
            master,
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
        if accept_public.version != PAIRING_VERSION {
            return Err(PairingError::Protocol(format!(
                "peer answered pairing version {} to an offer that spoke {PAIRING_VERSION}",
                accept_public.version
            )));
        }

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
            transcript,
            prover: self.prover,
            master: self.master,
            wire_version: PAIRING_VERSION,
        })
    }
}

/// The offering side after the handshake, holding the SAS the user must confirm.
pub struct OfferedHandshake {
    sas: String,
    bundle: ConfigBundle,
    channel: Channel,
    transcript: [u8; 32],
    prover: Arc<dyn DeviceProver + Send + Sync>,
    master: Option<Arc<dyn MasterEnvelopeProvider + Send + Sync>>,
    wire_version: u8,
}

impl OfferedHandshake {
    pub fn sas(&self) -> &str {
        &self.sas
    }

    /// Send the user's decision and this device's identity, then read the peer's. The bundle
    /// leaves only when both this side and the peer accepted; otherwise both sides return
    /// [`PairingError::Declined`].
    ///
    /// The returned [`PairedPeer`] is unverified: the caller must check `proof` over `transcript`
    /// before trusting or enrolling the identity.
    pub async fn finish(mut self, accepted: bool) -> Result<PairedPeer, PairingError> {
        let frame = accept_frame(&*self.prover, accepted, &self.transcript)?;
        self.channel.send(MSG_ACCEPT, &frame).await?;
        let reply = self.channel.recv(MSG_ACCEPT).await?;
        let peer = parse_accept(&reply)?;
        if !accepted {
            return Err(PairingError::Declined("this device rejected the SAS"));
        }
        let Some(peer) = peer else {
            return Err(PairingError::Declined("the other device rejected the SAS"));
        };
        let peer = PairedPeer {
            identity: peer.identity,
            proof: peer.proof,
            transcript: self.transcript,
            sas: self.sas,
        };
        // The envelope is built here, after the acceptor's identity arrived: the provider checks
        // the proof and wraps the master key to that device before a single bundle byte leaves.
        let master = match &self.master {
            Some(provider) => provider.envelope(&peer)?,
            None => None,
        };
        let bytes = self.bundle.to_json()?;
        let payload = encode_payload(&bytes, master.as_ref(), self.wire_version)?;
        self.channel.send(MSG_BUNDLE, &payload).await?;
        Ok(peer)
    }
}

/// The accepting side after the handshake, holding the SAS the user must confirm.
pub struct PendingPairing {
    sas: String,
    channel: Channel,
    transcript: [u8; 32],
    /// The version the offer's hello named. `1` means a legacy offer with a raw bundle payload.
    wire_version: u8,
    /// The sealed identity this device will send when it accepts; built in [`connect`].
    accept_frame: Vec<u8>,
}

impl PendingPairing {
    pub fn sas(&self) -> &str {
        &self.sas
    }

    /// Send the user's decision and, when both sides accepted, receive and validate the bundle.
    ///
    /// The returned [`PairedBundle::peer`] is unverified: the caller must check `proof` over
    /// `transcript` before trusting or enrolling the identity, and before importing the bundle.
    pub async fn finish(mut self, accepted: bool) -> Result<PairedBundle, PairingError> {
        let frame = if accepted {
            self.accept_frame.as_slice()
        } else {
            &[0u8][..]
        };
        self.channel.send(MSG_ACCEPT, frame).await?;
        let reply = self.channel.recv(MSG_ACCEPT).await?;
        let peer = parse_accept(&reply)?;
        if !accepted {
            return Err(PairingError::Declined("this device rejected the SAS"));
        }
        let Some(peer) = peer else {
            return Err(PairingError::Declined("the other device rejected the SAS"));
        };
        let bytes = self.channel.recv(MSG_BUNDLE).await?;
        let (bundle_json, master) = decode_payload(&bytes, self.wire_version)?;
        Ok(PairedBundle {
            bundle: ConfigBundle::from_json(&bundle_json)?,
            peer: PairedPeer {
                identity: peer.identity,
                proof: peer.proof,
                transcript: self.transcript,
                sas: self.sas,
            },
            master,
        })
    }
}

/// Connect to an offering device and complete the handshake. The code is the one the offer shows.
/// A wrong code is not detectable here (the offering side holds the expected value), so it shows
/// up as a failed [`PendingPairing::finish`] on both ends. `prover` supplies this device's
/// identity and its proof over the transcript, sent only if the user accepts.
pub async fn connect(
    endpoint: &str,
    ticket: &PairingTicket,
    code: &str,
    prover: &(dyn DeviceProver + Send + Sync),
) -> Result<PendingPairing, PairingError> {
    let expected_id = from_hex_array::<PAIRING_ID_BYTES>(&ticket.pairing_id)?;
    let mut stream = TcpStream::connect(endpoint).await?;
    stream.set_nodelay(true)?;

    let hello = read_frame(&mut stream, MAX_HELLO_FRAME).await?;
    let (version, pairing_id, offer_public, offer_nonce) = parse_offer_hello(&hello)?;
    if version == 0 || version > PAIRING_VERSION {
        return Err(PairingError::Protocol(format!(
            "peer speaks pairing version {version}, this build speaks 1..={PAIRING_VERSION}"
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
    ack.push(version);
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
        transcript,
        wire_version: version,
        accept_frame: accept_frame(prover, true, &transcript)?,
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
    version: u8,
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
    if hello[0] == 0 || hello[0] > PAIRING_VERSION {
        return Err(PairingError::Protocol(format!(
            "peer speaks pairing version {}, this build speaks 1..={PAIRING_VERSION}",
            hello[0]
        )));
    }
    let mut public = [0u8; 32];
    public.copy_from_slice(&hello[1..33]);
    let mut nonce = [0u8; 32];
    nonce.copy_from_slice(&hello[33..]);
    Ok(AcceptHello {
        version: hello[0],
        public: PublicKey::from(public),
        nonce,
    })
}

/// The peer's identity and proof, as carried in an accept frame.
struct SealedPeer {
    identity: Vec<u8>,
    proof: [u8; 64],
}

/// Builds the accept frame: one flag, and when accepting, the identity length, the identity and
/// the proof over `transcript`. A decline carries no identity at all.
fn accept_frame(
    prover: &dyn DeviceProver,
    accepted: bool,
    transcript: &[u8; 32],
) -> Result<Vec<u8>, PairingError> {
    if !accepted {
        return Ok(vec![0]);
    }
    let identity = prover.identity();
    if identity.len() > MAX_IDENTITY_LEN {
        return Err(PairingError::Protocol(format!(
            "identity is {} bytes, above the {MAX_IDENTITY_LEN}-byte frame bound",
            identity.len()
        )));
    }
    let proof = prover.sign(transcript);
    let mut frame = Vec::with_capacity(3 + identity.len() + 64);
    frame.push(1);
    frame.extend_from_slice(&(identity.len() as u16).to_be_bytes());
    frame.extend_from_slice(identity);
    frame.extend_from_slice(&proof);
    Ok(frame)
}

/// `None` is a decline; `Some` carries the peer's identity and proof, still unverified.
fn parse_accept(reply: &[u8]) -> Result<Option<SealedPeer>, PairingError> {
    match reply {
        [0] => Ok(None),
        [1, rest @ ..] if rest.len() >= 2 + 64 => {
            let len = u16::from_be_bytes([rest[0], rest[1]]) as usize;
            let body = &rest[2..];
            if len > MAX_IDENTITY_LEN || body.len() != len + 64 {
                return Err(PairingError::Protocol(format!(
                    "accept frame claims {len} identity bytes in {} body bytes",
                    body.len()
                )));
            }
            let mut proof = [0u8; 64];
            proof.copy_from_slice(&body[len..]);
            Ok(Some(SealedPeer {
                identity: body[..len].to_vec(),
                proof,
            }))
        }
        other => Err(PairingError::Protocol(format!(
            "accept frame is {} bytes, expected a flag or a sealed identity",
            other.len()
        ))),
    }
}

/// Builds the bundle payload: version and flags, the optional epoch and envelope, then the bundle
/// JSON. A version-1 payload stays the raw bundle bytes, so an old offer's frame is unchanged.
fn encode_payload(
    bundle: &[u8],
    master: Option<&MasterEnvelope>,
    wire_version: u8,
) -> Result<Vec<u8>, PairingError> {
    if bundle.len() > MAX_BUNDLE_JSON_BYTES {
        return Err(PairingError::Protocol(format!(
            "bundle is {} bytes, above the {MAX_BUNDLE_JSON_BYTES}-byte payload bound",
            bundle.len()
        )));
    }
    if wire_version == 1 {
        // Nothing on a version-1 wire may carry an envelope: the old peer would treat it as JSON.
        return Ok(bundle.to_vec());
    }
    let mut out = Vec::with_capacity(
        PAYLOAD_HEADER_LEN
            + MASTER_PREFIX_LEN
            + master.map(|envelope| envelope.bytes.len()).unwrap_or(0)
            + bundle.len(),
    );
    out.push(PAIRING_VERSION);
    out.push(if master.is_some() {
        PAYLOAD_FLAG_MASTER
    } else {
        0
    });
    if let Some(envelope) = master {
        if envelope.bytes.len() > MAX_MASTER_ENVELOPE_BYTES {
            return Err(PairingError::Protocol(format!(
                "master envelope is {} bytes, above the {MAX_MASTER_ENVELOPE_BYTES}-byte bound",
                envelope.bytes.len()
            )));
        }
        out.extend_from_slice(&envelope.epoch.to_be_bytes());
        out.extend_from_slice(&(envelope.bytes.len() as u32).to_be_bytes());
        out.extend_from_slice(&envelope.bytes);
    }
    out.extend_from_slice(bundle);
    Ok(out)
}

/// Splits a received payload into the bundle JSON and the optional envelope. A version-1 offer
/// sent raw JSON; anything else is framed and its unknown bits or versions are refused.
fn decode_payload(
    payload: &[u8],
    wire_version: u8,
) -> Result<(Vec<u8>, Option<MasterEnvelope>), PairingError> {
    if wire_version == 1 {
        if payload.len() > MAX_BUNDLE_JSON_BYTES {
            return Err(PairingError::Protocol(format!(
                "bundle is {} bytes, above the {MAX_BUNDLE_JSON_BYTES}-byte payload bound",
                payload.len()
            )));
        }
        return Ok((payload.to_vec(), None));
    }
    if payload.len() <= PAYLOAD_HEADER_LEN {
        return Err(PairingError::Protocol(format!(
            "bundle payload is {} bytes, too short to hold a header and a bundle",
            payload.len()
        )));
    }
    if payload[0] != PAIRING_VERSION {
        return Err(PairingError::Protocol(format!(
            "bundle payload names version {}, this build speaks {PAIRING_VERSION}",
            payload[0]
        )));
    }
    let flags = payload[1];
    if flags & !PAYLOAD_FLAGS_KNOWN != 0 {
        return Err(PairingError::Protocol(format!(
            "bundle payload sets unknown flags {flags:#04x}"
        )));
    }
    let mut cursor = PAYLOAD_HEADER_LEN;
    let master = if flags & PAYLOAD_FLAG_MASTER != 0 {
        let prefix = payload
            .get(cursor..cursor + MASTER_PREFIX_LEN)
            .ok_or_else(|| {
                PairingError::Protocol("bundle payload ends inside the envelope prefix".into())
            })?;
        let epoch = u32::from_be_bytes(prefix[..4].try_into().expect("four bytes"));
        let len = u32::from_be_bytes(prefix[4..].try_into().expect("four bytes")) as usize;
        if len == 0 || len > MAX_MASTER_ENVELOPE_BYTES {
            return Err(PairingError::Protocol(format!(
                "master envelope length {len} is outside 1..={MAX_MASTER_ENVELOPE_BYTES}"
            )));
        }
        cursor += MASTER_PREFIX_LEN;
        let bytes = payload.get(cursor..cursor + len).ok_or_else(|| {
            PairingError::Protocol("bundle payload ends inside the master envelope".into())
        })?;
        cursor += len;
        Some(MasterEnvelope {
            epoch,
            bytes: bytes.to_vec(),
        })
    } else {
        None
    };
    let bundle = &payload[cursor..];
    if bundle.is_empty() {
        return Err(PairingError::Protocol(
            "bundle payload carries no bundle after the envelope".into(),
        ));
    }
    if bundle.len() > MAX_BUNDLE_JSON_BYTES {
        return Err(PairingError::Protocol(format!(
            "bundle is {} bytes, above the {MAX_BUNDLE_JSON_BYTES}-byte payload bound",
            bundle.len()
        )));
    }
    Ok((bundle.to_vec(), master))
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
