//! Link previews: the homeserver's Open Graph summary of a URL.
//!
//! The homeserver fetches the page, so the request reveals the link to it. Callers decide when to ask
//! (see the UI's `linkPreviews` setting). Results are cached in memory for `CACHE_TTL`.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use crate::{AlloyClient, AlloyError, AlloyResult};
use alloy_protocol::UrlPreviewDto;
use matrix_sdk::ruma::api::client::{authenticated_media, media};
use matrix_sdk::ruma::api::error::ErrorKind;

/// Longest URL accepted for a preview, in bytes. Longer links are not previewed.
const MAX_URL_LEN: usize = 2048;
/// How long a fetched preview, or the absence of one, is reused.
const CACHE_TTL: Duration = Duration::from_secs(10 * 60);
/// Cache size at which every entry is dropped to make room. Keeps memory bounded on long sessions.
const CACHE_MAX_ENTRIES: usize = 256;

/// Rejects URLs the homeserver should not be asked to fetch: too long, unparseable, or not http(s).
pub(crate) fn validate_preview_url(url: &str) -> AlloyResult<()> {
    if url.len() > MAX_URL_LEN {
        return Err(AlloyError::InvalidUrl(format!(
            "longer than {MAX_URL_LEN} bytes"
        )));
    }
    let parsed = url::Url::parse(url).map_err(|e| AlloyError::InvalidUrl(e.to_string()))?;
    match parsed.scheme() {
        "http" | "https" => Ok(()),
        other => Err(AlloyError::InvalidUrl(format!(
            "scheme `{other}` is not http or https"
        ))),
    }
}

/// Builds the preview from the homeserver's JSON body. `None` when the body is missing, is not JSON, or
/// carries none of the title, description, site name or image.
pub(crate) fn preview_from_body(url: &str, body: Option<&str>) -> Option<UrlPreviewDto> {
    let json: serde_json::Value = serde_json::from_str(body?).ok()?;
    let preview = UrlPreviewDto {
        url: url.to_owned(),
        title: text_field(&json, "og:title"),
        description: text_field(&json, "og:description"),
        site_name: text_field(&json, "og:site_name"),
        // The image is fetched through `GetMedia`, which only takes `mxc://` URIs.
        image: text_field(&json, "og:image").filter(|uri| uri.starts_with("mxc://")),
        image_width: dimension(&json, "og:image:width"),
        image_height: dimension(&json, "og:image:height"),
    };
    let has_content = preview.title.is_some()
        || preview.description.is_some()
        || preview.site_name.is_some()
        || preview.image.is_some();
    has_content.then_some(preview)
}

