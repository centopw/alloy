//! Bounded, redacted log capture for Settings → Debug.
//!
//! The app ships no tracing subscriber, so `tracing` events go nowhere. [`init`] installs one
//! global subscriber made of two layers: the usual stdout `fmt` layer for a terminal and
//! [`CaptureLayer`], which renders each event into one line, redacts it through [`redact_line`] and
//! keeps the newest [`RING_CAPACITY`] lines in memory. [`debug_logs`] hands a byte-capped slice of
//! that buffer to the settings page, [`debug_clear_logs`] empties it and [`debug_save_logs`] writes
//! it through the native save dialog.
//!
//! Redaction is applied before a line enters the buffer, so a secret never sits in memory: see
//! [`redact_line`] for the full list of categories.

use std::collections::VecDeque;
use std::fmt::Write as _;
use std::io::IsTerminal;
use std::sync::{LazyLock, Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;
use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::layer::{Context, Layer, SubscriberExt};

/// Newest lines kept in memory.
const RING_CAPACITY: usize = 2_000;
/// Longest single stored line, so one runaway event cannot fill the buffer.
const MAX_LINE_BYTES: usize = 4_096;
/// Longest `text` returned to the page, roughly 256 KiB.
const MAX_TEXT_BYTES: usize = 256 * 1024;
/// File name offered by the save dialog.
const DEFAULT_FILE_NAME: &str = "alloy-debug.log";
/// What a redacted value turns into. ASCII only: no emoji, no locale-dependent text.
const MARK: &str = "<redacted>";

/// The one process-wide log buffer. A failed subscriber install still reaches the same lines.
static BUFFER: LazyLock<LogBuffer> = LazyLock::new(LogBuffer::new);

fn global() -> &'static LogBuffer {
    &BUFFER
}

/// Installs the global tracing subscriber. Safe to call twice: the first install wins.
pub fn init() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_ansi(std::io::stdout().is_terminal())
        .with_target(false);
    let subscriber = tracing_subscriber::registry()
        .with(filter)
        .with(fmt_layer)
        .with(CaptureLayer);
    let _ = tracing::subscriber::set_global_default(subscriber);
}

/// What the settings page reads: redacted lines, oldest first.
#[derive(Debug, Clone, Serialize)]
pub struct DebugLogs {
    /// Redacted lines joined with `\n`; empty when nothing was logged yet.
    pub text: String,
    /// Number of lines in `text`.
    pub lines: usize,
    /// True when older lines were dropped to stay inside the 256 KiB response cap.
    pub truncated: bool,
}

/// Returns the captured log, newest line last.
#[tauri::command]
pub fn debug_logs() -> DebugLogs {
    global().snapshot()
}

/// Empties the captured log.
#[tauri::command]
pub fn debug_clear_logs() {
    global().clear();
}

/// Writes the captured log to a file the user picks. Resolves `false` when the dialog is cancelled.
#[tauri::command]
pub async fn debug_save_logs(app: AppHandle) -> Result<bool, String> {
    let text = global().snapshot().text;
    tauri::async_runtime::spawn_blocking(move || save_text(&app, &text))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

fn save_text(app: &AppHandle, text: &str) -> std::io::Result<bool> {
    let Some(target) = app
        .dialog()
        .file()
        .set_file_name(DEFAULT_FILE_NAME)
        .blocking_save_file()
    else {
        return Ok(false);
    };
    let dest = target
        .into_path()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e.to_string()))?;
    std::fs::write(dest, text)?;
    Ok(true)
}

/// Fixed-size ring of formatted, already-redacted log lines.
struct LogBuffer {
    lines: Mutex<VecDeque<String>>,
}

impl LogBuffer {
    fn new() -> Self {
        Self {
            lines: Mutex::new(VecDeque::with_capacity(RING_CAPACITY)),
        }
    }

