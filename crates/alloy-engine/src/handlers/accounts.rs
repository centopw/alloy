//! Account commands: list, login, logout.

use std::path::Path;

use alloy_connector::{LoginStep, SessionBlob};
use alloy_protocol::{
    AccountId, EngineCommand, EngineEvent, EngineResponse, LoginId, LoginInputDto, LoginStepDto,
    Network,
};
use alloy_store::AccountState as StoreAccountState;
use alloy_store::{AccountRecord, NewAccount};
use uuid::Uuid;

use crate::accounts::{account_dto, accounts_dto, start_with_connector};
use crate::context::EngineContext;
use crate::convert;
use crate::error::{EngineError, map_connector, map_store};
use crate::factory;
use crate::login::PendingLogin;
use crate::router::{CommandGroup, GroupHandler, command_name};

/// Handler for account lifecycle commands (`CommandGroup::Accounts`).
pub struct AccountsHandler;

impl AccountsHandler {
    pub fn new() -> Self {
        Self
    }
}

impl Default for AccountsHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl GroupHandler for AccountsHandler {
    fn group(&self) -> CommandGroup {
        CommandGroup::Accounts
    }

    async fn handle(
        &self,
        ctx: &EngineContext,
        cmd: EngineCommand,
    ) -> Result<EngineResponse, EngineError> {
        let name = command_name(&cmd);
        match cmd {
            EngineCommand::ListAccounts => Ok(EngineResponse::AccountsList {
                accounts: accounts_dto(ctx).await?,
            }),
            EngineCommand::ListNetworks => Ok(EngineResponse::Networks {
                networks: ctx.factory.networks().await,
            }),
            EngineCommand::BeginLogin { network } => begin_login(ctx, network).await,
            EngineCommand::SubmitLogin { login_id, input } => {
                submit_login(ctx, login_id, input).await
            }
            EngineCommand::CancelLogin { login_id } => cancel_login(ctx, &login_id).await,
            EngineCommand::Logout { account_id } => logout(ctx, &account_id).await,
            EngineCommand::SetTelegramApp { api_id, api_hash } => {
                set_telegram_app(ctx, api_id, api_hash.expose()).await
            }
            EngineCommand::ClearTelegramApp => clear_telegram_app(ctx).await,
            EngineCommand::GetTelegramApp => get_telegram_app(ctx).await,
            _ => Err(EngineError::invalid_argument(format!(
                "Command '{}' does not belong to the Accounts group",
                name
            ))),
        }
    }
}

async fn begin_login(ctx: &EngineContext, network: Network) -> Result<EngineResponse, EngineError> {
    // Checked before any directory is created, so a login that cannot start leaves nothing behind.
    // Telegram is always listed, but not ready until its application pair is configured.
    let ready = ctx
        .factory
        .networks()
        .await
        .iter()
        .any(|info| info.network == network && info.ready);
    if !ready {
        return Err(if network == Network::Telegram {
            factory::telegram_app_missing_error()
        } else {
            EngineError::not_supported(convert::network_label(network))
        });
    }
    let login_id = LoginId(Uuid::new_v4().to_string());
    // The account id is allocated before the first network call, so the connector's working
    // directory is the account's own directory. Nothing is renamed while a connector uses it.
    let account_id = Uuid::new_v4().to_string();
    let dir = ctx.accounts_dir().join(&account_id);
    std::fs::create_dir_all(&dir).map_err(|err| {
        EngineError::internal_error(format!(
            "cannot create the account directory {}: {err}",
            dir.display()
        ))
    })?;
    let connector = ctx
        .factory
        .create(network, dir.clone())
        .await
        .map_err(map_connector)?;
    let step = connector.begin_login().await;
    let attempt = PendingLogin {
        account_id,
        network,
        connector,
        dir,
    };
    settle_step(ctx, login_id, attempt, step).await
}

