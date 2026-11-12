//! Request/response plumbing, event forwarding, and process supervision.
//!
//! One [`Runtime`] owns the current child process. Every request gets a host-chosen id and a
//! oneshot channel; the reader task completes the channel when the matching response arrives, so
//! the writer never blocks on a response and events can interleave freely. When the pipe breaks or
//! the child exits, the pending requests fail, the host reports `Offline`, and a supervised
//! restart replays the session before live updates resume.

use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

use alloy_connector::{
    Capabilities, ConnectionState, ConnectorError, ConnectorEvent, Frame, Network, SessionBlob,
    decode_frame, encode_frame,
};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tokio::io::{AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};
use tokio::sync::{Mutex, mpsc::Sender, oneshot};
use tokio::time::{sleep, timeout};

use crate::config::SidecarConfig;
use crate::process::{self, Launched, LineRead, MAX_LINE_BYTES};

/// Events the sidecar emits before the host calls `start` are kept for the first `start` call.
const MAX_QUEUED_EVENTS: usize = 1024;

/// Restart attempts are counted in this rolling window, so a stable process never runs out of
/// restarts while a crash loop exhausts them quickly.
const RESTART_WINDOW: Duration = Duration::from_secs(10 * 60);

/// How long `stop` waits for the child to exit after stdin closes.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(5);

/// How long the host waits for an exit status before killing a process whose pipes already closed.
const EXIT_STATUS_TIMEOUT: Duration = Duration::from_secs(2);

/// Reported once automatic restarts are exhausted.
const CRASHED_REPEATEDLY: &str = "sidecar crashed repeatedly";

/// What the sidecar announces about itself in `hello`. Immutable for the life of the connector:
/// protocol version and network must stay the same, and a restart announcing anything else is a
/// protocol violation.
pub(crate) struct Identity {
    pub(crate) network: Network,
    pub(crate) capabilities: Capabilities,
}

/// A request waiting for its response.
type PendingResponse = oneshot::Sender<Result<Value, ConnectorError>>;

/// Why the transport stopped.
enum Failure {
    /// The pipe closed or the process exited.
    Exited,
    /// The sidecar violated the framing or method contract.
    Protocol(String),
    /// Reading from or writing to the pipe failed.
    Transport(String),
}

/// The host side of one sidecar process.
pub(crate) struct Runtime {
    pub(crate) config: SidecarConfig,
    pub(crate) identity: Identity,
    state: Mutex<State>,
}

struct State {
    /// Bumped whenever a process is installed or torn down, so callbacks from an old process are
    /// ignored instead of failing the new one's requests.
    generation: u64,
    child: Option<Child>,
    writer: Option<Arc<Mutex<ChildStdin>>>,
    pending: HashMap<u64, PendingResponse>,
    next_id: u64,
    event_sender: Option<Sender<ConnectorEvent>>,
    queued_events: VecDeque<ConnectorEvent>,
    started: bool,
    auto_restart: bool,
    /// The newest session blob seen from `restore`, a completed login, or `session_updated`.
    session: Option<SessionBlob>,
    restarts: VecDeque<Instant>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            generation: 0,
            child: None,
            writer: None,
            pending: HashMap::new(),
            next_id: 0,
            event_sender: None,
            queued_events: VecDeque::new(),
            started: false,
            auto_restart: true,
            session: None,
            restarts: VecDeque::new(),
        }
    }
}

impl Runtime {
    /// Launches the sidecar and performs the handshake.
    pub(crate) async fn spawn(config: SidecarConfig) -> Result<Arc<Self>, ConnectorError> {
        let launched = process::launch(&config, None).await?;
        let identity = Identity {
            network: launched.network,
            capabilities: launched.capabilities.clone(),
        };
        let runtime = Arc::new(Self {
            config,
            identity,
            state: Mutex::new(State::default()),
        });
        runtime.install(launched).await;
        Ok(runtime)
    }

    /// Starts a fresh process and makes it the current transport.
    async fn bring_up(self: &Arc<Self>) -> Result<(), ConnectorError> {
        let launched = process::launch(&self.config, Some(self.identity.network)).await?;
        self.install(launched).await;
        Ok(())
    }

    /// Stores the process and starts its reader and stderr tasks.
    async fn install(self: &Arc<Self>, launched: Launched) {
        let Launched {
            child,
            stdin,
            stdout,
            stderr,
            ..
        } = launched;

        let generation = {
            let mut state = self.state.lock().await;
            state.generation += 1;
            state.child = Some(child);
            state.writer = Some(Arc::new(Mutex::new(stdin)));
            state.generation
        };

        process::forward_stderr(stderr);

        let runtime = Arc::clone(self);
        tokio::spawn(async move {
            runtime.read_loop(generation, stdout).await;
        });
    }

