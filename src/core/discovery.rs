// LogSleuth - core/discovery.rs
//
// Recursive directory traversal and log file discovery.
//
// Architecture note: this module uses `walkdir` for directory traversal as an
// OS abstraction (similar to using std::path::Path). It reads only file
// *metadata* (size, mtime), never file *contents* -- that boundary is owned
// by the app layer (app::scan), which passes sample lines here for profile
// auto-detection.
//
// Rule 11 compliance:
//   - Per-file I/O errors are non-fatal and collected as warnings.
//   - max_files is enforced with an explicit named-constant upper bound.
//   - Exclude patterns short-circuit directory descent via filter_entry so
//     excluded subtrees (e.g. node_modules/) are never traversed at all.

use crate::core::model::DiscoveredFile;
use crate::util::error::DiscoveryError;
use chrono::{DateTime, Utc};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

// =============================================================================
// Configuration
// =============================================================================

/// Configuration for a discovery operation.
///
/// All limits reference named constants from `util::constants` so they are
/// auditable in a single place (DevWorkflow Part A Rule 11).
#[derive(Debug, Clone)]
pub struct DiscoveryConfig {
    /// Maximum directory recursion depth.
    pub max_depth: usize,

    /// Maximum number of matching files to return before stopping.
    pub max_files: usize,

    /// Glob patterns (filename-only) that a file MUST match to be included.
    /// An empty list means "include everything that is not excluded".
    pub include_patterns: Vec<String>,

    /// Glob patterns matched against filenames AND directory component names.
    /// Matching files are skipped; matching directories are not descended into.
    pub exclude_patterns: Vec<String>,

    /// File size (bytes) above which the `is_large` flag is set.
    pub large_file_threshold: u64,

    /// When set, only files whose last-modified timestamp is on or after this
    /// instant are included in the scan.  Files with no readable mtime are always
    /// included (fail-open) so permission-restricted metadata does not silently
    /// hide relevant log files.
    ///
    /// This field is always expressed in UTC.  Callers that work in local time
    /// should convert via `Local.from_local_datetime(&ndt).single().map(|d| d.to_utc())`
    /// so the comparison is correct for all timezones.
    pub modified_since: Option<DateTime<Utc>>,

    /// Maximum total log entries to load across all files.
    ///
    /// Once this cap is reached the scan stops ingesting further entries and
    /// emits a warning.  Decoupled from `max_files` so users can tune either
    /// limit independently (e.g. allow 5 000 files but cap entries at 100 000).
    pub max_total_entries: usize,

    /// Optional cancel flag.  When `Some`, the discovery loop checks this flag
    /// on every walker iteration and stops early (returning partial results) if
    /// it is set to `true`.  The caller (`app::scan::run_scan`) detects the
    /// cancel after `discover_files` returns and sends `ScanProgress::Cancelled`.
    ///
    /// `None` means no cancellation support (used in tests and when the caller
    /// does not need mid-discovery cancellation).
    pub cancel_flag: Option<Arc<AtomicBool>>,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        use crate::util::constants;
        Self {
            max_depth: constants::DEFAULT_MAX_DEPTH,
            max_files: constants::DEFAULT_MAX_FILES,
            include_patterns: constants::DEFAULT_INCLUDE_PATTERNS
                .iter()
                .map(|s| (*s).to_string())
                .collect(),
            exclude_patterns: constants::DEFAULT_EXCLUDE_PATTERNS
                .iter()
                .map(|s| (*s).to_string())
                .collect(),
            large_file_threshold: constants::DEFAULT_LARGE_FILE_THRESHOLD,
            modified_since: None,
            max_total_entries: constants::MAX_TOTAL_ENTRIES,
            cancel_flag: None,
        }
    }
}

// =============================================================================
// Discovery
// =============================================================================

