use crate::accounts::{AccountEntry, AccountIndex};
use crate::context::{EngineContext, PendingOidcLogin};
use crate::error::EngineError;
use crate::router::{CommandGroup, GroupHandler, command_name};
use alloy_core::{AlloyClient, AlloyError, auth::discover_homeserver, delete_account_secrets};
use alloy_protocol::{Account, EngineCommand, EngineEvent, EngineResponse};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Matrix display names are capped at 255 characters.
const MAX_DISPLAY_NAME_CHARS: usize = 255;

/// How long a started OIDC login waits for the app to deliver the authorization redirect.
const OIDC_LOGIN_TTL: Duration = Duration::from_secs(10 * 60);

/// Handler for authentication commands (`CommandGroup::Auth`).
///
/// Owned by stream: `core-auth`. Supports discovery, password and OIDC (MAS) login, session
/// restore, logout and account listing.
#[derive(Default)]
pub struct AuthHandler;

impl AuthHandler {
    pub fn new() -> Self {
        Self
    }
}

fn map_err(e: AlloyError) -> EngineError {
    match e {
        AlloyError::Oidc { reason, message } => EngineError::auth_failed_with(reason, message),
        AlloyError::Auth(m) => EngineError::auth_failed(m),
        AlloyError::AccountNotFound(m) => EngineError::not_found(m),
        AlloyError::NotLoggedIn => EngineError::auth_failed("not logged in"),
        AlloyError::InvalidHomeserverUrl => EngineError::invalid_argument("invalid homeserver"),
        AlloyError::InsecureHomeserverUrl => EngineError {
            code: "insecure_homeserver".to_string(),
            message: "homeserver must use https (http is allowed only for localhost)".to_string(),
            reason: None,
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

/// Starts persisting refreshed OAuth tokens for the active session, replacing an earlier watcher.
fn start_session_watch(ctx: &EngineContext, client: &Arc<AlloyClient>) {
    let mut slot = ctx
        .session_watch
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(handle) = slot.take() {
        handle.abort();
    }
    if !client.is_oauth_session() {
        return;
    }
    *slot = Some(tokio::spawn(client.clone().watch_session_tokens()));
}

/// Stops the token watcher, e.g. when the session is logged out.
fn stop_session_watch(ctx: &EngineContext) {
    let mut slot = ctx
        .session_watch
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(handle) = slot.take() {
        handle.abort();
    }
}

/// Registers a freshly authenticated client: fetches its profile, indexes the account, makes it
/// the active client and starts its background work. Shared by password and OIDC login.
async fn finish_login(
    ctx: &EngineContext,
    index: &AccountIndex,
    client: AlloyClient,
    store_id: &str,
) -> Result<Account, EngineError> {
    let profile = match client.account_profile().await {
        Ok(profile) => profile,
        Err(e) => {
            // A session was stored by the login; revoke it on the server as well.
            if let Err(logout_err) = client.logout().await {
                tracing::warn!(error = %logout_err, "remote logout failed after login");
            }
            purge_account(ctx, index, store_id).await;
            return Err(map_err(e));
        }
    };
    let entry = AccountEntry {
        user_id: profile.user_id,
        homeserver_url: profile.homeserver_url,
        display_name: profile.display_name,
        avatar_url: profile.avatar_url,
        store_id: store_id.to_owned(),
    };
    let account = entry.to_account();
    let replaced = match index.upsert(entry) {
        Ok(replaced) => replaced,
        Err(e) => {
            purge_account(ctx, index, store_id).await;
            return Err(EngineError::internal_error(e.to_string()));
        }
    };
    if let Some(old) = replaced {
        purge_account(ctx, index, &old.store_id).await;
    }
    ctx.set_client(Some(client)).await;
    if ctx.background
        && let Ok(active) = ctx.active_client().await
    {
        super::crypto::start_background(ctx, active.clone()).await;
        start_session_watch(ctx, &active);
    }
    ctx.sink.emit(EngineEvent::AuthState {
        account: Some(account.clone()),
    });
    Ok(account)
}

/// Waits for the browser's loopback redirect and finishes the pending login.
///
/// The listener is bound before the authorization URL is handed to the UI, so the redirect can
/// never arrive before the app is ready for it. Exactly one request is served; a timeout leaves
/// the pending login to expire on its own.
async fn await_oidc_redirect(
    ctx: EngineContext,
    listener: tokio::net::TcpListener,
    store_id: String,
) {
    let accepted = tokio::time::timeout(OIDC_LOGIN_TTL, listener.accept()).await;
    let Ok(Ok((mut stream, _peer))) = accepted else {
        return;
    };
    let Some(target) = read_request_target(&mut stream).await else {
        respond(&mut stream, 400, "This is not an OIDC redirect.").await;
        return;
    };
    let Some((path, query)) = target.split_once('?') else {
        respond(&mut stream, 400, "The authorization response is missing.").await;
        return;
    };
    if path != "/callback" {
        respond(&mut stream, 404, "Not found.").await;
        return;
    }
    match finish_oidc_from_query(&ctx, &store_id, query).await {
        Ok(account) => {
            let body = format!(
                "You are signed in as {}. You can close this window and return to Alloy.",
                account.user_id.0
            );
            respond(&mut stream, 200, &body).await;
        }
        Err(err) => {
            respond(
                &mut stream,
                400,
                &format!("Sign-in failed: {}", err.message),
            )
            .await;
        }
    }
}

/// Completes the pending login for `store_id` from the redirect's query string.
async fn finish_oidc_from_query(
    ctx: &EngineContext,
    store_id: &str,
    query: &str,
) -> Result<Account, EngineError> {
    let pending = {
        let mut slot = ctx.pending_oidc.lock().await;
        match slot.as_ref() {
            Some(pending) if pending.store_id == store_id => slot.take(),
            _ => None,
        }
    };
    let Some(pending) = pending else {
        return Err(EngineError::auth_failed_with(
            "not_pending",
            "no OIDC login in progress",
        ));
    };
    let PendingOidcLogin {
        client,
        store_id,
        started_at,
        ..
    } = pending;
    let index = AccountIndex::new(&ctx.data_dir);
    if started_at.elapsed() > OIDC_LOGIN_TTL {
        let _ = client.logout().await;
        drop_pending_store(ctx, &index, client, &store_id).await;
        return Err(EngineError::auth_failed_with(
            "expired",
            "OIDC login expired; start again",
        ));
    }
    if let Err(e) = client.complete_oidc_login(&format!("?{query}")).await {
        let _ = client.logout().await;
        drop_pending_store(ctx, &index, client, &store_id).await;
        return Err(map_err(e));
    }
    finish_login(ctx, &index, client, &store_id).await
}

/// Releases a pending login's client before deleting its store.
///
/// Windows keeps the store's file handles open until the client is dropped, so purging first
/// would leave the directory behind and leak an account store on every cancelled attempt.
async fn drop_pending_store(
    ctx: &EngineContext,
    index: &AccountIndex,
    client: AlloyClient,
    store_id: &str,
) {
    drop(client);
    purge_account(ctx, index, store_id).await;
}

/// The request target (`/callback?...`) of the first GET line, or `None` for anything else.
async fn read_request_target(stream: &mut tokio::net::TcpStream) -> Option<String> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        let read = tokio::io::AsyncReadExt::read(stream, &mut chunk)
            .await
            .ok()?;
        if read == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..read]);
        if buf.windows(4).any(|window| window == b"\r\n\r\n") || buf.len() > 16 * 1024 {
            break;
        }
    }
    let head = String::from_utf8_lossy(&buf);
    let mut parts = head.lines().next()?.split_whitespace();
    if parts.next()? != "GET" {
        return None;
    }
    parts.next().map(ToOwned::to_owned)
}

