// LogSleuth - app/session.rs
//
// Session persistence: save and restore the scan path, filter state,
// file colour assignments, and bookmarks between application restarts.
//
// Design principles:
// - Session is saved atomically (write→temp, rename→final) so a crash
//   during save never corrupts the previous good session.
// - Load errors are silently discarded (corrupt or incompatible sessions
//   just start the app fresh rather than surfacing errors to the user).
// - The data directory is created on first save; no user action required.
// - Entries are NOT persisted — log files are re-parsed on restore so the
//   timeline always reflects current file content including new lines.
//   This means bookmark IDs remain valid only if log file content is stable.

use crate::core::model::Severity;
use crate::util::constants::SESSION_FILE_NAME;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Version stamp for forward-compatibility checks.
///
/// Increment this constant whenever `SessionData` gains or removes fields
/// in a breaking way. Version mismatches silently discard the session.
pub const SESSION_VERSION: u32 = 1;

// =============================================================================
// On-disk data structures
// =============================================================================

/// Complete persistent session snapshot.
///
/// All fields are optional-friendly; deserialisation failures for individual
/// fields are handled by serde defaults so minor format additions are tolerated
/// without bumping the version.
#[derive(Debug, Serialize, Deserialize)]
pub struct SessionData {
    /// Schema version — must equal `SESSION_VERSION` to be accepted.
    pub version: u32,

    /// Directory that was scanned in the last session (`File > Open Directory`).
    /// Restored at startup to re-run the scan automatically.
    pub scan_path: Option<PathBuf>,

    /// Individual files loaded via `File > Add File(s)…` in the last session.
    /// Re-queued in append mode after the main scan_path scan completes.
    #[serde(default)]
    pub extra_files: Vec<PathBuf>,

    /// Filter state: the serialisable subset of `FilterState`.
    pub filter: PersistedFilter,

    /// Per-file palette colours as `(path, [r, g, b, a])` tuples so that
    /// the same colour stripe is used when the same files are re-scanned.
    #[serde(default)]
    pub file_colours: Vec<(PathBuf, [u8; 4])>,

    /// Bookmarks: `(entry_id, label)` pairs.
    ///
    /// Entry IDs are stable across restarts as long as file content has not
    /// changed before the previous entries (i.e. no log rotation or prepend).
    #[serde(default)]
    pub bookmarks: Vec<(u64, String)>,

    /// Correlation window size in seconds (restored but overlay starts disabled).
    #[serde(default = "default_correlation_window")]
    pub correlation_window_secs: i64,

    /// Date/time filter string typed by the user in the scan controls.
    ///
    /// Persisted so the filter is restored on next launch and the `initial_scan`
    /// re-run applies the same cutoff as the original scan.
    /// An empty string means no filter was set.
    #[serde(default)]
    pub discovery_date_input: String,
}

fn default_correlation_window() -> i64 {
    crate::util::constants::DEFAULT_CORRELATION_WINDOW_SECS
}

/// Serialisable snapshot of `FilterState`.
///
/// Only the user-visible, stable fields are persisted.  Runtime-only state
/// (`regex_search`, `bookmarked_ids`, `time_start`/`time_end`) is excluded
/// and re-derived on restore.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct PersistedFilter {
    /// Active severity level filter.  Empty = all severities shown.
    #[serde(default)]
    pub severity_levels: Vec<Severity>,

    /// Active source-file whitelist.  Empty = all files shown (unless
    /// `hide_all_sources` is true).
    #[serde(default)]
    pub source_files: Vec<PathBuf>,

    /// When true, ALL source files are hidden (explicit "none selected" state).
    #[serde(default)]
    pub hide_all_sources: bool,

    /// Text search term (exact or fuzzy depending on `fuzzy`).
    #[serde(default)]
    pub text_search: String,

    /// Raw regex pattern string.  Re-compiled on restore.
    #[serde(default)]
    pub regex_pattern: String,

    /// Whether fuzzy (subsequence) matching is active for `text_search`.
    #[serde(default)]
    pub fuzzy: bool,

    /// Relative time window in seconds.  `None` = no time filter.
    #[serde(default)]
    pub relative_time_secs: Option<u64>,

    /// Whether the bookmarks-only filter is active.
    #[serde(default)]
    pub bookmarks_only: bool,
}

// =============================================================================
// I/O helpers
// =============================================================================

/// Resolve the session file path from the platform data directory.
pub fn session_path(data_dir: &Path) -> PathBuf {
    data_dir.join(SESSION_FILE_NAME)
}

