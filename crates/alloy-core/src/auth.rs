//! Authentication: homeserver discovery, password and OIDC (MAS) login, session persistence,
//! logout.
//!
//! Owned by stream core-auth.

use crate::client::require_secure_homeserver;
use crate::{AlloyClient, AlloyError, AlloyResult};
use matrix_sdk::authentication::oauth::registration::{
    ApplicationType, ClientMetadata, Localized, OAuthGrantType,
};
use matrix_sdk::ruma::serde::Raw;
use matrix_sdk::utils::UrlOrQuery;

/// Project page advertised as the OAuth client's `client_uri`.
const OIDC_CLIENT_URI: &str = "https://github.com/centopw/alloy";

/// Name shown to the user on the authorization server's consent screen.
const OIDC_CLIENT_NAME: &str = "Alloy";

/// Result of probing a homeserver address or server name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HomeserverDiscovery {
    /// Resolved homeserver base URL (after `.well-known` lookup).
    pub url: String,
    /// Human readable server name (host).
    pub name: String,
    /// Whether the homeserver advertises OAuth/OIDC (MAS) login.
    pub supports_oidc: bool,
    /// Whether the homeserver advertises password login.
    pub supports_password: bool,
}

/// Public profile of the logged-in account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountProfile {
    pub user_id: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub homeserver_url: String,
}

/// What the app needs to present an OIDC login: the URL to open and the URI to expect back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OidcLoginStart {
    /// Authorization URL to open in the system browser.
    pub authorization_url: String,
    /// Redirect URI registered with the server; the app must receive this URL.
    pub redirect_uri: String,
}

/// Resolve a server name (`example.org`) or URL into a homeserver, probing login support.
pub async fn discover_homeserver(input: &str) -> AlloyResult<HomeserverDiscovery> {
    let input = input.trim();
    if input.is_empty() {
        return Err(AlloyError::InvalidHomeserverUrl);
    }
    if input.contains("://") {
        let typed = url::Url::parse(input).map_err(|_| AlloyError::InvalidHomeserverUrl)?;
        require_secure_homeserver(&typed)?;
    }
    let http =
        crate::tls::matrix_http_client().map_err(|e| AlloyError::HttpClient(e.to_string()))?;
    let client = matrix_sdk::Client::builder()
        .http_client(http)
        .server_name_or_homeserver_url(input)
        .build()
        .await
        .map_err(|e| AlloyError::MatrixSdk(e.to_string()))?;
    let url = client.homeserver();
    // `.well-known` can redirect to any URL, so the resolved homeserver is checked too.
    require_secure_homeserver(&url)?;
    let name = url.host_str().unwrap_or(input).to_owned();
    let supports_oidc = client.oauth().server_metadata().await.is_ok();
    let supports_password = client
        .matrix_auth()
        .get_login_types()
        .await
        .map(|r| {
            r.flows.iter().any(|f| {
                matches!(
                    f,
                    matrix_sdk::ruma::api::client::session::get_login_types::v3::LoginType::Password(_)
                )
            })
        })
        .unwrap_or(false);
    Ok(HomeserverDiscovery {
        url: url.to_string(),
        name,
        supports_oidc,
        supports_password,
    })
}

impl AlloyClient {
    /// Login with username/password and persist the session in the secret store.
    pub async fn login_password(
        &self,
        username: &str,
        password: &str,
        device_name: Option<&str>,
    ) -> AlloyResult<()> {
        let auth = self.client.matrix_auth();
        auth.login_username(username, password)
            .initial_device_display_name(device_name.unwrap_or("Alloy"))
            .send()
            .await
            .map_err(|e| AlloyError::Auth(e.to_string()))?;

        let session = auth
            .session()
            .ok_or_else(|| AlloyError::Auth("login returned no session".into()))?;
        self.save_matrix_session(session).await
    }

    /// Starts an OIDC (MAS) authorization-code login and returns the URL the app should open.
    ///
    /// The client registers itself with the authorization server as a native public client,
    /// announcing the loopback `redirect_uri` the caller is listening on (RFC 8252). The PKCE
    /// verifier and CSRF state stay in this client, so the same client must receive
    /// [`Self::complete_oidc_login`].
    pub async fn start_oidc_login(&self, redirect_uri: &str) -> AlloyResult<OidcLoginStart> {
        let redirect_uri = url::Url::parse(redirect_uri).map_err(|e| AlloyError::Oidc {
            reason: "invalid_redirect",
            message: format!("invalid OIDC redirect URI: {e}"),
        })?;
        let client_uri = url::Url::parse(OIDC_CLIENT_URI).map_err(|e| AlloyError::Oidc {
            reason: "registration_failed",
            message: format!("invalid OIDC client URI: {e}"),
        })?;
        let oauth = self.client.oauth();

        // A homeserver without an authorization server cannot log anyone in this way; report it
        // before registering anything. A transient failure stays a network error so the UI can
        // offer a retry instead of telling the user the server does not support OIDC.
        if let Err(e) = oauth.server_metadata().await {
            if e.is_not_supported() {
                return Err(AlloyError::Oidc {
                    reason: "unsupported",
                    message: "homeserver does not support OIDC login".into(),
                });
            }
            return Err(AlloyError::Oidc {
                reason: "network",
                message: e.to_string(),
            });
        }

        let mut metadata = ClientMetadata::new(
            ApplicationType::Native,
            vec![OAuthGrantType::AuthorizationCode {
                redirect_uris: vec![redirect_uri.clone()],
            }],
            Localized::new(client_uri, []),
        );
        metadata.client_name = Some(Localized::new(OIDC_CLIENT_NAME.to_owned(), []));
        let metadata = Raw::new(&metadata).map_err(|e| AlloyError::Oidc {
            reason: "registration_failed",
            message: format!("could not build OIDC client metadata: {e}"),
        })?;

        let authorization = oauth
            .login(redirect_uri.clone(), None, Some(metadata.into()), None)
            .build()
            .await
            .map_err(|e| AlloyError::Oidc {
                reason: "registration_failed",
                message: format!("could not start OIDC login: {e}"),
            })?;

        Ok(OidcLoginStart {
            authorization_url: authorization.url.to_string(),
            redirect_uri: redirect_uri.to_string(),
        })
    }