    /// A poisoned lock only means another thread panicked while holding it; the deque itself is
    /// still consistent because every mutation is a single push or clear.
    fn lock(&self) -> MutexGuard<'_, VecDeque<String>> {
        self.lines
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn push(&self, line: String) {
        let mut lines = self.lock();
        if lines.len() == RING_CAPACITY {
            lines.pop_front();
        }
        lines.push_back(line);
    }

    fn clear(&self) {
        self.lock().clear();
    }

    /// Newest lines that fit in [`MAX_TEXT_BYTES`], oldest first, plus whether anything was cut.
    fn snapshot(&self) -> DebugLogs {
        let lines = self.lock();
        let mut selected: Vec<&str> = Vec::with_capacity(lines.len().min(64));
        let mut bytes = 0usize;
        for line in lines.iter().rev() {
            let extra = line.len() + 1;
            // Always return at least one line; `push` caps its length.
            if !selected.is_empty() && bytes + extra > MAX_TEXT_BYTES {
                break;
            }
            bytes += extra;
            selected.push(line.as_str());
        }
        let truncated = selected.len() < lines.len();
        selected.reverse();
        DebugLogs {
            text: selected.join("\n"),
            lines: selected.len(),
            truncated,
        }
    }
}

/// The in-memory layer: renders each event into one line and appends it to the shared buffer.
struct CaptureLayer;

impl<S: Subscriber> Layer<S> for CaptureLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let meta = event.metadata();
        let mut line = String::with_capacity(160);
        let _ = write!(
            line,
            "{} {:<5} {}",
            utc_clock(),
            meta.level(),
            meta.target()
        );
        event.record(&mut LineVisitor { out: &mut line });
        global().push(redact_line(&truncate_line(line)));
    }
}

/// Writes the event message unquoted, like `fmt` does, and every other field as ` key=value`.
struct LineVisitor<'a> {
    out: &'a mut String,
}

impl Visit for LineVisitor<'_> {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.out.push_str(": ");
            self.out.push_str(value);
        } else {
            let _ = write!(self.out, " {}={value:?}", field.name());
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            // `fmt::Arguments` Debug delegates to Display, so this is the bare message.
            self.out.push_str(": ");
            let _ = write!(self.out, "{value:?}");
        } else {
            let _ = write!(self.out, " {}={value:?}", field.name());
        }
    }
}

/// `HH:MM:SSZ` (UTC): no date dependency for a debug page.
fn utc_clock() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    let day = secs % 86_400;
    format!(
        "{:02}:{:02}:{:02}Z",
        day / 3_600,
        (day % 3_600) / 60,
        day % 60
    )
}

/// Cuts a runaway line at a UTF-8 boundary and marks the cut.
fn truncate_line(mut line: String) -> String {
    if line.len() <= MAX_LINE_BYTES {
        return line;
    }
    let mut end = MAX_LINE_BYTES;
    while end > 0 && !line.is_char_boundary(end) {
        end -= 1;
    }
    line.truncate(end);
    line.push_str(" ...");
    line
}

/// Keys whose value is a message body or chat title: everything after them is private.
const FREE_TEXT_KEYS: [&str; 4] = ["body", "text", "title", "caption"];
/// Keys whose value is a credential or session material.
const SECRET_KEYS: [&str; 17] = [
    "api_hash",
    "api-hash",
    "apihash",
    "auth_key",
    "authkey",
    "session",
    "session_id",
    "session_blob",
    "session_key",
    "session_token",
    "token",
    "access_token",
    "refresh_token",
    "secret",
    "password",
    "passwd",
    "cookie",
];
/// The only absolute home roots redacted, as `~`.
const HOME_PREFIXES: [&str; 3] = ["/users/", "/home/", "\\users\\"];

