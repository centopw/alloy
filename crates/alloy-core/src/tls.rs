//! TLS root-certificate configuration for HTTP clients.
//!
//! On Android, reqwest's default verifier is the rustls platform verifier,
//! which must be initialised through JNI before first use. This crate cannot
//! perform that initialisation (`#![forbid(unsafe_code)]`), so every HTTP
//! client built on Android gets an explicit root store read from the OS
//! certificate directories instead. Other targets keep reqwest's defaults.

use std::collections::HashSet;
use std::path::Path;
#[cfg(target_os = "android")]
use std::sync::LazyLock;
use std::time::Duration;

use rustls::RootCertStore;
use rustls::pki_types::{CertificateDer, pem::PemObject};
use sha2::{Digest, Sha256};

/// Android certificate directories, in load order. Missing directories are skipped.
#[cfg(target_os = "android")]
const ANDROID_CA_DIRS: [&str; 3] = [
    "/apex/com.android.conscrypt/cacerts",
    "/system/etc/security/cacerts",
    "/data/misc/user/0/cacerts-added",
];

#[cfg(target_os = "android")]
static SYSTEM_ROOTS: LazyLock<Vec<reqwest::Certificate>> = LazyLock::new(|| {
    let dirs: Vec<&Path> = ANDROID_CA_DIRS.iter().map(Path::new).collect();
    let certs = load_pem_certificates(&dirs);
    if certs.is_empty() {
        tracing::error!("no OS root certificates loaded; HTTPS requests will fail");
    }
    certs
});

/// Configures `builder` to trust the OS root certificates instead of the
/// platform verifier. On Android, if no certificate can be loaded, the builder
/// still uses an empty root store, so TLS handshakes fail with an error rather
/// than reaching the platform verifier, which aborts the process.
#[cfg(target_os = "android")]
pub fn with_system_roots(builder: reqwest::ClientBuilder) -> reqwest::ClientBuilder {
    builder.tls_certs_only(SYSTEM_ROOTS.iter().cloned())
}

/// Non-Android targets keep reqwest's default certificate verification.
#[cfg(not(target_os = "android"))]
pub fn with_system_roots(builder: reqwest::ClientBuilder) -> reqwest::ClientBuilder {
    builder
}

/// Builds the HTTP client handed to matrix-sdk via `ClientBuilder::http_client`.
///
/// A supplied client replaces matrix-sdk's own HTTP settings, so the defaults it would
/// otherwise apply are repeated here: an `alloy/<version>` user agent, a TLS 1.2 minimum
/// (BCP 195) and a 30 second request timeout.
pub fn matrix_http_client() -> Result<reqwest::Client, reqwest::Error> {
    with_system_roots(
        reqwest::Client::builder()
            .user_agent(concat!("alloy/", env!("CARGO_PKG_VERSION")))
            .min_tls_version(reqwest::tls::Version::TLS_1_2)
            .timeout(Duration::from_secs(30)),
    )
    .build()
}

/// Reads every file in `dirs` (directories in order, files sorted by name) and
/// returns the PEM certificates the TLS stack accepts as trust anchors.
///
/// Unreadable directories and files are skipped. Files with identical content
/// are loaded once. Content that is not a valid certificate is skipped.
pub fn load_pem_certificates(dirs: &[&Path]) -> Vec<reqwest::Certificate> {
    let mut seen: HashSet<[u8; 32]> = HashSet::new();
    let mut scratch = RootCertStore::empty();
    let mut certs = Vec::new();

    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        let mut paths: Vec<_> = entries.flatten().map(|entry| entry.path()).collect();
        paths.sort();

        for path in paths {
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            let digest: [u8; 32] = Sha256::digest(&bytes).into();
            if !seen.insert(digest) {
                continue;
            }
            for der in CertificateDer::pem_slice_iter(&bytes).flatten() {
                if scratch.add(der.clone()).is_err() {
                    continue;
                }
                if let Ok(cert) = reqwest::Certificate::from_der(der.as_ref()) {
                    certs.push(cert);
                }
            }
        }
    }

    certs
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Public self-signed test certificate (no private key).
    const TEST_CERT_PEM: &str = "-----BEGIN CERTIFICATE-----
MIIBcjCCARegAwIBAgITEkMRFb+tyo8fTIH12rHOitnAUTAKBggqhkjOPQQDAjAe
MRwwGgYDVQQDDBNhbGxveS10bHMtdW5pdC10ZXN0MCAXDTI2MTAxODA5NDE1N1oY
DzIxMjYwOTI0MDk0MTU3WjAeMRwwGgYDVQQDDBNhbGxveS10bHMtdW5pdC10ZXN0
MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAEe/Kn3vV8Nvs3jCyXWiMNBwF2ExEM
aU7EGeg/XB7W4QBc+8kctiij86zq3rN+hk8VbJsQOLD+ePC+iWXO1aBJX6MyMDAw
HQYDVR0OBBYEFNB9AzR7dmRNn27p/ZBp0uAlpMldMA8GA1UdEwEB/wQFMAMBAf8w
CgYIKoZIzj0EAwIDSQAwRgIhAOrFYoXCoHmXLETPKBj+fAf5kt0zXuev7iMIctjd
0IcOAiEAxjccBiMr3kLrbWTJF1jquCCMeif/FVdcTOuBIOBL65I=
-----END CERTIFICATE-----
";

    #[test]
    fn loads_valid_certificates_and_skips_junk_and_duplicates() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.pem"), TEST_CERT_PEM).unwrap();
        std::fs::write(dir.path().join("b-copy.pem"), TEST_CERT_PEM).unwrap();
        std::fs::write(dir.path().join("junk"), "not a certificate").unwrap();
        let missing = dir.path().join("does-not-exist");

        let certs = load_pem_certificates(&[dir.path(), missing.as_path()]);

        assert_eq!(certs.len(), 1);
    }
}
