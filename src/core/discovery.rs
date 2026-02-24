// LogSleuth - core/discovery.rs
//
// Recursive directory traversal and log file discovery.
// Core layer: operates on abstract path inputs, no direct filesystem access.
// The app layer provides the actual directory listing via platform::fs.
//
// Implementation: next increment.

use crate::core::model::DiscoveredFile;
use crate::util::error::DiscoveryError;
use std::path::Path;

/// Configuration for a discovery operation.
#[derive(Debug, Clone)]
pub struct DiscoveryConfig {
    pub max_depth: usize,
    pub max_files: usize,
    pub include_patterns: Vec<String>,
    pub exclude_patterns: Vec<String>,
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

/// Discover log files in the given root directory.
///
/// Returns a list of discovered files with metadata, or an error if the
/// root path is invalid.
///
/// Files that cannot be accessed (permissions, locks) are reported as
/// warnings in the returned warnings vec, not as fatal errors.
pub fn discover_files(
    root: &Path,
    config: &DiscoveryConfig,
) -> Result<(Vec<DiscoveredFile>, Vec<String>), DiscoveryError> {
    // Validate root path exists and is a directory
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

    // TODO: Implement recursive traversal with walkdir in next increment.
    // For now, return empty results (the app layer handles this gracefully).
    tracing::debug!(
        root = %root.display(),
        max_depth = config.max_depth,
        max_files = config.max_files,
        "Discovery started (stub)"
    );

    Ok((Vec::new(), Vec::new()))
}
