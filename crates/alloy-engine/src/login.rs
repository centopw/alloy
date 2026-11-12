//! Login attempts that are waiting for the user's next answer.
//!
//! A connector's login is a state machine (`alloy_connector::login`); the engine only keeps the
//! connector, the network, the pre-allocated account id and the account directory between
//! `BeginLogin` and the next `SubmitLogin`. `Done` and `Failed` are terminal, so the attempt is
//! dropped as soon as either is returned.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use alloy_connector::Connector;
use alloy_protocol::{LoginId, Network};
use tokio::sync::Mutex;

/// One in-progress login attempt.
pub struct PendingLogin {
    /// Local account id, allocated up front so the connector's working directory can be the
    /// account's own directory from the first network call (the directory is never renamed).
    pub account_id: String,
    pub network: Network,
    pub connector: Arc<dyn Connector>,
    /// The account directory handed to the connector at creation.
    pub dir: PathBuf,
}

/// Every in-progress login, keyed by the id handed to the UI.
#[derive(Clone, Default)]
pub struct LoginManager {
    pending: Arc<Mutex<HashMap<LoginId, PendingLogin>>>,
}

impl LoginManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn insert(&self, login_id: LoginId, login: PendingLogin) {
        self.pending.lock().await.insert(login_id, login);
    }

    /// Removes and returns the attempt; a submitted answer consumes the attempt it answers.
    pub async fn take(&self, login_id: &LoginId) -> Option<PendingLogin> {
        self.pending.lock().await.remove(login_id)
    }
}
