//! Endpoint pinning for the sync client (ADR 0020 invariant 12).
//!
//! The client never trusts the system root store for this endpoint: the pinned SPKI SHA-256 is
//! the trust anchor. On the first successful `ConfigureSync` the verifier accepts the leaf it
//! saw and records its pin; every later run compares the leaf against that pin and fails the
//! handshake before a single request (and therefore the bearer token) is sent.
//!
//! Extracting the SubjectPublicKeyInfo from the leaf is ASN.1 framing only, not cryptography:
//! `Certificate ::= SEQUENCE { tbsCertificate, ... }`, and the SPKI is the seventh field of
//! `TBSCertificate` after the optional version tag. Every length is bounded before a slice is
//! taken.

use std::fmt;
use std::sync::Mutex;

use alloy_sync::Pin;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{WebPkiSupportedAlgorithms, verify_tls12_signature, verify_tls13_signature};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{CertificateError, DigitallySignedStruct, Error as TlsError, SignatureScheme};

/// The verifier one HTTPS client is built with.
pub struct EndpointVerifier {
    /// `Some` after the first pin: the handshake fails unless the leaf hashes to this pin.
    expected: Option<Pin>,
    observed: Mutex<Option<Pin>>,
    algorithms: WebPkiSupportedAlgorithms,
}

impl EndpointVerifier {
    pub fn new(expected: Option<Pin>, algorithms: WebPkiSupportedAlgorithms) -> Self {
        Self {
            expected,
            observed: Mutex::new(None),
            algorithms,
        }
    }

    /// The leaf pin this verifier saw during the handshake, when one was presented.
    pub fn observed(&self) -> Option<Pin> {
        *self
            .observed
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }
}

impl fmt::Debug for EndpointVerifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EndpointVerifier")
            .field("expected", &self.expected)
            .field("observed", &self.observed())
            .finish()
    }
}

impl ServerCertVerifier for EndpointVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, TlsError> {
        let spki = spki_der(end_entity.as_ref())
            .ok_or(TlsError::InvalidCertificate(CertificateError::BadEncoding))?;
        let presented = Pin::of_spki(spki);
        *self
            .observed
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = Some(presented);
        match self.expected {
            Some(expected) if expected != presented => Err(TlsError::InvalidCertificate(
                CertificateError::ApplicationVerificationFailure,
            )),
            _ => Ok(ServerCertVerified::assertion()),
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        verify_tls12_signature(message, cert, dss, &self.algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        verify_tls13_signature(message, cert, dss, &self.algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algorithms.supported_schemes()
    }
}

struct Tlv<'a> {
    tag: u8,
    body: &'a [u8],
    full: &'a [u8],
}

/// One DER TLV, bounded by the input. Long-form lengths are accepted up to three bytes; anything
/// larger is a certificate no real endpoint presents and is refused rather than guessed at.
fn tlv(bytes: &[u8]) -> Option<Tlv<'_>> {
    let tag = *bytes.first()?;
    let first = *bytes.get(1)?;
    let (len, header) = if first & 0x80 == 0 {
        (usize::from(first), 2)
    } else {
        let count = usize::from(first & 0x7f);
        if count == 0 || count > 3 {
            return None;
        }
        let mut len = 0usize;
        for index in 0..count {
            len = (len << 8) | usize::from(*bytes.get(2 + index)?);
        }
        (len, 2 + count)
    };
    let end = header.checked_add(len)?;
    Some(Tlv {
        tag,
        body: bytes.get(header..end)?,
        full: bytes.get(..end)?,
    })
}

/// The SubjectPublicKeyInfo TLV (tag, length and contents) of a DER X.509 certificate.
pub fn spki_der(certificate: &[u8]) -> Option<&[u8]> {
    let outer = tlv(certificate)?;
    if outer.tag != 0x30 {
        return None;
    }
    let tbs = tlv(outer.body)?;
    if tbs.tag != 0x30 {
        return None;
    }
    let mut cursor = tbs.body;
    // version [0] EXPLICIT is optional; serialNumber, signature, issuer, validity, subject follow.
    if cursor.first() == Some(&0xa0) {
        let consumed = tlv(cursor)?.full.len();
        cursor = cursor.get(consumed..)?;
    }
    for _ in 0..5 {
        let consumed = tlv(cursor)?.full.len();
        cursor = cursor.get(consumed..)?;
    }
    let spki = tlv(cursor)?;
    if spki.tag != 0x30 {
        return None;
    }
    Some(spki.full)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_certificate_without_a_version_tag_still_locates_the_spki() {
        // SEQUENCE { SEQUENCE { serial INTEGER, signature SEQ, issuer SEQ, validity SEQ,
        // subject SEQ, SPKI SEQ { 0x01 0x02 } }, signature SEQ }
        let spki = [0x30, 0x02, 0x01, 0x02];
        let mut tbs = Vec::new();
        for _ in 0..5 {
            tbs.extend_from_slice(&[0x30, 0x00]);
        }
        tbs.extend_from_slice(&spki);
        tbs.extend_from_slice(&[0x30, 0x00]);
        let mut body = vec![0x30, tbs.len() as u8];
        body.extend_from_slice(&tbs);
        let mut cert = vec![0x30, body.len() as u8];
        cert.extend_from_slice(&body);
        assert_eq!(spki_der(&cert), Some(&spki[..]));
    }
}
