//! The sync server's HTTP contract, and nothing above it.
//!
//! One small API (ADR 0020): `GET`/`PUT` of `head`, `escrow`, `roster` and opaque blobs, guarded
//! by a bearer token. `head` is compare-and-swap: the server answers `409` with the bytes and
//! `ETag` that won, so the caller can rebase instead of overwriting a competing writer.

use std::sync::Arc;
use std::time::Duration;

use alloy_protocol::EngineError;
use alloy_sync::Pin;
use reqwest::StatusCode;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, ETAG, IF_MATCH, IF_NONE_MATCH};
use reqwest::{Client, Url};
use rustls::crypto::CryptoProvider;

use super::tls::EndpointVerifier;

/// Body type every write must declare; the server answers `415` otherwise.
const OCTET_STREAM: &str = "application/octet-stream";

/// What a `PUT /v1/head` asked for.
#[derive(Debug, Clone)]
pub enum Precondition {
    /// `If-None-Match: *`: only while the resource does not exist.
    CreateOnly,
    /// `If-Match`: only while the current tag is exactly this one.
    Match(String),
}

/// The outcome of a compare-and-swap write.
#[derive(Debug, Clone)]
pub enum WriteOutcome {
    Written,
    /// A competing writer won (`409` or `412`); `bytes`/`etag` are the current head when the
    /// server returned it.
    Conflict {
        bytes: Option<Vec<u8>>,
        etag: Option<String>,
    },
}

/// One configured endpoint.
pub struct Endpoint {
    base: Url,
    token: String,
    client: Client,
    verifier: Option<Arc<EndpointVerifier>>,
    /// The pin the client was built to enforce, so a failed handshake can name both sides.
    expected_pin: Option<Pin>,
}

impl std::fmt::Debug for Endpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Endpoint")
            .field("base", &self.base)
            .field("verifier", &self.verifier)
            .finish_non_exhaustive()
    }
}

/// A parsed, validated endpoint URL.
#[derive(Debug)]
pub struct Target {
    pub base: Url,
    pub secure: bool,
}

/// Parses `raw` and refuses anything the transport must not carry.
///
/// `https` is accepted for any host. Plain `http` is accepted only for `127.0.0.1`, `::1` or
/// `localhost`: there is no insecure escape hatch for the internet.
pub fn parse_target(raw: &str) -> Result<Target, EngineError> {
    let base = Url::parse(raw.trim())
        .map_err(|_| EngineError::invalid_argument("the sync URL is not a valid URL"))?;
    if !base.username().is_empty() || base.password().is_some() {
        return Err(EngineError::invalid_argument(
            "the sync URL must not carry credentials; the token is configured separately",
        ));
    }
    if base.query().is_some() || base.fragment().is_some() {
        return Err(EngineError::invalid_argument(
            "the sync URL must not carry a query or fragment",
        ));
    }
    let host = base
        .host_str()
        .ok_or_else(|| EngineError::invalid_argument("the sync URL has no host"))?;
    let bare_host = host.trim_start_matches('[').trim_end_matches(']');
    match base.scheme() {
        "https" => Ok(Target { base, secure: true }),
        "http" => {
            if matches!(bare_host, "127.0.0.1" | "::1" | "localhost") {
                Ok(Target {
                    base,
                    secure: false,
                })
            } else {
                Err(EngineError::invalid_argument(format!(
                    "plain http is only allowed for a loopback sync endpoint, not '{host}'"
                )))
            }
        }
        other => Err(EngineError::invalid_argument(format!(
            "the sync URL scheme '{other}' is not supported"
        ))),
    }
}

