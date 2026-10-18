//! Authentication: homeserver discovery, password login, session persistence, logout.
//!
//! Owned by stream core-auth. OIDC (MAS) login is tracked separately and is not
//! implemented here.

use crate::client::require_secure_homeserver;
use crate::{AlloyClient, AlloyError, AlloyResult};

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
        let json = serde_json::to_string(&session).map_err(|e| AlloyError::Auth(e.to_string()))?;
        self.secret_store
            .set_secret(&self.session_key(), &json)
            .await?;
        Ok(())
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
    /// Local secrets are deleted even when the server call fails, so a failed logout never leaves
    /// a usable access token on this device. The remote error, if any, is still returned.
    pub async fn logout(&self) -> AlloyResult<()> {
        let remote = self
            .client
            .matrix_auth()
            .logout()
            .await
            .map_err(|e| AlloyError::Auth(e.to_string()));
        let local = self.delete_local_secrets().await;
        remote.map(|_| ())?;
        local
    }
}
