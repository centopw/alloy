//! Secret sealing for connector session material.

use chacha20poly1305::aead::{Aead, Generate, KeyInit, Payload};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};

use crate::error::StoreError;

/// Bumped only when the sealing scheme changes; stored next to every ciphertext.
pub(crate) const KEY_VERSION: u32 = 1;

pub(crate) struct Sealed {
    pub nonce: Vec<u8>,
    pub ciphertext: Vec<u8>,
}

/// Encrypt with XChaCha20-Poly1305 under a random 24-byte nonce, binding the ciphertext to
/// `aad` (the account id).
pub(crate) fn seal(key: &[u8; 32], aad: &[u8], plaintext: &[u8]) -> Result<Sealed, StoreError> {
    let cipher = XChaCha20Poly1305::new(&Key::from(*key));
    let nonce = XNonce::generate();
    let ciphertext = cipher
        .encrypt(
            &nonce,
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| StoreError::Encrypt)?;
    Ok(Sealed {
        nonce: nonce.to_vec(),
        ciphertext,
    })
}

pub(crate) fn open(
    key: &[u8; 32],
    aad: &[u8],
    nonce: &[u8],
    ciphertext: &[u8],
    account_id: &str,
) -> Result<Vec<u8>, StoreError> {
    let nonce = XNonce::try_from(nonce).map_err(|_| StoreError::Decrypt {
        account_id: account_id.to_owned(),
    })?;
    let cipher = XChaCha20Poly1305::new(&Key::from(*key));
    cipher
        .decrypt(
            &nonce,
            Payload {
                msg: ciphertext,
                aad,
            },
        )
        .map_err(|_| StoreError::Decrypt {
            account_id: account_id.to_owned(),
        })
}
