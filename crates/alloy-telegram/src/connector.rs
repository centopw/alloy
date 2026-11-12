//! The connector: pool lifecycle, the [`Connector`] implementation, and shared state.

use std::collections::{BTreeSet, HashMap};
use std::fmt::Debug;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex, MutexGuard, RwLock};
use std::time::{Duration, Instant};

use alloy_connector::{
    Capabilities, ConnectionState, Connector, ConnectorError, ConnectorEvent, LocalAttachment,
    LoginFailure, LoginInput, LoginStep, MediaRef, Network, OutgoingMessage, Page, RemoteChat,
    RemoteMessage, RemoteUser, SendAck, SessionBlob,
};
use async_trait::async_trait;
use grammers_client::client::{Client, PasswordToken};
use grammers_client::media::Attribute;
use grammers_client::message::{InputMessage, InputReactions};
use grammers_mtsender::{
    ConnectionParams, InvocationError, NoRetries, SenderPool, SenderPoolFatHandle,
    UpdatesConfiguration,
};
use grammers_session::Session;
use grammers_session::storages::MemorySession;
use grammers_session::types::{PeerAuth, PeerInfo, PeerKind, PeerRef, UpdateState, UpdatesState};
use grammers_tl_types as tl;
use tokio::sync::Mutex;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::task::JoinHandle;
use tracing::{debug, warn};

use crate::config::TelegramConfig;
use crate::error;
use crate::login::{self, LoginFlow};
use crate::mapping::{self, MediaLocation, MediaPointer, PeerEntry, PeerKindTag};
use crate::session::{self, Envelope, PeerCache, SelfProfile};
use crate::updates::MessageIndex;

/// Largest page `messages.getDialogs` accepts.
const MAX_DIALOG_PAGE: usize = 100;
/// How many message ids the delete/echo index remembers.
const MESSAGE_INDEX_CAPACITY: usize = 4_096;
/// Download attempts: the first with the stored reference, one more after refreshing it.
const MAX_MEDIA_ATTEMPTS: usize = 2;
/// How often the session checkpoint is written while updates flow.
const CHECKPOINT_INTERVAL: Duration = Duration::from_secs(60);

/// A Telegram personal-account connector.
///
/// The host holds exactly one per account: the connector owns one connection pool and one update
/// stream, and every data call is serialized behind it.
pub struct TelegramConnector {
    shared: Arc<Shared>,
    inner: Mutex<Inner>,
}

/// State every part of the connector can reach without ordering between API calls.
pub(crate) struct Shared {
    config: TelegramConfig,
    pub(crate) session: Arc<MemorySession>,
    peers: RwLock<PeerCache>,
    index: StdMutex<MessageIndex>,
    sink: StdMutex<Option<Sender<ConnectorEvent>>>,
    myself: RwLock<Option<SelfProfile>>,
    dc_ids: StdMutex<BTreeSet<i32>>,
    checkpoint: StdMutex<Option<Instant>>,
    logged_in: AtomicBool,
}

/// State that only the API methods touch, serialized by the connector’s mutex.
struct Inner {
    client: Option<Client>,
    pool: Option<PoolRuntime>,
    login: Option<LoginFlow>,
}

/// The network side: the sender pool, its runner task and, once started, the update pump.
struct PoolRuntime {
    handle: SenderPoolFatHandle,
    runner: JoinHandle<()>,
    /// Updates as grammers receives them, until the pump takes the receiver.
    updates: Option<Receiver<grammers_session::updates::UpdatesLike>>,
    pump: Option<JoinHandle<()>>,
}

impl Drop for PoolRuntime {
    fn drop(&mut self) {
        // `quit` asks the runner to close its connections; aborting guarantees neither task
        // outlives the session a later pool will share.
        self.handle.quit();
        self.runner.abort();
        if let Some(pump) = &self.pump {
            pump.abort();
        }
    }
}

/// One page of the dialog list plus where to resume inside the same folder.
struct DialogPage {
    chats: Vec<RemoteChat>,
    resume: Option<mapping::ChatCursor>,
}

/// Why a download attempt failed, before it is mapped to the contract.
enum DownloadFailure {
    Rpc(InvocationError),
    Io(std::io::Error),
}