/// A non-blank string field, trimmed.
fn text_field(json: &serde_json::Value, key: &str) -> Option<String> {
    json.get(key)?
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

fn dimension(json: &serde_json::Value, key: &str) -> Option<u32> {
    json.get(key)?.as_u64().and_then(|n| u32::try_from(n).ok())
}

/// HTTP status of a homeserver error response; `None` for transport failures.
fn api_status(err: &matrix_sdk::Error) -> Option<u16> {
    err.as_client_api_error().map(|e| e.status_code.as_u16())
}

/// Whether a failed preview request means there is no preview to give, rather than a failure to report:
/// previews disabled on the server (`M_UNRECOGNIZED`), the endpoint missing, or the server failing to
/// fetch or time out on the page (502, 504).
fn preview_unavailable(err: &matrix_sdk::Error) -> bool {
    err.as_client_api_error().is_some_and(|e| {
        matches!(e.error_kind(), Some(ErrorKind::Unrecognized))
            || matches!(e.status_code.as_u16(), 404 | 502 | 504)
    })
}

/// Recently fetched previews keyed by URL. Absent previews are cached too, so a link without one is not
/// requested again on every render.
#[derive(Default)]
pub(crate) struct PreviewCache {
    entries: Mutex<HashMap<String, (Instant, Option<UrlPreviewDto>)>>,
}

impl PreviewCache {
    /// The cached result for `url`, or `None` when it is absent or stale.
    fn get(&self, url: &str) -> Option<Option<UrlPreviewDto>> {
        let entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        let (stored_at, preview) = entries.get(url)?;
        (stored_at.elapsed() < CACHE_TTL).then(|| preview.clone())
    }

    fn put(&self, url: &str, preview: Option<UrlPreviewDto>) {
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        entries.retain(|_, (stored_at, _)| stored_at.elapsed() < CACHE_TTL);
        if entries.len() >= CACHE_MAX_ENTRIES {
            entries.clear();
        }
        entries.insert(url.to_owned(), (Instant::now(), preview));
    }
}

impl AlloyClient {
    /// The homeserver's link preview for `url`, or `None` when it has none. Only http(s) URLs of at most
    /// 2048 bytes are accepted; anything else is `AlloyError::InvalidUrl`.
    pub async fn url_preview(&self, url: &str) -> AlloyResult<Option<UrlPreviewDto>> {
        validate_preview_url(url)?;
        if let Some(cached) = self.url_previews.get(url) {
            return Ok(cached);
        }
        let preview = self.fetch_url_preview(url).await?;
        self.url_previews.put(url, preview.clone());
        Ok(preview)
    }

    /// Tries the stable endpoint, then the deprecated one for servers that predate it (they answer 404).
    async fn fetch_url_preview(&self, url: &str) -> AlloyResult<Option<UrlPreviewDto>> {
        let body = match self.preview_body_v1(url).await {
            Err(e) if api_status(&e) == Some(404) => self.preview_body_v3(url).await,
            result => result,
        };
        match body {
            Ok(body) => Ok(preview_from_body(url, body.as_deref())),
            Err(e) if preview_unavailable(&e) => Ok(None),
            Err(e) => Err(AlloyError::MatrixSdk(e.to_string())),
        }
    }

    /// The response body as JSON text, or `None` when the server sent none.
    async fn preview_body_v1(&self, url: &str) -> Result<Option<String>, matrix_sdk::Error> {
        let request = authenticated_media::get_media_preview::v1::Request::new(url.to_owned());
        let response = self.client.send(request).await?;
        Ok(response.data.map(|raw| raw.get().to_owned()))
    }

    #[allow(deprecated)]
    async fn preview_body_v3(&self, url: &str) -> Result<Option<String>, matrix_sdk::Error> {
        let request = media::get_media_preview::v3::Request::new(url.to_owned());
        let response = self.client.send(request).await?;
        Ok(response.data.map(|raw| raw.get().to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_http_and_https_urls() {
        assert!(validate_preview_url("https://example.com/a?b=c").is_ok());
        assert!(validate_preview_url("http://localhost:8008/").is_ok());
    }

    #[test]
    fn rejects_other_schemes_and_unparseable_urls() {
        for url in [
            "ftp://example.com",
            "file:///etc/passwd",
            "javascript:alert(1)",
            "not a url",
        ] {
            assert!(
                matches!(validate_preview_url(url), Err(AlloyError::InvalidUrl(_))),
                "{url} should be rejected"
            );
        }
    }

    #[test]
    fn rejects_urls_longer_than_the_limit() {
        let long = format!("https://example.com/{}", "a".repeat(MAX_URL_LEN));
        assert!(matches!(
            validate_preview_url(&long),
            Err(AlloyError::InvalidUrl(_))
        ));
        let at_limit = format!(
            "https://e.co/{}",
            "a".repeat(MAX_URL_LEN - "https://e.co/".len())
        );
        assert_eq!(at_limit.len(), MAX_URL_LEN);
        assert!(validate_preview_url(&at_limit).is_ok());
    }

    #[test]
    fn maps_open_graph_fields() {
        let body = r#"{
            "og:title": "  Example  ",
            "og:description": "A page",
            "og:site_name": "Site",
            "og:image": "mxc://example.org/abc",
            "og:image:width": 640,
            "og:image:height": 480
        }"#;
        let preview = preview_from_body("https://example.com", Some(body)).unwrap();
        assert_eq!(preview.url, "https://example.com");
        assert_eq!(preview.title.as_deref(), Some("Example"));
        assert_eq!(preview.description.as_deref(), Some("A page"));
        assert_eq!(preview.site_name.as_deref(), Some("Site"));
        assert_eq!(preview.image.as_deref(), Some("mxc://example.org/abc"));
        assert_eq!(preview.image_width, Some(640));
        assert_eq!(preview.image_height, Some(480));
    }

    #[test]
    fn drops_images_that_are_not_mxc_and_blank_fields() {
        let body = r#"{"og:title": "  ", "og:image": "https://cdn.example/x.png"}"#;
        assert_eq!(preview_from_body("https://example.com", Some(body)), None);

        let body = r#"{"og:title": "T", "og:image": "https://cdn.example/x.png"}"#;
        let preview = preview_from_body("https://example.com", Some(body)).unwrap();
        assert_eq!(preview.title.as_deref(), Some("T"));
        assert_eq!(preview.image, None);
    }

    #[test]
    fn empty_or_missing_bodies_have_no_preview() {
        assert_eq!(preview_from_body("https://example.com", None), None);
        assert_eq!(preview_from_body("https://example.com", Some("{}")), None);
        assert_eq!(
            preview_from_body("https://example.com", Some("not json")),
            None
        );
    }

    #[test]
    fn cache_returns_fresh_entries_including_absent_ones() {
        let cache = PreviewCache::default();
        assert_eq!(cache.get("https://example.com"), None);

        cache.put("https://example.com", None);
        assert_eq!(cache.get("https://example.com"), Some(None));

        let preview = UrlPreviewDto {
            url: "https://example.org".to_owned(),
            title: Some("T".to_owned()),
            description: None,
            site_name: None,
            image: None,
            image_width: None,
            image_height: None,
        };
        cache.put("https://example.org", Some(preview.clone()));
        assert_eq!(cache.get("https://example.org"), Some(Some(preview)));
    }
}
