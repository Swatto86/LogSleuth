// LogSleuth - app/dir_watcher.rs
//
// Recursive directory watcher: polls the scan directory on a background thread
// and reports new log files that appear after the initial scan.
//
// Architecture:
//   - `DirWatcher` lives on the UI thread; `run_dir_watcher` executes on a
//     background thread polling the directory on a fixed interval.
//   - An `Arc<AtomicBool>` cancel flag allows the UI to stop the watcher.
//   - New file paths are sent as `DirWatchProgress::NewFiles` over an mpsc channel.
//   - The UI thread polls the channel each frame (same pattern as TailManager).
//
// Rule 11 compliance:
//   - Per-entry directory walk errors are non-fatal and skipped via `.flatten()`.
//   - The poll loop sleeps in sub-intervals so cancel is checked promptly
//     (within DIR_WATCH_CANCEL_CHECK_INTERVAL_MS of the cancel flag being set).
//   - `known_paths` is updated immediately after each `NewFiles` send so
//     subsequent polls do not re-report the same files.

use crate::core::model::DirWatchProgress;
use crate::util::constants::DIR_WATCH_CANCEL_CHECK_INTERVAL_MS;
use glob::Pattern;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Duration;

// =============================================================================
// Watch configuration
// =============================================================================

/// Lightweight configuration for the directory watcher.
///
/// Mirrors the relevant subset of `core::discovery::DiscoveryConfig` so the
/// watcher uses the same file-matching rules as the initial scan.
#[derive(Debug, Clone)]
pub struct DirWatchConfig {
    /// Glob patterns (filename only) that a file must match to be reported.
    pub include_patterns: Vec<String>,
    /// Glob patterns (filename or directory name) of paths to skip.
    pub exclude_patterns: Vec<String>,
    /// Maximum directory recursion depth (matches initial scan depth).
    pub max_depth: usize,
    /// How often to poll the directory tree for new files (ms).
    pub poll_interval_ms: u64,
}

impl Default for DirWatchConfig {
    fn default() -> Self {
        use crate::util::constants;
        Self {
            include_patterns: constants::DEFAULT_INCLUDE_PATTERNS
                .iter()
                .map(|s| (*s).to_string())
                .collect(),
            exclude_patterns: constants::DEFAULT_EXCLUDE_PATTERNS
                .iter()
                .map(|s| (*s).to_string())
                .collect(),
            max_depth: constants::DEFAULT_MAX_DEPTH,
            poll_interval_ms: constants::DIR_WATCH_POLL_INTERVAL_MS,
        }
    }
}

// =============================================================================
// DirWatcher
// =============================================================================

/// Manages a background directory polling watcher.
///
/// Mirrors the lifecycle interface of `TailManager`:
/// `start_watch`, `stop_watch`, and `poll_progress` for the periodic
/// repaint loop in `gui.rs`.
pub struct DirWatcher {
    /// Channel receiver for the UI to poll directory watch progress messages.
    pub progress_rx: Option<mpsc::Receiver<DirWatchProgress>>,
    /// Cancel flag shared with the background thread.
    cancel_flag: Option<Arc<AtomicBool>>,
}

impl DirWatcher {
    /// Create an inactive watcher.  No thread is started until `start_watch`.
    pub fn new() -> Self {
        Self {
            progress_rx: None,
            cancel_flag: None,
        }
    }

    /// Returns `true` if a watcher thread is currently running.
    pub fn is_active(&self) -> bool {
        self.cancel_flag
            .as_ref()
            .map(|f| !f.load(Ordering::Relaxed))
            .unwrap_or(false)
    }

    /// Start watching `root` for new files.
    ///
    /// `known_paths` is the set of file paths already loaded by the initial scan.
    /// Any path discovered during polling that is **not** in this set is reported
    /// as a `DirWatchProgress::NewFiles` message.
    ///
    /// Calling `start_watch` while a watcher is already running stops the
    /// previous watcher first to avoid duplicate notification channels.
    pub fn start_watch(
        &mut self,
        root: PathBuf,
        known_paths: HashSet<PathBuf>,
        config: DirWatchConfig,
    ) {
        // Stop any existing watcher before starting a new one.
        self.stop_watch();

        let cancel = Arc::new(AtomicBool::new(false));
        self.cancel_flag = Some(Arc::clone(&cancel));

        let (tx, rx) = mpsc::channel();
        self.progress_rx = Some(rx);

        std::thread::spawn(move || {
            run_dir_watcher(root, known_paths, config, tx, cancel);
        });

        tracing::debug!("Directory watcher started");
    }

    /// Signal the background watcher thread to stop and clean up handles.
    pub fn stop_watch(&mut self) {
        if let Some(flag) = self.cancel_flag.take() {
            flag.store(true, Ordering::Relaxed);
        }
        self.progress_rx = None;
    }

    /// Drain all pending messages from the background thread without blocking.
    ///
    /// Returns the accumulated messages since the last call.
    /// Returns an empty `Vec` when the watcher is inactive.
    pub fn poll_progress(&mut self) -> Vec<DirWatchProgress> {
        let Some(rx) = &self.progress_rx else {
            return Vec::new();
        };
        let mut messages = Vec::new();
        loop {
            match rx.try_recv() {
                Ok(msg) => messages.push(msg),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    // Background thread exited; clean up handles.
                    self.progress_rx = None;
                    self.cancel_flag = None;
                    break;
                }
            }
        }
        messages
    }
}

