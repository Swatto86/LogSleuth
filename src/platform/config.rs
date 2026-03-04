// LogSleuth - platform/config.rs
//
// Platform-specific configuration, data directory resolution, and config.toml
// loading with startup validation (DevWorkflow Part A Rule 13).
//
// Uses the `directories` crate for XDG (Linux), AppData (Windows),
// Library (macOS) compliance.

use crate::util::constants;
use directories::ProjectDirs;
use std::path::{Path, PathBuf};

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
            // Profiles live one level above config/ so the user-visible path is
            // %APPDATA%\LogSleuth\profiles\ rather than the deeper
            // %APPDATA%\LogSleuth\config\profiles\.
            let user_profiles_dir = config_dir
                .parent()
                .unwrap_or(&config_dir)
                .join(constants::PROFILES_DIR_NAME);
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

// =============================================================================
// config.toml loading and validation (Rule 13)
// =============================================================================

/// Raw deserialisable shape of config.toml.
///
/// Unknown keys are silently ignored for forward compatibility -- a newer
/// config file can be used with an older binary without crashing.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
pub struct RawConfig {
    /// `[discovery]` section.
    pub discovery: DiscoverySection,
    /// `[parsing]` section.
    pub parsing: ParsingSection,
    /// `[ui]` section.
    pub ui: UiSection,
    /// `[export]` section.
    pub export: ExportSection,
    /// `[profiles]` section.
    pub profiles: ProfilesSection,
    /// `[logging]` section.
    pub logging: LoggingSection,
}

/// `[discovery]` config section.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
pub struct DiscoverySection {
    /// Maximum directory recursion depth.
    pub max_depth: Option<usize>,
    /// Maximum files to discover per scan.
    pub max_files: Option<usize>,
    /// Include glob patterns.
    pub include_patterns: Option<Vec<String>>,
    /// Exclude glob patterns.
    pub exclude_patterns: Option<Vec<String>>,
}

/// `[parsing]` config section.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
pub struct ParsingSection {
    /// Read chunk size in bytes.
    pub chunk_size_bytes: Option<usize>,
    /// Maximum single entry size in bytes.
    pub max_entry_size_bytes: Option<usize>,
    /// Large file warning threshold in bytes.
    pub large_file_threshold_bytes: Option<u64>,
    /// Number of worker threads (0 = auto).
    pub worker_threads: Option<usize>,
    /// Lines sampled for format auto-detection.
    pub content_detection_lines: Option<usize>,
}

/// `[ui]` config section.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
pub struct UiSection {
    /// Theme: "dark" or "light".
    pub theme: Option<String>,
    /// Correlation window in seconds.
    pub correlation_window_seconds: Option<i64>,
    /// Text filter debounce in ms.
    pub filter_debounce_ms: Option<u64>,
    /// Body font size in points.
    pub font_size: Option<f32>,
}

/// `[export]` config section.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
pub struct ExportSection {
    /// Warn before exporting this many entries.
    pub large_export_warning_threshold: Option<usize>,
}

/// `[profiles]` config section.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
pub struct ProfilesSection {
    /// Additional profile directory.
    pub user_profile_directory: Option<String>,
}

/// `[logging]` config section.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
pub struct LoggingSection {
    /// Log level: "error", "warn", "info", "debug", "trace".
    pub level: Option<String>,
    /// Log file path (empty = stderr only).
    pub file: Option<String>,
}

/// Validated application configuration derived from `config.toml`.
///
/// All values are validated against named constants at load time (Rule 13).
/// Invalid values produce actionable warnings and fall back to defaults.
#[derive(Debug, Clone)]
pub struct AppConfig {
    // -- Discovery --
    /// Maximum directory recursion depth.
    pub max_depth: usize,
    /// Maximum files to discover per scan.
    pub max_files: usize,

    // -- UI --
    /// Dark mode (true) or light mode (false).
    pub dark_mode: bool,
    /// Correlation window in seconds.
    pub correlation_window_secs: i64,
    /// Body font size in points.
    pub font_size: f32,

    // -- Logging --
    /// Logging level string (for init before tracing is available).
    pub log_level: Option<String>,
    /// Log file path.
    pub log_file: Option<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            max_depth: constants::DEFAULT_MAX_DEPTH,
            max_files: constants::DEFAULT_MAX_FILES,
            dark_mode: true,
            correlation_window_secs: constants::DEFAULT_CORRELATION_WINDOW_SECS,
            font_size: constants::DEFAULT_FONT_SIZE,
            log_level: None,
            log_file: None,
        }
    }
}

