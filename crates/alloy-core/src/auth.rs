//! Owned by stream core-auth

use crate::{AlloyClient, AlloyError, AlloyResult};

impl AlloyClient {
    /// Login with username/password.
    pub async fn login_password(
        &self,
        username: &str,
        password: &str,
        device_name: Option<&str>,
    ) -> AlloyResult<()> {
        let auth = self.client.matrix_auth();
        let _response = auth
            .login_username(username, password)
            .initial_device_display_name(device_name.unwrap_or("Alloy"))
            .send()
            .await
            .map_err(|e| AlloyError::Auth(e.to_string()))?;

        // Store session in secret store for session restoration
        if let Some(session) = self.client.matrix_auth().session()
            && let Ok(json) = serde_json::to_string(&session)
        {
            let _ = self.secret_store.set_secret("alloy:session", &json).await;
        }

        Ok(())
    }
}