impl Default for DirWatcher {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Background thread
// =============================================================================

/// Entry point for the background directory polling thread.
///
/// Polls `root` every `DIR_WATCH_POLL_INTERVAL_MS` ms, looking for files that
/// match the include patterns and are **not** in `known_paths` yet.
/// New files are sent via `tx` and immediately added to `known_paths` so they
/// are not reported again on the next poll cycle.
fn run_dir_watcher(
    root: PathBuf,
    mut known_paths: HashSet<PathBuf>,
    config: DirWatchConfig,
    tx: mpsc::Sender<DirWatchProgress>,
    cancel: Arc<AtomicBool>,
) {
    // Compile glob patterns once for the lifetime of this watcher.
    let include_pats: Vec<Pattern> = config
        .include_patterns
        .iter()
        .filter_map(|p| Pattern::new(p).ok())
        .collect();
    let exclude_pats: Vec<Pattern> = config
        .exclude_patterns
        .iter()
        .filter_map(|p| Pattern::new(p).ok())
        .collect();

    let poll_interval = Duration::from_millis(config.poll_interval_ms);
    let cancel_check = Duration::from_millis(DIR_WATCH_CANCEL_CHECK_INTERVAL_MS);
    // Number of cancel-check sub-sleeps that make up one full poll interval.
    let sub_iters: u32 = (poll_interval.as_millis() / cancel_check.as_millis()).max(1) as u32;

    tracing::debug!(
        root = %root.display(),
        known = known_paths.len(),
        "Directory watcher thread running"
    );

    loop {
        // Sleep in small sub-intervals so cancellation is detected promptly.
        for _ in 0..sub_iters {
            if cancel.load(Ordering::Relaxed) {
                tracing::debug!("Directory watcher thread: cancel flag set, exiting");
                return;
            }
            std::thread::sleep(cancel_check);
        }

        if cancel.load(Ordering::Relaxed) {
            return;
        }

        // Walk the directory and collect files not yet in `known_paths`.
        let new_files = walk_for_new_files(
            &root,
            &known_paths,
            &include_pats,
            &exclude_pats,
            config.max_depth,
        );

        if !new_files.is_empty() {
            tracing::debug!(
                count = new_files.len(),
                "Directory watcher: new files detected"
            );
            // Update known_paths before sending so the next poll cycle won't
            // re-report the same files even if the channel is slow to drain.
            for p in &new_files {
                known_paths.insert(p.clone());
            }
            if tx.send(DirWatchProgress::NewFiles(new_files)).is_err() {
                // UI thread dropped the receiver — exit cleanly.
                tracing::debug!("Directory watcher: receiver dropped, exiting");
                return;
            }
        }
    }
}

/// Walk `root` up to `max_depth` levels and return all regular files that:
/// 1. Are not already in `known_paths`.
/// 2. Match at least one include pattern (or the include list is empty).
/// 3. Do not match any exclude pattern.
///
/// Per-entry I/O errors are silently skipped (non-fatal per Rule 11).
fn walk_for_new_files(
    root: &Path,
    known_paths: &HashSet<PathBuf>,
    include_pats: &[Pattern],
    exclude_pats: &[Pattern],
    max_depth: usize,
) -> Vec<PathBuf> {
    let mut found = Vec::new();

    let walker = walkdir::WalkDir::new(root)
        .max_depth(max_depth)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            // Short-circuit: never descend into excluded directories,
            // skipping their entire subtree in a single filter_entry call.
            let name = e.file_name().to_string_lossy();
            !exclude_pats.iter().any(|p| p.matches(&name))
        });

    for entry in walker.flatten() {
        if !entry.file_type().is_file() {
            continue;
        }

        let path = entry.path().to_path_buf();

        // Skip already-known files.
        if known_paths.contains(&path) {
            continue;
        }

        // Must match at least one include pattern (empty list = accept all).
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        let matches_include =
            include_pats.is_empty() || include_pats.iter().any(|p| p.matches(&file_name));

        if matches_include {
            found.push(path);
        }
    }

    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Verify that walk_for_new_files finds a newly created file that was not in
    /// the known set, and does not re-report files that are already known.
    #[test]
    fn test_walk_finds_new_file_and_skips_known() {
        let dir = TempDir::new().expect("tmpdir");
        let new_path = dir.path().join("app.log");
        fs::write(&new_path, b"hello").expect("write");

        let include = vec![Pattern::new("*.log").unwrap()];
        let exclude: Vec<Pattern> = vec![];
        let known = HashSet::new();

        let found = walk_for_new_files(dir.path(), &known, &include, &exclude, 5);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0], new_path);

        // If the file is already known it must not be reported again.
        let mut known2 = HashSet::new();
        known2.insert(new_path.clone());
        let found2 = walk_for_new_files(dir.path(), &known2, &include, &exclude, 5);
        assert!(found2.is_empty());
    }

    /// Files that do not match the include pattern must be ignored.
    #[test]
    fn test_walk_respects_include_patterns() {
        let dir = TempDir::new().expect("tmpdir");
        fs::write(dir.path().join("app.log"), b"").expect("write");
        fs::write(dir.path().join("readme.txt"), b"").expect("write");

        let include = vec![Pattern::new("*.log").unwrap()];
        let exclude: Vec<Pattern> = vec![];
        let known = HashSet::new();

        let found = walk_for_new_files(dir.path(), &known, &include, &exclude, 5);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].file_name().unwrap().to_str().unwrap(), "app.log");
    }

    /// DirWatcher: start and stop without panicking.
    #[test]
    fn test_dir_watcher_start_stop() {
        let dir = TempDir::new().expect("tmpdir");
        let mut watcher = DirWatcher::new();
        assert!(!watcher.is_active());

        watcher.start_watch(
            dir.path().to_path_buf(),
            HashSet::new(),
            DirWatchConfig::default(),
        );
        assert!(watcher.is_active());

        watcher.stop_watch();
        // is_active may transiently return true until the thread checks the flag;
        // just verify stop_watch doesn't panic.
    }
}