    /// Reads frames until the pipe closes, then hands control to the supervisor.
    async fn read_loop(self: Arc<Self>, generation: u64, mut stdout: BufReader<ChildStdout>) {
        loop {
            let bytes = match process::read_line_limited(&mut stdout, MAX_LINE_BYTES).await {
                Ok(LineRead::Line(bytes)) => bytes,
                Ok(LineRead::TooLong) => {
                    self.disconnect(
                        generation,
                        Failure::Protocol(format!(
                            "the sidecar sent a frame longer than {limit} bytes",
                            limit = MAX_LINE_BYTES
                        )),
                    )
                    .await;
                    return;
                }
                Ok(LineRead::Eof) => {
                    self.disconnect(generation, Failure::Exited).await;
                    return;
                }
                Err(error) => {
                    self.disconnect(
                        generation,
                        Failure::Transport(format!("cannot read from the sidecar: {error}")),
                    )
                    .await;
                    return;
                }
            };

            if !self.is_current(generation).await {
                return;
            }

            let text = match std::str::from_utf8(&bytes) {
                Ok(text) => text,
                Err(_) => {
                    self.disconnect(
                        generation,
                        Failure::Protocol("the sidecar sent a frame that is not UTF-8".to_string()),
                    )
                    .await;
                    return;
                }
            };

            let frame = match decode_frame(text) {
                Ok(frame) => frame,
                Err(error) => {
                    self.disconnect(generation, Failure::Protocol(error.to_string()))
                        .await;
                    return;
                }
            };

            if let Err(reason) = self.handle_frame(frame).await {
                self.disconnect(generation, Failure::Protocol(reason)).await;
                return;
            }
        }
    }

    /// Applies one frame. `Err` means the peer broke the protocol and the process must be replaced.
    async fn handle_frame(&self, frame: Frame) -> Result<(), String> {
        match frame {
            Frame::Hello { .. } => Err("the sidecar sent `hello` twice".to_string()),
            Frame::Request { .. } => Err("the sidecar sent a request to the host".to_string()),
            Frame::Response { id, result, error } => {
                if result.is_some() && error.is_some() {
                    return Err(format!("response {id} carries both a result and an error"));
                }
                let pending = self.state.lock().await.pending.remove(&id);
                match pending {
                    Some(sender) => {
                        let outcome = match error {
                            Some(error) => Err(error),
                            None => Ok(result.unwrap_or(Value::Null)),
                        };
                        let _ = sender.send(outcome);
                    }
                    None => {
                        tracing::debug!(
                            target: "alloy_sidecar",
                            "ignoring a response for unknown request {id}"
                        );
                    }
                }
                Ok(())
            }
            Frame::Event { event } => {
                if let ConnectorEvent::SessionUpdated { session } = &event {
                    self.state.lock().await.session = Some(session.clone());
                }
                self.deliver(event).await;
                Ok(())
            }
        }
    }

    /// Fails the pending calls, reports the transport as offline, and restarts when allowed.
    async fn disconnect(self: &Arc<Self>, generation: u64, failure: Failure) {
        let (child, writer, pending, auto_restart) = {
            let mut state = self.state.lock().await;
            if state.generation != generation {
                return;
            }
            state.generation += 1;
            (
                state.child.take(),
                state.writer.take(),
                std::mem::take(&mut state.pending),
                state.auto_restart,
            )
        };
        drop(writer);

        let status = describe_exit(child).await;
        let (reason, error) = match failure {
            Failure::Exited => {
                let message = format!("sidecar exited: {status}");
                (message.clone(), ConnectorError::Network { message })
            }
            Failure::Protocol(message) => (message.clone(), ConnectorError::Protocol { message }),
            Failure::Transport(message) => (message.clone(), ConnectorError::Network { message }),
        };

        tracing::warn!(target: "alloy_sidecar", "sidecar transport closed: {reason}");

        for (_, sender) in pending {
            let _ = sender.send(Err(error.clone()));
        }

        self.deliver(ConnectorEvent::Connection {
            state: ConnectionState::Offline { reason },
        })
        .await;

        if auto_restart {
            let runtime = Arc::clone(self);
            tokio::spawn(runtime.restart_loop());
        }
    }