/// Takes a shared lock, recovering from a poisoned mutex.
///
/// These tables are tiny and a panic elsewhere cannot leave them half-updated in a way that would
/// corrupt a later call, so recovering beats poisoning every subsequent request.
pub(crate) fn lock<T>(mutex: &StdMutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl TelegramConnector {
    /// Creates a connector with no session and no open connection.
    ///
    /// No network activity happens here; the first `begin_login`, `restore` or `start` opens the
    /// pool.
    pub fn new(config: TelegramConfig) -> Self {
        let mut dc_ids = BTreeSet::new();
        for id in 1..=5 {
            dc_ids.insert(id);
        }
        Self {
            shared: Arc::new(Shared {
                config,
                session: Arc::new(MemorySession::default()),
                peers: RwLock::new(PeerCache::new()),
                index: StdMutex::new(MessageIndex::new(MESSAGE_INDEX_CAPACITY)),
                sink: StdMutex::new(None),
                myself: RwLock::new(None),
                dc_ids: StdMutex::new(dc_ids),
                checkpoint: StdMutex::new(None),
                logged_in: AtomicBool::new(false),
            }),
            inner: Mutex::new(Inner {
                client: None,
                pool: None,
                login: None,
            }),
        }
    }

    /// Opens the pool if it is not open yet.
    ///
    /// No stream is created here: the update pump starts with the first [`Connector::start`], so a
    /// login that happens before it is not missed and no events are produced while nobody listens.
    async fn ensure_pool(&self, inner: &mut Inner) -> Result<Client, ConnectorError> {
        if let Some(client) = &inner.client {
            return Ok(client.clone());
        }

        let pool = SenderPool::with_configuration(
            Arc::clone(&self.shared.session),
            self.shared.config.api_id,
            connection_params(&self.shared.config),
        );
        let SenderPool {
            runner,
            handle,
            updates,
        } = pool;
        let client = Client::new(handle.clone());
        let runner_task = tokio::spawn(runner.run());

        if let Ok(home) = self.shared.session.home_dc_id() {
            self.shared.note_dc(home);
        }
        inner.client = Some(client.clone());
        inner.pool = Some(PoolRuntime {
            handle,
            runner: runner_task,
            updates: Some(updates),
            pump: None,
        });
        Ok(client)
    }

    /// Starts the update pump the first time it is needed.
    async fn ensure_pump(&self, inner: &mut Inner) -> Result<(), ConnectorError> {
        let Some(client) = inner.client.clone() else {
            return Ok(());
        };
        let Some(runtime) = inner.pool.as_mut() else {
            return Ok(());
        };
        if runtime.pump.is_some() {
            return Ok(());
        }
        let Some(updates) = runtime.updates.take() else {
            return Ok(());
        };
        let stream = client
            .stream_updates(updates, UpdatesConfiguration { catch_up: true })
            .await
            .map_err(|error| ConnectorError::Network {
                message: error.to_string(),
            })?;
        runtime.pump = Some(tokio::spawn(crate::updates::run(
            Arc::clone(&self.shared),
            stream,
        )));
        Ok(())
    }

    fn require_login(&self) -> Result<(), ConnectorError> {
        if self.shared.logged_in() {
            Ok(())
        } else {
            Err(ConnectorError::Unauthorized)
        }
    }

    /// Resolves a remote chat id to something Telegram accepts.
    async fn resolve_peer(&self, remote_chat_id: &str) -> Result<PeerRef, ConnectorError> {
        if let Some(reference) = self.shared.peer_ref(remote_chat_id) {
            return Ok(reference);
        }
        let (kind, bare_id) =
            mapping::decode_peer(remote_chat_id).ok_or_else(|| ConnectorError::InvalidInput {
                message: format!("invalid chat id: {remote_chat_id}"),
            })?;
        let id = mapping::peer_id(kind, bare_id).ok_or_else(|| ConnectorError::InvalidInput {
            message: format!("invalid chat id: {remote_chat_id}"),
        })?;
        if let Some(info) = self
            .shared
            .session
            .peer(id)
            .await
            .map_err(session::session_error)?
            && let Some(auth) = info.auth()
        {
            let reference = PeerRef { id, auth };
            self.shared.insert_peer(kind, bare_id, auth.hash());
            return Ok(reference);
        }
        Err(ConnectorError::InvalidInput {
            message: format!("unknown chat: {remote_chat_id}"),
        })
    }

    /// Moves the session to another datacenter, as Telegram asks during login.
    async fn migrate(&self, inner: &Inner, dc_id: i32) -> Result<(), ConnectorError> {
        let old = self
            .shared
            .session
            .home_dc_id()
            .map_err(session::session_error)?;
        if let Some(pool) = &inner.pool {
            pool.handle.disconnect_from_dc(old);
        }
        self.shared
            .session
            .set_home_dc_id(dc_id)
            .await
            .map_err(session::session_error)?;
        self.shared.note_dc(dc_id);
        Ok(())
    }

    /// Caches the account the session belongs to and records its update state.
    async fn establish(
        &self,
        client: &Client,
        user: &tl::enums::User,
    ) -> Result<SelfProfile, ConnectorError> {
        let entry = mapping::entry_from_user(user).ok_or_else(|| ConnectorError::Protocol {
            message: "telegram returned an empty user for this account".to_string(),
        })?;
        let is_bot = matches!(user, tl::enums::User::User(user) if user.bot);

        self.shared
            .session
            .cache_peer(PeerInfo::User {
                id: entry.bare_id,
                auth: (entry.auth != 0).then(|| PeerAuth::from_hash(entry.auth)),
                bot: Some(is_bot),
                is_self: Some(true),
            })
            .await
            .map_err(session::session_error)?;

        // Mirrors grammers' own post-login bookkeeping: without a state the message box would have
        // to guess where updates resume.
        if let Ok(tl::enums::updates::State::State(state)) =
            client.invoke(&tl::functions::updates::GetState {}).await
        {
            let updates_state = UpdatesState {
                pts: state.pts,
                qts: state.qts,
                date: state.date,
                seq: state.seq,
                channels: Vec::new(),
            };
            self.shared
                .session
                .set_update_state(UpdateState::All(updates_state))
                .await
                .map_err(session::session_error)?;
        }

        self.shared.remember_entry(&entry);
        let profile = SelfProfile {
            remote_user_id: entry.id,
            bare_id: entry.bare_id,
            display_name: entry.title,
            avatar: entry.avatar,
        };
        self.shared.set_self(profile.clone());
        Ok(profile)
    }

    /// Finishes a login that already produced an authorization.
    async fn finish_login(
        &self,
        inner: &mut Inner,
        authorization: &tl::types::auth::Authorization,
    ) -> LoginStep {
        let client = match self.ensure_pool(inner).await {
            Ok(client) => client,
            Err(failure) => {
                return LoginStep::Failed {
                    reason: login_failure(&failure),
                };
            }
        };
        self.complete_login(inner, &client, &authorization.user)
            .await
    }

    /// Caches the account, emits the fresh session blob and returns the terminal step.
    async fn complete_login(
        &self,
        inner: &mut Inner,
        client: &Client,
        user: &tl::enums::User,
    ) -> LoginStep {
        let profile = match self.establish(client, user).await {
            Ok(profile) => profile,
            Err(failure) => {
                return LoginStep::Failed {
                    reason: login_failure(&failure),
                };
            }
        };
        let session = match self.shared.blob().await {
            Ok(session) => session,
            Err(failure) => {
                return LoginStep::Failed {
                    reason: login_failure(&failure),
                };
            }
        };
        inner.login = None;
        self.shared.set_logged_in(true);
        if let Err(failure) = self.shared.emit_session().await {
            debug!(error = %failure, "cannot emit the telegram session after login");
        }
        LoginStep::Done {
            remote_user_id: profile.remote_user_id,
            display_name: profile.display_name,
            session,
        }
    }

    /// Answers a QR refresh: exports a token, follows a datacenter migration, or finishes the login.
    async fn qr_step(&self, inner: &mut Inner) -> LoginStep {
        let client = match self.ensure_pool(inner).await {
            Ok(client) => client,
            Err(failure) => {
                return LoginStep::Failed {
                    reason: login_failure(&failure),
                };
            }
        };
        let request = tl::functions::auth::ExportLoginToken {
            api_id: self.shared.config.api_id,
            api_hash: self.shared.config.api_hash.clone(),
            except_ids: Vec::new(),
        };
        let mut exported = match client.invoke(&request).await {
            Ok(token) => token,
            Err(failure) => {
                return LoginStep::Failed {
                    reason: error::map_login(failure, "qr"),
                };
            }
        };

        for _ in 0..login::MAX_QR_MIGRATIONS {
            match exported {
                tl::enums::auth::LoginToken::Token(token) => {
                    let expires_at = mapping::now_ms() + i64::from(token.expires) * 1_000;
                    inner.login = Some(LoginFlow::Qr);
                    return LoginStep::Qr {
                        payload: login::qr_payload(&token.token),
                        expires_at,
                    };
                }
                tl::enums::auth::LoginToken::MigrateTo(migrate) => {
                    if let Err(failure) = self.migrate(inner, migrate.dc_id).await {
                        return LoginStep::Failed {
                            reason: login_failure(&failure),
                        };
                    }
                    let request = tl::functions::auth::ImportLoginToken {
                        token: migrate.token.clone(),
                    };
                    exported = match client.invoke_in_dc(migrate.dc_id, &request).await {
                        Ok(token) => token,
                        Err(failure) => {
                            return LoginStep::Failed {
                                reason: error::map_login(failure, "qr"),
                            };
                        }
                    };
                }
                tl::enums::auth::LoginToken::Success(success) => {
                    return match success.authorization {
                        tl::enums::auth::Authorization::Authorization(authorization) => {
                            self.finish_login(inner, &authorization).await
                        }
                        // A QR token cannot create an account.
                        _ => LoginStep::Failed {
                            reason: LoginFailure::NotSupported,
                        },
                    };
                }
            }
        }

        LoginStep::Failed {
            reason: LoginFailure::Expired,
        }
    }

    /// Requests the login code for a phone number.
    async fn request_code(&self, inner: &mut Inner, phone: String) -> LoginStep {
        let client = match self.ensure_pool(inner).await {
            Ok(client) => client,
            Err(failure) => {
                return LoginStep::Failed {
                    reason: login_failure(&failure),
                };
            }
        };
        // Telegram answers a number that belongs to another datacenter with a 303 migration; one
        // retry after moving is enough because it cannot migrate twice.
        for attempt in 0..2 {
            let settings = tl::types::CodeSettings {
                allow_flashcall: false,
                current_number: false,
                allow_app_hash: false,
                allow_missed_call: false,
                allow_firebase: false,
                logout_tokens: None,
                token: None,
                app_sandbox: None,
                unknown_number: false,
            };
            let request = tl::functions::auth::SendCode {
                phone_number: phone.clone(),
                api_id: self.shared.config.api_id,
                api_hash: self.shared.config.api_hash.clone(),
                settings: settings.into(),
            };
            match client.invoke(&request).await {
                Ok(tl::enums::auth::SentCode::Code(code)) => {
                    let (via, length) = mapping::code_channel(&code.r#type);
                    inner.login = Some(LoginFlow::Code {
                        phone,
                        phone_code_hash: code.phone_code_hash,
                    });
                    return LoginStep::Code { via, length };
                }
                Ok(_) => {
                    // `sentCodeSuccess` means the account is already authorized, and
                    // `sentCodePaymentRequired` is a premium-code flow this connector does not run.
                    return LoginStep::Failed {
                        reason: LoginFailure::NotSupported,
                    };
                }
                Err(failure) => match migration_dc(&failure) {
                    Some(dc_id) if attempt == 0 => {
                        if let Err(failure) = self.migrate(inner, dc_id).await {
                            return LoginStep::Failed {
                                reason: login_failure(&failure),
                            };
                        }
                    }
                    _ => {
                        return LoginStep::Failed {
                            reason: error::map_login(failure, "phone"),
                        };
                    }
                },
            }
        }

        LoginStep::Failed {
            reason: error::invalid_input("phone", "Telegram kept asking for another datacenter"),
        }
    }

    /// Submits the login code, moving to the password step when the account uses 2FA.
    async fn submit_code(
        &self,
        inner: &mut Inner,
        phone: String,
        phone_code_hash: String,
        code: String,
    ) -> LoginStep {
        let client = match self.ensure_pool(inner).await {
            Ok(client) => client,
            Err(failure) => {
                return LoginStep::Failed {
                    reason: login_failure(&failure),
                };
            }
        };
        let request = tl::functions::auth::SignIn {
            phone_number: phone,
            phone_code_hash,
            phone_code: Some(code),
            email_verification: None,
        };
        match client.invoke(&request).await {
            Ok(tl::enums::auth::Authorization::Authorization(authorization)) => {
                self.finish_login(inner, &authorization).await
            }
            Ok(_) => LoginStep::Failed {
                reason: error::invalid_input(
                    "phone",
                    "this number has no Telegram account; create one in an official app first",
                ),
            },
            Err(failure) if failure.is("SESSION_PASSWORD_NEEDED") => {
                match self.password_token(&client).await {
                    Ok((token, hint)) => {
                        inner.login = Some(LoginFlow::Password { token });
                        LoginStep::Password { hint }
                    }
                    Err(failure) => LoginStep::Failed {
                        reason: login_failure(&failure),
                    },
                }
            }
            Err(failure) => LoginStep::Failed {
                reason: error::map_login(failure, "code"),
            },
        }
    }

    /// Reads the SRP parameters needed to check a two-step verification password.
    async fn password_token(
        &self,
        client: &Client,
    ) -> Result<(PasswordToken, Option<String>), ConnectorError> {
        let password: tl::types::account::Password = client
            .invoke(&tl::functions::account::GetPassword {})
            .await
            .map_err(error::map_invocation)?
            .into();
        let token = PasswordToken::new(password);
        let hint = token.hint().map(str::to_string);
        Ok((token, hint))
    }

    /// Checks the two-step verification password.
    async fn submit_password(
        &self,
        inner: &mut Inner,
        token: PasswordToken,
        password: String,
    ) -> LoginStep {
        let client = match self.ensure_pool(inner).await {
            Ok(client) => client,
            Err(failure) => {
                return LoginStep::Failed {
                    reason: login_failure(&failure),
                };
            }
        };
        match client.check_password(token, password).await {
            Ok(user) => self.complete_login(inner, &client, &user.raw).await,
            Err(grammers_client::SignInError::InvalidPassword(_)) => LoginStep::Failed {
                reason: error::invalid_input(
                    "password",
                    "Telegram rejected the two-step verification password",
                ),
            },
            Err(grammers_client::SignInError::PasswordRequired(token)) => {
                let hint = token.hint().map(str::to_string);
                inner.login = Some(LoginFlow::Password { token });
                LoginStep::Password { hint }
            }
            Err(grammers_client::SignInError::Other(failure)) => LoginStep::Failed {
                reason: error::map_login(failure, "password"),
            },
            Err(failure) => LoginStep::Failed {
                reason: error::invalid_input("password", failure.to_string()),
            },
        }
    }

    /// One page of dialogs from one folder, with a resume position when more remain.
    async fn dialog_page(
        &self,
        client: &Client,
        folder: i32,
        position: Option<&mapping::ChatCursor>,
        want: usize,
    ) -> Result<DialogPage, ConnectorError> {
        let want = want.min(MAX_DIALOG_PAGE);
        let limit = i32::try_from(want).map_err(|_| ConnectorError::InvalidInput {
            message: "limit is too large".to_string(),
        })?;
        let offset_peer = match position.and_then(|cursor| cursor.peer.as_deref()) {
            Some(id) => tl::enums::InputPeer::from(self.resolve_peer(id).await?),
            None => tl::enums::InputPeer::Empty,
        };
        let request = tl::functions::messages::GetDialogs {
            exclude_pinned: position.is_some(),
            folder_id: if folder == 1 { Some(1) } else { None },
            offset_date: position.map_or(0, |cursor| cursor.date),
            offset_id: position.map_or(0, |cursor| cursor.id),
            offset_peer,
            limit,
            hash: 0,
        };
        let response = client
            .invoke(&request)
            .await
            .map_err(error::map_invocation)?;
        let (dialogs, messages, users, chats) = match response {
            tl::enums::messages::Dialogs::Dialogs(page) => {
                (page.dialogs, page.messages, page.users, page.chats)
            }
            tl::enums::messages::Dialogs::Slice(page) => {
                (page.dialogs, page.messages, page.users, page.chats)
            }
            // `hash` is always zero, so Telegram never answers with the not-modified form.
            tl::enums::messages::Dialogs::NotModified(_) => {
                return Ok(DialogPage {
                    chats: Vec::new(),
                    resume: None,
                });
            }
        };

        let mut entries: HashMap<(PeerKindTag, i64), PeerEntry> =
            HashMap::with_capacity(users.len() + chats.len());
        let mut infos: Vec<PeerInfo> = Vec::with_capacity(users.len() + chats.len());
        for user in &users {
            if let Some(entry) = mapping::entry_from_user(user) {
                self.shared.remember_entry(&entry);
                infos.push(PeerInfo::from(user));
                entries.insert((entry.kind, entry.bare_id), entry);
            }
        }
        for chat in &chats {
            if let Some(entry) = mapping::entry_from_chat(chat) {
                self.shared.remember_entry(&entry);
                infos.push(PeerInfo::from(chat));
                entries.insert((entry.kind, entry.bare_id), entry);
            }
        }
        // grammers needs the same peers cached to resolve update gaps later.
        if !infos.is_empty() {
            self.shared
                .session
                .cache_peers(infos)
                .await
                .map_err(session::session_error)?;
        }

        let mut dates: HashMap<(String, i32), i32> = HashMap::with_capacity(messages.len());
        for raw in &messages {
            if let Some(chat) = mapping::message_chat(raw)
                && let Some(date) = mapping::raw_date(raw)
            {
                dates.insert((chat, mapping::message_id(raw)), date);
            }
        }

        let now_seconds = mapping::now_ms() / 1_000;
        let returned = dialogs.len();
        let mut mapped = Vec::with_capacity(returned);
        let mut resume = None;
        for dialog in &dialogs {
            let Some((kind, bare_id)) = mapping::dialog_peer(dialog) else {
                continue;
            };
            let Some(entry) = entries.get(&(kind, bare_id)) else {
                continue;
            };
            let activity = mapping::dialog_activity(dialog, &dates);
            if let Some(chat) = mapping::chat_from_dialog(dialog, entry, activity, now_seconds) {
                // The cursor follows a peer we can resolve again, never the archive pseudo-entry.
                resume = Some(mapping::ChatCursor {
                    folder,
                    date: i32::try_from(activity / 1_000).unwrap_or_default(),
                    id: mapping::dialog_top_message(dialog),
                    peer: Some(entry.id.clone()),
                });
                mapped.push(chat);
            }
        }

        Ok(DialogPage {
            chats: mapped,
            resume: if returned >= want { resume } else { None },
        })
    }

    /// Writes `location` into `dest`, returning the number of bytes written.
    async fn download_once(
        client: &Client,
        location: &MediaLocation,
        dest: &Path,
    ) -> Result<u64, DownloadFailure> {
        use tokio::io::AsyncWriteExt as _;

        let mut file = tokio::fs::File::create(dest)
            .await
            .map_err(DownloadFailure::Io)?;
        let mut download = client.iter_download(location);
        let mut written = 0u64;
        while let Some(chunk) = download.next().await.map_err(DownloadFailure::Rpc)? {
            file.write_all(&chunk).await.map_err(DownloadFailure::Io)?;
            written += chunk.len() as u64;
        }
        file.flush().await.map_err(DownloadFailure::Io)?;
        Ok(written)
    }

    /// Re-reads the message a media pointer came from and returns its current location.
    async fn refreshed_location(
        &self,
        client: &Client,
        pointer: &MediaPointer,
        peer: PeerRef,
    ) -> Result<Option<MediaLocation>, ConnectorError> {
        let mut messages = client
            .get_messages_by_id(peer, &[pointer.msg])
            .await
            .map_err(error::map_invocation)?;
        let Some(message) = messages.pop().flatten() else {
            return Err(ConnectorError::InvalidInput {
                message: "the message with this media is gone".to_string(),
            });
        };
        let media = match &message.raw {
            tl::enums::Message::Message(message) => message.media.as_ref(),
            _ => None,
        };
        Ok(media.and_then(mapping::location_for))
    }

    /// Uploads an attachment and attaches it to the message being sent.
    async fn attach(
        &self,
        client: &Client,
        mut input: InputMessage,
        attachment: &LocalAttachment,
    ) -> Result<InputMessage, ConnectorError> {
        let path = Path::new(&attachment.path);
        let mut file =
            tokio::fs::File::open(path)
                .await
                .map_err(|error| ConnectorError::InvalidInput {
                    message: format!("cannot read {}: {error}", attachment.path),
                })?;
        let size = file
            .metadata()
            .await
            .map_err(|error| ConnectorError::InvalidInput {
                message: format!("cannot read {}: {error}", attachment.path),
            })?
            .len();
        let size = usize::try_from(size).map_err(|_| ConnectorError::InvalidInput {
            message: format!("{} is too large to upload", attachment.path),
        })?;
        let name = attachment
            .file_name
            .clone()
            .or_else(|| {
                path.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| attachment.path.clone());
        let uploaded = client
            .upload_stream(&mut file, size, name.clone())
            .await
            .map_err(|error| ConnectorError::InvalidInput {
                message: format!("cannot upload {}: {error}", attachment.path),
            })?;

        let mime = attachment
            .mime
            .clone()
            .unwrap_or_else(|| mapping::mime_from_name(&name));
        input = input.mime_type(&mime);
        if mime.starts_with("image/") && mime != "image/gif" {
            return Ok(input.photo(uploaded));
        }
        input = input.document(uploaded);
        if mime == "audio/ogg"
            && let Some(duration) = ogg_duration(path).await
        {
            // Telegram only renders an audio track when the document carries its length.
            input = input.attribute(Attribute::Audio {
                duration,
                title: None,
                performer: None,
            });
        }
        Ok(input)
    }
}

#[async_trait]
impl Connector for TelegramConnector {
    fn network(&self) -> Network {
        Network::Telegram
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            edit: true,
            // Telegram accepts edits for 48 hours in most chats.
            edit_window_secs: Some(172_800),
            delete_for_everyone: true,
            reactions: true,
            // Only premium accounts may keep more than one reaction per message.
            max_reactions_per_user: Some(1),
            replies: true,
            threads: false,
            typing: true,
            read_receipts: true,
            delivered_receipts: false,
            voice: true,
            polls: false,
            location: false,
            max_upload_bytes: Some(2_097_152_000),
            formatted_text: true,
        }
    }

    async fn begin_login(&self) -> LoginStep {
        let mut inner = self.inner.lock().await;
        inner.login = None;
        if let Err(failure) = self.ensure_pool(&mut inner).await {
            return LoginStep::Failed {
                reason: login_failure(&failure),
            };
        }
        inner.login = Some(LoginFlow::Phone);
        LoginStep::PhoneNumber
    }

    async fn submit_login(&self, input: LoginInput) -> LoginStep {
        let mut inner = self.inner.lock().await;
        let Some(flow) = inner.login.take() else {
            return LoginStep::Failed {
                reason: error::invalid_input(
                    "login",
                    "no login attempt is in progress; call begin_login first",
                ),
            };
        };
        match (flow, input) {
            (LoginFlow::Phone | LoginFlow::Qr, LoginInput::QrRefresh) => {
                self.qr_step(&mut inner).await
            }
            (LoginFlow::Phone, LoginInput::PhoneNumber { phone }) => {
                self.request_code(&mut inner, phone).await
            }
            (
                LoginFlow::Code {
                    phone,
                    phone_code_hash,
                },
                LoginInput::Code { code },
            ) => {
                self.submit_code(&mut inner, phone, phone_code_hash, code)
                    .await
            }
            (LoginFlow::Password { token, .. }, LoginInput::Password { password }) => {
                self.submit_password(&mut inner, token, password).await
            }
            (_, input) => LoginStep::Failed {
                reason: error::invalid_input(
                    login::field_of(&input),
                    "this answer does not match the current login step",
                ),
            },
        }
    }

    async fn restore(&self, session_blob: SessionBlob) -> Result<RemoteUser, ConnectorError> {
        let envelope = session::decode(session_blob.as_bytes())?;
        let mut inner = self.inner.lock().await;
        drop_pool(&mut inner);
        inner.login = None;
        session::import(&self.shared.session, &envelope).await?;
        self.shared.load(&envelope);

        let client = self.ensure_pool(&mut inner).await?;
        if !client
            .is_authorized()
            .await
            .map_err(error::map_invocation)?
        {
            return Err(ConnectorError::Unauthorized);
        }
        let users = client
            .invoke(&tl::functions::users::GetUsers {
                id: vec![tl::enums::InputUser::UserSelf],
            })
            .await
            .map_err(error::map_invocation)?;
        let myself = users
            .into_iter()
            .next()
            .ok_or(ConnectorError::Unauthorized)?;
        let profile = self.establish(&client, &myself).await?;
        self.shared.set_logged_in(true);
        if let Err(failure) = self.shared.emit_session().await {
            debug!(error = %failure, "cannot emit the telegram session after restore");
        }
        Ok(RemoteUser {
            remote_user_id: profile.remote_user_id,
            display_name: profile.display_name,
            avatar: profile.avatar,
        })
    }

    async fn start(&self, events: Sender<ConnectorEvent>) -> Result<(), ConnectorError> {
        self.require_login()?;
        let mut inner = self.inner.lock().await;
        self.ensure_pool(&mut inner).await?;
        self.ensure_pump(&mut inner).await?;
        *lock(&self.shared.sink) = Some(events);
        self.shared
            .emit(ConnectorEvent::Connection {
                state: ConnectionState::Connecting,
            })
            .await;
        self.shared
            .emit(ConnectorEvent::Connection {
                state: ConnectionState::Online,
            })
            .await;
        Ok(())
    }

    async fn stop(&self) -> Result<(), ConnectorError> {
        *lock(&self.shared.sink) = None;
        Ok(())
    }

    async fn list_chats(
        &self,
        cursor: Option<String>,
        limit: usize,
    ) -> Result<Page<RemoteChat>, ConnectorError> {
        if limit == 0 {
            return Err(ConnectorError::InvalidInput {
                message: "limit must be greater than zero".to_string(),
            });
        }
        let mut position = cursor
            .map(|value| mapping::decode_cursor(&value))
            .transpose()?;
        let mut inner = self.inner.lock().await;
        self.require_login()?;
        let client = self.ensure_pool(&mut inner).await?;

        // Telegram keeps the archive in its own folder, so the list is the main folder followed by
        // the archive; each part is newest-first and the cursor says which part to resume in.
        let mut folder = position.as_ref().map_or(0, |cursor| cursor.folder);
        let mut remaining = limit;
        let mut items: Vec<RemoteChat> = Vec::with_capacity(limit.min(MAX_DIALOG_PAGE));
        let next_cursor = loop {
            let page = self
                .dialog_page(&client, folder, position.as_ref(), remaining)
                .await?;
            remaining = remaining.saturating_sub(page.chats.len());
            items.extend(page.chats);
            if let Some(resume) = page.resume {
                break Some(mapping::encode_cursor(&resume));
            }
            if folder != 0 {
                break None;
            }
            if remaining == 0 {
                break Some(mapping::encode_cursor(&mapping::ChatCursor {
                    folder: 1,
                    ..mapping::ChatCursor::default()
                }));
            }
            folder = 1;
            position = None;
        };
        Ok(Page { items, next_cursor })
    }

    async fn backfill(
        &self,
        remote_chat_id: &str,
        before: Option<String>,
        limit: usize,
    ) -> Result<Page<RemoteMessage>, ConnectorError> {
        if limit == 0 {
            return Err(ConnectorError::InvalidInput {
                message: "limit must be greater than zero".to_string(),
            });
        }
        let before = before
            .map(|value| mapping::parse_msg_id(&value))
            .transpose()?;
        let mut inner = self.inner.lock().await;
        self.require_login()?;
        let client = self.ensure_pool(&mut inner).await?;
        let peer = self.resolve_peer(remote_chat_id).await?;

        let mut iterator = client.iter_messages(peer).limit(limit);
        if let Some(id) = before {
            iterator = iterator.offset_id(id);
        }
        let mut items = Vec::with_capacity(limit.min(MAX_DIALOG_PAGE));
        while items.len() < limit {
            let Some(message) = iterator.next().await.map_err(error::map_invocation)? else {
                break;
            };
            let txn_id = self.shared.txn_of(mapping::message_id(&message.raw));
            if let Some(mapped) = mapping::message_from_raw(
                &message.raw,
                remote_chat_id,
                self.shared.self_bare_id(),
                txn_id,
            ) {
                items.push(mapped);
            }
        }
        let next_cursor = items.last().map(|message| message.remote_msg_id.clone());
        Ok(Page { items, next_cursor })
    }

    async fn send(&self, message: OutgoingMessage) -> Result<SendAck, ConnectorError> {
        let OutgoingMessage {
            txn_id,
            remote_chat_id,
            body,
            formatted,
            reply_to,
            attachment,
        } = message;
        let mut inner = self.inner.lock().await;
        self.require_login()?;
        let client = self.ensure_pool(&mut inner).await?;
        let peer = self.resolve_peer(&remote_chat_id).await?;
        if body.is_none() && attachment.is_none() {
            return Err(ConnectorError::InvalidInput {
                message: "a message needs a body or an attachment".to_string(),
            });
        }
        let reply_to = reply_to
            .map(|value| mapping::parse_msg_id(&value))
            .transpose()?;
        let mut input = InputMessage::new()
            .text(body.unwrap_or_default())
            .reply_to(reply_to);
        if let Some(html) = formatted {
            let (_, entities) = grammers_client::parsers::parse_html_message(&html);
            if !entities.is_empty() {
                input = input.fmt_entities(entities);
            }
        }
        if let Some(attachment) = &attachment {
            input = self.attach(&client, input, attachment).await?;
        }

        let sent = client
            .send_message(peer, input)
            .await
            .map_err(error::map_invocation)?;
        let id = sent.id();
        let sent_at = mapping::raw_date(&sent.raw)
            .map(mapping::seconds_to_ms)
            .ok_or_else(|| ConnectorError::Protocol {
                message: "telegram accepted the message without a date".to_string(),
            })?;
        if id <= 0 {
            return Err(ConnectorError::Protocol {
                message: "telegram accepted the message without returning its id".to_string(),
            });
        }
        self.shared
            .remember_message(id, remote_chat_id, Some(txn_id.clone()));
        Ok(SendAck {
            txn_id,
            remote_msg_id: id.to_string(),
            sent_at,
        })
    }

    async fn edit(
        &self,
        remote_chat_id: &str,
        remote_msg_id: &str,
        body: String,
        formatted: Option<String>,
    ) -> Result<(), ConnectorError> {
        let mut inner = self.inner.lock().await;
        self.require_login()?;
        let client = self.ensure_pool(&mut inner).await?;
        let peer = self.resolve_peer(remote_chat_id).await?;
        let id = mapping::parse_msg_id(remote_msg_id)?;

        let mut input = InputMessage::new().text(body);
        if let Some(html) = formatted {
            let (_, entities) = grammers_client::parsers::parse_html_message(&html);
            if !entities.is_empty() {
                input = input.fmt_entities(entities);
            }
        }
        client
            .edit_message(peer, id, input)
            .await
            .map_err(error::map_invocation)
    }

    async fn delete(
        &self,
        remote_chat_id: &str,
        remote_msg_id: &str,
        for_everyone: bool,
    ) -> Result<(), ConnectorError> {
        let mut inner = self.inner.lock().await;
        self.require_login()?;
        let client = self.ensure_pool(&mut inner).await?;
        let peer = self.resolve_peer(remote_chat_id).await?;
        let id = mapping::parse_msg_id(remote_msg_id)?;

        if !for_everyone {
            if peer.id.kind() == PeerKind::Channel {
                // Telegram has no "delete for me only" for channel messages: the method removes the
                // message for every participant.
                return Err(ConnectorError::NotSupported {
                    feature: "delete_for_self".to_string(),
                });
            }
            client
                .invoke(&tl::functions::messages::DeleteMessages {
                    revoke: false,
                    id: vec![id],
                })
                .await
                .map_err(error::map_invocation)?;
            return Ok(());
        }
        client
            .delete_messages(peer, &[id])
            .await
            .map_err(error::map_invocation)?;
        Ok(())
    }

    async fn react(
        &self,
        remote_chat_id: &str,
        remote_msg_id: &str,
        emoji: &str,
    ) -> Result<(), ConnectorError> {
        let mut inner = self.inner.lock().await;
        self.require_login()?;
        let client = self.ensure_pool(&mut inner).await?;
        let peer = self.resolve_peer(remote_chat_id).await?;
        let id = mapping::parse_msg_id(remote_msg_id)?;

        // Sending a reaction replaces this account's previous one, which is exactly the single
        // reaction per message the capability promises.
        client
            .send_reactions(peer, id, emoji)
            .await
            .map_err(error::map_invocation)
    }

    async fn unreact(
        &self,
        remote_chat_id: &str,
        remote_msg_id: &str,
        _emoji: &str,
    ) -> Result<(), ConnectorError> {
        let mut inner = self.inner.lock().await;
        self.require_login()?;
        let client = self.ensure_pool(&mut inner).await?;
        let peer = self.resolve_peer(remote_chat_id).await?;
        let id = mapping::parse_msg_id(remote_msg_id)?;

        // The remaining set after removing one of one is empty.
        client
            .send_reactions(peer, id, InputReactions::remove())
            .await
            .map_err(error::map_invocation)
    }

    async fn mark_read(
        &self,
        remote_chat_id: &str,
        up_to_remote_msg_id: &str,
    ) -> Result<(), ConnectorError> {
        let mut inner = self.inner.lock().await;
        self.require_login()?;
        let client = self.ensure_pool(&mut inner).await?;
        let peer = self.resolve_peer(remote_chat_id).await?;
        let id = mapping::parse_msg_id(up_to_remote_msg_id)?;

        if peer.id.kind() == PeerKind::Channel {
            client
                .invoke(&tl::functions::channels::ReadHistory {
                    channel: peer.into(),
                    max_id: id,
                })
                .await
                .map_err(error::map_invocation)?;
        } else {
            client
                .invoke(&tl::functions::messages::ReadHistory {
                    peer: peer.into(),
                    max_id: id,
                })
                .await
                .map_err(error::map_invocation)?;
        }
        Ok(())
    }

    async fn set_typing(&self, remote_chat_id: &str, typing: bool) -> Result<(), ConnectorError> {
        let mut inner = self.inner.lock().await;
        self.require_login()?;
        let client = self.ensure_pool(&mut inner).await?;
        let peer = self.resolve_peer(remote_chat_id).await?;

        let action: tl::enums::SendMessageAction = if typing {
            tl::types::SendMessageTypingAction {}.into()
        } else {
            tl::types::SendMessageCancelAction {}.into()
        };
        client
            .invoke(&tl::functions::messages::SetTyping {
                peer: peer.into(),
                top_msg_id: None,
                action,
            })
            .await
            .map_err(error::map_invocation)?;
        Ok(())
    }

    async fn fetch_media(&self, media: &MediaRef, dest_path: &Path) -> Result<u64, ConnectorError> {
        let mut inner = self.inner.lock().await;
        self.require_login()?;
        let client = self.ensure_pool(&mut inner).await?;
        let pointer = mapping::decode_pointer(&media.remote_ref)?;
        let peer = self.resolve_peer(&pointer.chat).await?;

        let mut location = Some(mapping::pointer_location(&pointer, peer)?);
        for attempt in 0..MAX_MEDIA_ATTEMPTS {
            let current = location
                .clone()
                .ok_or_else(|| ConnectorError::InvalidInput {
                    message: "the media message no longer carries a file".to_string(),
                })?;
            match Self::download_once(&client, &current, dest_path).await {
                Ok(written) => return Ok(written),
                Err(DownloadFailure::Io(error)) => {
                    return Err(ConnectorError::InvalidInput {
                        message: format!("cannot write {}: {error}", dest_path.display()),
                    });
                }
                Err(DownloadFailure::Rpc(failure))
                    if failure.is("FILE_REFERENCE_EXPIRED") && attempt + 1 < MAX_MEDIA_ATTEMPTS =>
                {
                    // Telegram rotated the file reference; re-read the message it came from.
                    location = self.refreshed_location(&client, &pointer, peer).await?;
                }
                Err(DownloadFailure::Rpc(failure)) => return Err(error::map_invocation(failure)),
            }
        }

        Err(ConnectorError::InvalidInput {
            message: "the media reference expired twice".to_string(),
        })
    }

    async fn logout(&self) -> Result<(), ConnectorError> {
        let mut inner = self.inner.lock().await;
        if self.shared.logged_in()
            && let Some(client) = &inner.client
        {
            // Best effort: a failed revocation must not keep local credentials alive.
            if let Err(failure) = client.sign_out().await {
                warn!(error = %failure, "telegram logout request failed; wiping locally anyway");
            }
        }
        drop_pool(&mut inner);
        inner.login = None;
        session::wipe(&self.shared.session).await?;
        self.shared.wipe();
        Ok(())
    }
}

impl Shared {
    /// Sends one event to the host, if it is listening.
    pub(crate) async fn emit(&self, event: ConnectorEvent) {
        let sink = lock(&self.sink).clone();
        if let Some(sink) = sink
            && let Err(error) = sink.send(event).await
        {
            debug!(error = %error, "telegram event sink is closed");
        }
    }

    /// Builds the current session blob and reports it as [`ConnectorEvent::SessionUpdated`].
    ///
    /// Does nothing while no host is listening: the blob is only useful to a listener, and building
    /// it walks the whole peer cache.
    pub(crate) async fn emit_session(&self) -> Result<(), ConnectorError> {
        if lock(&self.sink).is_none() {
            return Ok(());
        }
        let blob = self.blob().await?;
        self.emit(ConnectorEvent::SessionUpdated { session: blob })
            .await;
        Ok(())
    }

    /// The current session blob, as the host must persist it.
    async fn blob(&self) -> Result<SessionBlob, ConnectorError> {
        let peers = self.peers_snapshot();
        let myself = self.self_profile();
        let dc_ids = lock(&self.dc_ids).clone();
        let envelope = session::snapshot(&self.session, &peers, myself.as_ref(), &dc_ids).await?;
        Ok(SessionBlob::new(session::encode(&envelope)?))
    }

    /// Whether the session is authorized.
    fn logged_in(&self) -> bool {
        self.logged_in.load(Ordering::SeqCst)
    }

    fn set_logged_in(&self, logged_in: bool) {
        self.logged_in.store(logged_in, Ordering::SeqCst);
    }

    /// This account’s Telegram user id.
    pub(crate) fn self_bare_id(&self) -> Option<i64> {
        self.self_profile().map(|profile| profile.bare_id)
    }

    /// The account this connector is logged in as.
    pub(crate) fn self_profile(&self) -> Option<SelfProfile> {
        self.myself
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    fn set_self(&self, profile: SelfProfile) {
        *self
            .myself
            .write()
            .unwrap_or_else(|error| error.into_inner()) = Some(profile);
    }

    /// Connector id → resolvable peer, for the session blob.
    fn peers_snapshot(&self) -> PeerCache {
        self.peers
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    /// The resolvable reference cached for a remote id.
    fn peer_ref(&self, remote_chat_id: &str) -> Option<PeerRef> {
        let peers = self.peers.read().unwrap_or_else(|error| error.into_inner());
        let cached = peers.get(remote_chat_id)?;
        mapping::peer_ref(remote_chat_id, cached.auth)
    }

    /// Caches a resolvable peer.
    pub(crate) fn insert_peer(&self, kind: PeerKindTag, bare_id: i64, auth: i64) {
        self.peers
            .write()
            .unwrap_or_else(|error| error.into_inner())
            .insert(
                mapping::encode_peer(kind, bare_id),
                session::CachedPeer {
                    bare_id,
                    kind,
                    auth,
                },
            );
    }

    /// Caches a peer grammers handed us, keeping its access hash.
    pub(crate) fn remember_peer(&self, reference: PeerRef) {
        if let Some((kind, bare_id)) = mapping::tag_of(reference) {
            self.insert_peer(kind, bare_id, reference.auth.hash());
        }
    }

    /// Caches a peer from a dialog response.
    pub(crate) fn remember_entry(&self, entry: &PeerEntry) {
        self.insert_peer(entry.kind, entry.bare_id, entry.auth);
    }

    /// Remembers where a locally sent or observed message lives.
    pub(crate) fn remember_message(&self, id: i32, chat: String, txn_id: Option<String>) {
        lock(&self.index).insert(id, chat, txn_id);
    }

    /// The transaction id of a locally sent message, while it is remembered.
    pub(crate) fn txn_of(&self, id: i32) -> Option<String> {
        lock(&self.index).txn_id(id)
    }

    /// The chat a remembered message belongs to.
    pub(crate) fn chat_of_message(&self, id: i32) -> Option<String> {
        lock(&self.index).chat(id)
    }

    /// Forgets a message that will never need attributing again.
    pub(crate) fn forget_message(&self, id: i32) {
        lock(&self.index).remove(id);
    }

    /// Records a datacenter so the next blob carries its address.
    fn note_dc(&self, dc_id: i32) {
        lock(&self.dc_ids).insert(dc_id);
    }

    /// Whether the session checkpoint is due.
    pub(crate) fn checkpoint_due(&self) -> bool {
        let checkpoint = lock(&self.checkpoint);
        checkpoint
            .as_ref()
            .is_none_or(|last| last.elapsed() >= CHECKPOINT_INTERVAL)
    }

    /// Records that a checkpoint was just written.
    pub(crate) fn mark_checkpoint(&self) {
        *lock(&self.checkpoint) = Some(Instant::now());
    }

    /// Applies the state of a restored blob.
    fn load(&self, envelope: &Envelope) {
        *self
            .peers
            .write()
            .unwrap_or_else(|error| error.into_inner()) = envelope.peers.clone();
        *self
            .myself
            .write()
            .unwrap_or_else(|error| error.into_inner()) = envelope.myself.clone();
        let mut dc_ids = lock(&self.dc_ids);
        dc_ids.clear();
        dc_ids.extend(1..=5);
        dc_ids.extend(session::datacenters(envelope));
        *lock(&self.checkpoint) = None;
        self.set_logged_in(false);
    }

    /// Forgets everything the logged-out account left behind.
    fn wipe(&self) {
        self.peers
            .write()
            .unwrap_or_else(|error| error.into_inner())
            .clear();
        *self
            .myself
            .write()
            .unwrap_or_else(|error| error.into_inner()) = None;
        *lock(&self.sink) = None;
        *lock(&self.index) = MessageIndex::new(MESSAGE_INDEX_CAPACITY);
        let mut dc_ids = lock(&self.dc_ids);
        dc_ids.clear();
        dc_ids.extend(1..=5);
        *lock(&self.checkpoint) = None;
        self.set_logged_in(false);
    }
}

/// Closes the pool and stops reporting updates.
fn drop_pool(inner: &mut Inner) {
    if let Some(runtime) = inner.pool.take() {
        drop(runtime);
    }
    inner.client = None;
}

/// Telegram’s `303` migration code carries the datacenter to move to.
fn migration_dc(error: &InvocationError) -> Option<i32> {
    match error {
        InvocationError::Rpc(rpc) if rpc.code == 303 => {
            rpc.value.and_then(|value| i32::try_from(value).ok())
        }
        _ => None,
    }
}

fn connection_params(config: &TelegramConfig) -> ConnectionParams {
    ConnectionParams {
        device_model: config.device_model.clone(),
        system_version: config.system_version.clone(),
        app_version: config.app_version.clone(),
        lang_code: config.lang_code.clone(),
        // Surface flood waits to the host as `RateLimited` instead of sleeping inside grammers;
        // the host owns retry policy.
        retry_policy: Box::new(NoRetries),
        ..ConnectionParams::default()
    }
}

/// Builds the duration of an Ogg file from its tail, if the file can be read.
async fn ogg_duration(path: &Path) -> Option<Duration> {
    use tokio::io::{AsyncReadExt as _, AsyncSeekExt as _};

    const TAIL: u64 = 64 * 1024;

    let mut file = tokio::fs::File::open(path).await.ok()?;
    let length = file.metadata().await.ok()?.len();
    file.seek(std::io::SeekFrom::Start(length.saturating_sub(TAIL)))
        .await
        .ok()?;
    let mut tail = Vec::new();
    file.read_to_end(&mut tail).await.ok()?;
    mapping::ogg_duration(&tail)
}

/// Maps a data-call error into a login failure.
fn login_failure(error: &ConnectorError) -> LoginFailure {
    match error {
        ConnectorError::RateLimited { retry_after_ms } => LoginFailure::RateLimited {
            retry_after_ms: *retry_after_ms,
        },
        ConnectorError::InvalidInput { message } => LoginFailure::InvalidInput {
            field: None,
            message: message.clone(),
        },
        ConnectorError::NotSupported { feature } => LoginFailure::InvalidInput {
            field: None,
            message: format!("Telegram does not support {feature} here"),
        },
        ConnectorError::Protocol { message } | ConnectorError::Network { message } => {
            LoginFailure::Network {
                message: message.clone(),
            }
        }
        ConnectorError::Unauthorized => LoginFailure::Network {
            message: "the stored session is no longer valid".to_string(),
        },
    }
}

impl Debug for TelegramConnector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TelegramConnector")
            .field("api_id", &self.shared.config.api_id)
            .field("logged_in", &self.shared.logged_in())
            .finish_non_exhaustive()
    }
}
