//! Per-device X25519 keys and the sealed wrap that carries a rotated master key to a survivor
//! (ADR 0020 invariant 14).
//!
//! Rotation changes the root key, and the revoked device still holds the old one, so the new key
//! cannot simply be published under a key derived from the old master. Each device therefore has
//! its own X25519 identity, and the revoker seals the new master once per surviving device:
//!
//! - a fresh ephemeral X25519 key agrees a one-off DH secret with the recipient's static key;
//! - HKDF-SHA256 expands that secret into a `ChaCha20-Poly1305` key;
//! - the AAD is the canonical wrap header, which binds version, epoch, recipient device id,
//!   recipient public key, sender device id and the ephemeral public key, so a wrap relabelled to
//!   another recipient or epoch fails to decrypt;
//! - the sender's Ed25519 device key signs the same bytes, so a server or a third device cannot
//!   invent a wrap that a survivor would adopt.
//!
//! A wrap names the epoch it installs. A survivor accepts one only for the epoch the verified head
//! demands, so a replayed wrap from an earlier rotation is never adopted, and a wrap addressed to
//! another device never opens: the recipient's private key is the only one that completes the DH.
//!
//! Nothing here implements a primitive: X25519, HKDF, ChaCha20-Poly1305 and Ed25519 come from
//! RustCrypto.

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use hkdf::Hkdf;
use rand::RngCore;
use sha2::Sha256;
use thiserror::Error;
use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::Zeroizing;

use crate::keys::{MASTER_KEY_LEN, MasterKey};

/// Version byte of the wrap encoding. A future format is refused, never guessed at.
pub const DEVICE_WRAP_VERSION: u8 = 1;

/// Domain separator for both the KDF and the signed header.
const WRAP_DOMAIN: &[u8] = b"alloy-sync-v1/device-wrap/1";

/// HKDF info for the wrap key.
const WRAP_INFO: &[u8] = b"alloy-sync-v1/device-wrap/key";

/// Nonce length of the wrap AEAD.
pub const WRAP_NONCE_LEN: usize = 12;

/// Largest wrap body accepted from a server. One master plus the AEAD tag is 48 bytes; the bound
/// is generous but finite, so untrusted input is never allocated past it.
pub const MAX_WRAP_BYTES: usize = 1024;

/// Largest device id embedded in a wrap. The same bound the chain applies to a signer.
const MAX_DEVICE_ID: usize = 128;

const X25519_KEY_LEN: usize = 32;
const TAG_LEN: usize = 16;

/// A device's static X25519 secret. Wiped on drop; never serialized by this crate.
pub struct DeviceSecretKey(Zeroizing<[u8; X25519_KEY_LEN]>);

impl DeviceSecretKey {
    /// Draw a fresh device key from the OS RNG.
    pub fn generate() -> Self {
        let mut bytes = Zeroizing::new([0u8; X25519_KEY_LEN]);
        rand::rng().fill_bytes(&mut bytes[..]);
        Self(bytes)
    }

    /// Rebuild a device key from bytes a secret store returned.
    pub fn from_bytes(bytes: [u8; X25519_KEY_LEN]) -> Self {
        Self(Zeroizing::new(bytes))
    }

    /// The raw key, for sealing it into the secret store. Callers must not keep the reference
    /// after the value drops.
    pub fn expose_secret(&self) -> &[u8; X25519_KEY_LEN] {
        &self.0
    }

    /// The public half another device wraps to.
    pub fn public_key(&self) -> [u8; X25519_KEY_LEN] {
        let secret = StaticSecret::from(*self.0);
        PublicKey::from(&secret).to_bytes()
    }
}

impl std::fmt::Debug for DeviceSecretKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DeviceSecretKey(<redacted>)")
    }
}