/// Scrubs one log line. Idempotent: running it twice changes nothing.
///
/// Categories, in the order applied:
/// 1. `Cookie`, `Set-Cookie` and `Authorization` headers: the value is dropped.
/// 2. `Bearer <token>`: the token is dropped.
/// 3. Secret key/value pairs (`api_hash`, `auth_key`, `session*`, `token`, `secret`, `password`,
///    `cookie`): the value is dropped.
/// 4. Free text after `body=`, `text=`, `title=` or `caption=`: message bodies and chat titles.
/// 5. Telegram `api_hash`-shaped runs (32+ hex characters mixing digits and letters).
/// 6. Email addresses.
/// 7. Phone numbers, E.164 and local: 9–15 digits, with spaces, dots, dashes or parentheses.
/// 8. Absolute home paths (`/Users/<name>`, `/home/<name>`, `C:\Users\<name>`) collapse to `~`.
pub fn redact_line(raw: &str) -> String {
    let mut line = redact_headers(raw);
    line = redact_bearer(&line);
    line = redact_key_values(&line);
    line = redact_api_hashes(&line);
    line = redact_emails(&line);
    line = redact_phones(&line);
    redact_home_paths(&line)
}

fn is_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn is_key_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-'
}

fn redact_headers(line: &str) -> String {
    let mut out = line.to_owned();
    for name in ["set-cookie", "cookie", "authorization"] {
        out = redact_header(&out, name);
        if out.ends_with(MARK) {
            // The whole value was dropped, including anything after it: nothing left to scan.
            break;
        }
    }
    out
}

/// Replaces `name: value` (any case) with `name: <redacted>` through the end of the line.
fn redact_header(line: &str, name: &str) -> String {
    let lower = line.to_ascii_lowercase();
    let mut search = 0usize;
    while let Some(pos) = lower[search..].find(name) {
        let start = search + pos;
        let after = start + name.len();
        let boundary = start == 0 || !(is_key_byte(lower.as_bytes()[start - 1]));
        let rest = &line[after..];
        let trimmed = rest.trim_start_matches([' ', '\t']);
        if boundary && trimmed.starts_with(':') {
            let colon = after + (rest.len() - trimmed.len());
            return format!("{}: {}", &line[..colon], MARK);
        }
        search = after;
    }
    line.to_owned()
}

/// Drops the token in `Bearer <token>` (any case).
fn redact_bearer(line: &str) -> String {
    let lower = line.to_ascii_lowercase();
    let mut out = String::with_capacity(line.len());
    let mut cursor = 0usize;
    while let Some(pos) = lower[cursor..].find("bearer") {
        let start = cursor + pos;
        let after = start + "bearer".len();
        let boundary = start == 0 || !is_word_byte(lower.as_bytes()[start - 1]);
        let mut value_start = after;
        while value_start < line.len() && matches!(line.as_bytes()[value_start], b' ' | b'\t') {
            value_start += 1;
        }
        let mut value_end = value_start;
        while value_end < line.len() && !line.as_bytes()[value_end].is_ascii_whitespace() {
            value_end += 1;
        }
        if boundary && value_end > value_start {
            out.push_str(&line[cursor..value_start]);
            out.push_str(MARK);
            cursor = value_end;
        } else {
            out.push_str(&line[cursor..after]);
            cursor = after;
        }
    }
    out.push_str(&line[cursor..]);
    out
}

