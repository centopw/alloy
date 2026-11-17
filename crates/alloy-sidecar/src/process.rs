//! Spawning the sidecar, the `hello` handshake, and reading frames without unbounded buffering.

use std::io;
use std::pin::Pin;
use std::process::Stdio;

use alloy_connector::{
    Capabilities, ConnectorError, Frame, Network, PROTOCOL_VERSION, decode_frame,
};
use serde_json::Value;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, BufReader};
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command};
use tokio::time::timeout;

use crate::config::SidecarConfig;

/// Environment variable that tells the sidecar where it may keep credentials and media.
pub(crate) const DATA_DIR_ENV: &str = "ALLOY_SIDECAR_DATA_DIR";

/// Largest accepted frame. A longer line means the two ends disagree about the protocol, so the
/// host drops the process instead of buffering unbounded input.
pub(crate) const MAX_LINE_BYTES: usize = 16 * 1024 * 1024;

/// Largest part of an offending line echoed in a diagnostic.
const ECHO_BYTES: usize = 200;

/// One `ConnectorError::Protocol` with a formatted message.
pub(crate) fn protocol_error(message: impl Into<String>) -> ConnectorError {
    ConnectorError::Protocol {
        message: message.into(),
    }
}

/// Renders bytes for a diagnostic: escaped, so a payload cannot forge log lines or feed control
/// sequences into the terminal, and truncated, so one oversized frame cannot flood the log.
pub(crate) fn describe_line(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let mut end = text.len().min(ECHO_BYTES);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let escaped = text[..end].escape_debug().to_string();
    if end < text.len() {
        format!("{escaped}...")
    } else {
        escaped
    }
}

/// A sidecar process that has completed the handshake and not been installed yet.
pub(crate) struct Launched {
    pub(crate) child: Child,
    pub(crate) stdin: ChildStdin,
    pub(crate) stdout: BufReader<ChildStdout>,
    pub(crate) stderr: ChildStderr,
    pub(crate) network: Network,
    pub(crate) capabilities: Capabilities,
}

/// One read from a byte stream, bounded by a maximum line length.
pub(crate) enum LineRead {
    /// A complete line, without its terminating `\n`.
    Line(Vec<u8>),
    /// The peer sent a line longer than the allowed maximum.
    TooLong,
    /// The stream ended before any byte of the next line arrived.
    Eof,
}

/// Starts the executable, waits for `hello`, and refuses a process that does not speak this host's
/// protocol version.
///
/// `expected_network` is `None` for the first launch, where the handshake is what identifies the
/// connector, and `Some` for restarts, where a sidecar announcing a different network is a
/// protocol violation.
pub(crate) async fn launch(
    config: &SidecarConfig,
    expected_network: Option<Network>,
) -> Result<Launched, ConnectorError> {
    std::fs::create_dir_all(&config.data_dir).map_err(|error| ConnectorError::InvalidInput {
        message: format!(
            "cannot create the sidecar data directory {}: {error}",
            config.data_dir.display()
        ),
    })?;

    let mut command = Command::new(&config.program);
    command
        .args(&config.args)
        .envs(
            config
                .env
                .iter()
                .map(|(key, value)| (key.as_str(), value.as_str())),
        )
        .env(DATA_DIR_ENV, &config.data_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let mut child = command.spawn().map_err(|error| ConnectorError::Network {
        message: format!(
            "cannot start the sidecar `{}`: {error}",
            config.program.display()
        ),
    })?;

    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| protocol_error("the sidecar stdin pipe is unavailable"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| protocol_error("the sidecar stdout pipe is unavailable"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| protocol_error("the sidecar stderr pipe is unavailable"))?;

    let mut stdout = BufReader::new(stdout);
    let frame = match timeout(config.hello_timeout, read_hello(&mut stdout)).await {
        Ok(result) => result?,
        Err(_) => {
            return Err(ConnectorError::Network {
                message: format!(
                    "the sidecar did not send `hello` within {:?}",
                    config.hello_timeout
                ),
            });
        }
    };

    let Frame::Hello {
        protocol_version,
        network,
        capabilities,
    } = frame
    else {
        return Err(protocol_error(
            "the sidecar must send `hello` before any other frame",
        ));
    };

    if protocol_version != PROTOCOL_VERSION {
        return Err(protocol_error(format!(
            "protocol version {protocol_version} does not match host {host}",
            host = PROTOCOL_VERSION
        )));
    }
    if let Some(expected) = expected_network.filter(|expected| *expected != network) {
        return Err(protocol_error(format!(
            "the sidecar speaks `{network}`, but this connector expects `{expected}`"
        )));
    }

    Ok(Launched {
        child,
        stdin,
        stdout,
        stderr,
        network,
        capabilities,
    })
}

/// Reads and decodes the first frame the sidecar sends.
async fn read_hello(reader: &mut BufReader<ChildStdout>) -> Result<Frame, ConnectorError> {
    match read_line_limited(reader, MAX_LINE_BYTES).await {
        Ok(LineRead::Line(bytes)) => {
            let text = std::str::from_utf8(&bytes)
                .map_err(|_| protocol_error("the sidecar `hello` frame is not valid UTF-8"))?;
            decode_frame(text).map_err(|error| {
                protocol_error(format!(
                    "the sidecar `hello` frame is invalid: {error}; line: {line}",
                    line = describe_line(&bytes)
                ))
            })
        }
        Ok(LineRead::TooLong) => Err(protocol_error(format!(
            "the sidecar `hello` frame exceeds {limit} bytes",
            limit = MAX_LINE_BYTES
        ))),
        Ok(LineRead::Eof) => Err(ConnectorError::Network {
            message: "the sidecar exited before sending `hello`".to_string(),
        }),
        Err(error) => Err(ConnectorError::Network {
            message: format!("cannot read the sidecar `hello` frame: {error}"),
        }),
    }
}

/// Reads one line, never buffering more than `limit` bytes.
pub(crate) async fn read_line_limited<R>(reader: &mut R, limit: usize) -> io::Result<LineRead>
where
    R: AsyncBufRead + Unpin,
{
    let mut line = Vec::new();
    loop {
        let available = reader.fill_buf().await?;
        if available.is_empty() {
            return Ok(if line.is_empty() {
                LineRead::Eof
            } else {
                LineRead::Line(line)
            });
        }

        let newline = available.iter().position(|byte| *byte == b'\n');
        let taken = newline.unwrap_or(available.len());
        line.extend_from_slice(&available[..taken]);
        if line.len() > limit {
            return Ok(LineRead::TooLong);
        }

        match newline {
            Some(position) => {
                Pin::new(&mut *reader).consume(position + 1);
                return Ok(LineRead::Line(line));
            }
            None => Pin::new(&mut *reader).consume(taken),
        }
    }
}

/// Copies the child's stderr into `tracing` so sidecar diagnostics reach the same log as the host.
///
/// The sidecar logs zerolog JSON, which carries its own severity in `level`; forwarding every
/// line at `info` would hide its warnings and bury the host's log under its trace payload dumps.
pub(crate) fn forward_stderr(stderr: ChildStderr) {
    tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        loop {
            match lines.next_line().await {
                Ok(Some(line)) => forward_stderr_line(&line),
                Ok(None) => break,
                Err(error) => {
                    tracing::warn!(
                        target: "alloy_sidecar::stderr",
                        "cannot read the sidecar's stderr: {error}"
                    );
                    break;
                }
            }
        }
    });
}

