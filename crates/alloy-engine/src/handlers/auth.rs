use crate::accounts::{AccountEntry, AccountIndex};
use crate::context::EngineContext;
use crate::error::EngineError;
use crate::router::{CommandGroup, GroupHandler, command_name};
use alloy_core::{AlloyClient, AlloyError, auth::discover_homeserver, delete_account_secrets};
use alloy_protocol::{EngineCommand, EngineEvent, EngineResponse};
use std::path::PathBuf;

/// Matrix display names are capped at 255 characters.
const MAX_DISPLAY_NAME_CHARS: usize = 255;

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
        AlloyError::AccountNotFound(m) => EngineError::not_found(m),
        AlloyError::NotLoggedIn => EngineError::auth_failed("not logged in"),
        AlloyError::InvalidHomeserverUrl => EngineError::invalid_argument("invalid homeserver"),
        AlloyError::InsecureHomeserverUrl => EngineError {
            code: "insecure_homeserver".to_string(),
            message: "homeserver must use https (http is allowed only for localhost)".to_string(),
        },
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

/// Remove a stored account's secrets and store directory. Keyring entries cannot be listed, so the
/// secrets are deleted by name. A secret-store failure is logged and local removal still happens.
async fn purge_account(ctx: &EngineContext, index: &AccountIndex, store_id: &str) {
    if let Err(e) = delete_account_secrets(ctx.secrets.as_ref(), Some(store_id)).await {
        tracing::warn!(error = %e, "failed to delete account secrets");
    }
    let _ = std::fs::remove_dir_all(index.store_dir(store_id));
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
                if let Err(e) = client.login_password(&username, &password, None).await {
                    drop(client);
                    purge_account(ctx, &index, &store_id).await;
                    return Err(map_err(e));
                }
                let profile = match client.account_profile().await {
                    Ok(profile) => profile,
                    Err(e) => {
                        // A session was stored by the login; revoke it on the server as well.
                        if let Err(logout_err) = client.logout().await {
                            tracing::warn!(error = %logout_err, "remote logout failed after login");
                        }
                        purge_account(ctx, &index, &store_id).await;
                        return Err(map_err(e));
                    }
                };
                let entry = AccountEntry {
                    user_id: profile.user_id,
                    homeserver_url: profile.homeserver_url,
                    display_name: profile.display_name,
                    avatar_url: profile.avatar_url,
                    store_id: store_id.clone(),
                };
                let account = entry.to_account();
                let replaced = match index.upsert(entry) {
                    Ok(replaced) => replaced,
                    Err(e) => {
                        purge_account(ctx, &index, &store_id).await;
                        return Err(EngineError::internal_error(e.to_string()));
                    }
                };
                if let Some(old) = replaced {
                    purge_account(ctx, &index, &old.store_id).await;
                }
                ctx.set_client(Some(client)).await;
                if ctx.background
                    && let Ok(active) = ctx.active_client().await
                {
                    super::crypto::start_background(ctx, active).await;
                }
                ctx.sink.emit(EngineEvent::AuthState {
                    account: Some(account.clone()),
                });
                Ok(EngineResponse::LoggedIn { account })
            }
            EngineCommand::RestoreSession { account_id } => {
                let (client, entry) = alloy_core::restore_account(
                    &ctx.data_dir,
                    Some(&account_id.0),
                    Some(Box::new(ctx.secrets.clone())),
                )
                .await
                .map_err(map_err)?;
                let account = entry.to_account();
                ctx.set_client(Some(client)).await;
                if ctx.background
                    && let Ok(active) = ctx.active_client().await
                {
                    super::crypto::start_background(ctx, active).await;
                }
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
                // Local state is removed even when the server refuses the logout; the remote
                // error is still returned so the caller knows the token may remain valid.
                let remote = match active_client {
                    Some(client) => {
                        client.stop_sync().await;
                        let remote = client.logout().await;
                        ctx.set_client(None).await;
                        remote
                    }
                    None => Ok(()),
                };
                purge_account(ctx, &index, &entry.store_id).await;
                let _ = index.remove(&account_id.0);
                // The media cache holds decrypted media and is shared by all accounts on this device.
                if let Err(e) = alloy_core::media::clear_media_cache(&ctx.data_dir).await {
                    tracing::warn!(error = %e, "failed to clear media cache on logout");
                }
                ctx.sink.emit(EngineEvent::AuthState { account: None });
                remote.map_err(map_err)?;
                Ok(EngineResponse::LoggedOut)
            }
            EngineCommand::ListAccounts => Ok(EngineResponse::AccountsList {
                accounts: index.list().iter().map(AccountEntry::to_account).collect(),
            }),
            EngineCommand::StartOidcLogin { .. } | EngineCommand::CompleteOidcLogin { .. } => {
                Err(EngineError::not_implemented(&name))
            }
            EngineCommand::GetOwnProfile => {
                let client = ctx.active_client().await?;
                let profile = client.own_profile().await.map_err(map_err)?;
                Ok(EngineResponse::OwnProfile { profile })
            }
            EngineCommand::SetDisplayName { name } => {
                let name = name.trim();
                if name.is_empty() {
                    return Err(EngineError::invalid_argument(
                        "display name must not be empty",
                    ));
                }
                if name.chars().count() > MAX_DISPLAY_NAME_CHARS {
                    return Err(EngineError::invalid_argument(format!(
                        "display name must be at most {MAX_DISPLAY_NAME_CHARS} characters"
                    )));
                }
                let client = ctx.active_client().await?;
                client.set_display_name(name).await.map_err(map_err)?;
                Ok(EngineResponse::Unit)
            }
            EngineCommand::SetOwnAvatar { file_path } => {
                let client = ctx.active_client().await?;
                let path = file_path.map(PathBuf::from);
                if let Some(path) = &path {
                    if !ctx.attachment_path_allowed(path) {
                        return Err(EngineError::forbidden(format!(
                            "attachment {} is not registered for sending",
                            path.display()
                        )));
                    }
                    if !path.is_file() {
                        return Err(EngineError::not_found(format!("file {}", path.display())));
                    }
                }
                client
                    .set_own_avatar(path.as_deref())
                    .await
                    .map_err(map_err)?;
                Ok(EngineResponse::Unit)
            }
            EngineCommand::ListDevices => {
                let client = ctx.active_client().await?;
                let devices = client.list_devices().await.map_err(map_err)?;
                Ok(EngineResponse::Devices { devices })
            }
            EngineCommand::GetSessionInfo => {
                let client = ctx.active_client().await?;
                let info = client.session_info().await.map_err(map_err)?;
                Ok(EngineResponse::SessionInfo { info })
            }
            EngineCommand::DeleteDevices {
                device_ids,
                password,
            } => {
                let client = ctx.active_client().await?;
                match client
                    .delete_devices(&device_ids, password.as_deref())
                    .await
                {
                    Ok(alloy_core::DeleteDevicesOutcome::Deleted(deleted)) => {
                        Ok(EngineResponse::DevicesDeleted { deleted })
                    }
                    Ok(alloy_core::DeleteDevicesOutcome::AuthRequired(flows)) => {
                        Ok(EngineResponse::AuthRequired { flows })
                    }
                    Err(e) => Err(map_err(e)),
                }
            }
            EngineCommand::IgnoreUser { user_id } => {
                let client = ctx.active_client().await?;
                client.ignore_user(&user_id).await.map_err(map_err)?;
                Ok(EngineResponse::UserIgnored { user_id })
            }
            EngineCommand::UnignoreUser { user_id } => {
                let client = ctx.active_client().await?;
                client.unignore_user(&user_id).await.map_err(map_err)?;
                Ok(EngineResponse::UserUnignored { user_id })
            }
            EngineCommand::ListIgnoredUsers => {
                let client = ctx.active_client().await?;
                let user_ids = client.list_ignored_users().await.map_err(map_err)?;
                Ok(EngineResponse::IgnoredUsers { user_ids })
            }
            _ => Err(EngineError::invalid_argument(format!(
                "Command '{}' does not belong to Auth group",
                name
            ))),
        }
    }
}