    /// Respawns the sidecar with exponential backoff while the restart budget allows it.
    ///
    /// The future is boxed deliberately: `install` → `read_loop` → `disconnect` → `restart_loop`
    /// → `bring_up` → `install` is a cycle in the async fn types, so proving the spawned futures
    /// `Send` would recurse forever. A boxed `dyn Future + Send` ends the auto-trait computation
    /// at this edge.
    fn restart_loop(self: Arc<Self>) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        Box::pin(async move {
            let mut attempt: u32 = 0;
            loop {
                if !self.restart_allowed().await {
                    return;
                }
                if !self.claim_restart().await {
                    self.deliver(ConnectorEvent::Connection {
                        state: ConnectionState::Offline {
                            reason: CRASHED_REPEATEDLY.to_string(),
                        },
                    })
                    .await;
                    return;
                }

                attempt += 1;
                sleep(backoff(attempt)).await;

                if !self.restart_allowed().await {
                    return;
                }

                match self.bring_up().await {
                    Ok(()) => {
                        if let Err(error) = self.replay().await {
                            tracing::warn!(
                                target: "alloy_sidecar",
                                "the sidecar restarted but the session could not be resumed: {error}"
                            );
                        }
                        return;
                    }
                    Err(error) => {
                        tracing::warn!(
                            target: "alloy_sidecar",
                            "cannot restart the sidecar: {error}"
                        );
                    }
                }
            }
        })
    }

    /// Re-establishes the session and live updates on a freshly launched process.
    async fn replay(&self) -> Result<(), ConnectorError> {
        let (started, session) = {
            let state = self.state.lock().await;
            (state.started, state.session.clone())
        };
        if !started {
            return Ok(());
        }
        let Some(session) = session else {
            return Ok(());
        };

        self.request("restore", json!({ "session": session }))
            .await?;
        self.request("start", json!({})).await?;
        self.deliver(ConnectorEvent::Connection {
            state: ConnectionState::Online,
        })
        .await;
        Ok(())
    }

    /// Starts live updates: remembers the sender, flushes pre-start events, then starts the
    /// sidecar.
    pub(crate) async fn start(
        self: &Arc<Self>,
        events: Sender<ConnectorEvent>,
    ) -> Result<(), ConnectorError> {
        let (running, session) = {
            let state = self.state.lock().await;
            (state.writer.is_some(), state.session.clone())
        };
        if !running {
            self.bring_up().await?;
            if let Some(session) = session {
                self.request("restore", json!({ "session": session }))
                    .await?;
            }
        }

        let queued = {
            let mut state = self.state.lock().await;
            state.event_sender = Some(events.clone());
            state.started = true;
            state.auto_restart = true;
            std::mem::take(&mut state.queued_events)
        };
        for event in queued {
            if events.send(event).await.is_err() {
                break;
            }
        }

        if let Err(error) = self.request("start", json!({})).await {
            let mut state = self.state.lock().await;
            state.started = false;
            state.event_sender = None;
            return Err(error);
        }
        Ok(())
    }

    /// Stops live updates and terminates the child process.
    pub(crate) async fn stop(&self) -> Result<(), ConnectorError> {
        let has_transport = {
            let mut state = self.state.lock().await;
            state.started = false;
            state.auto_restart = false;
            state.event_sender = None;
            state.writer.is_some()
        };

        if has_transport && let Err(error) = self.request("stop", json!({})).await {
            tracing::debug!(
                target: "alloy_sidecar",
                "the sidecar did not acknowledge `stop`: {error}"
            );
        }

        let (child, writer, pending) = {
            let mut state = self.state.lock().await;
            state.generation += 1;
            (
                state.child.take(),
                state.writer.take(),
                std::mem::take(&mut state.pending),
            )
        };
        for (_, sender) in pending {
            let _ = sender.send(Err(ConnectorError::Network {
                message: "the sidecar was stopped".to_string(),
            }));
        }
        drop(writer);

        let Some(mut child) = child else {
            return Ok(());
        };
        match timeout(SHUTDOWN_GRACE, child.wait()).await {
            Ok(Ok(_status)) => Ok(()),
            Ok(Err(error)) => Err(ConnectorError::Network {
                message: format!("cannot wait for the sidecar to exit: {error}"),
            }),
            Err(_) => child.kill().await.map_err(|error| ConnectorError::Network {
                message: format!("cannot terminate the sidecar: {error}"),
            }),
        }
    }

    /// Sends one request and waits for its response.
    async fn request(&self, method: &str, params: Value) -> Result<Value, ConnectorError> {
        let (id, writer, receiver) = {
            let mut state = self.state.lock().await;
            let writer = state
                .writer
                .clone()
                .ok_or_else(|| ConnectorError::Network {
                    message: "the sidecar is not running; call start to relaunch it".to_string(),
                })?;
            let id = state.next_id;
            state.next_id += 1;
            let (sender, receiver) = oneshot::channel();
            state.pending.insert(id, sender);
            (id, writer, receiver)
        };

        let frame = Frame::Request {
            id,
            method: method.to_string(),
            params,
        };
        if let Err(error) = write_frame(&writer, &frame).await {
            self.state.lock().await.pending.remove(&id);
            return Err(ConnectorError::Network {
                message: format!("cannot write to the sidecar: {error}"),
            });
        }

        match timeout(self.config.request_timeout, receiver).await {
            Ok(response) => response.unwrap_or_else(|_| {
                Err(ConnectorError::Network {
                    message: format!("the sidecar dropped the response to `{method}`"),
                })
            }),
            Err(_) => {
                self.state.lock().await.pending.remove(&id);
                Err(ConnectorError::Network {
                    message: format!(
                        "the sidecar did not answer `{method}` within {:?}",
                        self.config.request_timeout
                    ),
                })
            }
        }
    }

    /// Sends a request and decodes its result.
    pub(crate) async fn call<T: DeserializeOwned>(
        &self,
        method: &str,
        params: Value,
    ) -> Result<T, ConnectorError> {
        let value = self.request(method, params).await?;
        serde_json::from_value(value).map_err(|error| ConnectorError::Protocol {
            message: format!("the sidecar returned an invalid `{method}` result: {error}"),
        })
    }

    /// Sends a request whose method returns nothing.
    pub(crate) async fn call_unit(
        &self,
        method: &str,
        params: Value,
    ) -> Result<(), ConnectorError> {
        match self.request(method, params).await? {
            Value::Null => Ok(()),
            other => Err(ConnectorError::Protocol {
                message: format!("`{method}` returned a value where none was expected: {other}"),
            }),
        }
    }

    /// Remembers the newest session blob so a restart can restore it.
    pub(crate) async fn remember_session(&self, session: SessionBlob) {
        self.state.lock().await.session = Some(session);
    }

    /// Hands one event to `start`'s sender, or queues it until `start` is called.
    async fn deliver(&self, event: ConnectorEvent) {
        let sender = {
            let mut state = self.state.lock().await;
            match state.event_sender.clone() {
                Some(sender) => sender,
                None => {
                    if state.queued_events.len() >= MAX_QUEUED_EVENTS {
                        state.queued_events.pop_front();
                        tracing::warn!(
                            target: "alloy_sidecar",
                            "dropping the oldest queued connector event: {limit} buffered",
                            limit = MAX_QUEUED_EVENTS
                        );
                    }
                    state.queued_events.push_back(event);
                    return;
                }
            }
        };

        if sender.send(event).await.is_err() {
            let mut state = self.state.lock().await;
            if state
                .event_sender
                .as_ref()
                .is_some_and(|current| current.same_channel(&sender))
            {
                state.event_sender = None;
            }
        }
    }

    async fn is_current(&self, generation: u64) -> bool {
        self.state.lock().await.generation == generation
    }

    async fn restart_allowed(&self) -> bool {
        self.state.lock().await.auto_restart
    }

    /// Counts one restart against the rolling budget; `false` means the sidecar must stay down.
    async fn claim_restart(&self) -> bool {
        let mut state = self.state.lock().await;
        let now = Instant::now();
        while let Some(first) = state.restarts.front() {
            if now.duration_since(*first) < RESTART_WINDOW {
                break;
            }
            state.restarts.pop_front();
        }
        if state.restarts.len() >= self.config.max_restarts as usize {
            return false;
        }
        state.restarts.push_back(now);
        true
    }
}

/// Writes one frame, serialised with every other writer through the stdin mutex.
async fn write_frame(writer: &Mutex<ChildStdin>, frame: &Frame) -> std::io::Result<()> {
    let line = encode_frame(frame);
    let mut writer = writer.lock().await;
    writer.write_all(line.as_bytes()).await?;
    writer.flush().await
}

/// Backoff before restart attempt `attempt`: 1 s, 2 s, 4 s, ... capped at 60 s.
fn backoff(attempt: u32) -> Duration {
    let exponent = attempt.saturating_sub(1).min(6);
    Duration::from_secs((1_u64 << exponent).min(60))
}

/// Waits for the child's exit status, killing it when the pipe closed but the process lingers.
async fn describe_exit(child: Option<Child>) -> String {
    let Some(mut child) = child else {
        return "no process handle".to_string();
    };
    match timeout(EXIT_STATUS_TIMEOUT, child.wait()).await {
        Ok(Ok(status)) => status.to_string(),
        Ok(Err(error)) => format!("wait failed: {error}"),
        Err(_) => {
            let _ = child.kill().await;
            "killed after the pipe closed".to_string()
        }
    }
}
