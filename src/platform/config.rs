// LogSleuth - platform/config.rs
//
// Platform-specific configuration and data directory resolution.
// Uses the `directories` crate for XDG (Linux), AppData (Windows),
// Library (macOS) compliance.

use crate::util::constants;
use directories::ProjectDirs;
use std::path::PathBuf;

/// Resolved platform paths for LogSleuth data and configuration.
#[derive(Debug, Clone)]
pub struct PlatformPaths {
    /// Configuration directory (e.g. ~/.config/logsleuth/ or %APPDATA%\LogSleuth\)
    pub config_dir: PathBuf,

    /// User profile directory (e.g. ~/.config/logsleuth/profiles/)
    pub user_profiles_dir: PathBuf,

    /// Data directory for logs, caches, etc.
    pub data_dir: PathBuf,
}

impl PlatformPaths {
    /// Resolve platform-appropriate paths.
    ///
    /// Falls back to current directory if platform dirs cannot be determined.
    pub fn resolve() -> Self {
        if let Some(proj_dirs) = ProjectDirs::from("", "", constants::APP_ID) {
            let config_dir = proj_dirs.config_dir().to_path_buf();
            let user_profiles_dir = config_dir.join(constants::PROFILES_DIR_NAME);
            let data_dir = proj_dirs.data_dir().to_path_buf();

            tracing::debug!(
                config = %config_dir.display(),
                profiles = %user_profiles_dir.display(),
                data = %data_dir.display(),
                "Platform paths resolved"
            );

            Self {
                config_dir,
                user_profiles_dir,
                data_dir,
            }
        } else {
            tracing::warn!("Could not determine platform directories, using current directory");
            let fallback = PathBuf::from(".");
            Self {
                config_dir: fallback.clone(),
                user_profiles_dir: fallback.join(constants::PROFILES_DIR_NAME),
                data_dir: fallback,
            }
        }
    }
}
