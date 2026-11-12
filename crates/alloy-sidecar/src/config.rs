//! How the host launches and supervises one sidecar process.

use std::path::PathBuf;
use std::time::Duration;

/// Launch parameters for a [`crate::SidecarConnector`].
///
/// The sidecar is a plain executable: the host passes the command line, extra environment
/// variables, and the data directory it may write to. Nothing about the protocol is configured
/// here; the executable announces its network and capabilities in the `hello` frame.
#[derive(Debug, Clone)]
pub struct SidecarConfig {
    /// Executable to run, either absolute or resolved through `PATH`.
    pub program: PathBuf,
    /// Arguments passed to the executable, in order.
    pub args: Vec<String>,
    /// Additional environment variables for the child, on top of the host's own.
    pub env: Vec<(String, String)>,
    /// Directory the sidecar may keep credentials and media in. Passed as
    /// `ALLOY_SIDECAR_DATA_DIR`; created if it does not exist.
    pub data_dir: PathBuf,
    /// How long one request may stay unanswered before it fails. A sidecar that needs longer than
    /// this for a legitimate call is a bug worth surfacing.
    pub request_timeout: Duration,
    /// How long the sidecar may take to send `hello` after the process starts.
    pub hello_timeout: Duration,
    /// How many automatic restarts are allowed inside a rolling ten-minute window.
    pub max_restarts: u32,
}

impl SidecarConfig {
    /// A configuration with the documented defaults: a 60 s request timeout, a 10 s handshake
    /// timeout, five restarts per ten minutes, no extra arguments and no extra environment.
    pub fn new(program: impl Into<PathBuf>, data_dir: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            env: Vec::new(),
            data_dir: data_dir.into(),
            request_timeout: Duration::from_secs(60),
            hello_timeout: Duration::from_secs(10),
            max_restarts: 5,
        }
    }
}
