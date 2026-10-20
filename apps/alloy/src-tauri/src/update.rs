//! In-app updates.
//!
//! Desktop uses the Tauri updater: the manifest and every package are verified against the
//! public key in `tauri.conf.json` before anything is installed. A found update can be downloaded
//! ahead of time and kept in memory until it is applied. Android cannot replace its own APK
//! silently, so it only reports that a newer release exists and hands back its page.

use serde::Serialize;

/// Result of an update check, shared by every platform.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct UpdateInfo {
    pub available: bool,
    pub version: String,
    /// Release notes (markdown) from the update manifest's `notes` field.
    pub notes: Option<String>,
    /// The app can download and install this update itself.
    pub can_install: bool,
    /// Release page for platforms that cannot install in place.
    pub url: String,
    /// The verified package for this version is already stored and `apply_update` will not download.
    pub downloaded: bool,
}

impl UpdateInfo {
    fn up_to_date() -> Self {
        Self {
            available: false,
            version: String::new(),
            notes: None,
            can_install: false,
            url: String::new(),
            downloaded: false,
        }
    }
}

/// Parses `v1.2.3`, `1.2.3` or `1.2.3-rc1` into a comparable tuple; pre-release suffixes are
/// ignored, so `1.2.3-rc1` equals `1.2.3`.
#[cfg_attr(not(mobile), allow(dead_code))]
fn parse_version(v: &str) -> Option<(u64, u64, u64)> {
    let core = v.trim().trim_start_matches('v');
    let core = core.split(['-', '+']).next()?;
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().unwrap_or("0").parse().ok()?;
    let patch = parts.next().unwrap_or("0").parse().ok()?;
    parts.next().is_none().then_some((major, minor, patch))
}

#[cfg(desktop)]
mod platform {
    use super::UpdateInfo;
    use serde::Serialize;
    use std::sync::{Arc, Mutex, MutexGuard};
    use tauri::{AppHandle, Emitter, Manager};
    use tauri_plugin_updater::{Update, UpdaterExt};

    const PROGRESS_EVENT: &str = "update://progress";
    const DOWNLOADED_EVENT: &str = "update://downloaded";
    /// Progress is reported per this many bytes, so the UI is not flooded by network chunks.
    const PROGRESS_STEP: u64 = 256 * 1024;
    const NO_OFFER: &str = "no update has been found; check for updates first";
    const CHANGED: &str =
        "the offered update changed while it was downloading; check for updates again";

    #[derive(Clone, Serialize)]
    struct Progress {
        downloaded: u64,
        total: Option<u64>,
    }

    /// Identity of an offered package. A re-published version with a new signature is a new offer.
    #[derive(Clone, PartialEq, Eq)]
    struct Key {
        version: String,
        signature: String,
    }

    impl Key {
        fn of(update: &Update) -> Self {
            Self {
                version: update.version.clone(),
                signature: update.signature.clone(),
            }
        }
    }

    /// The update offered by the last check and, once downloaded, its verified package.
    #[derive(Clone)]
    struct Offer<U> {
        key: Key,
        update: U,
        package: Option<Arc<Vec<u8>>>,
    }

    /// State transitions for the single offered update. Generic over the update handle so the
    /// rules can be tested without a live updater.
    struct Slot<U>(Option<Offer<U>>);

    impl<U> Default for Slot<U> {
        fn default() -> Self {
            Self(None)
        }
    }

    impl<U> Slot<U> {
        /// Records the offer from a check. Offering the same release again keeps its package;
        /// offering anything else drops it. Returns whether the package is already stored.
        fn offer(&mut self, key: Key, update: U) -> bool {
            let package = self
                .0
                .take()
                .filter(|o| o.key == key)
                .and_then(|o| o.package);
            let downloaded = package.is_some();
            self.0 = Some(Offer {
                key,
                update,
                package,
            });
            downloaded
        }

        fn clear(&mut self) {
            self.0 = None;
        }

        fn snapshot(&self) -> Option<Offer<U>>
        where
            U: Clone,
        {
            self.0.clone()
        }

        /// Stores the package downloaded for `key`. Returns false, storing nothing, when that
        /// release is no longer the offer.
        fn keep(&mut self, key: &Key, package: Arc<Vec<u8>>) -> bool {
            match self.0.as_mut() {
                Some(offer) if offer.key == *key => {
                    offer.package = Some(package);
                    true
                }
                _ => false,
            }
        }
    }

    /// Reports whether a progress event should go out: every `PROGRESS_STEP` bytes, and always at
    /// the end of a download of known size.
    fn should_report(reported: u64, downloaded: u64, total: Option<u64>) -> bool {
        downloaded.saturating_sub(reported) >= PROGRESS_STEP || total == Some(downloaded)
    }

