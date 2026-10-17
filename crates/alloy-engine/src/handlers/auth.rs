use crate::accounts::{AccountEntry, AccountIndex};
use crate::context::EngineContext;
use crate::error::EngineError;
use crate::router::{CommandGroup, GroupHandler, command_name};
use alloy_core::{AlloyClient, AlloyError, auth::discover_homeserver};
use alloy_protocol::{EngineCommand, EngineEvent, EngineResponse};

/// Handler for authentication commands (`CommandGroup::Auth`).
///
/// Owned by stream: `core-auth`. Supports discovery, password login, session restore,
/// logout and account listing. OIDC (`StartOidcLogin`/`CompleteOidcLogin`) is not implemented.
#[derive(Default)]
pub struct AuthHandler;

impl AuthHandler {
    pub fn new() -> Self {
        Self
    }
}

fn map_err(e: AlloyError) -> EngineError {
    match e {
        AlloyError::Auth(m) => EngineError::auth_failed(m),
        AlloyError::NotLoggedIn => EngineError::auth_failed("not logged in"),
        AlloyError::InvalidHomeserverUrl => EngineError::invalid_argument("invalid homeserver"),
        AlloyError::MatrixSdk(m) => EngineError::network_error(m),
        other => EngineError::internal_error(other.to_string()),
    }
}

async fn open_client(
    ctx: &EngineContext,
    homeserver_url: &str,
    store_id: &str,
) -> Result<AlloyClient, EngineError> {
    let index = AccountIndex::new(&ctx.data_dir);
    AlloyClient::open(
        homeserver_url,
        &index.store_dir(store_id),
        Some(Box::new(ctx.secrets.clone())),
        Some(store_id),
    )
    .await
    .map_err(map_err)
}

#[async_trait::async_trait]
impl GroupHandler for AuthHandler {
    fn group(&self) -> CommandGroup {
        CommandGroup::Auth
    }

    async fn handle(
        &self,
        ctx: &EngineContext,
        cmd: EngineCommand,
    ) -> Result<EngineResponse, EngineError> {
        let name = command_name(&cmd);
        let index = AccountIndex::new(&ctx.data_dir);
        match cmd {
            EngineCommand::DiscoverHomeserver { input } => {
                let d = discover_homeserver(&input).await.map_err(map_err)?;
                Ok(EngineResponse::HomeserverInfo {
                    url: d.url,
                    name: d.name,
                    supports_oidc: d.supports_oidc,
                })
            }
            EngineCommand::LoginPassword {
                homeserver_url,
                username,
                password,
            } => {
                let store_id = uuid::Uuid::new_v4().to_string();
                let client = open_client(ctx, &homeserver_url, &store_id).await?;
                let cleanup = |idx: &AccountIndex| {
                    let _ = std::fs::remove_dir_all(idx.store_dir(&store_id));
                };
                if let Err(e) = client.login_password(&username, &password, None).await {
                    drop(client);
                    cleanup(&index);
                    return Err(map_err(e));
                }
                let profile = client.account_profile().await.map_err(map_err)?;
                let entry = AccountEntry {
                    user_id: profile.user_id,
                    homeserver_url: profile.homeserver_url,
                    display_name: profile.display_name,
                    avatar_url: profile.avatar_url,
                    store_id,
                };
                let account = entry.to_account();
                if let Some(old) = index
                    .upsert(entry)
                    .map_err(|e| EngineError::internal_error(e.to_string()))?
                {
                    let _ = std::fs::remove_dir_all(index.store_dir(&old.store_id));
                }
                ctx.set_client(Some(client)).await;
                ctx.sink.emit(EngineEvent::AuthState {
                    account: Some(account.clone()),
                });
                Ok(EngineResponse::LoggedIn { account })
            }
            EngineCommand::RestoreSession { account_id } => {
                let entry = index
                    .get(&account_id.0)
                    .ok_or_else(|| EngineError::not_found(format!("account {}", account_id.0)))?;
                let client = open_client(ctx, &entry.homeserver_url, &entry.store_id).await?;
                if !client.restore_session().await.map_err(map_err)? {
                    return Err(EngineError::auth_failed("no stored session; log in again"));
                }
                let account = entry.to_account();
                ctx.set_client(Some(client)).await;
                ctx.sink.emit(EngineEvent::AuthState {
                    account: Some(account.clone()),
                });
                Ok(EngineResponse::SessionRestored { account })
            }
            EngineCommand::Logout { account_id } => {
                let entry = index
                    .get(&account_id.0)
                    .ok_or_else(|| EngineError::not_found(format!("account {}", account_id.0)))?;
                let active_client = ctx
                    .client
                    .read()
                    .await
                    .clone()
                    .filter(|c| c.user_id() == Some(account_id.0.as_str()));
                if let Some(client) = active_client {
                    client.stop_sync().await;
                    client.logout().await.map_err(map_err)?;
                    ctx.set_client(None).await;
                }
                let _ = index.remove(&account_id.0);
                let _ = std::fs::remove_dir_all(index.store_dir(&entry.store_id));
                ctx.sink.emit(EngineEvent::AuthState { account: None });
                Ok(EngineResponse::LoggedOut)
            }
            EngineCommand::ListAccounts => Ok(EngineResponse::AccountsList {
                accounts: index.list().iter().map(AccountEntry::to_account).collect(),
            }),
            EngineCommand::StartOidcLogin { .. } | EngineCommand::CompleteOidcLogin { .. } => {
                Err(EngineError::not_implemented(&name))
            }
            _ => Err(EngineError::invalid_argument(format!(
                "Command '{}' does not belong to Auth group",
                name
            ))),
        }
    }
}