/// Forwards one sidecar diagnostic at the level the host assigns it. A line that is not a zerolog
/// object is forwarded at `info`, because the sidecar wrote it deliberately.
fn forward_stderr_line(line: &str) {
    match forwarded_level(line) {
        Some(StderrLevel::Trace) => tracing::trace!(target: "alloy_sidecar::stderr", "{line}"),
        Some(StderrLevel::Debug) => tracing::debug!(target: "alloy_sidecar::stderr", "{line}"),
        Some(StderrLevel::Info) | None => {
            tracing::info!(target: "alloy_sidecar::stderr", "{line}")
        }
        Some(StderrLevel::Warn) => tracing::warn!(target: "alloy_sidecar::stderr", "{line}"),
        Some(StderrLevel::Error) => tracing::error!(target: "alloy_sidecar::stderr", "{line}"),
    }
}

/// The severity a sidecar diagnostic announces in its leading zerolog JSON object.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StderrLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

/// JSON field the sidecar stamps on every line a bundled dependency wrote through its logger.
const DEPENDENCY_ORIGIN_FIELD: &str = "origin";

/// Field value that marks a line as dependency output rather than an Alloy report.
const DEPENDENCY_ORIGIN: &str = "dependency";

/// One sidecar diagnostic as the host will treat it, before it reaches `tracing`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct StderrDiagnostic {
    level: StderrLevel,
    dependency: bool,
}

/// The level the host forwards one diagnostic at. `None` means the line is not a zerolog object or
/// names a level this host does not know, and it is forwarded at `info`.
///
/// Classification is by the sidecar's provenance marker, never by the prose of the line: libsignal,
/// whatsmeow and messagix log their own failed decryptions, MAC mismatches and retry probes at
/// warn/error, and those are not events Alloy reports. Alloy's own reports carry no marker and keep
/// their level.
fn forwarded_level(line: &str) -> Option<StderrLevel> {
    let StderrDiagnostic { level, dependency } = classify_stderr_line(line)?;
    // Any dependency line above debug is chatter: forwarded at debug so a debug run still shows it.
    if dependency && !matches!(level, StderrLevel::Trace | StderrLevel::Debug) {
        return Some(StderrLevel::Debug);
    }
    Some(level)
}

