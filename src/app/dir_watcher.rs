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
//   - mtime changes to existing files are sent as `DirWatchProgress::FileMtimeUpdates`
//     so the UI can show a live "last modified" time for each file.
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
use chrono::{DateTime, Utc};
use glob::Pattern;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, SystemTime};

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
    /// When `Some`, only newly-discovered files whose OS last-modified time is
    /// on or after this instant are reported.
    ///
    /// Mirrors `DiscoveryConfig::modified_since` so the watcher honours the
    /// same date filter that was active during the initial directory scan.  If
    /// the user set a "files modified on or after YYYY-MM-DD" filter when they
    /// opened the directory, the watcher must apply the same gate — otherwise
    /// older files that happen to be created or renamed into the directory after
    /// the scan would slip through.
    ///
    /// Fail-open: files whose mtime cannot be read are always included so that
    /// a permission-restricted metadata call never silently hides a log file.
    pub modified_since: Option<DateTime<Utc>>,
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
            modified_since: None,
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

    /// Drain at most `max` pending messages from the background thread without
    /// blocking.  Any messages beyond the budget remain in the channel and are
    /// delivered on the next call (Rule 11: per-frame drain budget).
    /// Returns an empty `Vec` when the watcher is inactive.
    pub fn poll_progress(&mut self, max: usize) -> Vec<DirWatchProgress> {
        let Some(rx) = &self.progress_rx else {
            return Vec::new();
        };
        let mut messages = Vec::with_capacity(max.min(8));
        loop {
            if messages.len() >= max {
                break;
            }
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
    // The quotient is a u128; clamp to u32::MAX before casting so an extreme
    // (misconfigured) poll interval cannot silently truncate to a tiny loop
    // count (Rule 2: no silent truncation via `as` on fallible casts).
    let sub_iters: u32 = u32::try_from(
        (poll_interval.as_millis() / cancel_check.as_millis())
            .max(1)
            .min(u32::MAX as u128),
    )
    .unwrap_or(u32::MAX);

    tracing::debug!(
        root = %root.display(),
        known = known_paths.len(),
        "Directory watcher thread running"
    );

    // Seed initial mtimes for all known files so the first poll compares
    // against the scan-time state rather than treating everything as changed.
    // Files whose metadata cannot be read are skipped (fail-open: they will
    // be picked up on the next poll if they become readable).
    let mut tracked_mtimes: HashMap<PathBuf, SystemTime> = known_paths
        .iter()
        .filter_map(|p| {
            std::fs::metadata(p)
                .ok()
                .and_then(|m| m.modified().ok())
                .map(|t| (p.clone(), t))
        })
        .collect();

    // Receiver for the currently in-flight walk sub-thread, if any.
    //
    // Only ONE walk sub-thread is allowed at a time to prevent thread
    // accumulation on slow UNC/SMB shares where each walkdir call can block
    // for tens of seconds per entry.  A new walk is only spawned after the
    // previous one has delivered its result (or the disconnected signal).
    // Using `try_recv` here keeps the mtime-tracking loop below running every
    // 2 s regardless of how long the directory walk takes.
    let mut walk_in_flight: Option<mpsc::Receiver<Vec<PathBuf>>> = None;

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

        // ---------------------------------------------------------------
        // New-file discovery via a persistent walk sub-thread.
        //
        // Pattern:
        //   1. If no walk is in flight, snapshot known_paths and spawn one.
        //   2. Poll the in-flight receiver non-blockingly (try_recv).
        //   3. If the result is ready, process it; if not, skip new-file
        //      detection this cycle and continue to the mtime loop below.
        //
        // This guarantees at most one walk thread at a time, preventing
        // thread accumulation on slow UNC shares where the OS can stall a
        // walkdir call for 30-60 s per directory entry.  The mtime loop
        // below is unaffected and always runs on every 2 s cycle.
        // ---------------------------------------------------------------
        if walk_in_flight.is_none() {
            let known_snap = known_paths.clone();
            let root_owned = root.clone();
            let include_owned = include_pats.clone();
            let exclude_owned = exclude_pats.clone();
            let max_depth = config.max_depth;
            let modified_since = config.modified_since;
            let (walk_tx, walk_rx) = mpsc::channel::<Vec<PathBuf>>();
            std::thread::spawn(move || {
                let found = walk_for_new_files(
                    &root_owned,
                    &known_snap,
                    &include_owned,
                    &exclude_owned,
                    max_depth,
                    modified_since,
                );
                // If the receiver was already dropped (watcher stopped), the
                // send fails silently — the thread exits cleanly on return.
                let _ = walk_tx.send(found);
            });
            walk_in_flight = Some(walk_rx);
            // Notify the UI that a walk cycle has started so it can show a
            // "scanning for new files..." indicator in the status bar.
            if tx.send(DirWatchProgress::WalkStarted).is_err() {
                return;
            }
        }

        let new_files = match walk_in_flight.as_ref().map(|rx| rx.try_recv()) {
            Some(Ok(files)) => {
                // Walk finished — clear the in-flight slot so the next cycle
                // starts a fresh walk with the updated known_paths.
                let new_count = files.len();
                walk_in_flight = None;
                // Always notify the UI that the walk is done so it can clear
                // the "scanning..." indicator even when no new files were found.
                if tx
                    .send(DirWatchProgress::WalkComplete { new_count })
                    .is_err()
                {
                    return;
                }
                files
            }
            Some(Err(mpsc::TryRecvError::Empty)) => {
                // Walk still running — nothing to process this cycle.
                Vec::new()
            }
            Some(Err(mpsc::TryRecvError::Disconnected)) | None => {
                // Thread exited without sending (shouldn't happen but handle
                // gracefully: clear slot so a fresh walk starts next cycle).
                walk_in_flight = None;
                Vec::new()
            }
        };

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

        // ------------------------------------------------------------------
        // mtime tracking: stat every known file and report any that have a
        // newer modification timestamp than last seen.
        //
        // `tracked_mtimes.entry().or_insert(mtime)` handles new files that
        // were just added to `known_paths` above: their entry is seeded with
        // the current mtime so the NEXT poll is the baseline, preventing a
        // spurious "changed" event on the same cycle they were first detected.
        //
        // Per-file stat errors are silently skipped (Rule 11: non-fatal).
        // ------------------------------------------------------------------
        let mut mtime_updates: Vec<(PathBuf, DateTime<Utc>)> = Vec::new();
        for path in &known_paths {
            match std::fs::metadata(path).and_then(|m| m.modified()) {
                Ok(mtime) => {
                    let entry = tracked_mtimes.entry(path.clone()).or_insert(mtime);
                    if *entry != mtime {
                        *entry = mtime;
                        let dt = DateTime::<Utc>::from(mtime);
                        mtime_updates.push((path.clone(), dt));
                    }
                }
                Err(_) => {
                    // Cannot stat; skip quietly — will be retried next cycle.
                }
            }
        }
        if !mtime_updates.is_empty() {
            tracing::debug!(
                count = mtime_updates.len(),
                "Directory watcher: file mtime changes detected"
            );
            if tx
                .send(DirWatchProgress::FileMtimeUpdates(mtime_updates))
                .is_err()
            {
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
/// 4. Have an OS last-modified time on or after `modified_since` (if set).
///    Files whose mtime cannot be read are included (fail-open) so that a
///    permission-restricted metadata call never silently hides a log file.
///
/// Per-entry I/O errors are silently skipped (non-fatal per Rule 11).
fn walk_for_new_files(
    root: &Path,
    known_paths: &HashSet<PathBuf>,
    include_pats: &[Pattern],
    exclude_pats: &[Pattern],
    max_depth: usize,
    modified_since: Option<DateTime<Utc>>,
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

        if !matches_include {
            continue;
        }

        // Apply the modification-date filter: skip files modified before the
        // requested start date.  Mirrors the identical check in
        // `core::discovery::discover_files` so the watcher never adds a file
        // that the initial scan would have rejected.  Fail-open when mtime is
        // unreadable so permission errors do not silently suppress log files.
        if let Some(since) = modified_since {
            let mtime: Option<DateTime<Utc>> = entry
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .map(DateTime::<Utc>::from);
            if let Some(t) = mtime {
                if t < since {
                    tracing::trace!(
                        file = %path.display(),
                        mtime = %t,
                        since = %since,
                        "Dir-watcher: skipped new file (modified before date filter)"
                    );
                    continue;
                }
            }
        }

        found.push(path);
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

        let found = walk_for_new_files(dir.path(), &known, &include, &exclude, 5, None);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0], new_path);

        // If the file is already known it must not be reported again.
        let mut known2 = HashSet::new();
        known2.insert(new_path.clone());
        let found2 = walk_for_new_files(dir.path(), &known2, &include, &exclude, 5, None);
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

        let found = walk_for_new_files(dir.path(), &known, &include, &exclude, 5, None);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].file_name().unwrap().to_str().unwrap(), "app.log");
    }

    /// Newly-discovered files older than `modified_since` must be ignored;
    /// files on or after the cutoff and files with unreadable mtime are included.
    #[test]
    fn test_walk_respects_modified_since() {
        use chrono::TimeZone;
        let dir = TempDir::new().expect("tmpdir");

        // Three log files with controlled last-modified times can't easily be
        // set via std::fs — use the current time as a proxy.  We test the
        // filtering logic directly: pass a `since` in the future so the file's
        // real mtime is before it, expecting it to be rejected.  Then pass a
        // `since` in the past so the file is accepted.
        //
        // The assumption that a freshly created temp file has mtime < Utc::now()
        // + 1 day is safe on all supported platforms.
        let path = dir.path().join("new.log");
        fs::write(&path, b"line").expect("write");

        let include = vec![Pattern::new("*.log").unwrap()];
        let exclude: Vec<Pattern> = vec![];
        let known = HashSet::new();

        // Cutoff far in the future: mtime of a freshly created file is before it
        // → file should be rejected.
        let future = Utc.with_ymd_and_hms(9999, 1, 1, 0, 0, 0).unwrap();
        let found = walk_for_new_files(dir.path(), &known, &include, &exclude, 5, Some(future));
        assert!(
            found.is_empty(),
            "file modified before the cutoff must be excluded"
        );

        // Cutoff far in the past: any file passes.
        let past = Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap();
        let found2 = walk_for_new_files(dir.path(), &known, &include, &exclude, 5, Some(past));
        assert_eq!(found2.len(), 1, "file after the cutoff must be included");

        // No filter: all matching files are included regardless of mtime.
        let found3 = walk_for_new_files(dir.path(), &known, &include, &exclude, 5, None);
        assert_eq!(found3.len(), 1, "no filter: file must be included");
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