/// Why a wrap was not produced or not accepted. Every variant is terminal: a caller never retries
/// a bad wrap against the same bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum DeviceWrapError {
    #[error("the key wrap is truncated or has trailing bytes")]
    Malformed,
    #[error("the key wrap declares version {found}; this build supports {expected}")]
    UnsupportedVersion { found: u8, expected: u8 },
    #[error("the key wrap is addressed to another device")]
    WrongRecipient,
    #[error("the key wrap installs epoch {found}, not the demanded epoch {expected}")]
    WrongEpoch { found: u32, expected: u32 },
    #[error("the key wrap's device signature does not verify")]
    Signature,
    #[error("the key wrap does not open with this device key")]
    Authentication,
}

/// Seals `master` to one surviving device at `epoch`.
///
/// `recipient_id` and `recipient_key` are the device's roster identity, `sender_id` names the
/// revoker, and `signer` is the revoker's Ed25519 key: the recipient checks both, so a wrap cannot
/// be replayed under another name or produced by a device the head did not vouch for.
pub fn wrap_master(
    master: &MasterKey,
    recipient_id: &str,
    recipient_key: &[u8; X25519_KEY_LEN],
    epoch: u32,
    sender_id: &str,
    signer: &SigningKey,
) -> Result<Vec<u8>, DeviceWrapError> {
    if recipient_id.len() > MAX_DEVICE_ID || sender_id.len() > MAX_DEVICE_ID {
        return Err(DeviceWrapError::Malformed);
    }

    let mut ephemeral_bytes = Zeroizing::new([0u8; X25519_KEY_LEN]);
    rand::rng().fill_bytes(&mut ephemeral_bytes[..]);
    let ephemeral = StaticSecret::from(*ephemeral_bytes);
    let ephemeral_public = PublicKey::from(&ephemeral).to_bytes();

    let shared = ephemeral.diffie_hellman(&PublicKey::from(*recipient_key));
    if !shared.was_contributory() {
        return Err(DeviceWrapError::Malformed);
    }

    let mut nonce = [0u8; WRAP_NONCE_LEN];
    rand::rng().fill_bytes(&mut nonce);

    let key = Zeroizing::new(expand(shared.as_bytes()));
    let cipher = ChaCha20Poly1305::new(
        &Key::try_from(&key[..]).expect("a derived wrap key is exactly 32 bytes"),
    );
    let header = header_bytes(
        epoch,
        recipient_id,
        recipient_key,
        sender_id,
        &ephemeral_public,
        &nonce,
    );
    let ciphertext = cipher
        .encrypt(
            &Nonce::try_from(&nonce[..]).expect("a wrap nonce is exactly 12 bytes"),
            Payload {
                msg: &master.expose_secret()[..],
                aad: &header,
            },
        )
        .map_err(|_| DeviceWrapError::Malformed)?;

    let mut unsigned = header;
    unsigned.extend_from_slice(&(ciphertext.len() as u16).to_be_bytes());
    unsigned.extend_from_slice(&ciphertext);
    let signature = signer.sign(&unsigned).to_bytes();

    let mut out = Vec::with_capacity(unsigned.len() + signature.len());
    out.extend_from_slice(&unsigned);
    out.extend_from_slice(&signature);
    Ok(out)
}