/// Discover log files under `root`, applying include/exclude glob patterns.
///
/// # Progress reporting
/// `on_file_found` is called once per accepted file, receiving the file path
/// and the running count of files accepted so far. The callback should be cheap
/// (e.g. send a channel message); it is called on the caller's thread.
///
/// # Non-fatal errors
/// Files/directories that cannot be accessed due to permission or I/O errors
/// are recorded as human-readable strings in the returned warnings vector and
/// do NOT cause the function to return `Err`.
///
/// # Fatal errors
/// Returns `Err` only if the root path is invalid (`RootNotFound`,
/// `NotADirectory`).
pub fn discover_files<F>(
    root: &Path,
    config: &DiscoveryConfig,
    mut on_file_found: F,
) -> Result<(Vec<DiscoveredFile>, Vec<String>, usize), DiscoveryError>
where
    F: FnMut(&DiscoveredFile, usize),
{
    use crate::util::constants;

    // --- Pre-flight validation (Rule 17) ---
    // Run the metadata check on a background thread with a short timeout.
    // On a UNC/SMB path whose host is unreachable, `fs::metadata()` can block
    // for 30+ seconds while Windows retries the name-resolution / connect cycle.
    // Running it in a thread lets us enforce a wall-clock deadline (Rule 11).
    //
    // We use `fs::metadata()` rather than `Path::exists()` / `Path::is_dir()`
    // because those helpers map ALL errors — including PermissionDenied — to
    // `false`, making it impossible to distinguish an access-denied UNC path
    // from a path that genuinely does not exist.
    const PREFLIGHT_TIMEOUT_SECS: u64 = 10;
    {
        /// Outcome categories that need to be communicated back to the scan thread.
        enum PreflightResult {
            /// Path exists and is a directory — safe to proceed.
            IsDirectory,
            /// Path exists but is a regular file (or symlink) — not a directory.
            IsFile,
            /// Path does not exist (fs::metadata returned NotFound).
            NotFound,
            /// Path exists but access was denied (requires credentials).
            AccessDenied(std::io::Error),
            /// Other I/O error (e.g. invalid name, broken symlink); treated
            /// the same as not-found for user messaging purposes.
            OtherError,
        }

        let root_buf = root.to_path_buf();
        let (tx, rx) = std::sync::mpsc::channel::<PreflightResult>();
        std::thread::spawn(move || {
            let result = match std::fs::metadata(&root_buf) {
                Ok(meta) => {
                    if meta.is_dir() {
                        PreflightResult::IsDirectory
                    } else {
                        PreflightResult::IsFile
                    }
                }
                Err(e) => match e.kind() {
                    std::io::ErrorKind::PermissionDenied => PreflightResult::AccessDenied(e),
                    std::io::ErrorKind::NotFound => PreflightResult::NotFound,
                    _ => PreflightResult::OtherError,
                },
            };
            let _ = tx.send(result);
        });

        match rx.recv_timeout(std::time::Duration::from_secs(PREFLIGHT_TIMEOUT_SECS)) {
            Ok(PreflightResult::IsDirectory) => {} // root exists and is a directory — proceed
            Ok(PreflightResult::IsFile) => {
                return Err(DiscoveryError::NotADirectory {
                    path: root.to_path_buf(),
                });
            }
            Ok(PreflightResult::NotFound) | Ok(PreflightResult::OtherError) => {
                return Err(DiscoveryError::RootNotFound {
                    path: root.to_path_buf(),
                });
            }
            Ok(PreflightResult::AccessDenied(source)) => {
                return Err(DiscoveryError::PermissionDenied {
                    path: root.to_path_buf(),
                    source,
                });
            }
            Err(_) => {
                // Timed out or thread panicked — the host is likely
                // unreachable rather than the path not existing.  Surface a
                // specific timeout error so the user sees an actionable
                // message instead of "path does not exist" (Bug fix).
                tracing::warn!(
                    root = %root.display(),
                    timeout_secs = PREFLIGHT_TIMEOUT_SECS,
                    "Pre-flight path check timed out (unreachable UNC host?)"
                );
                return Err(DiscoveryError::Timeout {
                    path: root.to_path_buf(),
                    timeout_secs: PREFLIGHT_TIMEOUT_SECS,
                });
            }
        }
    }

    // Clamp config limits to absolute bounds (Rule 11 input validation).
    let max_files = config.max_files.min(constants::ABSOLUTE_MAX_FILES);
    let max_depth = config.max_depth.min(constants::ABSOLUTE_MAX_DEPTH);

    tracing::debug!(
        root = %root.display(),
        max_depth,
        max_files,
        include = ?config.include_patterns,
        exclude = ?config.exclude_patterns,
        "Discovery starting"
    );

    // Compile glob patterns once; log and skip any that fail compilation.
    let include_pats = compile_patterns(&config.include_patterns, "include");
    let exclude_pats = compile_patterns(&config.exclude_patterns, "exclude");

    let mut files: Vec<DiscoveredFile> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    // Build the walker. `filter_entry` short-circuits directory descent for
    // excluded directory names, so we never recurse into node_modules/.git/etc.
    let walker = walkdir::WalkDir::new(root)
        .max_depth(max_depth)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            // For directories: skip if the directory's own name matches an
            // exclude pattern that has no wildcards (e.g. "node_modules", ".git").
            // Wildcard patterns (e.g. "*.bak") are only tested against filenames.
            if e.file_type().is_dir() {
                let name = e.file_name().to_str().unwrap_or("");
                // Always allow the root itself
                if e.depth() == 0 {
                    return true;
                }
                return !is_excluded_component(name, &exclude_pats);
            }
            true // Visit files; we filter them individually below
        });

    for entry_result in walker {
        // Check cancel flag on every iteration so UNC / large tree scans can
        // be interrupted promptly without blocking until walkdir finishes.
        if config
            .cancel_flag
            .as_ref()
            .is_some_and(|f| f.load(Ordering::SeqCst))
        {
            tracing::debug!("Discovery cancelled by request");
            break;
        }

        let entry = match entry_result {
            Ok(e) => e,
            Err(e) => {
                // Inaccessible entry: non-fatal, record warning (Rule 11).
                let path_str = e
                    .path()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "<unknown>".to_string());
                let msg = format!("Cannot access '{path_str}': {e}");
                tracing::debug!(warning = %msg, "Discovery warning");
                warnings.push(msg);
                continue;
            }
        };

        // Skip directories (they are handled above by filter_entry).
        if entry.file_type().is_dir() {
            continue;
        }

        let path = entry.path();

        // Resolve the filename for pattern matching.
        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n,
            None => {
                warnings.push(format!("Skipping '{}': non-UTF-8 filename", path.display()));
                continue;
            }
        };

        // Apply exclude patterns to the filename itself (for *.gz, *.bak, etc.).
        if is_excluded_filename(file_name, &exclude_pats) {
            tracing::trace!(file = file_name, "Excluded by pattern");
            continue;
        }

        // Apply include patterns to the filename.
        if !is_included(file_name, &include_pats) {
            tracing::trace!(file = file_name, "Not matched by include patterns");
            continue;
        }

        // Collect file metadata.
        let metadata = match entry.metadata() {
            Ok(m) => m,
            Err(e) => {
                let msg = format!("Cannot read metadata for '{}': {e}", path.display());
                tracing::debug!(warning = %msg, "Discovery warning");
                warnings.push(msg);
                continue;
            }
        };

        let size = metadata.len();
        let modified: Option<DateTime<Utc>> = metadata.modified().ok().map(DateTime::<Utc>::from);
        let is_large = size >= config.large_file_threshold;

        // Apply the modification-date filter: skip files modified before the
        // requested date.  Files with no readable mtime are included (fail-open).
        if let Some(since) = config.modified_since {
            if let Some(mtime) = modified {
                if mtime < since {
                    tracing::trace!(
                        file = %path.display(),
                        mtime = %mtime,
                        since = %since,
                        "Skipped: modified before date filter"
                    );
                    continue;
                }
            }
        }

        if is_large {
            tracing::debug!(
                file = %path.display(),
                size_mb = size / (1024 * 1024),
                "Large file flagged"
            );
        }

        let discovered = DiscoveredFile {
            path: path.to_path_buf(),
            size,
            modified,
            // profile_id and detection_confidence are filled in by app::scan
            // after reading sample lines for auto-detection.
            profile_id: None,
            detection_confidence: 0.0,
            is_large,
        };

        let count = files.len() + 1;
        on_file_found(&discovered, count);
        files.push(discovered);
    }

    let total_found = files.len();

    // If more files were found than the configured limit, keep only the
    // `max_files` most recently modified ones so the user always sees the
    // freshest content rather than an arbitrary subset.
    if total_found > max_files {
        // Sort descending by modification time (None floats to the end so
        // files without an mtime are considered oldest and dropped first).
        files.sort_unstable_by(|a, b| match (b.modified, a.modified) {
            (Some(bm), Some(am)) => bm.cmp(&am),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        });
        files.truncate(max_files);

        warnings.push(format!(
            "{total_found} log files were found but the ingest limit is {max_files}. \
             Only the {max_files} most recently modified files have been loaded. \
             Raise the limit in Options if you need more."
        ));

        tracing::info!(
            total_found,
            limit = max_files,
            "File list truncated to most recently modified files"
        );
    }

    tracing::debug!(
        total_found,
        files_loaded = files.len(),
        warnings = warnings.len(),
        "Discovery complete"
    );

    // Third element: total files found before the limit was applied.
    Ok((files, warnings, total_found))
}

