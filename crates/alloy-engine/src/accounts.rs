//! Running accounts: one connector, one event actor and one outbox worker per connected account.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use alloy_connector::{Capabilities, Connector, ConnectorEvent, SessionBlob};
use alloy_protocol::{Account as AccountDto, AccountId, EngineEvent, MediaSource, UserId};
use alloy_store::{AccountRecord, AccountState as StoreAccountState, Network as StoreNetwork};
use tokio::sync::{Notify, RwLock, mpsc};
use tokio::task::AbortHandle;

use crate::context::EngineContext;
use crate::convert;
use crate::error::{EngineError, map_connector, map_store};

/// How many chat-list pages a startup sync follows before it gives up on a runaway cursor.
const MAX_SYNC_PAGES: u32 = 50;
/// How many chats get their history backfilled on first start.
const BACKFILL_CHATS: u32 = 20;
/// How many newest messages of a chat are fetched when it has no local history.
const BACKFILL_MESSAGES: usize = 30;
/// Chat page size for the startup sync.
const SYNC_PAGE: usize = 100;

/// One connected account: its connector and the tasks driving it.
pub struct AccountRuntime {
    pub account_id: String,
    pub network: StoreNetwork,
    /// The network's id for the signed-in user; `None` when the connector did not report one.
    pub remote_user_id: Option<String>,
    pub connector: Arc<dyn Connector>,
    pub capabilities: Capabilities,
    wake: Arc<Notify>,
    tasks: Mutex<Vec<AbortHandle>>,
}

impl AccountRuntime {
    /// Wakes the outbox worker; the permit survives until the worker waits again.
    pub fn notify_outbox(&self) {
        self.wake.notify_one();
    }

    /// Waits until the outbox worker is woken; used to avoid polling the queue.
    pub async fn wait_outbox(&self) {
        self.wake.notified().await;
    }

    fn track(&self, handle: AbortHandle) {
        self.tasks
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(handle);
    }

    /// Stops the account's tasks. The connector itself is stopped by the caller when it still
    /// matters (logout); aborting the actor only ends local processing.
    pub fn abort_tasks(&self) {
        let mut tasks = self.tasks.lock().unwrap_or_else(PoisonError::into_inner);
        for handle in tasks.drain(..) {
            handle.abort();
        }
    }
}

/// The connected accounts of this engine process.
#[derive(Clone, Default)]
pub struct AccountManager {
    running: Arc<RwLock<HashMap<String, Arc<AccountRuntime>>>>,
}

impl AccountManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn get(&self, account_id: &str) -> Option<Arc<AccountRuntime>> {
        self.running.read().await.get(account_id).cloned()
    }

    /// Registers a runtime, stopping a previous one for the same account.
    pub async fn insert(&self, runtime: Arc<AccountRuntime>) {
        let previous = self
            .running
            .write()
            .await
            .insert(runtime.account_id.clone(), runtime);
        if let Some(previous) = previous {
            previous.abort_tasks();
        }
    }

    /// Removes the runtime for `account_id` and aborts its tasks.
    pub async fn remove(&self, account_id: &str) -> Option<Arc<AccountRuntime>> {
        let runtime = self.running.write().await.remove(account_id);
        if let Some(runtime) = &runtime {
            runtime.abort_tasks();
        }
        runtime
    }

    /// Stops every runtime; used on teardown and before a sync refresh.
    ///
    /// Connectors are told to stop before their tasks are aborted, so a connector that owns
    /// sockets or a sidecar process releases them instead of being dropped mid-stream.
    pub async fn stop_all(&self) {
        let runtimes: Vec<Arc<AccountRuntime>> = self
            .running
            .write()
            .await
            .drain()
            .map(|(_, runtime)| runtime)
            .collect();
        for runtime in runtimes {
            if let Err(err) = runtime.connector.stop().await {
                tracing::warn!(account = %runtime.account_id, error = %err, "connector did not stop");
            }
            runtime.abort_tasks();
        }
    }
}

/// The protocol DTO of one stored account.
pub async fn account_dto(
    ctx: &EngineContext,
    record: &AccountRecord,
) -> Result<AccountDto, EngineError> {
    let capabilities = match ctx.accounts.get(&record.account_id).await {
        Some(runtime) => convert::capabilities_dto(&runtime.capabilities),
        None => convert::no_capabilities(),
    };
    Ok(AccountDto {
        id: AccountId(record.account_id.clone()),
        network: convert::network_store_to_protocol(record.network),
        remote_user_id: record.remote_user_id.clone().map(UserId),
        display_name: record.display_name.clone(),
        avatar: record
            .avatar_media_id
            .clone()
            .map(|media_id| MediaSource { media_id }),
        state: convert::account_state_store_to_protocol(record.state),
        capabilities,
    })
}

/// Every stored account, for `ListAccounts` and the `AccountsChanged` event.
pub async fn accounts_dto(ctx: &EngineContext) -> Result<Vec<AccountDto>, EngineError> {
    let records = ctx.store.list_accounts().await.map_err(map_store)?;
    let mut accounts = Vec::with_capacity(records.len());
    for record in &records {
        accounts.push(account_dto(ctx, record).await?);
    }
    Ok(accounts)
}