async fn submit_login(
    ctx: &EngineContext,
    login_id: LoginId,
    input: LoginInputDto,
) -> Result<EngineResponse, EngineError> {
    let attempt =
        ctx.logins.take(&login_id).await.ok_or_else(|| {
            EngineError::invalid_argument(format!("unknown login '{}'", login_id.0))
        })?;
    let step = attempt
        .connector
        .submit_login(convert::login_input(input))
        .await;
    settle_step(ctx, login_id, attempt, step).await
}

/// Handles the step a connector returned: finish the account, drop the attempt, or ask the user.
async fn settle_step(
    ctx: &EngineContext,
    login_id: LoginId,
    attempt: PendingLogin,
    step: LoginStep,
) -> Result<EngineResponse, EngineError> {
    match step {
        LoginStep::Done {
            remote_user_id,
            display_name,
            session,
        } => {
            complete_login(
                ctx,
                login_id,
                attempt,
                remote_user_id,
                display_name,
                session,
            )
            .await
        }
        LoginStep::Failed { reason } => {
            // The attempt never became an account, so its directory belongs to nobody.
            drop_directory(&attempt.dir);
            Ok(EngineResponse::LoginStep {
                login_id,
                step: LoginStepDto::Failed {
                    reason: convert::login_failure(reason),
                },
            })
        }
        step => {
            let dto = convert::pending_login_step(step)
                .ok_or_else(|| EngineError::internal_error("login step was not renderable"))?;
            ctx.logins.insert(login_id.clone(), attempt).await;
            Ok(EngineResponse::LoginStep {
                login_id,
                step: dto,
            })
        }
    }
}

async fn complete_login(
    ctx: &EngineContext,
    login_id: LoginId,
    attempt: PendingLogin,
    remote_user_id: String,
    display_name: String,
    session: SessionBlob,
) -> Result<EngineResponse, EngineError> {
    // The id and the directory were fixed when the attempt started, so the store row, the
    // connector and the directory agree without any move.
    let record = ctx
        .store
        .insert_account(NewAccount {
            account_id: Some(attempt.account_id),
            network: convert::network_protocol_to_store(attempt.network),
            remote_user_id: Some(remote_user_id.clone()),
            display_name: Some(display_name),
            state: StoreAccountState::Active,
        })
        .await
        .map_err(|err| {
            drop_directory(&attempt.dir);
            map_store(err)
        })?;

    if let Err(err) = ctx
        .store
        .put_secret(&record.account_id, session.as_bytes())
        .await
    {
        // An account without its session could never be restored; undo the row and the directory.
        let _ = ctx.store.delete_account(&record.account_id).await;
        drop_directory(&attempt.dir);
        return Err(map_store(err));
    }

    // A connector that cannot start does not undo the login: the credentials are valid and
    // stored, so the account exists and only its session needs attention. Its directory is the
    // account directory, so it is kept for the next start.
    if let Err(err) =
        start_with_connector(ctx, record.clone(), attempt.connector, Some(remote_user_id)).await
    {
        tracing::warn!(
            account = %record.account_id,
            error = %err.message,
            "account was stored but its connector did not start"
        );
        let _ = ctx
            .store
            .update_account_state(&record.account_id, StoreAccountState::NeedsRelogin)
            .await;
    }

    let record = ctx
        .store
        .get_account(&record.account_id)
        .await
        .map_err(map_store)?
        .unwrap_or(record);
    let account = account_dto(ctx, &record).await?;
    let accounts = accounts_dto(ctx).await?;
    ctx.sink.emit(EngineEvent::AccountsChanged { accounts });
    Ok(EngineResponse::LoginStep {
        login_id,
        step: LoginStepDto::Done { account },
    })
}

/// Abandons an in-progress login: the attempt is dropped and its pre-allocated directory removed.
async fn cancel_login(
    ctx: &EngineContext,
    login_id: &LoginId,
) -> Result<EngineResponse, EngineError> {
    if let Some(attempt) = ctx.logins.take(login_id).await {
        drop_directory(&attempt.dir);
    }
    // A login that already finished is not an error: the account it created stays.
    Ok(EngineResponse::LoginCancelled)
}