/// Opens a wrap addressed to this device at exactly `expected_epoch`.
///
/// The caller must pass the verifying key of the device that signed the verified head it is
/// following; `sender_id` inside the wrap is returned so the caller can check it names that device.
pub fn open_master(
    secret: &DeviceSecretKey,
    recipient_id: &str,
    expected_epoch: u32,
    sender: &VerifyingKey,
    bytes: &[u8],
) -> Result<(MasterKey, String), DeviceWrapError> {
    let wrap = parse(bytes)?;
    if wrap.epoch != expected_epoch {
        return Err(DeviceWrapError::WrongEpoch {
            found: wrap.epoch,
            expected: expected_epoch,
        });
    }
    if wrap.recipient != recipient_id {
        return Err(DeviceWrapError::WrongRecipient);
    }
    sender
        .verify_strict(&wrap.signed, &Signature::from_bytes(&wrap.signature))
        .map_err(|_| DeviceWrapError::Signature)?;

    let static_secret = StaticSecret::from(*secret.0);
    let shared = static_secret.diffie_hellman(&PublicKey::from(wrap.ephemeral_key));
    if !shared.was_contributory() {
        return Err(DeviceWrapError::Authentication);
    }
    let key = Zeroizing::new(expand(shared.as_bytes()));
    let cipher = ChaCha20Poly1305::new(
        &Key::try_from(&key[..]).expect("a derived wrap key is exactly 32 bytes"),
    );
    let plaintext = cipher
        .decrypt(
            &Nonce::try_from(&wrap.nonce[..]).expect("a wrap nonce is exactly 12 bytes"),
            Payload {
                msg: &wrap.ciphertext,
                aad: &wrap.unsigned,
            },
        )
        .map_err(|_| DeviceWrapError::Authentication)?;
    let master = <[u8; MASTER_KEY_LEN]>::try_from(plaintext.as_slice())
        .map(MasterKey::from_bytes)
        .map_err(|_| DeviceWrapError::Authentication)?;
    Ok((master, wrap.sender))
}

fn expand(shared: &[u8; X25519_KEY_LEN]) -> [u8; MASTER_KEY_LEN] {
    let hkdf = Hkdf::<Sha256>::new(Some(WRAP_DOMAIN), shared);
    let mut out = [0u8; MASTER_KEY_LEN];
    hkdf.expand(WRAP_INFO, &mut out[..])
        .expect("32 bytes is a valid HKDF-SHA256 output length");
    out
}

/// The canonical, signed header the AEAD binds: domain, version, epoch, both ids (length
/// prefixed) and the three public values that decide the key.
fn header_bytes(
    epoch: u32,
    recipient_id: &str,
    recipient_key: &[u8; X25519_KEY_LEN],
    sender_id: &str,
    ephemeral_key: &[u8; X25519_KEY_LEN],
    nonce: &[u8; WRAP_NONCE_LEN],
) -> Vec<u8> {
    let mut out = Vec::with_capacity(
        WRAP_DOMAIN.len()
            + 1
            + 4
            + 2
            + recipient_id.len()
            + X25519_KEY_LEN
            + 2
            + sender_id.len()
            + X25519_KEY_LEN
            + WRAP_NONCE_LEN,
    );
    out.extend_from_slice(WRAP_DOMAIN);
    out.push(DEVICE_WRAP_VERSION);
    out.extend_from_slice(&epoch.to_be_bytes());
    out.extend_from_slice(&(recipient_id.len() as u16).to_be_bytes());
    out.extend_from_slice(recipient_id.as_bytes());
    out.extend_from_slice(recipient_key);
    out.extend_from_slice(&(sender_id.len() as u16).to_be_bytes());
    out.extend_from_slice(sender_id.as_bytes());
    out.extend_from_slice(ephemeral_key);
    out.extend_from_slice(nonce);
    out
}

#[derive(Debug)]
struct ParsedWrap {
    epoch: u32,
    recipient: String,
    sender: String,
    ephemeral_key: [u8; X25519_KEY_LEN],
    nonce: [u8; WRAP_NONCE_LEN],
    ciphertext: Vec<u8>,
    signature: [u8; 64],
    /// The canonical header: the AEAD's AAD.
    unsigned: Vec<u8>,
    /// Every byte the sender signed: the header, the ciphertext length and the ciphertext.
    signed: Vec<u8>,
}

