//! TLS root-certificate configuration for the app's own HTTP clients.
//!
//! On Android, reqwest's default verifier is the rustls platform verifier, which must be
//! initialised through JNI before first use. The app cannot perform that initialisation (the
//! workspace forbids `unsafe`), so every client built on Android gets an explicit root store read
//! from the OS certificate directories instead. Other targets keep reqwest's defaults.
//!
//! This lived in `alloy-core` while Matrix was the only HTTP client; Matrix is gone (ADR 0016) and
//! the remaining mobile client is the update check, so the helper lives next to its caller.

#[cfg(target_os = "android")]
use std::collections::HashSet;
#[cfg(target_os = "android")]
use std::path::Path;
#[cfg(target_os = "android")]
use std::sync::LazyLock;

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

/// Configures `builder` to trust the OS root certificates instead of the platform verifier. On
/// Android, if no certificate can be loaded, the builder still uses an empty root store, so TLS
/// handshakes fail with an error rather than reaching the platform verifier, which aborts the
/// process.
#[cfg(target_os = "android")]
pub fn with_system_roots(builder: reqwest::ClientBuilder) -> reqwest::ClientBuilder {
    builder.tls_certs_only(SYSTEM_ROOTS.iter().cloned())
}

/// Non-Android targets keep reqwest's default certificate verification.
#[cfg(not(target_os = "android"))]
pub fn with_system_roots(builder: reqwest::ClientBuilder) -> reqwest::ClientBuilder {
    builder
}

/// Reads every file in `dirs` (directories in order, files sorted by name) and returns the PEM
/// certificates the TLS stack accepts as trust anchors.
///
/// Unreadable directories and files are skipped. Files with identical content are loaded once.
/// Content that is not a valid certificate is skipped: one malformed file in the OS store must not
/// make the client unbuildable.
#[cfg(target_os = "android")]
fn load_pem_certificates(dirs: &[&Path]) -> Vec<reqwest::Certificate> {
    use rustls::pki_types::{CertificateDer, pem::PemObject};

    let mut seen: HashSet<Vec<u8>> = HashSet::new();
    let mut valid = rustls::RootCertStore::empty();
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
            if !seen.insert(bytes.clone()) {
                continue;
            }
            for der in CertificateDer::pem_slice_iter(&bytes).flatten() {
                if valid.add(der.clone()).is_err() {
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