    /// Finishes an OIDC login from the URL the app received, and persists the session.
    ///
    /// The URL's state is checked by the SDK against the one this client generated; a foreign or
    /// expired callback is rejected.
    pub async fn complete_oidc_login(&self, redirect_url: &str) -> AlloyResult<()> {
        let redirect_url = redirect_url.trim();
        if redirect_url.is_empty() {
            return Err(AlloyError::Auth("empty OIDC callback URL".into()));
        }
        let input = match url::Url::parse(redirect_url) {
            Ok(url) => UrlOrQuery::Url(url),
            // A bare query string is accepted too, which keeps app-side parsing simple.
            Err(_) => UrlOrQuery::Query(redirect_url.trim_start_matches('?').to_owned()),
        };
        self.client
            .oauth()
            .finish_login(input)
            .await
            .map_err(|e| AlloyError::Oidc {
                reason: oidc_failure_reason(&e),
                message: format!("OIDC login failed: {e}"),
            })?;
        self.persist_oauth_session().await
    }

    /// Fetch display name and avatar of the logged-in account.
    pub async fn account_profile(&self) -> AlloyResult<AccountProfile> {
        let user_id = self
            .client
            .user_id()
            .ok_or(AlloyError::NotLoggedIn)?
            .to_string();
        let profile = self
            .client
            .account()
            .fetch_user_profile()
            .await
            .map_err(|e| AlloyError::MatrixSdk(e.to_string()))?;
        Ok(AccountProfile {
            user_id,
            display_name: profile
                .get("displayname")
                .and_then(|v| v.as_str().map(str::to_owned)),
            avatar_url: profile
                .get("avatar_url")
                .and_then(|v| v.as_str().map(str::to_owned)),
            homeserver_url: self.client.homeserver().to_string(),
        })
    }

    /// Log out on the server and delete every local secret for this account.
    ///
    /// An OAuth (MAS) session is revoked through the authorization server's revocation endpoint;
    /// a password session uses the homeserver's `/logout`. Local secrets are deleted even when the
    /// server call fails, so a failed logout never leaves a usable access token on this device. The
    /// remote error, if any, is still returned.
    pub async fn logout(&self) -> AlloyResult<()> {
        let remote = if self.is_oauth_session() {
            self.client
                .oauth()
                .logout()
                .await
                .map_err(|e| AlloyError::Auth(e.to_string()))
        } else {
            self.client
                .matrix_auth()
                .logout()
                .await
                .map(|_| ())
                .map_err(|e| AlloyError::Auth(e.to_string()))
        };
        let local = self.delete_local_secrets().await;
        remote.map(|_| ())?;
        local
    }
}

/// Stable reason for a failed OIDC completion, so the UI can branch on states rather than prose.
///
/// The SDK does not expose the authorization-code error variants publicly, so a cancellation,
/// denial or state mismatch is recognised from the error text and everything else falls back to
/// `exchange_failed`. The reason set is stable even if the text changes.
fn oidc_failure_reason(error: &matrix_sdk::Error) -> &'static str {
    use matrix_sdk::authentication::oauth::OAuthError;

    match error {
        matrix_sdk::Error::OAuth(inner) => match inner.as_ref() {
            OAuthError::Discovery(_) => "unsupported",
            OAuthError::ClientRegistration(_) => "registration_failed",
            OAuthError::SessionMismatch | OAuthError::NotRegistered => "state_mismatch",
            OAuthError::NotAuthenticated => "not_pending",
            OAuthError::AuthorizationCode(_) => {
                let text = inner.to_string().to_ascii_lowercase();
                if text.contains("cancel") || text.contains("denied") {
                    "denied"
                } else if text.contains("state") {
                    "state_mismatch"
                } else {
                    "exchange_failed"
                }
            }
            _ => "exchange_failed",
        },
        matrix_sdk::Error::Http(_) => "network",
        _ => "exchange_failed",
    }
}