// =============================================================================
// Glob helpers
// =============================================================================

/// Compile a list of glob pattern strings into `glob::Pattern` objects.
/// Patterns that fail to compile are logged as warnings and skipped.
fn compile_patterns(patterns: &[String], kind: &str) -> Vec<glob::Pattern> {
    patterns
        .iter()
        .filter_map(|p| match glob::Pattern::new(p) {
            Ok(compiled) => Some(compiled),
            Err(e) => {
                tracing::warn!(pattern = p, kind, error = %e, "Invalid glob pattern, skipping");
                None
            }
        })
        .collect()
}

/// Returns true if `dir_name` matches any exclude pattern that contains no
/// wildcard characters. These are treated as directory component exclusions
/// (e.g. "node_modules", ".git") rather than filename glob patterns.
fn is_excluded_component(dir_name: &str, exclude_pats: &[glob::Pattern]) -> bool {
    exclude_pats.iter().any(|p| {
        let s = p.as_str();
        // Only literal patterns (no wildcards) are used as component matchers.
        !s.contains('*') && !s.contains('?') && !s.contains('[') && p.matches(dir_name)
    })
}

/// Returns true if `file_name` matches any exclude pattern (wildcard or literal).
fn is_excluded_filename(file_name: &str, exclude_pats: &[glob::Pattern]) -> bool {
    exclude_pats.iter().any(|p| p.matches(file_name))
}

