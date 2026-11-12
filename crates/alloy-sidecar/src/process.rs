//! Spawning the sidecar, the `hello` handshake, and reading frames without unbounded buffering.

use std::io;
use std::pin::Pin;
use std::process::Stdio;

use alloy_connector::{
    Capabilities, ConnectorError, Frame, Network, PROTOCOL_VERSION, decode_frame,
};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, BufReader};
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command};
use tokio::time::timeout;

use crate::config::SidecarConfig;

/// Environment variable that tells the sidecar where it may keep credentials and media.
pub(crate) const DATA_DIR_ENV: &str = "ALLOY_SIDECAR_DATA_DIR";

/// Largest accepted frame. A longer line means the two ends disagree about the protocol, so the
/// host drops the process instead of buffering unbounded input.
pub(crate) const MAX_LINE_BYTES: usize = 16 * 1024 * 1024;

/// One `ConnectorError::Protocol` with a formatted message.
pub(crate) fn protocol_error(message: impl Into<String>) -> ConnectorError {
    ConnectorError::Protocol {
        message: message.into(),
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
            decode_frame(text)
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
pub(crate) fn forward_stderr(stderr: ChildStderr) {
    tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        loop {
            match lines.next_line().await {
                Ok(Some(line)) => tracing::info!(target: "alloy_sidecar::stderr", "{line}"),
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