/// Drops the value after a secret or free-text key, whatever separator was used.
fn redact_key_values(line: &str) -> String {
    let bytes = line.as_bytes();
    let mut out = String::with_capacity(line.len());
    let mut cursor = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        let separator = bytes[i];
        if separator != b'=' && separator != b':' {
            i += 1;
            continue;
        }
        let mut key_end = i;
        while key_end > cursor && matches!(bytes[key_end - 1], b' ' | b'\t') {
            key_end -= 1;
        }
        let mut key_start = key_end;
        while key_start > cursor && is_key_byte(bytes[key_start - 1]) {
            key_start -= 1;
        }
        if key_start == key_end {
            i += 1;
            continue;
        }
        let key = line[key_start..key_end].to_ascii_lowercase();
        let free_text = FREE_TEXT_KEYS.contains(&key.as_str());
        if !free_text && !SECRET_KEYS.contains(&key.as_str()) {
            i += 1;
            continue;
        }

        let mut value_start = i + 1;
        while value_start < bytes.len() && matches!(bytes[value_start], b' ' | b'\t') {
            value_start += 1;
        }
        if value_start >= bytes.len() {
            i += 1;
            continue;
        }
        let quote = match bytes[value_start] {
            b'"' => Some('"'),
            b'\'' => Some('\''),
            _ => None,
        };
        let (value_end, replacement) = if let Some(quote) = quote {
            let mut end = value_start + 1;
            while end < bytes.len() {
                if bytes[end] == quote as u8 && bytes[end - 1] != b'\\' {
                    end += 1;
                    break;
                }
                end += 1;
            }
            (end, format!("{quote}{MARK}{quote}"))
        } else if free_text {
            // A body or title runs to the end of the line; the rest cannot be trusted as metadata.
            (bytes.len(), MARK.to_owned())
        } else {
            let mut end = value_start;
            while end < bytes.len() && !bytes[end].is_ascii_whitespace() {
                end += 1;
            }
            (end, MARK.to_owned())
        };
        out.push_str(&line[cursor..value_start]);
        out.push_str(&replacement);
        cursor = value_end;
        i = value_end;
    }
    out.push_str(&line[cursor..]);
    out
}

/// Drops 32+ character hex runs that mix digits and letters (a Telegram `api_hash`).
fn redact_api_hashes(line: &str) -> String {
    let bytes = line.as_bytes();
    let mut out = String::with_capacity(line.len());
    let mut cursor = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        if !bytes[i].is_ascii_hexdigit() {
            i += 1;
            continue;
        }
        let start = i;
        while i < bytes.len() && bytes[i].is_ascii_hexdigit() {
            i += 1;
        }
        let run = &line[start..i];
        let boundary = (start == 0 || !is_word_byte(bytes[start - 1]))
            && (i == bytes.len() || !is_word_byte(bytes[i]));
        let mixed =
            run.bytes().any(|b| b.is_ascii_digit()) && run.bytes().any(|b| b.is_ascii_alphabetic());
        if boundary && run.len() >= 32 && mixed {
            out.push_str(&line[cursor..start]);
            out.push_str(MARK);
            cursor = i;
        }
    }
    out.push_str(&line[cursor..]);
    out
}

fn is_email_local(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'%' | b'+' | b'-')
}

fn is_email_domain(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-')
}

fn redact_emails(line: &str) -> String {
    let bytes = line.as_bytes();
    let mut out = String::with_capacity(line.len());
    let mut cursor = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] != b'@' {
            i += 1;
            continue;
        }
        let mut start = i;
        while start > cursor && is_email_local(bytes[start - 1]) {
            start -= 1;
        }
        let mut end = i + 1;
        while end < bytes.len() && is_email_domain(bytes[end]) {
            end += 1;
        }
        while end > i + 1 && bytes[end - 1] == b'.' {
            end -= 1;
        }
        let domain = &line[i + 1..end];
        let tld = domain.rsplit('.').next().unwrap_or("");
        if start < i
            && domain.contains('.')
            && tld.len() >= 2
            && tld.bytes().all(|b| b.is_ascii_alphabetic())
        {
            out.push_str(&line[cursor..start]);
            out.push_str(MARK);
            cursor = end;
            i = end;
        } else {
            i += 1;
        }
    }
    out.push_str(&line[cursor..]);
    out
}