/// Load and validate `config.toml` from the given config directory.
///
/// Returns `AppConfig` with validated values and a list of non-fatal warnings.
/// If the file does not exist, returns defaults with no warnings (first-run).
/// If the file is unparseable, returns defaults with an error warning
/// (fail-fast on misconfiguration per Rule 13 -- the application still starts
/// but the user is informed).
pub fn load_config(config_dir: &Path) -> (AppConfig, Vec<String>) {
    let config_path = config_dir
        .parent()
        .unwrap_or(config_dir)
        .join(constants::CONFIG_FILE_NAME);

    let mut warnings: Vec<String> = Vec::new();

    if !config_path.exists() {
        tracing::debug!(path = %config_path.display(), "No config.toml found; using defaults");
        return (AppConfig::default(), warnings);
    }

    let content = match std::fs::read_to_string(&config_path) {
        Ok(c) => c,
        Err(e) => {
            let msg = format!(
                "Could not read config file '{}': {e}. Using defaults.",
                config_path.display()
            );
            tracing::warn!("{}", msg);
            warnings.push(msg);
            return (AppConfig::default(), warnings);
        }
    };

    let raw: RawConfig = match toml::from_str(&content) {
        Ok(r) => r,
        Err(e) => {
            let msg = format!(
                "Failed to parse config file '{}': {e}. Using defaults. \
                 See config.example.toml for the expected format.",
                config_path.display()
            );
            tracing::warn!("{}", msg);
            warnings.push(msg);
            return (AppConfig::default(), warnings);
        }
    };

    tracing::info!(path = %config_path.display(), "Loaded config.toml");

    // Validate each field against named constants, accumulating all errors.
    let mut config = AppConfig::default();

    // -- Discovery: max_depth --
    if let Some(depth) = raw.discovery.max_depth {
        if (1..=constants::ABSOLUTE_MAX_DEPTH).contains(&depth) {
            config.max_depth = depth;
        } else {
            warnings.push(format!(
                "[discovery] max_depth = {depth} is out of range (1-{}). Using default ({}).",
                constants::ABSOLUTE_MAX_DEPTH,
                constants::DEFAULT_MAX_DEPTH,
            ));
        }
    }

    // -- Discovery: max_files --
    if let Some(files) = raw.discovery.max_files {
        if (constants::MIN_MAX_FILES..=constants::ABSOLUTE_MAX_FILES).contains(&files) {
            config.max_files = files;
        } else {
            warnings.push(format!(
                "[discovery] max_files = {files} is out of range ({}-{}). Using default ({}).",
                constants::MIN_MAX_FILES,
                constants::ABSOLUTE_MAX_FILES,
                constants::DEFAULT_MAX_FILES,
            ));
        }
    }

    // -- UI: theme --
    if let Some(ref theme) = raw.ui.theme {
        match theme.to_lowercase().as_str() {
            "dark" => config.dark_mode = true,
            "light" => config.dark_mode = false,
            other => {
                warnings.push(format!(
                    "[ui] theme = \"{other}\" is not recognised. Expected \"dark\" or \"light\". Using default (dark).",
                ));
            }
        }
    }

    // -- UI: correlation_window_seconds --
    if let Some(secs) = raw.ui.correlation_window_seconds {
        if (constants::MIN_CORRELATION_WINDOW_SECS..=constants::MAX_CORRELATION_WINDOW_SECS)
            .contains(&secs)
        {
            config.correlation_window_secs = secs;
        } else {
            warnings.push(format!(
                "[ui] correlation_window_seconds = {secs} is out of range ({}-{}). Using default ({}).",
                constants::MIN_CORRELATION_WINDOW_SECS,
                constants::MAX_CORRELATION_WINDOW_SECS,
                constants::DEFAULT_CORRELATION_WINDOW_SECS,
            ));
        }
    }

    // -- UI: font_size --
    if let Some(size) = raw.ui.font_size {
        if (constants::MIN_FONT_SIZE..=constants::MAX_FONT_SIZE).contains(&size) {
            config.font_size = size;
        } else {
            warnings.push(format!(
                "[ui] font_size = {size} is out of range ({}-{}). Using default ({}).",
                constants::MIN_FONT_SIZE,
                constants::MAX_FONT_SIZE,
                constants::DEFAULT_FONT_SIZE,
            ));
        }
    }

    // -- Logging: level --
    if let Some(ref level) = raw.logging.level {
        let valid = ["error", "warn", "info", "debug", "trace"];
        if valid.contains(&level.to_lowercase().as_str()) {
            config.log_level = Some(level.clone());
        } else {
            warnings.push(format!(
                "[logging] level = \"{level}\" is not recognised. \
                 Valid values: error, warn, info, debug, trace. Using default (info).",
            ));
        }
    }

    // -- Logging: file --
    if let Some(ref file) = raw.logging.file {
        if !file.is_empty() {
            config.log_file = Some(file.clone());
        }
    }

    if !warnings.is_empty() {
        tracing::warn!(
            count = warnings.len(),
            "Config validation produced warnings"
        );
    }

    (config, warnings)
}