fn parse(bytes: &[u8]) -> Result<ParsedWrap, DeviceWrapError> {
    if bytes.len() > MAX_WRAP_BYTES {
        return Err(DeviceWrapError::Malformed);
    }
    let mut cursor = 0usize;
    take(bytes, &mut cursor, WRAP_DOMAIN.len())?;
    let version = take(bytes, &mut cursor, 1)?[0];
    if version != DEVICE_WRAP_VERSION {
        return Err(DeviceWrapError::UnsupportedVersion {
            found: version,
            expected: DEVICE_WRAP_VERSION,
        });
    }
    let epoch = u32::from_be_bytes(take(bytes, &mut cursor, 4)?.try_into().expect("4 bytes"));
    let recipient_len = usize::from(u16::from_be_bytes(
        take(bytes, &mut cursor, 2)?.try_into().expect("2 bytes"),
    ));
    if recipient_len > MAX_DEVICE_ID {
        return Err(DeviceWrapError::Malformed);
    }
    let recipient = std::str::from_utf8(take(bytes, &mut cursor, recipient_len)?)
        .map_err(|_| DeviceWrapError::Malformed)?
        .to_string();
    let recipient_key: [u8; X25519_KEY_LEN] = take(bytes, &mut cursor, X25519_KEY_LEN)?
        .try_into()
        .expect("32 bytes");
    let sender_len = usize::from(u16::from_be_bytes(
        take(bytes, &mut cursor, 2)?.try_into().expect("2 bytes"),
    ));
    if sender_len > MAX_DEVICE_ID {
        return Err(DeviceWrapError::Malformed);
    }
    let sender = std::str::from_utf8(take(bytes, &mut cursor, sender_len)?)
        .map_err(|_| DeviceWrapError::Malformed)?
        .to_string();
    let ephemeral_key: [u8; X25519_KEY_LEN] = take(bytes, &mut cursor, X25519_KEY_LEN)?
        .try_into()
        .expect("32 bytes");
    let nonce: [u8; WRAP_NONCE_LEN] = take(bytes, &mut cursor, WRAP_NONCE_LEN)?
        .try_into()
        .expect("12 bytes");

    // The header the sender signed is exactly these bytes; rebuild it from the parsed fields so
    // the check never depends on the transport layout.
    let unsigned = header_bytes(
        epoch,
        &recipient,
        &recipient_key,
        &sender,
        &ephemeral_key,
        &nonce,
    );
    if bytes.get(..cursor) != Some(unsigned.as_slice()) {
        return Err(DeviceWrapError::Malformed);
    }

    let ciphertext_len = usize::from(u16::from_be_bytes(
        take(bytes, &mut cursor, 2)?.try_into().expect("2 bytes"),
    ));
    if ciphertext_len != MASTER_KEY_LEN + TAG_LEN {
        return Err(DeviceWrapError::Malformed);
    }
    let ciphertext = take(bytes, &mut cursor, ciphertext_len)?.to_vec();
    let signed = bytes[..cursor].to_vec();
    let signature: [u8; 64] = take(bytes, &mut cursor, 64)?.try_into().expect("64 bytes");
    if cursor != bytes.len() {
        return Err(DeviceWrapError::Malformed);
    }
    Ok(ParsedWrap {
        epoch,
        recipient,
        sender,
        ephemeral_key,
        nonce,
        ciphertext,
        signature,
        unsigned,
        signed,
    })
}