/// Save `data` to `path` atomically (write temp → rename).
///
/// Creates all parent directories as needed.  Returns a descriptive error
/// string suitable for a tracing warn! call; the caller decides whether to
/// surface it to the user (typically it is logged and ignored).
pub fn save(data: &SessionData, path: &Path) -> Result<(), String> {
    // Ensure the parent directory exists before writing.
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            format!(
                "cannot create session directory '{}': {e}",
                parent.display()
            )
        })?;
    }

    let json = serde_json::to_string_pretty(data)
        .map_err(|e| format!("failed to serialise session: {e}"))?;

    // Atomic write: write to a sibling temp file then rename.
    // A crash between write and rename loses the new session but never
    // corrupts the previous one (rename is atomic on all supported platforms).
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json.as_bytes())
        .map_err(|e| format!("failed to write session temp file '{}': {e}", tmp.display()))?;

    std::fs::rename(&tmp, path).map_err(|e| {
        // Clean up the temp file on failure; ignore any secondary error.
        let _ = std::fs::remove_file(&tmp);
        format!("failed to finalise session file '{}': {e}", path.display())
    })?;

    tracing::debug!(path = %path.display(), "Session saved");
    Ok(())
}

/// Load and validate a `SessionData` from `path`.
///
/// Returns `None` on any error (file not found, JSON parse failure,
/// version mismatch).  The caller should treat `None` as "start fresh".
pub fn load(path: &Path) -> Option<SessionData> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| {
            // Distinguish "file not found" (normal first run) from other errors.
            if e.kind() != std::io::ErrorKind::NotFound {
                tracing::debug!(path = %path.display(), error = %e, "Cannot read session file");
            }
        })
        .ok()?;

    let data: SessionData = serde_json::from_str(&content)
        .map_err(|e| {
            tracing::warn!(
                path = %path.display(),
                error = %e,
                "Session file is malformed — starting fresh"
            );
        })
        .ok()?;

    if data.version != SESSION_VERSION {
        tracing::warn!(
            found = data.version,
            expected = SESSION_VERSION,
            "Session file version mismatch — starting fresh"
        );
        return None;
    }

    tracing::info!(path = %path.display(), "Session file loaded");
    Some(data)
}

// =============================================================================
// Unit tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn sample_data() -> SessionData {
        SessionData {
            version: SESSION_VERSION,
            scan_path: Some(PathBuf::from("/tmp/logs")),
            extra_files: vec![PathBuf::from("/tmp/extra.log")],
            filter: PersistedFilter {
                text_search: "error".to_string(),
                fuzzy: true,
                relative_time_secs: Some(3600),
                ..Default::default()
            },
            file_colours: vec![(PathBuf::from("/tmp/logs/app.log"), [255, 128, 0, 255])],
            bookmarks: vec![(42, "important".to_string()), (100, String::new())],
            correlation_window_secs: 60,
            discovery_date_input: "2025-06-01 08:00:00".to_string(),
        }
    }

    /// Save and load must round-trip all fields accurately.
    #[test]
    fn test_session_save_load_round_trip() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("session.json");
        let original = sample_data();

        save(&original, &path).expect("save should succeed");
        let loaded = load(&path).expect("load should return Some after valid save");

        assert_eq!(loaded.version, SESSION_VERSION);
        assert_eq!(loaded.scan_path, original.scan_path);
        assert_eq!(loaded.extra_files, original.extra_files);
        assert_eq!(loaded.filter.text_search, "error");
        assert!(loaded.filter.fuzzy);
        assert_eq!(loaded.filter.relative_time_secs, Some(3600));
        assert_eq!(loaded.file_colours.len(), 1);
        assert_eq!(loaded.file_colours[0].1, [255, 128, 0, 255]);
        assert_eq!(loaded.bookmarks.len(), 2);
        assert_eq!(loaded.correlation_window_secs, 60);
        // Regression — Bug: discovery_date_input was not included in SessionData,
        // so the date filter was silently discarded on restart.
        assert_eq!(
            loaded.discovery_date_input, "2025-06-01 08:00:00",
            "discovery_date_input must survive a save/load round-trip"
        );
    }

    /// Load must return None when the file does not exist (first run).
    #[test]
    fn test_session_load_missing_file_returns_none() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("nonexistent.json");
        assert!(load(&path).is_none());
    }

    /// Load must return None when the JSON is malformed rather than panicking.
    #[test]
    fn test_session_load_malformed_json_returns_none() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("session.json");
        std::fs::write(&path, b"not valid json {{{{").unwrap();
        assert!(load(&path).is_none());
    }

    /// Load must return None when the version field is wrong.
    #[test]
    fn test_session_load_wrong_version_returns_none() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("session.json");
        let mut data = sample_data();
        data.version = 99;
        save(&data, &path).unwrap();
        // Manually patch the version so save() doesn't reject it during our write.
        // (save() writes whatever version we give it — validation is in load().)
        assert!(load(&path).is_none());
    }

    /// A crash during save (temp file exists) must not corrupt the original.
    #[test]
    fn test_session_save_atomic_does_not_corrupt_original() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("session.json");

        // Write an initial good session.
        let original = sample_data();
        save(&original, &path).unwrap();

        // Simulate a leftover temp file (e.g. from a previous crash).
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, b"garbage").unwrap();

        // Save a new session — should overwrite the temp file and rename correctly.
        let mut updated = sample_data();
        updated.correlation_window_secs = 120;
        save(&updated, &path).unwrap();

        let loaded = load(&path).unwrap();
        assert_eq!(loaded.correlation_window_secs, 120);
    }
}