/// Starts an account whose session is already stored, restoring it through a fresh connector.
pub async fn start_stored_account(
    ctx: &EngineContext,
    record: AccountRecord,
) -> Result<Arc<AccountRuntime>, EngineError> {
    let account_dir = ctx.accounts_dir().join(&record.account_id);
    std::fs::create_dir_all(&account_dir).map_err(|err| {
        EngineError::internal_error(format!(
            "cannot create the account directory {}: {err}",
            account_dir.display()
        ))
    })?;
    let connector = ctx
        .factory
        .create(
            convert::network_store_to_protocol(record.network),
            account_dir,
        )
        .await
        .map_err(map_connector)?;
    let session = ctx
        .store
        .get_secret(&record.account_id)
        .await
        .map_err(map_store)?
        .ok_or_else(|| EngineError::auth_failed("the account has no stored session"))?;
    let user = match connector.restore(SessionBlob::new(session)).await {
        Ok(user) => user,
        Err(err) => {
            mark_needs_relogin(ctx, &record.account_id).await;
            return Err(map_connector(err));
        }
    };
    start_with_connector(ctx, record, connector, Some(user.remote_user_id)).await
}

/// Starts the actor and outbox worker for an account whose connector is already usable.
///
/// Returns once the initial chat sync has been applied, so a caller that immediately subscribes
/// sees the account's chats instead of racing the first sync.
pub async fn start_with_connector(
    ctx: &EngineContext,
    record: AccountRecord,
    connector: Arc<dyn Connector>,
    remote_user_id: Option<String>,
) -> Result<Arc<AccountRuntime>, EngineError> {
    let runtime = Arc::new(AccountRuntime {
        account_id: record.account_id.clone(),
        network: record.network,
        remote_user_id: remote_user_id.or_else(|| record.remote_user_id.clone()),
        capabilities: connector.capabilities(),
        connector: connector.clone(),
        wake: Arc::new(Notify::new()),
        tasks: Mutex::new(Vec::new()),
    });

    let (sender, receiver) = mpsc::channel::<ConnectorEvent>(512);
    connector.start(sender).await.map_err(map_connector)?;

    let actor = tokio::spawn(crate::actor::run(ctx.clone(), runtime.clone(), receiver));
    runtime.track(actor.abort_handle());
    let outbox = tokio::spawn(crate::outbox::run(ctx.clone(), runtime.clone()));
    runtime.track(outbox.abort_handle());

    ctx.accounts.insert(runtime.clone()).await;
    initial_sync(ctx, &runtime).await;
    Ok(runtime)
}

/// Fetches the chat list and the newest history of the most recent chats that have none locally.
pub async fn initial_sync(ctx: &EngineContext, runtime: &AccountRuntime) {
    let account_id = runtime.account_id.as_str();
    let mut cursor: Option<String> = None;
    let mut pages = 0;
    loop {
        let page = match runtime
            .connector
            .list_chats(cursor.clone(), SYNC_PAGE)
            .await
        {
            Ok(page) => page,
            Err(err) => {
                tracing::warn!(account = %account_id, error = %err, "chat list sync failed");
                break;
            }
        };
        for chat in &page.items {
            if let Err(err) = crate::actor::apply_chat(ctx, account_id, chat.clone()).await {
                tracing::warn!(account = %account_id, error = %err.message, "failed to store a chat");
            }
        }
        pages += 1;
        match page.next_cursor {
            Some(next) if pages < MAX_SYNC_PAGES => cursor = Some(next),
            _ => break,
        }
    }

    let mut chats = match ctx
        .store
        .list_chats(Some(account_id.to_string()), false, BACKFILL_CHATS)
        .await
    {
        Ok(chats) => chats,
        Err(err) => {
            tracing::warn!(account = %account_id, error = %err, "chat projection unreadable");
            ctx.bus.chats();
            return;
        }
    };
    if let Ok(archived) = ctx
        .store
        .list_chats(Some(account_id.to_string()), true, BACKFILL_CHATS)
        .await
    {
        chats.extend(archived);
    }
    chats.sort_by_key(|chat| std::cmp::Reverse(chat.last_activity_at.unwrap_or(0)));
    chats.truncate(BACKFILL_CHATS as usize);

    for chat in chats {
        match ctx.store.page_messages(&chat.chat_id, None, 1).await {
            Ok(existing) if !existing.is_empty() => continue,
            Ok(_) => {}
            Err(err) => {
                tracing::warn!(chat = %chat.chat_id, error = %err, "chat history unreadable");
                continue;
            }
        }
        let page = match runtime
            .connector
            .backfill(&chat.remote_chat_id, None, BACKFILL_MESSAGES)
            .await
        {
            Ok(page) => page,
            Err(err) => {
                tracing::warn!(chat = %chat.chat_id, error = %err, "history backfill failed");
                continue;
            }
        };
        for message in page.items {
            if let Err(err) = crate::actor::apply_message(ctx, account_id, message, false).await {
                tracing::warn!(chat = %chat.chat_id, error = %err.message, "failed to store history");
            }
        }
    }

    ctx.bus.chats();
}

/// Records that an account can only be used again after a fresh login, and tells the UI.
pub async fn mark_needs_relogin(ctx: &EngineContext, account_id: &str) {
    if let Err(err) = ctx
        .store
        .update_account_state(account_id, StoreAccountState::NeedsRelogin)
        .await
    {
        tracing::warn!(account = %account_id, error = %err, "cannot mark the account for re-login");
        return;
    }
    if let Ok(accounts) = accounts_dto(ctx).await {
        ctx.sink.emit(EngineEvent::AccountsChanged { accounts });
    }
}

/// Retry delay of the account's outbox after a retryable connector failure.
pub fn retry_delay(attempts: i64) -> Duration {
    let exponent = u32::try_from(attempts.clamp(0, 9)).unwrap_or(9);
    Duration::from_secs(2u64.saturating_pow(exponent).min(300))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_delay_grows_then_caps() {
        assert_eq!(retry_delay(0), Duration::from_secs(1));
        assert_eq!(retry_delay(1), Duration::from_secs(2));
        assert_eq!(retry_delay(4), Duration::from_secs(16));
        assert_eq!(retry_delay(20), Duration::from_secs(300));
    }
}