/// Drops phone numbers: 9–15 digits, optionally prefixed with `+`, optionally grouped.
fn redact_phones(line: &str) -> String {
    let bytes = line.as_bytes();
    let mut out = String::with_capacity(line.len());
    let mut cursor = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        let lead = bytes[i];
        if lead != b'+' && !lead.is_ascii_digit() {
            i += 1;
            continue;
        }
        if i > 0 && (is_word_byte(bytes[i - 1]) || bytes[i - 1] == b'+') {
            i += 1;
            continue;
        }
        if i < cursor {
            i += 1;
            continue;
        }
        let start = i;
        let plus = lead == b'+';
        let mut digits = if lead.is_ascii_digit() { 1 } else { 0 };
        let mut end = start + 1;
        while end < bytes.len() {
            let byte = bytes[end];
            if byte.is_ascii_digit() {
                digits += 1;
                end += 1;
            } else if matches!(byte, b' ' | b'-' | b'.' | b'(' | b')') {
                end += 1;
            } else {
                break;
            }
        }
        let mut trimmed = end;
        while trimmed > start && !bytes[trimmed - 1].is_ascii_digit() {
            trimmed -= 1;
        }
        let boundary = trimmed == bytes.len() || !is_word_byte(bytes[trimmed]);
        let min_digits = if plus { 8 } else { 9 };
        if boundary && digits >= min_digits && digits <= 15 {
            out.push_str(&line[cursor..start]);
            out.push_str(MARK);
            cursor = trimmed;
            i = trimmed;
        } else {
            i += 1;
        }
    }
    out.push_str(&line[cursor..]);
    out
}

/// Collapses an absolute home path to `~`, hiding the user name.
fn redact_home_paths(line: &str) -> String {
    let mut out = line.to_owned();
    for prefix in HOME_PREFIXES {
        out = redact_home_prefix(&out, prefix);
    }
    out
}