async fn logout(
    ctx: &EngineContext,
    account_id: &AccountId,
) -> Result<EngineResponse, EngineError> {
    let record = ctx
        .store
        .get_account(&account_id.0)
        .await
        .map_err(map_store)?
        .ok_or_else(|| EngineError::not_found(format!("account '{}'", account_id.0)))?;

    match ctx.accounts.remove(&account_id.0).await {
        Some(runtime) => {
            if let Err(err) = runtime.connector.logout().await {
                tracing::warn!(account = %account_id.0, error = %err, "remote logout failed");
            }
        }
        None => revoke_stored_session(ctx, &record).await,
    }

    let cached = ctx
        .store
        .clear_media_local_paths(Some(account_id.0.as_str()))
        .await
        .map_err(map_store)?;
    for path in cached {
        remove_cached_file(&path);
    }

    ctx.store
        .delete_account(&account_id.0)
        .await
        .map_err(map_store)?;
    let _ = std::fs::remove_dir_all(ctx.accounts_dir().join(&account_id.0));

    ctx.bus.chats();
    let accounts = accounts_dto(ctx).await?;
    ctx.sink.emit(EngineEvent::AccountsChanged { accounts });
    Ok(EngineResponse::LoggedOut {
        account_id: account_id.clone(),
    })
}

/// Best-effort remote revocation for an account that is not running.
async fn revoke_stored_session(ctx: &EngineContext, record: &AccountRecord) {
    let Ok(Some(session)) = ctx.store.get_secret(&record.account_id).await else {
        return;
    };
    let dir = ctx.accounts_dir().join(&record.account_id);
    let Ok(connector) = ctx
        .factory
        .create(convert::network_store_to_protocol(record.network), dir)
        .await
    else {
        tracing::warn!(account = %record.account_id, "cannot build a connector to revoke the session");
        return;
    };
    if connector.restore(SessionBlob::new(session)).await.is_err() {
        return;
    }
    if let Err(err) = connector.logout().await {
        tracing::warn!(account = %record.account_id, error = %err, "remote logout failed");
    }
}

fn drop_directory(dir: &Path) {
    match std::fs::remove_dir_all(dir) {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => {
            tracing::warn!(path = %dir.display(), error = %err, "cannot remove the login directory");
        }
    }
}

fn remove_cached_file(path: &str) {
    match std::fs::remove_file(path) {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => {
            tracing::warn!(path = %path, error = %err, "cannot remove a cached media file");
        }
    }
}

/// Stores the user-entered Telegram application pair, sealed in the OS keyring.
async fn set_telegram_app(
    ctx: &EngineContext,
    api_id: i32,
    api_hash: &str,
) -> Result<EngineResponse, EngineError> {
    let record = factory::TelegramAppRecord::parse(api_id, api_hash)
        .map_err(EngineError::invalid_argument)?;
    factory::save_telegram_app(ctx.secrets.as_ref(), &record).await?;
    Ok(EngineResponse::TelegramAppSet { configured: true })
}

/// Deletes the sealed pair; Telegram is unready again and cannot log in.
async fn clear_telegram_app(ctx: &EngineContext) -> Result<EngineResponse, EngineError> {
    factory::clear_telegram_app(ctx.secrets.as_ref()).await?;
    Ok(EngineResponse::TelegramAppCleared)
}

/// Reports the pair in effect without ever returning the hash.
async fn get_telegram_app(ctx: &EngineContext) -> Result<EngineResponse, EngineError> {
    let resolved = factory::resolve_telegram_app(ctx.secrets.as_ref()).await;
    Ok(match resolved {
        Some(resolved) => EngineResponse::TelegramApp {
            configured: true,
            api_id: Some(resolved.config.api_id),
            source: convert::telegram_app_source(resolved.source),
        },
        None => EngineResponse::TelegramApp {
            configured: false,
            api_id: None,
            source: alloy_protocol::TelegramAppSource::None,
        },
    })
}
