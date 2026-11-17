//! The Messenger session that lives in this Mac's Chrome profile, read when the person asks.
//!
//! Chrome encrypts every cookie with a key kept in the login Keychain, so reading them can raise a
//! system prompt. Nothing here runs at startup and no app state holds a reader: the only caller is
//! [`import_chrome_session`], which builds the reader, reads, and drops it. The session it hands
//! back goes into the connector's own cookie login, which stores it as the account's session
//! material, and Chrome is never read for that account again (ADR 0022).

use std::collections::BTreeMap;

use serde::Serialize;

/// The cookies the Messenger cookie login accepts (`cookieFields` in `sidecars/alloy-meta`).
#[cfg(target_os = "macos")]
const MESSENGER_COOKIES: [&str; 5] = ["xs", "c_user", "datr", "sb", "fr"];
/// Without these the login cannot load the inbox, so a profile that lacks them is not a session.
#[cfg(target_os = "macos")]
const MESSENGER_REQUIRED: [&str; 3] = ["xs", "c_user", "datr"];

/// What the page gets back.
///
/// `cookies` carries only [`MESSENGER_COOKIES`] and never leaves the process except as the login
/// answer, so it is never logged, printed in an error, or written to disk.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChromeSession {
    /// Name → value for the cookies the Messenger login asks for.
    pub cookies: BTreeMap<String, String>,
    /// The Chrome profile folder the session came from ("Default", "Profile 1").
    pub profile: String,
    /// Whether Chrome's session is still live; the flow warns about two clients sharing it then.
    pub active: bool,
}