/// Writes one small HTML page and closes the connection.
async fn respond(stream: &mut tokio::net::TcpStream, status: u16, message: &str) {
    let reason = match status {
        200 => "OK",
        404 => "Not Found",
        _ => "Bad Request",
    };
    let body = format!(
        "<!doctype html><meta charset=\"utf-8\"><title>Alloy</title><p>{}</p>",
        html_escape(message)
    );
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = tokio::io::AsyncWriteExt::write_all(stream, head.as_bytes()).await;
    let _ = tokio::io::AsyncWriteExt::write_all(stream, body.as_bytes()).await;
    let _ = tokio::io::AsyncWriteExt::flush(stream).await;
}

fn html_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
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
                let account = finish_login(ctx, &index, client, &store_id).await?;
                Ok(EngineResponse::LoggedIn { account })
            }
            EngineCommand::StartOidcLogin { homeserver_url } => {
                // One login at a time: drop any earlier attempt and its temporary store.
                if let Some(previous) = ctx.pending_oidc.lock().await.take() {
                    if let Some(handle) = previous.callback {
                        handle.abort();
                    }
                    let PendingOidcLogin {
                        client, store_id, ..
                    } = previous;
                    drop_pending_store(ctx, &index, client, &store_id).await;
                }
                let store_id = uuid::Uuid::new_v4().to_string();
                let client = open_client(ctx, &homeserver_url, &store_id).await?;
                // RFC 8252 loopback redirect: the authorization server rejects a custom scheme
                // by default (MAS does), so the engine listens on an ephemeral local port and
                // receives the code itself. Nothing else needs a platform deep-link registration.
                let listener = match tokio::net::TcpListener::bind(("127.0.0.1", 0)).await {
                    Ok(listener) => listener,
                    Err(e) => {
                        drop(client);
                        purge_account(ctx, &index, &store_id).await;
                        return Err(EngineError::internal_error(format!(
                            "cannot listen for the OIDC redirect: {e}"
                        )));
                    }
                };
                let port = match listener.local_addr() {
                    Ok(addr) => addr.port(),
                    Err(e) => {
                        drop(client);
                        purge_account(ctx, &index, &store_id).await;
                        return Err(EngineError::internal_error(format!(
                            "cannot read the OIDC redirect address: {e}"
                        )));
                    }
                };
                let redirect_uri = format!("http://127.0.0.1:{port}/callback");
                let start = match client.start_oidc_login(&redirect_uri).await {
                    Ok(start) => start,
                    Err(e) => {
                        drop(client);
                        purge_account(ctx, &index, &store_id).await;
                        return Err(map_err(e));
                    }
                };
                let listener_ctx = ctx.clone();
                let listener_store_id = store_id.clone();
                let callback = tokio::spawn(async move {
                    await_oidc_redirect(listener_ctx, listener, listener_store_id).await;
                });
                *ctx.pending_oidc.lock().await = Some(PendingOidcLogin {
                    client,
                    store_id,
                    started_at: Instant::now(),
                    callback: Some(callback),
                });
                Ok(EngineResponse::OidcUrl {
                    authorization_url: start.authorization_url,
                    redirect_uri: start.redirect_uri,
                })
            }
            EngineCommand::CompleteOidcLogin { redirect_url } => {
                let pending = ctx.pending_oidc.lock().await.take().ok_or_else(|| {
                    EngineError::auth_failed_with("not_pending", "no OIDC login in progress")
                })?;
                if let Some(handle) = pending.callback {
                    handle.abort();
                }
                let PendingOidcLogin {
                    client,
                    store_id,
                    started_at,
                    ..
                } = pending;
                if started_at.elapsed() > OIDC_LOGIN_TTL {
                    drop_pending_store(ctx, &index, client, &store_id).await;
                    return Err(EngineError::auth_failed_with(
                        "expired",
                        "OIDC login expired; start again",
                    ));
                }
                if let Err(e) = client.complete_oidc_login(&redirect_url).await {
                    // The authorization may have succeeded even when the session could not be
                    // stored; revoke it so no usable token is left behind on the server.
                    if let Err(logout_err) = client.logout().await {
                        tracing::debug!(error = %logout_err, "no OIDC session to revoke");
                    }
                    drop_pending_store(ctx, &index, client, &store_id).await;
                    return Err(map_err(e));
                }
                let account = finish_login(ctx, &index, client, &store_id).await?;
                Ok(EngineResponse::LoggedIn { account })
            }
            EngineCommand::CancelOidcLogin => {
                if let Some(pending) = ctx.pending_oidc.lock().await.take() {
                    if let Some(handle) = pending.callback {
                        handle.abort();
                    }
                    let PendingOidcLogin {
                        client, store_id, ..
                    } = pending;
                    drop_pending_store(ctx, &index, client, &store_id).await;
                }
                Ok(EngineResponse::Unit)
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
                    super::crypto::start_background(ctx, active.clone()).await;
                    start_session_watch(ctx, &active);
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
                        stop_session_watch(ctx);
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