/// Returns true if `file_name` matches at least one include pattern.
/// An empty include list means "include all" (returns true).
fn is_included(file_name: &str, include_pats: &[glob::Pattern]) -> bool {
    if include_pats.is_empty() {
        return true;
    }
    include_pats.iter().any(|p| p.matches(file_name))
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_temp_tree() -> TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();

        // Normal log files
        fs::write(root.join("app.log"), "[2024-01-01 12:00:00] Info Hello\n")
            .expect("write app.log");
        fs::write(
            root.join("service.log"),
            "[2024-01-01 12:00:01] Error Oops\n",
        )
        .expect("write service.log");
        fs::write(root.join("readme.txt"), "Just a readme\n").expect("write readme.txt");

        // Excluded file
        fs::write(root.join("backup.log.gz"), "binary").expect("write .gz");

        // Subdirectory
        let sub = root.join("subdir");
        fs::create_dir(&sub).expect("mkdir subdir");
        fs::write(sub.join("sub.log"), "[2024-01-01 12:00:02] Debug Detail\n")
            .expect("write sub.log");

        // Excluded directory
        let node = root.join("node_modules");
        fs::create_dir(&node).expect("mkdir node_modules");
        fs::write(node.join("module.log"), "should be excluded\n").expect("write module.log");

        dir
    }

    #[test]
    fn test_discovers_log_files() {
        let dir = make_temp_tree();
        let config = DiscoveryConfig::default();
        let (files, warnings, _) = discover_files(dir.path(), &config, |_, _| {}).unwrap();

        // Should find app.log, service.log, readme.txt, sub/sub.log
        // NOT backup.log.gz, NOT node_modules/module.log
        let paths: Vec<_> = files
            .iter()
            .map(|f| f.path.file_name().unwrap().to_str().unwrap().to_string())
            .collect();
        assert!(
            paths.contains(&"app.log".to_string()),
            "expected app.log, got {paths:?}"
        );
        assert!(paths.contains(&"service.log".to_string()));
        assert!(paths.contains(&"sub.log".to_string()));
        assert!(
            !paths.contains(&"backup.log.gz".to_string()),
            "gz should be excluded"
        );
        assert!(
            !paths.contains(&"module.log".to_string()),
            "node_modules should be excluded"
        );
        assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    }

    #[test]
    fn test_max_depth_zero_finds_no_files() {
        let dir = make_temp_tree();
        // max_depth = 0 means only the root dir entry, no files.
        let config = DiscoveryConfig {
            max_depth: 0,
            ..Default::default()
        };
        let (files, _, _) = discover_files(dir.path(), &config, |_, _| {}).unwrap();
        assert_eq!(files.len(), 0);
    }

    #[test]
    fn test_max_depth_1_excludes_subdirs() {
        let dir = make_temp_tree();
        let config = DiscoveryConfig {
            max_depth: 1, // root files only, no subdirectory descent
            ..Default::default()
        };
        let (files, _, _) = discover_files(dir.path(), &config, |_, _| {}).unwrap();
        let paths: Vec<_> = files
            .iter()
            .map(|f| f.path.file_name().unwrap().to_str().unwrap().to_string())
            .collect();
        assert!(
            !paths.contains(&"sub.log".to_string()),
            "sub.log should be excluded at depth 1"
        );
    }

    /// When more files are found than `max_files`, discovery must succeed (not
    /// error), return exactly `max_files` entries, include them in a warning,
    /// and expose the raw `total_found` count via the third tuple element.
    #[test]
    fn test_max_files_truncates_gracefully() {
        let dir = make_temp_tree(); // creates 4 matching files
        let config = DiscoveryConfig {
            max_files: 2,
            ..Default::default()
        };
        let (files, warnings, total_found) =
            discover_files(dir.path(), &config, |_, _| {}).unwrap();
        assert_eq!(files.len(), 2, "should return exactly max_files entries");
        assert_eq!(
            total_found, 4,
            "total_found should count all 4 matching files"
        );
        assert!(
            !warnings.is_empty(),
            "a truncation warning must be emitted when files are dropped"
        );
        // Verify the warning mentions both the total and the limit.
        let warning_text = warnings.join(" ");
        assert!(
            warning_text.contains('4') && warning_text.contains('2'),
            "warning should mention total and limit, got: {warning_text}"
        );
    }

    #[test]
    fn test_root_not_found() {
        let result = discover_files(
            Path::new("/nonexistent/path/logsleuth"),
            &DiscoveryConfig::default(),
            |_, _| {},
        );
        assert!(matches!(result, Err(DiscoveryError::RootNotFound { .. })));
    }

    #[test]
    fn test_root_not_a_directory() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("not_a_dir.log");
        fs::write(&file, "content").unwrap();
        let result = discover_files(&file, &DiscoveryConfig::default(), |_, _| {});
        assert!(matches!(result, Err(DiscoveryError::NotADirectory { .. })));
    }

    #[test]
    fn test_progress_callback_called_for_each_file() {
        let dir = make_temp_tree();
        let config = DiscoveryConfig::default();
        let mut callback_count = 0usize;
        let (files, _, _) = discover_files(dir.path(), &config, |_, _| {
            callback_count += 1;
        })
        .unwrap();
        assert_eq!(
            callback_count,
            files.len(),
            "callback should fire for each accepted file"
        );
    }

    #[test]
    fn test_is_large_flag() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("tiny.log"), "x").unwrap();

        let config = DiscoveryConfig {
            large_file_threshold: 999_999_999, // absurdly large so no real file qualifies
            ..Default::default()
        };
        let (files, _, _) = discover_files(dir.path(), &config, |_, _| {}).unwrap();
        assert!(
            !files[0].is_large,
            "tiny.log should not be flagged as large"
        );

        let config2 = DiscoveryConfig {
            large_file_threshold: 0, // everything is large
            ..Default::default()
        };
        let (files2, _, _) = discover_files(dir.path(), &config2, |_, _| {}).unwrap();
        assert!(
            files2[0].is_large,
            "all files should be large with threshold=0"
        );
    }

    #[test]
    fn test_file_metadata_collected() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("meta.log"), "hello world").unwrap();
        let (files, _, _) =
            discover_files(dir.path(), &DiscoveryConfig::default(), |_, _| {}).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].size, 11, "size should match 'hello world'");
        assert!(files[0].modified.is_some(), "modified time should be set");
        assert!(
            files[0].profile_id.is_none(),
            "profile_id is filled by app layer"
        );
    }
}