/// Reads the level and provenance of a diagnostic line.
fn classify_stderr_line(line: &str) -> Option<StderrDiagnostic> {
    if !line.starts_with('{') {
        return None;
    }
    let object: Value = serde_json::from_str(line).ok()?;
    let dependency =
        object.get(DEPENDENCY_ORIGIN_FIELD).and_then(Value::as_str) == Some(DEPENDENCY_ORIGIN);
    let level = match object.get("level")?.as_str()? {
        "trace" => StderrLevel::Trace,
        "debug" => StderrLevel::Debug,
        "info" => StderrLevel::Info,
        "warn" => StderrLevel::Warn,
        "error" => StderrLevel::Error,
        _ => return None,
    };
    Some(StderrDiagnostic { level, dependency })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The dependency lines captured from a real failed Messenger decryption, plus the one report
    /// the sidecar emits for it. The dependency lines carry the `origin` marker the sidecar now
    /// stamps; the final line is `handleE2EEUndecryptable`'s report.
    const DECRYPT_FAILURE_FIXTURES: [&str; 4] = [
        r#"{"level":"warn","origin":"dependency","component":"libsignal","caller":"SessionCipher.go:263","message":"mismatching MAC in signal message"}"#,
        r#"{"level":"error","origin":"dependency","component":"libsignal","caller":"SessionCipher.go:319","message":"mismatching MAC in signal message"}"#,
        r#"{"level":"warn","origin":"dependency","component":"whatsmeow","message":"failed to decrypt prekey message: no valid sessions"}"#,
        r#"{"level":"warn","message_id":"3EB0C1D2","chat":"e2ee:100009129333612@msgr","sender":"100009129333612:111@msgr","unavailable":false,"decrypt_fail_mode":"","message":"Failed to decrypt a WhatsApp message; a retry receipt was sent to the sender"}"#,
    ];

    #[test]
    fn one_decrypt_failure_yields_exactly_one_warn() {
        let forwarded: Vec<_> = DECRYPT_FAILURE_FIXTURES
            .iter()
            .map(|line| forwarded_level(line).expect("fixtures are zerolog objects"))
            .collect();

        assert_eq!(
            forwarded
                .iter()
                .filter(|level| **level == StderrLevel::Warn)
                .count(),
            1,
            "the sidecar's own report is the only warn: {forwarded:?}"
        );
        assert!(
            !forwarded.contains(&StderrLevel::Error),
            "a decrypt failure answered with a retry receipt is not an error: {forwarded:?}"
        );
        assert_eq!(forwarded[0], StderrLevel::Debug, "libsignal warn is capped");
        assert_eq!(
            forwarded[1],
            StderrLevel::Debug,
            "libsignal error is capped"
        );
        assert_eq!(forwarded[2], StderrLevel::Debug, "whatsmeow warn is capped");
        assert_eq!(
            forwarded[3],
            StderrLevel::Warn,
            "Alloy's report keeps its level"
        );
    }

    #[test]
    fn dependency_provenance_is_read_from_the_marker_not_the_prose() {
        assert_eq!(
            classify_stderr_line(DECRYPT_FAILURE_FIXTURES[0]),
            Some(StderrDiagnostic {
                level: StderrLevel::Warn,
                dependency: true,
            })
        );
        assert_eq!(
            classify_stderr_line(DECRYPT_FAILURE_FIXTURES[3]),
            Some(StderrDiagnostic {
                level: StderrLevel::Warn,
                dependency: false,
            })
        );
        // A dependency line is at most debug whatever level it announced, including info.
        assert_eq!(
            forwarded_level(
                r#"{"level":"info","origin":"dependency","component":"messagix","message":"x"}"#
            ),
            Some(StderrLevel::Debug)
        );
        assert_eq!(
            forwarded_level(
                r#"{"level":"trace","origin":"dependency","component":"messagix","message":"x"}"#
            ),
            Some(StderrLevel::Trace)
        );
    }

    #[test]
    fn stderr_level_follows_the_level_field() {
        assert!(matches!(
            forwarded_level(r#"{"level":"trace","message":"x"}"#),
            Some(StderrLevel::Trace)
        ));
        assert!(matches!(
            forwarded_level(r#"{"level":"debug","message":"x"}"#),
            Some(StderrLevel::Debug)
        ));
        assert!(matches!(
            forwarded_level(r#"{"level":"error","message":"x"}"#),
            Some(StderrLevel::Error)
        ));
        assert!(
            forwarded_level("sidecar: plain text").is_none(),
            "a line without a `level` is unstructured output"
        );
        assert!(
            forwarded_level(r#"{"level":"nonsense"}"#).is_none(),
            "an unknown level is not guessed at"
        );
        assert!(forwarded_level(r#"{"message":"no level field"}"#).is_none());
    }

    #[test]
    fn describe_line_escapes_and_truncates() {
        assert_eq!(describe_line(b"first\nsecond"), r"first\nsecond");

        let long = describe_line(&[b'x'; 500]);
        assert!(long.ends_with("..."), "the echo is cut short: {long}");
        assert!(
            long.len() < 300,
            "the echoed line stays near the limit: {} bytes",
            long.len()
        );
    }
}