impl Endpoint {
    /// Builds the HTTP client for `target`. `pin` is enforced on the TLS handshake; `None`
    /// captures the leaf's pin instead (first configure).
    pub fn new(target: &Target, token: String, pin: Option<Pin>) -> Result<Self, EngineError> {
        let mut builder = Client::builder()
            .timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none());
        let mut verifier = None;
        if target.secure {
            let provider = CryptoProvider::get_default()
                .cloned()
                .unwrap_or_else(|| Arc::new(rustls::crypto::aws_lc_rs::default_provider()));
            let algorithms = provider.signature_verification_algorithms;
            let endpoint_verifier = Arc::new(EndpointVerifier::new(pin, algorithms));
            let config = rustls::ClientConfig::builder_with_provider(provider)
                .with_protocol_versions(&[&rustls::version::TLS13])
                .map_err(|_| EngineError::internal_error("cannot build the TLS client"))?
                .dangerous()
                .with_custom_certificate_verifier(endpoint_verifier.clone())
                .with_no_client_auth();
            builder = builder.tls_backend_preconfigured(config);
            verifier = Some(endpoint_verifier);
        }
        let client = builder
            .build()
            .map_err(|_| EngineError::internal_error("cannot build the HTTP client"))?;
        Ok(Self {
            base: target.base.clone(),
            token,
            client,
            verifier,
            expected_pin: pin,
        })
    }

    /// The pin the TLS leaf presented during the last attempt, if any.
    pub fn observed_pin(&self) -> Option<Pin> {
        self.verifier
            .as_ref()
            .and_then(|verifier| verifier.observed())
    }

    /// Maps a transport failure, preferring a pin mismatch when the verifier saw one.
    fn transport_error(&self, error: reqwest::Error) -> EngineError {
        if let Some(pin) = self.expected_pin {
            match self.observed_pin() {
                Some(observed) if observed != pin => {
                    return EngineError::pin_mismatch(&pin.to_hex(), &observed.to_hex());
                }
                _ => {}
            }
        }
        // Never include the URL's query or the token; reqwest's own message carries neither.
        EngineError::network_error(format!("the sync server could not be reached: {error}"))
    }

    fn url(&self, path: &str) -> Result<Url, EngineError> {
        self.base
            .join(path)
            .map_err(|_| EngineError::internal_error("cannot build a sync request URL"))
    }

    fn request(
        &self,
        method: reqwest::Method,
        path: &str,
    ) -> Result<reqwest::RequestBuilder, EngineError> {
        let url = self.url(path)?;
        Ok(self
            .client
            .request(method, url)
            .header(AUTHORIZATION, format!("Bearer {}", self.token)))
    }

    /// `GET /v1/head`: `None` when the server has never stored one.
    pub async fn get_head(&self) -> Result<Option<(Vec<u8>, String)>, EngineError> {
        let response = self
            .request(reqwest::Method::GET, "/v1/head")?
            .send()
            .await
            .map_err(|error| self.transport_error(error))?;
        match response.status() {
            StatusCode::OK => {
                let etag = header(response.headers(), ETAG);
                let bytes = response
                    .bytes()
                    .await
                    .map_err(|error| self.transport_error(error))?;
                Ok(Some((bytes.to_vec(), etag.unwrap_or_default())))
            }
            StatusCode::NOT_FOUND => Ok(None),
            status => Err(map_status(status)),
        }
    }

    /// `PUT /v1/head` under `precondition`.
    pub async fn put_head(
        &self,
        bytes: &[u8],
        precondition: Precondition,
    ) -> Result<WriteOutcome, EngineError> {
        let mut request = self
            .request(reqwest::Method::PUT, "/v1/head")?
            .header(CONTENT_TYPE, OCTET_STREAM);
        request = match &precondition {
            Precondition::CreateOnly => request.header(IF_NONE_MATCH, "*"),
            Precondition::Match(etag) => request.header(IF_MATCH, etag.clone()),
        };
        let response = request
            .body(bytes.to_vec())
            .send()
            .await
            .map_err(|error| self.transport_error(error))?;
        match response.status() {
            StatusCode::NO_CONTENT | StatusCode::OK => Ok(WriteOutcome::Written),
            StatusCode::CONFLICT | StatusCode::PRECONDITION_FAILED => {
                let etag = header(response.headers(), ETAG);
                let body = response
                    .bytes()
                    .await
                    .map_err(|error| self.transport_error(error))?;
                Ok(WriteOutcome::Conflict {
                    bytes: (!body.is_empty()).then(|| body.to_vec()),
                    etag,
                })
            }
            status => Err(map_status(status)),
        }
    }

    /// `GET /v1/blob/{id}` with an already derived opaque id.
    pub async fn get_blob(&self, id: &str) -> Result<Option<Vec<u8>>, EngineError> {
        let response = self
            .request(reqwest::Method::GET, &format!("/v1/blob/{id}"))?
            .send()
            .await
            .map_err(|error| self.transport_error(error))?;
        match response.status() {
            StatusCode::OK => Ok(Some(
                response
                    .bytes()
                    .await
                    .map_err(|error| self.transport_error(error))?
                    .to_vec(),
            )),
            StatusCode::NOT_FOUND => Ok(None),
            status => Err(map_status(status)),
        }
    }

    /// `PUT /v1/blob/{id}`, overwrite allowed by the server.
    pub async fn put_blob(&self, id: &str, bytes: Vec<u8>) -> Result<(), EngineError> {
        let response = self
            .request(reqwest::Method::PUT, &format!("/v1/blob/{id}"))?
            .header(CONTENT_TYPE, OCTET_STREAM)
            .body(bytes)
            .send()
            .await
            .map_err(|error| self.transport_error(error))?;
        match response.status() {
            StatusCode::NO_CONTENT | StatusCode::OK => Ok(()),
            status => Err(map_status(status)),
        }
    }
}

fn header(
    headers: &reqwest::header::HeaderMap,
    name: reqwest::header::HeaderName,
) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
}

fn map_status(status: StatusCode) -> EngineError {
    match status {
        StatusCode::UNAUTHORIZED => EngineError::auth_failed_with(
            "sync_token_invalid",
            "the sync server rejected this device's token",
        ),
        StatusCode::PAYLOAD_TOO_LARGE => EngineError::payload_too_large(),
        StatusCode::TOO_MANY_REQUESTS => EngineError::rate_limited(),
        StatusCode::BAD_REQUEST => {
            EngineError::invalid_argument("the sync server rejected the request as malformed")
        }
        StatusCode::NOT_FOUND => EngineError::not_found("the sync server has no such resource"),
        other => EngineError::network_error(format!(
            "the sync server answered with HTTP {}",
            other.as_u16()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_http_is_only_loopback() {
        assert!(parse_target("http://127.0.0.1:8080").is_ok());
        assert!(parse_target("http://[::1]:8080").is_ok());
        assert!(parse_target("http://localhost:8080").is_ok());
        let error = parse_target("http://example.com").expect_err("internet http is refused");
        assert_eq!(error.code, "invalid_argument");
        assert!(error.message.contains("loopback"));
    }

    #[test]
    fn https_any_host_and_credentials_are_refused() {
        assert!(parse_target("https://sync.example.com").is_ok());
        assert!(parse_target("https://user:pw@sync.example.com").is_err());
        assert!(parse_target("ftp://sync.example.com").is_err());
        assert!(parse_target("https://sync.example.com?x=1").is_err());
    }
}