fn take<'a>(bytes: &'a [u8], cursor: &mut usize, len: usize) -> Result<&'a [u8], DeviceWrapError> {
    let end = cursor.checked_add(len).ok_or(DeviceWrapError::Malformed)?;
    let slice = bytes.get(*cursor..end).ok_or(DeviceWrapError::Malformed)?;
    *cursor = end;
    Ok(slice)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signer(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    fn recipient(seed: u8) -> DeviceSecretKey {
        DeviceSecretKey::from_bytes([seed; 32])
    }

    fn master() -> MasterKey {
        MasterKey::from_bytes([9u8; 32])
    }

    #[test]
    fn a_wrap_round_trips_to_its_recipient() {
        let device = recipient(1);
        let sender = signer(2);
        let id = "aa".repeat(32);
        let sender_id = "bb".repeat(32);
        let bytes = wrap_master(&master(), &id, &device.public_key(), 1, &sender_id, &sender)
            .expect("wrap");
        let (opened, from) =
            open_master(&device, &id, 1, &sender.verifying_key(), &bytes).expect("open");
        assert_eq!(opened.expose_secret(), master().expose_secret());
        assert_eq!(from, sender_id);
    }

    #[test]
    fn a_wrap_for_another_device_does_not_open() {
        let addressed = recipient(1);
        let other = recipient(2);
        let sender = signer(3);
        let id = "aa".repeat(32);
        let bytes =
            wrap_master(&master(), &id, &addressed.public_key(), 1, "bb", &sender).expect("wrap");
        // Even with the right id and epoch, the other device cannot complete the DH.
        let error = open_master(&other, &id, 1, &sender.verifying_key(), &bytes).unwrap_err();
        assert_eq!(error, DeviceWrapError::Authentication);
        // Relabelling the recipient id does not help either: it is inside the AAD and signature.
        let error = open_master(
            &addressed,
            &"cc".repeat(32),
            1,
            &sender.verifying_key(),
            &bytes,
        )
        .unwrap_err();
        assert_eq!(error, DeviceWrapError::WrongRecipient);
    }

    #[test]
    fn a_stale_wrap_is_rejected_for_a_later_epoch() {
        let device = recipient(1);
        let sender = signer(2);
        let id = "aa".repeat(32);
        let bytes =
            wrap_master(&master(), &id, &device.public_key(), 1, "bb", &sender).expect("wrap");
        let error = open_master(&device, &id, 2, &sender.verifying_key(), &bytes).unwrap_err();
        assert_eq!(
            error,
            DeviceWrapError::WrongEpoch {
                found: 1,
                expected: 2
            }
        );
    }

    #[test]
    fn a_tampered_wrap_fails_signature_or_authentication() {
        let device = recipient(1);
        let sender = signer(2);
        let id = "aa".repeat(32);
        let bytes =
            wrap_master(&master(), &id, &device.public_key(), 1, "bb", &sender).expect("wrap");
        let mut tampered = bytes.clone();
        let last = tampered.len() - 1;
        tampered[last] ^= 0x01;
        let error = open_master(&device, &id, 1, &sender.verifying_key(), &tampered).unwrap_err();
        assert_eq!(error, DeviceWrapError::Signature);

        let mut body = bytes.clone();
        // Flip a ciphertext byte, then re-sign so only the AEAD can catch it.
        let body_at = body.len() - 65;
        body[body_at] ^= 0x01;
        let unsigned = &body[..body.len() - 64];
        let signature = sender.sign(unsigned).to_bytes();
        body.truncate(body.len() - 64);
        body.extend_from_slice(&signature);
        let error = open_master(&device, &id, 1, &sender.verifying_key(), &body).unwrap_err();
        assert_eq!(error, DeviceWrapError::Authentication);
    }

    #[test]
    fn another_signer_is_refused() {
        let device = recipient(1);
        let sender = signer(2);
        let id = "aa".repeat(32);
        let bytes =
            wrap_master(&master(), &id, &device.public_key(), 1, "bb", &sender).expect("wrap");
        let error = open_master(&device, &id, 1, &signer(9).verifying_key(), &bytes).unwrap_err();
        assert_eq!(error, DeviceWrapError::Signature);
    }

    #[test]
    fn trailing_bytes_and_bad_lengths_are_refused() {
        let device = recipient(1);
        let sender = signer(2);
        let id = "aa".repeat(32);
        let mut bytes =
            wrap_master(&master(), &id, &device.public_key(), 1, "bb", &sender).expect("wrap");
        bytes.push(0);
        assert_eq!(parse(&bytes).unwrap_err(), DeviceWrapError::Malformed);
        assert_eq!(parse(&bytes[..10]).unwrap_err(), DeviceWrapError::Malformed);
    }

    #[test]
    fn debug_never_prints_key_material() {
        let device = recipient(1);
        let text = format!("{device:?}");
        assert!(!text.contains("0101"));
        assert_eq!(text, "DeviceSecretKey(<redacted>)");
    }
}