/// Reads the Messenger session out of Chrome's cookie store, now.
///
/// Delphi of the laziness rule: this command is the only code path in the app that opens Chrome's
/// files or asks the Keychain for a key, and it runs only when the person presses the button.
#[tauri::command]
pub async fn import_chrome_session() -> Result<ChromeSession, String> {
    #[cfg(target_os = "macos")]
    {
        tauri::async_runtime::spawn_blocking(mac::read)
            .await
            .map_err(|err| format!("the Chrome import stopped: {err}"))?
            .map_err(|err| err.to_string())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err("Chrome's cookie store is only read on macOS".to_owned())
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use std::collections::BTreeMap;
    use std::fmt;
    use std::fs;
    use std::io;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use aes::cipher::{BlockDecryptMut, KeyIvInit, block_padding::Pkcs7};
    use rusqlite::OpenFlags;
    use sha2::{Digest, Sha256};

    use super::{ChromeSession, MESSENGER_COOKIES, MESSENGER_REQUIRED};

    /// Chrome's published macOS scheme. Every constant here changes the decrypted bytes, which is
    /// why the test pins a ciphertext produced outside this code rather than round-tripping.
    const SALT: &[u8] = b"saltysalt";
    const ITERATIONS: u32 = 1003;
    const KEY_LEN: usize = 16;
    /// The scheme's IV, sixteen spaces.
    const IV: [u8; 16] = *b"                ";
    /// Chrome's stored expiry counts microseconds from 1601-01-01 UTC.
    const EPOCH_OFFSET_SECS: u64 = 11_644_473_600;

    /// Why an import produced no session. Messages name the failure, never the material.
    #[derive(Debug)]
    pub enum ChromeError {
        ChromeMissing,
        NotSignedIn,
        SeveralProfiles(Vec<String>),
        Incomplete(Vec<&'static str>),
        Keychain(String),
        CookieFile(io::Error),
        UnknownScheme,
        Decrypt,
        Text,
    }

    impl fmt::Display for ChromeError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::ChromeMissing => {
                    write!(f, "Google Chrome is not installed on this Mac.")
                }
                Self::NotSignedIn => write!(
                    f,
                    "This Chrome profile is not signed in to Messenger. Sign in at messenger.com \
                     in Chrome, then try again."
                ),
                Self::SeveralProfiles(names) => write!(
                    f,
                    "Chrome has a Messenger session in more than one profile ({}). Sign out of \
                     Messenger in all but one of them, then try again.",
                    names.join(", ")
                ),
                Self::Incomplete(names) => write!(
                    f,
                    "Chrome has a Messenger session, but it is missing the {} cookie(s). Sign in \
                     at messenger.com in Chrome again, then try again.",
                    names.join(", ")
                ),
                Self::Keychain(detail) => write!(
                    f,
                    "The Keychain did not give Alloy Chrome's cookie key ({detail}). Allow the \
                     access Chrome's key needs, then try again."
                ),
                Self::CookieFile(err) => write!(
                    f,
                    "Chrome's cookie file could not be read ({err}). If Chrome is running, quit \
                     it and try again."
                ),
                Self::UnknownScheme => write!(
                    f,
                    "This Chrome stores its cookies in a format Alloy does not know. Paste the \
                     cookies by hand instead."
                ),
                Self::Decrypt | Self::Text => write!(
                    f,
                    "A cookie from Chrome did not decrypt. Paste the cookies by hand instead."
                ),
            }
        }
    }

    /// Chrome's cookie key. Reading it is what raises the Keychain prompt, so it is asked for only
    /// once a profile with a Messenger session is known to exist.
    pub trait SafeStorage {
        fn password(&self) -> Result<String, ChromeError>;
    }

    /// The login Keychain item Chrome writes the first time it saves a cookie.
    struct Keychain;

    impl SafeStorage for Keychain {
        fn password(&self) -> Result<String, ChromeError> {
            keyring::Entry::new("Chrome Safe Storage", "Chrome")
                .and_then(|entry| entry.get_password())
                .map_err(|err| ChromeError::Keychain(err.to_string()))
        }
    }

    /// One Chrome user-data directory. Constructing it touches no file and no Keychain item: all
    /// I/O lives in [`ChromeProfile::read`].
    pub struct ChromeProfile<S: SafeStorage> {
        root: PathBuf,
        safe_storage: S,
    }

    impl ChromeProfile<Keychain> {
        /// Chrome's user-data directory under the person's home folder.
        fn standard() -> Result<Self, ChromeError> {
            let home = std::env::var_os("HOME").ok_or(ChromeError::ChromeMissing)?;
            Ok(Self {
                root: PathBuf::from(home).join("Library/Application Support/Google/Chrome"),
                safe_storage: Keychain,
            })
        }
    }

    /// The command's blocking body: build the reader, read, drop it.
    pub fn read() -> Result<ChromeSession, ChromeError> {
        ChromeProfile::standard()?.read()
    }

    impl<S: SafeStorage> ChromeProfile<S> {
        fn read(&self) -> Result<ChromeSession, ChromeError> {
            let mut sessions = Vec::new();
            for (profile, database) in self.profiles()? {
                let rows = read_rows(&database)?;
                if let Some(picked) = pick(rows) {
                    sessions.push((profile, picked));
                }
            }

            let (profile, picked) = match sessions.len() {
                0 => return Err(ChromeError::NotSignedIn),
                1 => sessions.pop().expect("one session"),
                _ => {
                    return Err(ChromeError::SeveralProfiles(
                        sessions.into_iter().map(|(name, _)| name).collect(),
                    ));
                }
            };

            // The prompt, if the Keychain raises one, happens here and nowhere earlier.
            let key = derive_key(&self.safe_storage.password()?);
            let active = picked
                .get("xs")
                .is_some_and(|row| row.is_live(SystemTime::now()));

            let mut cookies = BTreeMap::new();
            for (name, row) in picked {
                let value = if row.value.is_empty() {
                    decrypt(&key, &row.encrypted, &row.host)?
                } else {
                    row.value
                };
                if !value.is_empty() {
                    cookies.insert(name, value);
                }
            }

            let missing: Vec<&'static str> = MESSENGER_REQUIRED
                .into_iter()
                .filter(|name| !cookies.contains_key(*name))
                .collect();
            if !missing.is_empty() {
                return Err(ChromeError::Incomplete(missing));
            }

            Ok(ChromeSession {
                cookies,
                profile,
                active,
            })
        }

        /// Every Chrome profile folder that has a cookie store, with the store's path.
        ///
        /// Current Chrome keeps it under `Network/`; builds before that kept it beside the
        /// profile's other files, and a profile that has not been migrated still uses that spot.
        fn profiles(&self) -> Result<Vec<(String, PathBuf)>, ChromeError> {
            if !self.root.is_dir() {
                return Err(ChromeError::ChromeMissing);
            }
            let entries = fs::read_dir(&self.root).map_err(ChromeError::CookieFile)?;
            let mut found = Vec::new();
            for entry in entries.flatten() {
                let Ok(file_type) = entry.file_type() else {
                    continue;
                };
                if !file_type.is_dir() {
                    continue;
                }
                let name = entry.file_name().to_string_lossy().into_owned();
                if name != "Default" && !name.starts_with("Profile ") {
                    continue;
                }
                let dir = entry.path();
                let database = [dir.join("Network").join("Cookies"), dir.join("Cookies")]
                    .into_iter()
                    .find(|path| path.is_file());
                if let Some(database) = database {
                    found.push((name, database));
                }
            }
            found.sort();
            Ok(found)
        }
    }

    /// One row of Chrome's `cookies` table, kept as bytes until the key is available.
    struct Row {
        host: String,
        name: String,
        value: String,
        encrypted: Vec<u8>,
        expires_utc: i64,
    }

    impl Row {
        fn is_live(&self, now: SystemTime) -> bool {
            if self.expires_utc <= 0 {
                // A browser-session cookie: Chrome drops it when Chrome closes, not before.
                return true;
            }
            let now = now
                .duration_since(UNIX_EPOCH)
                .map(|elapsed| elapsed.as_secs())
                .unwrap_or(0)
                + EPOCH_OFFSET_SECS;
            self.expires_utc as u64 / 1_000_000 > now
        }
    }

    /// Which host a cookie has to belong to: messenger.com first, facebook.com as the fallback the
    /// browser also writes the session to. Anything else is dropped.
    fn rank(host: &str) -> Option<u8> {
        let host = host.trim_start_matches('.');
        if host == "messenger.com" || host.ends_with(".messenger.com") {
            Some(0)
        } else if host == "facebook.com" || host.ends_with(".facebook.com") {
            Some(1)
        } else {
            None
        }
    }

    /// Keeps the best row per wanted name and gives up unless the account id and session are both
    /// there; a profile without those is not a signed-in Messenger session.
    fn pick(rows: Vec<Row>) -> Option<BTreeMap<String, Row>> {
        let mut chosen: BTreeMap<String, Row> = BTreeMap::new();
        for row in rows {
            if !MESSENGER_COOKIES.contains(&row.name.as_str())
                || (row.value.is_empty() && row.encrypted.is_empty())
            {
                continue;
            }
            let Some(better_rank) = rank(&row.host) else {
                continue;
            };
            let keep = chosen
                .get(&row.name)
                .is_none_or(|existing| better_rank < rank(&existing.host).unwrap_or(u8::MAX));
            if keep {
                chosen.insert(row.name.clone(), row);
            }
        }
        (chosen.contains_key("xs") && chosen.contains_key("c_user")).then_some(chosen)
    }

    fn read_rows(database: &Path) -> Result<Vec<Row>, ChromeError> {
        let copy = CookieCopy::new(database)?;
        let connection = rusqlite::Connection::open_with_flags(
            &copy.database,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(sql_error)?;
        let mut statement = connection
            .prepare("SELECT host_key, name, value, encrypted_value, expires_utc FROM cookies")
            .map_err(sql_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok(Row {
                    host: row.get(0)?,
                    name: row.get(1)?,
                    value: row.get(2)?,
                    encrypted: row.get(3)?,
                    expires_utc: row.get(4)?,
                })
            })
            .map_err(sql_error)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(sql_error)?);
        }
        Ok(out)
    }

    /// A SQLite error names the failure, never a row's contents.
    fn sql_error(err: rusqlite::Error) -> ChromeError {
        ChromeError::CookieFile(io::Error::other(err.to_string()))
    }

    /// A private copy of Chrome's cookie database in a private temporary directory, so a running
    /// Chrome keeps its lock and Alloy never writes the real file. Removed when dropped.
    struct CookieCopy {
        directory: PathBuf,
        database: PathBuf,
    }

    impl CookieCopy {
        fn new(source: &Path) -> Result<Self, ChromeError> {
            let directory =
                std::env::temp_dir().join(format!("alloy-chrome-{}", uuid::Uuid::new_v4()));
            fs::create_dir(&directory).map_err(ChromeError::CookieFile)?;
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))
                .map_err(ChromeError::CookieFile)?;
            let database = directory.join("Cookies");
            copy_private(source, &database)?;
            // Write-ahead sidecars hold the newest cookies; without them the copy can look empty.
            for suffix in ["-wal", "-shm"] {
                let mut from = source.as_os_str().to_owned();
                from.push(suffix);
                let from = PathBuf::from(from);
                if from.is_file() {
                    let mut to = database.as_os_str().to_owned();
                    to.push(suffix);
                    copy_private(&from, Path::new(&to))?;
                }
            }
            Ok(Self {
                directory,
                database,
            })
        }
    }

    impl Drop for CookieCopy {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.directory);
        }
    }

    fn copy_private(source: &Path, destination: &Path) -> Result<(), ChromeError> {
        let mut out = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(destination)
            .map_err(ChromeError::CookieFile)?;
        let mut input = fs::File::open(source).map_err(ChromeError::CookieFile)?;
        io::copy(&mut input, &mut out).map_err(ChromeError::CookieFile)?;
        Ok(())
    }

    fn derive_key(password: &str) -> [u8; KEY_LEN] {
        let mut key = [0u8; KEY_LEN];
        pbkdf2::pbkdf2_hmac::<sha1::Sha1>(password.as_bytes(), SALT, ITERATIONS, &mut key);
        key
    }

    /// Decrypts one `v10` value: AES-128-CBC with the space IV, then Chrome's SHA-256(host) prefix
    /// removed when the build writes one.
    fn decrypt(key: &[u8; KEY_LEN], encrypted: &[u8], host: &str) -> Result<String, ChromeError> {
        let body = encrypted
            .strip_prefix(b"v10".as_slice())
            .ok_or(ChromeError::UnknownScheme)?;
        if body.is_empty() || body.len() % 16 != 0 {
            return Err(ChromeError::Decrypt);
        }
        let mut buffer = body.to_vec();
        let plain = cbc::Decryptor::<aes::Aes128>::new_from_slices(key, &IV)
            .map_err(|_| ChromeError::Decrypt)?
            .decrypt_padded_mut::<Pkcs7>(&mut buffer)
            .map_err(|_| ChromeError::Decrypt)?;
        let digest = Sha256::digest(host.as_bytes());
        let plain: &[u8] = plain;
        let value = plain.strip_prefix(digest.as_slice()).unwrap_or(plain);
        std::str::from_utf8(value)
            .map(str::to_owned)
            .map_err(|_| ChromeError::Text)
    }

    #[cfg(test)]
    mod tests {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        use super::*;

        // Ciphertexts made outside this code with the scheme above (key from "test-password",
        // SHA-256(host) prefix, PKCS#7). They fail if the KDF, the IV, or the prefix handling moves.
        const XS: &str = "7631309a052b517d07c06b4d58950b64169509173073880ef1f2c0cdad21e0809b89371e7b00d124fc292e73febaa21e610d87";
        const C_USER: &str = "7631309a052b517d07c06b4d58950b64169509173073880ef1f2c0cdad21e0809b89374cad15b8a5808c85bb76f37b88cc0fd9";
        const DATR: &str = "7631309a052b517d07c06b4d58950b64169509173073880ef1f2c0cdad21e0809b8937ee190afac74a035c6c3ec91d50ab11b2";
        const SB: &str = "7631309a052b517d07c06b4d58950b64169509173073880ef1f2c0cdad21e0809b89374365a219da73ca0869805763069ef19c";
        const FR: &str = "7631309a052b517d07c06b4d58950b64169509173073880ef1f2c0cdad21e0809b89376449aafad11e72f97e2f2a491296e253";
        const FACEBOOK_XS: &str = "763130658baeaebf5222496d03c1787ba4e3c1e49ec254109f153e8a94f954f9a9de7bba356375ac8b2d28249d04c969ae92ad";
        const GOOGLE_XS: &str = "7631301712cfebbd2b19888cec2be4dc805c434274e1725c967526742857b46344e4da53c1b70f9961b69396362365a81a6436";
        /// Expired long before this decade; only the selection and the `active` flag use it.
        const EXPIRED: i64 = 13_000_000_000_000_000;

        fn bytes(hex: &str) -> Vec<u8> {
            (0..hex.len())
                .step_by(2)
                .map(|at| u8::from_str_radix(&hex[at..at + 2], 16).unwrap())
                .collect()
        }

        struct FakeKeychain {
            calls: Arc<AtomicUsize>,
        }

        impl SafeStorage for FakeKeychain {
            fn password(&self) -> Result<String, ChromeError> {
                self.calls.fetch_add(1, Ordering::SeqCst);
                Ok("test-password".to_owned())
            }
        }

        fn future() -> i64 {
            (u64::MAX / 2) as i64
        }

        /// Writes one profile's cookie store with the rows the test needs.
        fn write_profile(root: &Path, profile: &str, rows: &[(&str, &str, &str, i64)]) {
            let directory = root.join(profile);
            fs::create_dir_all(directory.join("Network")).unwrap();
            let database = directory.join("Network").join("Cookies");
            let connection = rusqlite::Connection::open(&database).unwrap();
            connection
                .execute_batch(
                    "CREATE TABLE cookies (host_key TEXT, name TEXT, value TEXT, \
                     encrypted_value BLOB, expires_utc INTEGER);",
                )
                .unwrap();
            for (host, name, encrypted, expires) in rows {
                connection
                    .execute(
                        "INSERT INTO cookies (host_key, name, value, encrypted_value, expires_utc) \
                         VALUES (?1, ?2, '', ?3, ?4)",
                        rusqlite::params![host, name, bytes(encrypted), expires],
                    )
                    .unwrap();
            }
        }

        fn messenger_rows() -> Vec<(&'static str, &'static str, &'static str, i64)> {
            vec![
                (".messenger.com", "xs", XS, future()),
                (".messenger.com", "c_user", C_USER, future()),
                (".messenger.com", "datr", DATR, future()),
                (".messenger.com", "sb", SB, future()),
                (".messenger.com", "fr", FR, future()),
            ]
        }

        fn profile(root: &Path, calls: &Arc<AtomicUsize>) -> ChromeProfile<FakeKeychain> {
            ChromeProfile {
                root: root.to_path_buf(),
                safe_storage: FakeKeychain {
                    calls: calls.clone(),
                },
            }
        }

        fn scratch(name: &str) -> PathBuf {
            let root = std::env::temp_dir()
                .join(format!("alloy-chrome-test-{name}-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&root).unwrap();
            root
        }

        #[test]
        fn decrypts_chrome_v10_with_the_documented_keychain_scheme() {
            let key = derive_key("test-password");
            assert_eq!(
                key,
                [
                    0xc0, 0xff, 0xe4, 0xc2, 0x5f, 0x07, 0xf6, 0x2b, 0xfc, 0x6a, 0xb0, 0x11, 0xd9,
                    0xef, 0xa5, 0x4e
                ]
            );
            assert_eq!(
                decrypt(&key, &bytes(XS), ".messenger.com").unwrap(),
                "session-token-1"
            );
            // A wrong password or moved IV cannot reproduce the value.
            assert!(decrypt(&derive_key("other"), &bytes(XS), ".messenger.com").is_err());
            // A value bound to another host does not shed its prefix, so it is not taken as text
            // and certainly not as a session token.
            assert!(matches!(
                decrypt(&key, &bytes(XS), ".facebook.com"),
                Err(ChromeError::Text)
            ));
        }

        #[test]
        fn keeps_only_the_messenger_cookies_from_the_best_host() {
            let root = scratch("select");
            let mut rows = messenger_rows();
            // Wrong host, wrong name: dropped.
            rows.push((".google.com", "xs", GOOGLE_XS, future()));
            rows.push((".messenger.com", "other", XS, future()));
            // The facebook.com copy loses to messenger.com for the same name.
            rows.push((".facebook.com", "xs", FACEBOOK_XS, future()));
            write_profile(&root, "Default", &rows);
            // Another profile without a Messenger session is not a candidate.
            write_profile(
                &root,
                "Profile 1",
                &[(".google.com", "xs", GOOGLE_XS, future())],
            );

            let calls = Arc::new(AtomicUsize::new(0));
            let session = profile(&root, &calls).read().unwrap();

            assert_eq!(session.profile, "Default");
            assert!(session.active);
            assert_eq!(
                session.cookies.keys().collect::<Vec<_>>(),
                ["c_user", "datr", "fr", "sb", "xs"]
            );
            assert_eq!(session.cookies["xs"], "session-token-1");
            assert_eq!(session.cookies["c_user"], "100012345");
            assert_eq!(calls.load(Ordering::SeqCst), 1);
            fs::remove_dir_all(root).unwrap();
        }

        #[test]
        fn two_profiles_with_a_session_ask_which_one() {
            let root = scratch("several");
            write_profile(&root, "Default", &messenger_rows());
            write_profile(&root, "Profile 2", &messenger_rows());

            let calls = Arc::new(AtomicUsize::new(0));
            let error = profile(&root, &calls).read().unwrap_err();
            assert!(matches!(&error, ChromeError::SeveralProfiles(names)
                if names == &["Default".to_owned(), "Profile 2".to_owned()]));
            // The key is not asked for until one profile can be used.
            assert_eq!(calls.load(Ordering::SeqCst), 0);
            fs::remove_dir_all(root).unwrap();
        }

        #[test]
        fn an_incomplete_profile_names_the_cookies_it_lacks() {
            let root = scratch("incomplete");
            let mut rows = messenger_rows();
            rows.retain(|(_, name, _, _)| *name != "datr");
            write_profile(&root, "Default", &rows);

            let calls = Arc::new(AtomicUsize::new(0));
            let error = profile(&root, &calls).read().unwrap_err();
            assert!(matches!(&error, ChromeError::Incomplete(names) if names == &["datr"]));
            fs::remove_dir_all(root).unwrap();
        }

        #[test]
        fn an_expired_session_is_reported_as_not_live() {
            let root = scratch("expired");
            let mut rows = messenger_rows();
            for row in &mut rows {
                if row.1 == "xs" {
                    row.3 = EXPIRED;
                }
            }
            write_profile(&root, "Default", &rows);

            let calls = Arc::new(AtomicUsize::new(0));
            assert!(!profile(&root, &calls).read().unwrap().active);
            fs::remove_dir_all(root).unwrap();
        }

        #[test]
        fn building_the_reader_reads_nothing() {
            let calls = Arc::new(AtomicUsize::new(0));
            let root = std::env::temp_dir().join("alloy-chrome-does-not-exist");
            let reader = profile(&root, &calls);

            // Construction is the same work `app.manage` would do; it must reach neither the
            // filesystem nor the Keychain.
            assert_eq!(calls.load(Ordering::SeqCst), 0);
            assert!(matches!(reader.read(), Err(ChromeError::ChromeMissing)));
            assert_eq!(calls.load(Ordering::SeqCst), 0);
        }
    }
}
