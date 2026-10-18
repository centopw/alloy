//! In-app updates.
//!
//! Desktop uses the Tauri updater: the manifest and every package are verified against the
//! public key in `tauri.conf.json` before anything is installed. Android cannot replace its own
//! APK silently, so it only reports that a newer release exists and hands back its page.

use serde::Serialize;

/// Result of an update check, shared by every platform.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct UpdateInfo {
    pub available: bool,
    pub version: String,
    pub notes: Option<String>,
    /// The app can download and install this update itself.
    pub can_install: bool,
    /// Release page for platforms that cannot install in place.
    pub url: String,
}

impl UpdateInfo {
    fn up_to_date() -> Self {
        Self {
            available: false,
            version: String::new(),
            notes: None,
            can_install: false,
            url: String::new(),
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
    use std::sync::Mutex;
    use tauri::{AppHandle, Manager};
    use tauri_plugin_updater::{Update, UpdaterExt};

    /// The update found by the last check, kept so `install_update` installs exactly what was shown.
    #[derive(Default)]
    pub struct PendingUpdate(Mutex<Option<Update>>);

    pub async fn check(app: &AppHandle) -> Result<UpdateInfo, String> {
        let update = app
            .updater()
            .map_err(|e| e.to_string())?
            .check()
            .await
            .map_err(|e| e.to_string())?;
        let info = match &update {
            Some(u) => UpdateInfo {
                available: true,
                version: u.version.clone(),
                notes: u.body.clone(),
                can_install: true,
                url: String::new(),
            },
            None => UpdateInfo::up_to_date(),
        };
        *app.state::<PendingUpdate>()
            .0
            .lock()
            .map_err(|e| e.to_string())? = update;
        Ok(info)
    }

    pub async fn install(app: &AppHandle) -> Result<(), String> {
        let update = app
            .state::<PendingUpdate>()
            .0
            .lock()
            .map_err(|e| e.to_string())?
            .take()
            .ok_or_else(|| "no update has been found; check for updates first".to_string())?;
        update
            .download_and_install(|_, _| {}, || {})
            .await
            .map_err(|e| e.to_string())?;
        app.restart()
    }
}

#[cfg(mobile)]
mod platform {
    use super::{UpdateInfo, parse_version};
    use serde::Deserialize;
    use tauri::AppHandle;

    const RELEASES: &str = "https://api.github.com/repos/centopw/alloy/releases?per_page=10";

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
        let releases: Vec<Release> = reqwest::Client::builder()
            .user_agent(concat!("alloy/", env!("CARGO_PKG_VERSION")))
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
            },
            _ => UpdateInfo::up_to_date(),
        })
    }

    pub async fn install(_app: &AppHandle) -> Result<(), String> {
        Err("this platform installs updates from the release page".into())
    }
}

#[cfg(desktop)]
pub use platform::PendingUpdate;

#[tauri::command]
pub async fn check_update(app: tauri::AppHandle) -> Result<UpdateInfo, String> {
    platform::check(&app).await
}

#[tauri::command]
pub async fn install_update(app: tauri::AppHandle) -> Result<(), String> {
    platform::install(&app).await
}

#[cfg(test)]
mod tests {
    use super::parse_version;

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
