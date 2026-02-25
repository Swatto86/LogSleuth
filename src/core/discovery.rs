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
/// `NotADirectory`) or the `max_files` limit is exceeded.
pub fn discover_files<F>(
    root: &Path,
    config: &DiscoveryConfig,
    mut on_file_found: F,
) -> Result<(Vec<DiscoveredFile>, Vec<String>), DiscoveryError>
where
    F: FnMut(&Path, usize),
{
    use crate::util::constants;

    // --- Pre-flight validation (Rule 17) ---
    if !root.exists() {
        return Err(DiscoveryError::RootNotFound {
            path: root.to_path_buf(),
        });
    }
    if !root.is_dir() {
        return Err(DiscoveryError::NotADirectory {
            path: root.to_path_buf(),
        });
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

        // Enforce max_files *before* adding to the list.
        if files.len() >= max_files {
            return Err(DiscoveryError::MaxFilesExceeded { max: max_files });
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
        on_file_found(path, count);
        files.push(discovered);
    }

    tracing::debug!(
        files_found = files.len(),
        warnings = warnings.len(),
        "Discovery complete"
    );

    Ok((files, warnings))
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
        let (files, warnings) = discover_files(dir.path(), &config, |_, _| {}).unwrap();

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
        let (files, _) = discover_files(dir.path(), &config, |_, _| {}).unwrap();
        assert_eq!(files.len(), 0);
    }

    #[test]
    fn test_max_depth_1_excludes_subdirs() {
        let dir = make_temp_tree();
        let config = DiscoveryConfig {
            max_depth: 1, // root files only, no subdirectory descent
            ..Default::default()
        };
        let (files, _) = discover_files(dir.path(), &config, |_, _| {}).unwrap();
        let paths: Vec<_> = files
            .iter()
            .map(|f| f.path.file_name().unwrap().to_str().unwrap().to_string())
            .collect();
        assert!(
            !paths.contains(&"sub.log".to_string()),
            "sub.log should be excluded at depth 1"
        );
    }

    #[test]
    fn test_max_files_exceeded() {
        let dir = make_temp_tree();
        let config = DiscoveryConfig {
            max_files: 1,
            ..Default::default()
        };
        let result = discover_files(dir.path(), &config, |_, _| {});
        assert!(matches!(
            result,
            Err(DiscoveryError::MaxFilesExceeded { .. })
        ));
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
        let (files, _) = discover_files(dir.path(), &config, |_, _| {
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
        let (files, _) = discover_files(dir.path(), &config, |_, _| {}).unwrap();
        assert!(
            !files[0].is_large,
            "tiny.log should not be flagged as large"
        );

        let config2 = DiscoveryConfig {
            large_file_threshold: 0, // everything is large
            ..Default::default()
        };
        let (files2, _) = discover_files(dir.path(), &config2, |_, _| {}).unwrap();
        assert!(
            files2[0].is_large,
            "all files should be large with threshold=0"
        );
    }

    #[test]
    fn test_file_metadata_collected() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("meta.log"), "hello world").unwrap();
        let (files, _) =
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