    /// The update found by the last check, with its package once downloaded.
    #[derive(Default)]
    pub struct PendingUpdate {
        offer: Mutex<Slot<Update>>,
        /// Serializes downloads: a concurrent caller waits and then finds the package stored.
        download: tokio::sync::Mutex<()>,
    }

    impl PendingUpdate {
        fn slot(&self) -> Result<MutexGuard<'_, Slot<Update>>, String> {
            self.offer.lock().map_err(|e| e.to_string())
        }
    }

    fn emit<S: Serialize + Clone>(app: &AppHandle, event: &str, payload: S) {
        if let Err(e) = app.emit(event, payload) {
            tracing::warn!(error = %e, event, "failed to emit update event");
        }
    }

    pub async fn check(app: &AppHandle) -> Result<UpdateInfo, String> {
        let found = app
            .updater()
            .map_err(|e| e.to_string())?
            .check()
            .await
            .map_err(|e| e.to_string())?;
        let state = app.state::<PendingUpdate>();
        let mut slot = state.slot()?;
        let Some(update) = found else {
            slot.clear();
            return Ok(UpdateInfo::up_to_date());
        };
        let version = update.version.clone();
        let notes = update.body.clone();
        let downloaded = slot.offer(Key::of(&update), update);
        Ok(UpdateInfo {
            available: true,
            version,
            notes,
            can_install: true,
            url: String::new(),
            downloaded,
        })
    }

    /// Returns the offered update with its verified package, downloading the package first when
    /// it is not stored yet.
    async fn fetch(app: &AppHandle) -> Result<(Update, Arc<Vec<u8>>), String> {
        let state = app.state::<PendingUpdate>();
        let _serial = state.download.lock().await;
        let offer = state
            .slot()?
            .snapshot()
            .ok_or_else(|| NO_OFFER.to_string())?;
        if let Some(package) = offer.package {
            emit(app, DOWNLOADED_EVENT, ());
            return Ok((offer.update, package));
        }

        let (mut done, mut reported) = (0u64, 0u64);
        let bytes = offer
            .update
            .download(
                |chunk, total| {
                    done += chunk as u64;
                    if should_report(reported, done, total) {
                        reported = done;
                        emit(
                            app,
                            PROGRESS_EVENT,
                            Progress {
                                downloaded: done,
                                total,
                            },
                        );
                    }
                },
                || {},
            )
            .await
            .map_err(|e| e.to_string())?;

        let package = Arc::new(bytes);
        if !state.slot()?.keep(&offer.key, package.clone()) {
            return Err(CHANGED.to_string());
        }
        emit(app, DOWNLOADED_EVENT, ());
        Ok((offer.update, package))
    }

    pub async fn download(app: &AppHandle) -> Result<(), String> {
        fetch(app).await.map(|_| ())
    }

    pub async fn apply(app: &AppHandle) -> Result<(), String> {
        let (update, package) = fetch(app).await?;
        update
            .install(package.as_slice())
            .map_err(|e| e.to_string())?;
        app.restart()
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn key(version: &str, signature: &str) -> Key {
            Key {
                version: version.into(),
                signature: signature.into(),
            }
        }

        #[test]
        fn recheck_of_same_release_keeps_its_package() {
            let mut slot: Slot<&str> = Slot::default();
            assert!(!slot.offer(key("1.2.0", "a"), "first"));
            assert!(slot.keep(&key("1.2.0", "a"), Arc::new(vec![1])));
            assert!(slot.offer(key("1.2.0", "a"), "second"));
            let offer = slot.snapshot().expect("offer");
            assert_eq!(offer.update, "second");
            assert!(offer.package.is_some());
        }

        #[test]
        fn offer_of_another_version_drops_the_package() {
            let mut slot: Slot<&str> = Slot::default();
            slot.offer(key("1.2.0", "a"), "first");
            assert!(slot.keep(&key("1.2.0", "a"), Arc::new(vec![1])));
            assert!(!slot.offer(key("1.3.0", "b"), "next"));
            assert!(slot.snapshot().expect("offer").package.is_none());
        }

        #[test]
        fn same_version_with_new_signature_drops_the_package() {
            let mut slot: Slot<&str> = Slot::default();
            slot.offer(key("1.2.0", "a"), "first");
            assert!(slot.keep(&key("1.2.0", "a"), Arc::new(vec![1])));
            assert!(!slot.offer(key("1.2.0", "rebuilt"), "first"));
        }

        #[test]
        fn package_for_a_replaced_offer_is_rejected() {
            let mut slot: Slot<&str> = Slot::default();
            slot.offer(key("1.2.0", "a"), "first");
            slot.offer(key("1.3.0", "b"), "next");
            assert!(!slot.keep(&key("1.2.0", "a"), Arc::new(vec![1])));
            assert!(slot.snapshot().expect("offer").package.is_none());
        }

        #[test]
        fn package_without_an_offer_is_rejected() {
            let mut slot: Slot<&str> = Slot::default();
            assert!(!slot.keep(&key("1.2.0", "a"), Arc::new(vec![1])));
        }

        #[test]
        fn clear_forgets_the_offer() {
            let mut slot: Slot<&str> = Slot::default();
            slot.offer(key("1.2.0", "a"), "first");
            slot.clear();
            assert!(slot.snapshot().is_none());
        }

        #[test]
        fn progress_is_throttled_and_final_chunk_always_reported() {
            assert!(!should_report(0, 1, Some(10 * PROGRESS_STEP)));
            assert!(should_report(0, PROGRESS_STEP, Some(10 * PROGRESS_STEP)));
            assert!(should_report(
                PROGRESS_STEP,
                PROGRESS_STEP + 5,
                Some(PROGRESS_STEP + 5)
            ));
            assert!(!should_report(PROGRESS_STEP, PROGRESS_STEP + 5, None));
        }
    }
}