fn redact_home_prefix(line: &str, prefix: &str) -> String {
    let lower = line.to_ascii_lowercase();
    let mut out = String::with_capacity(line.len());
    let mut cursor = 0usize;
    let mut search = 0usize;
    while search < lower.len() {
        let Some(pos) = lower[search..].find(prefix) else {
            break;
        };
        let start = search + pos;
        let after = start + prefix.len();
        // Drop a Windows drive prefix too, so `C:\Users\name` becomes `~`.
        let path_start = if prefix.starts_with('\\')
            && start >= 2
            && lower.as_bytes()[start - 1] == b':'
            && lower.as_bytes()[start - 2].is_ascii_alphabetic()
        {
            start - 2
        } else {
            start
        };
        // A path starts at the beginning of a word: `https://example.com/home/x` is not a home.
        let boundary = path_start == 0
            || !(is_word_byte(lower.as_bytes()[path_start - 1])
                || matches!(lower.as_bytes()[path_start - 1], b'/' | b'\\' | b':'));
        if !boundary {
            search = after;
            continue;
        }
        let mut end = after;
        while end < lower.len() {
            let byte = lower.as_bytes()[end];
            if matches!(byte, b'/' | b'\\' | b'"' | b'\'') || byte.is_ascii_whitespace() {
                break;
            }
            end += 1;
        }
        if end == after {
            search = after;
            continue;
        }
        out.push_str(&line[cursor..path_start]);
        out.push('~');
        cursor = end;
        search = end;
    }
    out.push_str(&line[cursor..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const API_HASH: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn redacts_the_telegram_api_hash() {
        let line = format!("login api_hash={API_HASH} starting");
        let out = redact_line(&line);
        assert!(!out.contains(API_HASH), "{out}");
        assert!(out.contains("api_hash=<redacted>"), "{out}");
    }

    #[test]
    fn redacts_a_bare_hex_api_hash() {
        let out = redact_line(&format!("credentials {API_HASH} loaded"));
        assert!(!out.contains(API_HASH), "{out}");
        assert!(out.contains(MARK), "{out}");
    }

    #[test]
    fn redacts_session_material_and_bearer_tokens() {
        let line = "auth session=AAABBBCCCDDD auth_key=deadbeef Authorization: Bearer abc.def.ghi";
        let out = redact_line(line);
        for secret in ["AAABBBCCCDDD", "deadbeef", "abc.def.ghi"] {
            assert!(!out.contains(secret), "{out} leaks {secret}");
        }
    }

    #[test]
    fn redacts_cookie_headers() {
        let out = redact_line("request Cookie: sessionid=abc123; c_user=42 done");
        assert!(!out.contains("abc123"), "{out}");
        assert!(!out.contains("c_user"), "{out}");
    }

    #[test]
    fn redacts_phone_numbers() {
        for phone in [
            "+84 912 345 678",
            "+84912345678",
            "0912 345 678",
            "(555) 123-4567",
        ] {
            let out = redact_line(&format!("otp sent to {phone} now"));
            assert!(!out.contains(phone), "{out} leaks {phone}");
            assert!(out.contains(MARK), "{out}");
        }
    }

    #[test]
    fn leaves_dates_and_short_numbers_alone() {
        let line = "started 2026-11-16 at 12:34:56 (42 ms)";
        assert_eq!(redact_line(line), line);
    }

    #[test]
    fn redacts_email_addresses() {
        let out = redact_line("mail to cento+alloy@example.com failed");
        assert!(!out.contains("example.com"), "{out}");
        assert!(out.contains(MARK), "{out}");
    }

    #[test]
    fn redacts_message_bodies_and_titles() {
        let out = redact_line("message text=\"meet me at the pier\" incoming title=Family-chat");
        assert!(!out.contains("pier"), "{out}");
        assert!(!out.contains("Family-chat"), "{out}");
    }

    #[test]
    fn redacts_home_paths() {
        for line in [
            "opened /Users/cento/Library/Alloy/state.db",
            "opened /home/cento/.local/share/alloy",
            "opened C:\\Users\\cento\\AppData\\alloy",
        ] {
            let out = redact_line(line);
            assert!(
                !out.to_lowercase().contains("cento"),
                "{out} leaks the user name"
            );
            assert!(out.contains('~'), "{out}");
        }
    }

    #[test]
    fn redacts_a_line_that_mixes_every_category() {
        let line = format!(
            "login user=cento@example.com phone=+84 912 345 678 api_hash={API_HASH} \
             session=QUJDREVGRw== body=\"see you at noon\" path=/Users/cento/alloy.db"
        );
        let out = redact_line(&line);
        for secret in [
            "cento@example.com",
            "+84 912 345 678",
            API_HASH,
            "QUJDREVGRw==",
            "see you at noon",
            "/Users/cento",
        ] {
            assert!(!out.contains(secret), "{out} leaks {secret}");
        }
    }

    #[test]
    fn redaction_is_idempotent() {
        let once = redact_line("Cookie: a=b user@example.com +84912345678 /home/cento/x");
        assert_eq!(redact_line(&once), once);
    }

    #[test]
    fn truncates_runaway_lines() {
        let line = truncate_line("x".repeat(MAX_LINE_BYTES + 100));
        assert!(line.len() <= MAX_LINE_BYTES + 4);
        assert!(line.ends_with(" ..."), "{line}");
    }

    #[test]
    fn ring_keeps_the_newest_lines_and_clears() {
        let buffer = LogBuffer::new();
        for index in 0..(RING_CAPACITY + 5) {
            buffer.push(format!("line {index}"));
        }
        let snapshot = buffer.snapshot();
        assert_eq!(snapshot.lines, RING_CAPACITY);
        assert!(!snapshot.truncated);
        assert!(
            snapshot.text.starts_with("line 5"),
            "{}",
            &snapshot.text[..16]
        );
        assert!(
            snapshot
                .text
                .ends_with(&format!("line {}", RING_CAPACITY + 4))
        );
        buffer.clear();
        assert_eq!(buffer.snapshot().lines, 0);
    }

    #[test]
    fn snapshot_reports_truncation_past_the_byte_cap() {
        let buffer = LogBuffer::new();
        let line = "y".repeat(MAX_LINE_BYTES);
        for index in 0..100 {
            buffer.push(format!("{index:04} {line}"));
        }
        let snapshot = buffer.snapshot();
        assert!(snapshot.truncated);
        assert!(snapshot.text.len() <= MAX_TEXT_BYTES + MAX_LINE_BYTES);
        assert!(snapshot.lines < 100);
    }
}