#[cfg(mobile)]
mod platform {
    use super::{UpdateInfo, parse_version};
    use serde::Deserialize;
    use tauri::AppHandle;

    const RELEASES: &str = "https://api.github.com/repos/centopw/alloy/releases?per_page=10";
    const RELEASE_PAGE_ONLY: &str = "this platform installs updates from the release page";

    #[derive(Deserialize)]
    struct Release {
        tag_name: String,
        html_url: String,
        body: Option<String>,
        draft: bool,
    }

    pub async fn check(app: &AppHandle) -> Result<UpdateInfo, String> {
        let current = parse_version(&app.package_info().version.to_string())
            .ok_or_else(|| "unreadable app version".to_string())?;
        let releases: Vec<Release> = alloy_core::tls::with_system_roots(
            reqwest::Client::builder().user_agent(concat!("alloy/", env!("CARGO_PKG_VERSION"))),
        )
        .build()
        .map_err(|e| e.to_string())?
        .get(RELEASES)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
        // Releases are pre-releases for now, so the newest tagged one counts.
        let newest = releases
            .into_iter()
            .filter(|r| !r.draft)
            .filter_map(|r| parse_version(&r.tag_name).map(|v| (v, r)))
            .max_by_key(|(v, _)| *v);
        Ok(match newest {
            Some((v, r)) if v > current => UpdateInfo {
                available: true,
                version: r.tag_name.trim_start_matches('v').to_string(),
                notes: r.body,
                can_install: false,
                url: r.html_url,
                downloaded: false,
            },
            _ => UpdateInfo::up_to_date(),
        })
    }

    pub async fn download(_app: &AppHandle) -> Result<(), String> {
        Err(RELEASE_PAGE_ONLY.into())
    }

    pub async fn apply(_app: &AppHandle) -> Result<(), String> {
        Err(RELEASE_PAGE_ONLY.into())
    }
}

#[cfg(desktop)]
pub use platform::PendingUpdate;

/// A store build (F-Droid sets `ALLOY_DISTRIBUTION=fdroid` while compiling) is updated by the store, so the app
/// must not offer another download page. Takes the value as a parameter so it can be tested.
fn store_manages_updates(distribution: Option<&str>) -> bool {
    matches!(distribution, Some("fdroid"))
}

#[tauri::command]
pub async fn check_update(app: tauri::AppHandle) -> Result<UpdateInfo, String> {
    if store_manages_updates(option_env!("ALLOY_DISTRIBUTION")) {
        return Ok(UpdateInfo::up_to_date());
    }
    platform::check(&app).await
}

/// Downloads and verifies the offered update into memory without installing it.
#[tauri::command]
pub async fn download_update(app: tauri::AppHandle) -> Result<(), String> {
    platform::download(&app).await
}

/// Installs the offered update (downloading it first if needed) and restarts the app.
#[tauri::command]
pub async fn apply_update(app: tauri::AppHandle) -> Result<(), String> {
    platform::apply(&app).await
}

#[cfg(test)]
mod tests {
    use super::{parse_version, store_manages_updates};

    #[test]
    fn only_a_store_build_leaves_updates_to_the_store() {
        assert!(store_manages_updates(Some("fdroid")));
        assert!(!store_manages_updates(None));
        assert!(!store_manages_updates(Some("github")));
    }

    #[test]
    fn parses_tags_and_ignores_prerelease_suffixes() {
        assert_eq!(parse_version("v0.1.0"), Some((0, 1, 0)));
        assert_eq!(parse_version("1.2"), Some((1, 2, 0)));
        assert_eq!(parse_version("v1.2.3-rc1"), Some((1, 2, 3)));
        assert_eq!(parse_version("nightly"), None);
        assert_eq!(parse_version("1.2.3.4"), None);
    }

    #[test]
    fn versions_compare_numerically_not_lexically() {
        assert!(parse_version("v0.10.0") > parse_version("v0.9.9"));
    }
}
