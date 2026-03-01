// LogSleuth - app/scan.rs
//
// Scan lifecycle management. Orchestrates discovery and parsing on a
// background thread, sending progress messages to the UI thread via
// an mpsc channel.
//
// Architecture:
//   - `ScanManager` lives on the UI thread; `run_scan` runs on a background thread.
//   - An `Arc<AtomicBool>` cancel flag allows the UI to stop the scan cooperatively.
//   - All cross-thread communication is via `ScanProgress` channel messages.
//
// Rule 11 compliance:
//   - Transient I/O errors are retried with capped exponential backoff.
//   - All per-file errors are non-fatal; the scan continues to the next file.
//   - Entry batching via ENTRY_BATCH_SIZE caps memory usage between flushes.
//   - Cancel is checked before each file operation to enable prompt termination.

use crate::core::discovery::{self, DiscoveryConfig};
use crate::core::model::{FileSummary, FormatProfile, LogEntry, ScanProgress, ScanSummary};
use crate::core::parser::{self, ParseConfig};
use crate::core::profile;
use rayon::prelude::*;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

// =============================================================================
// Constants (Rule 11: named bounds)
// =============================================================================

/// Number of parsed entries to accumulate before sending an `EntriesBatch`.
const ENTRY_BATCH_SIZE: usize = 500;

/// Number of sample lines to read from each file for auto-detection.
const SAMPLE_LINES: usize = 20;

/// Retry limits for transient I/O errors.
const MAX_RETRIES: usize = 3;
const RETRY_DELAYS_MS: [u64; MAX_RETRIES] = [50, 100, 200];

/// Maximum seconds to wait for a full file read.
///
/// UNC/SMB shares can stall indefinitely on a dropped connection; this cap
/// keeps the scan responsive and allows Cancel to take effect promptly.
/// The spawned I/O thread is not forcibly killed (Rust does not support that)
/// but will exit on its own once the OS SMB timeout fires.
const FILE_READ_TIMEOUT_SECS: u64 = 30;

// =============================================================================
// Network path detection
// =============================================================================

/// Returns `true` if `path` is a UNC or network path where I/O is latency-bound.
///
/// UNC paths start with `\\` (Windows) or `//` (Unix SMB/NFS mounts).
/// When detected, the scan pipeline increases parallelism and skips memory
/// mapping (which performs poorly over SMB due to serial 4 KB page faults).
fn is_network_path(path: &Path) -> bool {
    let s = path.to_string_lossy();
    s.starts_with("\\\\") || s.starts_with("//")
}

/// Compute the number of parallel threads for the scan pipeline.
///
/// For network/UNC paths, I/O latency dominates CPU time.  Each file open is
/// a separate SMB round-trip (~5-50 ms LAN, ~20-100 ms WAN).  The default
/// `num_cpus` rayon pool leaves most threads idle waiting on I/O.  Multiplying
/// by `NETWORK_PARALLELISM_MULTIPLIER` (4x) overlaps more concurrent reads,
/// achieving 4-8x throughput improvement on typical UNC paths.
///
/// For local storage the default CPU-count parallelism is optimal because
/// parsing is CPU-bound and there is no per-file network latency.
fn compute_scan_parallelism(representative_path: Option<&Path>) -> usize {
    use crate::util::constants::{MAX_SCAN_THREADS, NETWORK_PARALLELISM_MULTIPLIER};

    let num_cpus = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);

    let is_network = representative_path.is_some_and(is_network_path);

    let threads = if is_network {
        (num_cpus * NETWORK_PARALLELISM_MULTIPLIER).min(MAX_SCAN_THREADS)
    } else {
        num_cpus
    };

    tracing::debug!(num_cpus, is_network, threads, "Scan parallelism computed");
    threads
}

// =============================================================================
// ScanManager
// =============================================================================

/// Manages a scan operation on a background thread.
pub struct ScanManager {
    /// Channel receiver for the UI to poll progress messages.
    pub progress_rx: Option<mpsc::Receiver<ScanProgress>>,

    /// Cancel flag shared with the background thread.
    cancel_flag: Option<Arc<AtomicBool>>,
}

impl ScanManager {
    pub fn new() -> Self {
        Self {
            progress_rx: None,
            cancel_flag: None,
        }
    }

    /// Start a scan of `root` using the given format profiles and discovery config.
    ///
    /// Spawns a background thread immediately; progress is sent over the channel.
    /// If a scan is already running it is cancelled first.
    pub fn start_scan(
        &mut self,
        root: PathBuf,
        profiles: Vec<FormatProfile>,
        config: DiscoveryConfig,
    ) {
        // Cancel any existing scan.
        self.cancel_scan();

        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));

        self.progress_rx = Some(rx);
        self.cancel_flag = Some(Arc::clone(&cancel));

        let parse_config = ParseConfig::default();

        std::thread::spawn(move || {
            let tx_guard = tx.clone();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                run_scan(root, profiles, config, parse_config, tx, cancel);
            }));
            if result.is_err() {
                tracing::error!("Scan thread panicked; sending Failed message");
                let _ = tx_guard.send(ScanProgress::Failed {
                    error: "Internal error: scan thread panicked unexpectedly".to_string(),
                });
            }
        });

        tracing::info!("Scan started");
    }

    /// Start parsing a pre-selected list of individual files in *append* mode.
    ///
    /// Unlike `start_scan`, this does NOT clear existing state — the caller is
    /// responsible for deciding whether to clear before calling.  The parsed
    /// entries are streamed via `EntriesBatch` and the file list is sent via
    /// `AdditionalFilesDiscovered` so the UI extends rather than replaces its
    /// current file list.
    ///
    /// `entry_id_start` must be set to `state.next_entry_id()` for append runs
    /// so new entry IDs do not collide with IDs already assigned during the
    /// initial scan.  Pass `0` for replace runs (fresh session after `clear()`).
    pub fn start_scan_files(
        &mut self,
        files: Vec<PathBuf>,
        profiles: Vec<FormatProfile>,
        max_total_entries: usize,
        entry_id_start: u64,
    ) {
        self.cancel_scan();

        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));

        self.progress_rx = Some(rx);
        self.cancel_flag = Some(Arc::clone(&cancel));

        let parse_config = ParseConfig::default();

        std::thread::spawn(move || {
            let tx_guard = tx.clone();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                run_files_scan(
                    files,
                    profiles,
                    parse_config,
                    tx,
                    cancel,
                    max_total_entries,
                    entry_id_start,
                );
            }));
            if result.is_err() {
                tracing::error!("File scan thread panicked; sending Failed message");
                let _ = tx_guard.send(ScanProgress::Failed {
                    error: "Internal error: file scan thread panicked unexpectedly".to_string(),
                });
            }
        });

        tracing::info!("File append scan started");
    }

    /// Request cancellation of the running scan.
    /// The background thread will send `ScanProgress::Cancelled` and exit.
    pub fn cancel_scan(&mut self) {
        if let Some(flag) = &self.cancel_flag {
            flag.store(true, Ordering::SeqCst);
        }
        self.cancel_flag = None;
    }

    /// Poll for progress messages without blocking.
    ///
    /// Drains at most `max` messages per call.  Any messages beyond the budget
    /// remain in the channel and are delivered on the next call.  This prevents
    /// a burst of queued messages from stalling the UI render loop for a whole
    /// frame (Rule 11: per-frame drain budget).
    pub fn poll_progress(&self, max: usize) -> Vec<ScanProgress> {
        let mut messages = Vec::with_capacity(max.min(64));
        if let Some(ref rx) = self.progress_rx {
            while messages.len() < max {
                match rx.try_recv() {
                    Ok(msg) => messages.push(msg),
                    Err(_) => break,
                }
            }
        }
        messages
    }
}

impl Default for ScanManager {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Background scan pipeline
// =============================================================================

/// Messages sent from the discovery sub-thread back to the scan thread.
///
/// Using a dedicated sub-thread for discovery ensures the scan thread can keep
/// polling the cancel flag even when `walkdir::next()` is blocked inside a
/// kernel UNC/SMB directory-listing call (which can stall for 30 + seconds on
/// an unreachable host).  The scan thread polls this channel with a short
/// timeout and re-checks cancel between each poll.
enum DiscoveryUpdate {
    /// A file was accepted by the walker.  Carries the full `DiscoveredFile`
    /// so the scan thread can accumulate files as they arrive and proceed even
    /// if the sub-thread stalls before sending `Done`.
    FileFound {
        file: crate::core::model::DiscoveredFile,
        count: usize,
    },
    /// Discovery finished successfully.
    Done {
        files: Vec<crate::core::model::DiscoveredFile>,
        warnings: Vec<String>,
        total_found: usize,
    },
    /// Discovery hit a fatal pre-flight error (bad root path, etc.).
    Error(String),
}

/// Full scan pipeline: discovery → auto-detection → parsing → batched delivery.
///
/// Runs on a background thread. Sends `ScanProgress` messages to `tx`.
/// Checks `cancel` before each significant operation.
fn run_scan(
    root: PathBuf,
    profiles: Vec<FormatProfile>,
    config: DiscoveryConfig,
    parse_config: ParseConfig,
    tx: mpsc::Sender<ScanProgress>,
    cancel: Arc<AtomicBool>,
) {
    macro_rules! send {
        ($msg:expr) => {
            if tx.send($msg).is_err() {
                return;
            }
        };
    }
    macro_rules! check_cancel {
        () => {
            if cancel.load(Ordering::SeqCst) {
                send!(ScanProgress::Cancelled);
                return;
            }
        };
    }

    // -------------------------------------------------------------------------
    // Phase 1: Discovery — runs in a dedicated sub-thread
    // -------------------------------------------------------------------------
    //
    // On UNC/SMB paths every `walkdir::next()` call issues a network RPC that
    // can block indefinitely when the remote host is unreachable or throttled.
    // Running discovery in a sub-thread and polling the result channel with a
    // short timeout lets the scan thread check the cancel flag (and return
    // `Cancelled` promptly) without waiting for the OS SMB timeout to fire.
    send!(ScanProgress::DiscoveryStarted);

    // Wire the cancel flag into the discovery config so the walkdir loop skips
    // entries promptly once cancellation is requested (belt-and-suspenders with
    // the polling loop below).
    let config = crate::core::discovery::DiscoveryConfig {
        cancel_flag: Some(Arc::clone(&cancel)),
        ..config
    };

    let (disc_tx, disc_rx) = mpsc::channel::<DiscoveryUpdate>();
    let disc_config = config.clone();
    let disc_root = root.clone();
    std::thread::spawn(move || {
        let result = discovery::discover_files(&disc_root, &disc_config, |file, count| {
            let _ = disc_tx.send(DiscoveryUpdate::FileFound {
                file: file.clone(),
                count,
            });
        });
        match result {
            Ok((files, warnings, total_found)) => {
                let _ = disc_tx.send(DiscoveryUpdate::Done {
                    files,
                    warnings,
                    total_found,
                });
            }
            Err(e) => {
                let _ = disc_tx.send(DiscoveryUpdate::Error(e.to_string()));
            }
        }
    });

    // How long to wait with no new message before giving up and proceeding
    // with whatever files have been accumulated.  On a stalled SMB/UNC path
    // `walkdir::next()` can block indefinitely; this cap lets the scan proceed
    // with partial results rather than waiting forever.
    const DISCOVERY_STALL_SECS: u64 = 30;

    // Accumulate DiscoveredFile structs as FileFound messages arrive so we can
    // proceed with a partial list if the sub-thread stalls or disconnects.
    let mut accumulated: Vec<crate::core::model::DiscoveredFile> = Vec::new();
    let mut last_activity = Instant::now();

    // Poll the discovery channel with a 100 ms timeout so the cancel flag is
    // re-checked between each poll, even while walkdir blocks inside an SMB call.
    let (discovered_files, warnings, total_found) = loop {
        if cancel.load(Ordering::SeqCst) {
            send!(ScanProgress::Cancelled);
            return;
        }

        // Stall guard: if no new progress for DISCOVERY_STALL_SECS, stop
        // waiting and parse whatever has been found so far.
        if last_activity.elapsed() >= Duration::from_secs(DISCOVERY_STALL_SECS) {
            let n = accumulated.len();
            let msg = format!(
                "Directory listing stalled for {DISCOVERY_STALL_SECS}s \
                 (UNC/network path not responding). \
                 Proceeding with {n} file(s) discovered so far. \
                 Results may be incomplete."
            );
            tracing::warn!("{}", msg);
            send!(ScanProgress::Warning { message: msg });
            let total = accumulated.len();
            break (accumulated, vec![], total);
        }

        match disc_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(DiscoveryUpdate::FileFound { file, count }) => {
                tracing::trace!(file = %file.path.display(), count, "File discovered");
                last_activity = Instant::now();
                let _ = tx.send(ScanProgress::FileDiscovered {
                    path: file.path.clone(),
                    files_found: count,
                });
                accumulated.push(file);
            }
            Ok(DiscoveryUpdate::Done {
                files,
                warnings,
                total_found,
            }) => {
                // Use the sub-thread's final list: it has been truncated to
                // max_files and sorted by mtime, unlike our `accumulated` copy.
                break (files, warnings, total_found);
            }
            Ok(DiscoveryUpdate::Error(e)) => {
                send!(ScanProgress::Failed { error: e });
                return;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // No message yet — loop; stall timer advances naturally.
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                // Sub-thread exited without Done/Error (panic or abort).
                // Fall back to the files accumulated so far rather than failing.
                if accumulated.is_empty() {
                    send!(ScanProgress::Failed {
                        error: "Discovery thread exited unexpectedly".to_string(),
                    });
                    return;
                }
                let n = accumulated.len();
                let msg =
                    format!("Discovery thread exited early; proceeding with {n} file(s) found.");
                tracing::warn!("{}", msg);
                send!(ScanProgress::Warning { message: msg });
                let total = accumulated.len();
                break (accumulated, vec![], total);
            }
        }
    };

    for warning in warnings {
        send!(ScanProgress::Warning { message: warning });
    }

    check_cancel!();

    run_parse_pipeline(
        discovered_files,
        profiles,
        parse_config,
        tx,
        cancel,
        false,
        total_found,
        config.max_total_entries,
        0, // fresh scan — always start IDs from zero
    );
}

// =============================================================================
// Phases 2+3: Auto-detection + Parsing (shared by directory scan and add-files)
// =============================================================================

/// Auto-detect format profiles for each file, then parse them all, streaming
/// results back over `tx`.
///
/// `append`: when `true` the discovered-file list is sent as
/// `AdditionalFilesDiscovered` (extends the UI list); when `false` it is sent
/// as `FilesDiscovered` (replaces the UI list).
///
/// `entry_id_start`: the first entry ID to assign.  Must be `0` for fresh
/// scans and `state.next_entry_id()` for append scans so IDs stay unique
/// across the entire session (bookmarks and correlation use these IDs).
#[allow(clippy::too_many_arguments)]
fn run_parse_pipeline(
    mut discovered_files: Vec<crate::core::model::DiscoveredFile>,
    profiles: Vec<FormatProfile>,
    parse_config: ParseConfig,
    tx: mpsc::Sender<ScanProgress>,
    cancel: Arc<AtomicBool>,
    append: bool,
    // Total files found during discovery before any limit was applied.
    // For user-selected file lists this equals `discovered_files.len()`.
    total_found: usize,
    // Maximum total entries to ingest across all files in this pipeline run.
    max_total_entries: usize,
    // Starting entry ID — 0 for fresh scans, state.next_entry_id() for appends.
    entry_id_start: u64,
) {
    macro_rules! send {
        ($msg:expr) => {
            if tx.send($msg).is_err() {
                return;
            }
        };
    }
    macro_rules! check_cancel {
        () => {
            if cancel.load(Ordering::SeqCst) {
                send!(ScanProgress::Cancelled);
                return;
            }
        };
    }

    // -------------------------------------------------------------------------
    // Parallel Phase: Merged auto-detection + parsing (single I/O pass)
    // -------------------------------------------------------------------------
    //
    // Performance architecture:
    //
    // The previous sequential approach read each file TWICE (sample lines for
    // profile detection, then full content for parsing) and processed files
    // one at a time.  On network/UNC paths every file open is an SMB
    // round-trip; sequential processing serialises that latency across all
    // files.
    //
    // This merged parallel phase reads each file ONCE, extracts sample lines
    // from the in-memory content for auto-detection, then parses immediately.
    // Rayon's work-stealing thread pool processes multiple files concurrently,
    // overlapping network latency.  On local storage the parallelism overlaps
    // parsing CPU work with disk I/O.
    //
    // Key gains:
    //   - Eliminates N redundant file opens (one per file) on UNC paths
    //   - Overlaps I/O latency across files via rayon parallelism
    //   - Timeout-guarded reads protect rayon workers from SMB stalls
    //   - Entry IDs are assigned sequentially after collection to maintain
    //     global uniqueness without cross-thread coordination
    //   - For UNC paths, a custom rayon pool with NETWORK_PARALLELISM_MULTIPLIER
    //     x CPU threads overlaps far more concurrent SMB reads (4-8x throughput)
    //   - Files are sorted by parent directory to improve SMB directory-handle
    //     caching and OS metadata prefetch

    let total_files = discovered_files.len();
    let scan_start = Instant::now();

    // Pre-sort files by parent directory so files in the same folder are
    // processed on nearby rayon iterations.  This improves SMB directory
    // handle caching: the OS can reuse the same open directory handle for
    // consecutive files in the same folder, avoiding extra SMB CREATE RPCs.
    // On local storage the improvement is negligible but harmless.
    discovered_files.sort_by(|a, b| {
        a.path
            .parent()
            .cmp(&b.path.parent())
            .then_with(|| a.path.cmp(&b.path))
    });

    /// Per-file result collected from the parallel processing phase.
    ///
    /// Entry IDs inside `entries` are temporary (start from 0 within each
    /// file).  The sequential post-processing step reassigns globally-unique
    /// IDs before streaming to the UI.
    struct FileResult {
        /// Index into `discovered_files` for updating profile info.
        idx: usize,
        /// Detected or fallback profile ID (None if file was unreadable/skipped).
        profile_id: Option<String>,
        /// Detection confidence score.
        detection_confidence: f64,
        /// Parsed entries with temporary IDs.
        entries: Vec<LogEntry>,
        /// File summary for the scan report.
        summary: Option<FileSummary>,
        /// Warning messages to surface to the user.
        warnings: Vec<String>,
        /// Number of parse errors in this file.
        error_count: usize,
    }

    // Shared atomic counter so parallel workers can report per-file progress
    // in real time (the UI sees FileParsed messages streaming in as files
    // complete, even though the order is non-deterministic).
    let files_completed_counter = Arc::new(AtomicUsize::new(0));
    let progress_tx = tx.clone();

    // Build a custom rayon pool.  For UNC/network paths this uses
    // NETWORK_PARALLELISM_MULTIPLIER x CPU threads to overlap SMB latency.
    // For local paths it matches the default (num_cpus) so CPU-bound parsing
    // is not penalised by excessive context switching.
    let representative = discovered_files.first().map(|f| f.path.as_path());
    let parallelism = compute_scan_parallelism(representative);
    let pool = match rayon::ThreadPoolBuilder::new()
        .num_threads(parallelism)
        .thread_name(|idx| format!("logsleuth-scan-{idx}"))
        .build()
    {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(error = %e, "Failed to build custom rayon pool; trying defaults");
            match rayon::ThreadPoolBuilder::new().build() {
                Ok(p) => p,
                Err(e2) => {
                    tracing::error!(error = %e2, "Failed to build any rayon pool");
                    send!(ScanProgress::Failed {
                        error: format!(
                            "Could not initialise parallel processing: {e2}. \
                             Try restarting the application."
                        ),
                    });
                    return;
                }
            }
        }
    };

    let file_results: Vec<FileResult> = pool.install(|| {
        discovered_files
        .par_iter()
        .enumerate()
        .map(|(idx, file)| {
            // Early exit on cancel -- each rayon worker checks independently.
            if cancel.load(Ordering::SeqCst) {
                return FileResult {
                    idx,
                    profile_id: None,
                    detection_confidence: 0.0,
                    entries: Vec::new(),
                    summary: None,
                    warnings: Vec::new(),
                    error_count: 0,
                };
            }

            let mut warnings: Vec<String> = Vec::new();

            // --- Single I/O pass: read full file content (timeout-guarded) ---
            let content = match read_file_content_timed(&file.path, file.is_large) {
                Ok(c) => c,
                Err(e) => {
                    let msg = format!("Cannot read '{}': {e}", file.path.display());
                    tracing::warn!(warning = %msg, "File read failed");
                    let _ = progress_tx.send(ScanProgress::Warning {
                        message: msg.clone(),
                    });
                    let completed = files_completed_counter.fetch_add(1, Ordering::SeqCst) + 1;
                    let _ = progress_tx.send(ScanProgress::FileParsed {
                        path: file.path.clone(),
                        entries: 0,
                        errors: 0,
                        files_completed: completed,
                        total_files,
                    });
                    return FileResult {
                        idx,
                        profile_id: None,
                        detection_confidence: 0.0,
                        entries: Vec::new(),
                        summary: None,
                        warnings: vec![msg],
                        error_count: 0,
                    };
                }
            };

            if cancel.load(Ordering::SeqCst) {
                return FileResult {
                    idx,
                    profile_id: None,
                    detection_confidence: 0.0,
                    entries: Vec::new(),
                    summary: None,
                    warnings: Vec::new(),
                    error_count: 0,
                };
            }

            // --- Auto-detect from first N lines of already-read content ---
            // Avoids a second file open that the old sequential Phase 2 required.
            let sample_lines: Vec<String> = content
                .lines()
                .take(SAMPLE_LINES)
                .map(String::from)
                .collect();
            let file_name = file.path.file_name().and_then(|n| n.to_str()).unwrap_or("");

            let (mut detected_profile_id, detection_confidence) = if let Some(detection) =
                profile::auto_detect(file_name, &sample_lines, &profiles)
            {
                tracing::debug!(
                    file = %file.path.display(),
                    profile = %detection.profile_id,
                    confidence = detection.confidence,
                    "Auto-detected profile"
                );
                (Some(detection.profile_id), detection.confidence)
            } else if profiles.iter().any(|p| p.id == "plain-text") {
                tracing::debug!(
                    file = %file.path.display(),
                    "No structured profile matched; falling back to plain-text"
                );
                (Some("plain-text".to_string()), 0.0)
            } else {
                (None, 0.0)
            };

            // Resolve profile or skip the file.
            let pid = match &detected_profile_id {
                Some(id) => id.clone(),
                None => {
                    let msg = format!(
                        "'{}': no format profile could be assigned \
                         (even plain-text was unavailable), file skipped",
                        file.path.display()
                    );
                    tracing::debug!(file = %file.path.display(), "No profile assigned, skipping");
                    let _ = progress_tx.send(ScanProgress::Warning {
                        message: msg.clone(),
                    });
                    warnings.push(msg);
                    let completed = files_completed_counter.fetch_add(1, Ordering::SeqCst) + 1;
                    let _ = progress_tx.send(ScanProgress::FileParsed {
                        path: file.path.clone(),
                        entries: 0,
                        errors: 0,
                        files_completed: completed,
                        total_files,
                    });
                    return FileResult {
                        idx,
                        profile_id: None,
                        detection_confidence: 0.0,
                        entries: Vec::new(),
                        summary: None,
                        warnings,
                        error_count: 0,
                    };
                }
            };

            let matched_profile = match profiles.iter().find(|p| p.id == pid) {
                Some(p) => p,
                None => {
                    tracing::warn!(profile = %pid, "Profile not found in loaded profiles");
                    let completed = files_completed_counter.fetch_add(1, Ordering::SeqCst) + 1;
                    let _ = progress_tx.send(ScanProgress::FileParsed {
                        path: file.path.clone(),
                        entries: 0,
                        errors: 0,
                        files_completed: completed,
                        total_files,
                    });
                    return FileResult {
                        idx,
                        profile_id: Some(pid),
                        detection_confidence,
                        entries: Vec::new(),
                        summary: None,
                        warnings,
                        error_count: 0,
                    };
                }
            };

            if cancel.load(Ordering::SeqCst) {
                return FileResult {
                    idx,
                    profile_id: Some(pid),
                    detection_confidence,
                    entries: Vec::new(),
                    summary: None,
                    warnings,
                    error_count: 0,
                };
            }

            // --- Parse (reuses already-read content -- zero additional I/O) ---
            let mut parse_result = parser::parse_content(
                &content,
                &file.path,
                matched_profile,
                &parse_config,
                0, // temporary IDs -- reassigned sequentially after collection
            );

            // Fallback: if the assigned profile produced zero entries but the
            // file has content, re-parse with plain-text so every non-empty
            // file contributes at least its raw line content to the timeline.
            let mut final_profile_id = pid;
            if parse_result.entries.is_empty() && !content.trim().is_empty() {
                if let Some(plain_profile) = profiles.iter().find(|p| p.id == "plain-text") {
                    if plain_profile.id != final_profile_id {
                        tracing::debug!(
                            file = %file.path.display(),
                            assigned_profile = %final_profile_id,
                            "Assigned profile yielded 0 entries; \
                             falling back to plain-text"
                        );
                        parse_result = parser::parse_content(
                            &content,
                            &file.path,
                            plain_profile,
                            &parse_config,
                            0,
                        );
                        final_profile_id = plain_profile.id.clone();
                    }
                }
            }

            // Stamp the source file's OS last-modified time on every entry.
            let file_mtime = file.modified;
            for entry in &mut parse_result.entries {
                entry.file_modified = file_mtime;
            }

            let entry_count = parse_result.entries.len();
            let error_count = parse_result.errors.len();

            // Build per-file summary.
            let mut earliest: Option<chrono::DateTime<chrono::Utc>> = None;
            let mut latest: Option<chrono::DateTime<chrono::Utc>> = None;
            for entry in &parse_result.entries {
                if let Some(ts) = entry.timestamp {
                    earliest = Some(match earliest {
                        Some(e) if e <= ts => e,
                        _ => ts,
                    });
                    latest = Some(match latest {
                        Some(l) if l >= ts => l,
                        _ => ts,
                    });
                }
            }
            let summary = FileSummary {
                path: file.path.clone(),
                profile_id: final_profile_id.clone(),
                entry_count,
                error_count,
                earliest,
                latest,
            };

            for err in &parse_result.errors {
                tracing::debug!(error = %err, "Parse error");
            }

            // Send per-file progress (order is non-deterministic in parallel
            // mode but the UI only uses files_completed / total_files for its
            // progress bar, so arrival order does not matter).
            let completed = files_completed_counter.fetch_add(1, Ordering::SeqCst) + 1;
            let _ = progress_tx.send(ScanProgress::FileParsed {
                path: file.path.clone(),
                entries: entry_count,
                errors: error_count,
                files_completed: completed,
                total_files,
            });

            detected_profile_id = Some(final_profile_id);

            FileResult {
                idx,
                profile_id: detected_profile_id,
                detection_confidence,
                entries: parse_result.entries,
                summary: Some(summary),
                warnings,
                error_count,
            }
        })
        .collect()
    }); // end pool.install

    check_cancel!();

    // -------------------------------------------------------------------------
    // Post-parallel: assemble results sequentially
    // -------------------------------------------------------------------------

    // Update discovered_files with the profile info detected during the
    // parallel phase so the UI file list shows the correct profile assignment.
    for result in &file_results {
        if let Some(pid) = &result.profile_id {
            discovered_files[result.idx].profile_id = Some(pid.clone());
            discovered_files[result.idx].detection_confidence = result.detection_confidence;
        }
    }

    send!(ScanProgress::DiscoveryCompleted {
        total_files,
        total_found,
    });

    if append {
        send!(ScanProgress::AdditionalFilesDiscovered {
            files: discovered_files.clone(),
        });
    } else {
        send!(ScanProgress::FilesDiscovered {
            files: discovered_files.clone(),
        });
    }

    // ParsingStarted is sent here for UI state-machine compatibility even
    // though parsing already completed during the parallel phase.  The
    // FileParsed messages were sent in real-time from parallel workers, so the
    // UI saw incremental progress.
    send!(ScanProgress::ParsingStarted { total_files });

    // Assign globally-unique entry IDs, enforce the hard entry cap, and
    // collect summaries -- all sequential to avoid cross-thread coordination.
    let mut total_errors: usize = 0;
    let mut files_with_entries: usize = 0;
    let mut entry_id: u64 = entry_id_start;
    let mut all_entries: Vec<LogEntry> = Vec::new();
    let mut file_summaries: Vec<FileSummary> = Vec::new();
    let mut entry_cap_reached = false;

    // Process results in original file order so entry IDs are deterministic.
    let mut sorted_results: Vec<FileResult> = file_results
        .into_iter()
        .filter(|r| r.summary.is_some())
        .collect();
    sorted_results.sort_by_key(|r| r.idx);

    for mut result in sorted_results {
        check_cancel!();

        if entry_cap_reached {
            break;
        }

        let entry_count = result.entries.len();
        let error_count = result.error_count;

        // Reassign entry IDs to maintain global uniqueness.
        for entry in &mut result.entries {
            entry.id = entry_id;
            entry_id += 1;
        }

        total_errors += error_count;
        if entry_count > 0 {
            files_with_entries += 1;
        }

        if let Some(summary) = result.summary {
            file_summaries.push(summary);
        }

        for w in &result.warnings {
            send!(ScanProgress::Warning { message: w.clone() });
        }

        // Enforce the hard entry cap (Rule 11: bounded collections).
        let remaining_capacity = max_total_entries.saturating_sub(all_entries.len());
        if remaining_capacity == 0 {
            // Defensive guard -- should not be reached with the break above.
        } else if result.entries.len() <= remaining_capacity {
            all_entries.extend(result.entries);
        } else {
            all_entries.extend(result.entries.into_iter().take(remaining_capacity));
            entry_cap_reached = true;
            let cap = max_total_entries;
            let msg = format!(
                "Entry limit reached: {cap} entries loaded. \
                 Remaining files in this scan have been skipped to \
                 prevent an out-of-memory crash. Use the date filter \
                 or reduce the max-files limit to target a smaller dataset."
            );
            tracing::warn!("{}", msg);
            send!(ScanProgress::Warning { message: msg });
        }
    }

    // Sort all collected entries chronologically on the background thread.
    // Entries with timestamps come first ordered by time; timestamp-less entries
    // retain their relative parse order at the end.  This prevents the UI thread
    // from blocking on a potentially large sort after ParsingCompleted arrives.
    tracing::debug!(
        entries = all_entries.len(),
        "Sorting entries chronologically on background thread"
    );
    all_entries.sort_by(|a, b| match (a.timestamp, b.timestamp) {
        (Some(ta), Some(tb)) => ta.cmp(&tb),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });

    // Stream sorted entries to the UI in batches.
    //
    // Performance: use `drain()` to MOVE entries out of the Vec rather than
    // `chunks().to_vec()` which CLONES every entry.  With 1M entries the
    // clone approach doubles peak memory usage; drain keeps it flat.
    //
    // Bug fix: capture the actual loaded count BEFORE draining.  The
    // accumulated `total_entries` counter can overcount when the entry cap
    // truncates a file mid-way (it includes the full file's entry count).
    // Using the concrete Vec length gives the accurate number.
    let actual_total_entries = all_entries.len();
    check_cancel!();
    while !all_entries.is_empty() {
        let batch_end = ENTRY_BATCH_SIZE.min(all_entries.len());
        let batch: Vec<LogEntry> = all_entries.drain(..batch_end).collect();
        send!(ScanProgress::EntriesBatch { entries: batch });
        check_cancel!();
    }

    let files_with_errors = file_summaries
        .iter()
        .filter(|fs| fs.error_count > 0)
        .count();
    let summary = ScanSummary {
        total_files_discovered: total_files,
        total_entries: actual_total_entries,
        total_parse_errors: total_errors,
        files_matched: files_with_entries,
        files_with_errors,
        file_summaries,
        duration: scan_start.elapsed(),
    };

    send!(ScanProgress::ParsingCompleted { summary });

    tracing::info!(
        files = total_files,
        entries = actual_total_entries,
        errors = total_errors,
        "Parse pipeline complete (append={})",
        append
    );
}

// =============================================================================
// Add-files scan (no directory walk, append to existing session)
// =============================================================================

/// Scan a user-provided list of individual file paths without clearing state.
///
/// For each path, reads OS metadata to build a `DiscoveredFile`, then hands
/// off to `run_parse_pipeline` with `append=true`.  Permission errors and
/// missing files are reported as non-fatal warnings.
fn run_files_scan(
    paths: Vec<PathBuf>,
    profiles: Vec<FormatProfile>,
    parse_config: ParseConfig,
    tx: mpsc::Sender<ScanProgress>,
    cancel: Arc<AtomicBool>,
    max_total_entries: usize,
    entry_id_start: u64,
) {
    use crate::util::constants::DEFAULT_LARGE_FILE_THRESHOLD;
    use chrono::DateTime;

    macro_rules! send {
        ($msg:expr) => {
            if tx.send($msg).is_err() {
                return;
            }
        };
    }

    send!(ScanProgress::DiscoveryStarted);

    let mut discovered: Vec<crate::core::model::DiscoveredFile> = Vec::with_capacity(paths.len());

    // Use a per-file timeout-guarded stat so that files on an unreachable
    // UNC/SMB share never hang the scan thread indefinitely (Rule 11).
    // The same pattern is used by read_file_content_timed.
    const META_TIMEOUT_SECS: u64 = 10;
    for path in &paths {
        if cancel.load(std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        let path_owned = path.clone();
        let (meta_tx, meta_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = meta_tx.send(std::fs::metadata(&path_owned));
        });
        let meta_result = match meta_rx.recv_timeout(Duration::from_secs(META_TIMEOUT_SECS)) {
            Ok(r) => r,
            Err(_) => {
                let msg = format!(
                    "Cannot access '{}': metadata timed out after {META_TIMEOUT_SECS}s \
                     (unreachable UNC host?)",
                    path.display()
                );
                tracing::warn!(warning = %msg, "Add-files metadata timeout");
                send!(ScanProgress::Warning { message: msg });
                continue;
            }
        };
        match meta_result {
            Ok(meta) => {
                let size = meta.len();
                let modified = meta.modified().ok().map(DateTime::<chrono::Utc>::from);
                let is_large = size >= DEFAULT_LARGE_FILE_THRESHOLD;
                discovered.push(crate::core::model::DiscoveredFile {
                    path: path.clone(),
                    size,
                    modified,
                    profile_id: None,
                    detection_confidence: 0.0,
                    is_large,
                });
            }
            Err(e) => {
                let msg = format!("Cannot access '{}': {e}", path.display());
                tracing::warn!(warning = %msg, "Add-files metadata error");
                send!(ScanProgress::Warning { message: msg });
            }
        }
    }

    // For user-selected files there is no discovery limit, so total_found
    // equals the number of files that were successfully stat'd.
    let total_found = discovered.len();
    run_parse_pipeline(
        discovered,
        profiles,
        parse_config,
        tx,
        cancel,
        true,
        total_found,
        max_total_entries,
        entry_id_start,
    );
}

// =============================================================================
// File reading helpers
// =============================================================================

/// Timeout-guarded wrapper around `read_file_content`.
///
/// Runs the I/O on a separate thread; if no result arrives within
/// `FILE_READ_TIMEOUT_SECS` returns a `TimedOut` error so the caller logs a
/// warning and skips the file (Rule 11 resilience).
fn read_file_content_timed(path: &Path, is_large: bool) -> io::Result<String> {
    let (tx, rx) = std::sync::mpsc::channel();
    let path_owned = path.to_path_buf();
    std::thread::spawn(move || {
        let _ = tx.send(read_file_content(&path_owned, is_large));
    });
    match rx.recv_timeout(Duration::from_secs(FILE_READ_TIMEOUT_SECS)) {
        Ok(result) => result,
        Err(_) => {
            tracing::warn!(
                file = %path.display(),
                timeout_secs = FILE_READ_TIMEOUT_SECS,
                "File read timed out (stalled network path?); skipping file"
            );
            Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!(
                    "read timed out after {FILE_READ_TIMEOUT_SECS}s \
                     (stalled UNC/network path)"
                ),
            ))
        }
    }
}

/// Read the full content of a file as a UTF-8 string.
///
/// Strategy selection:
///   - **Network paths (UNC)**: always use sequential buffered reading with a
///     large buffer (`NETWORK_IO_BUFFER_SIZE`).  Memory mapping over SMB causes
///     serial 4 KB page faults (one network round-trip per page), then the
///     subsequent `from_utf8()` + `.to_string()` reads the file over the network
///     a second time.  Sequential reads with OS read-ahead are vastly faster.
///   - **Local large files**: use `memmap2` to avoid copying the entire file
///     into heap memory.  This is safe and fast on local filesystems.
///   - **Local small files**: use `fs::read_to_string` with retry.
///
/// Transient I/O errors (WouldBlock, Interrupted, TimedOut) are retried with
/// capped exponential backoff (Rule 11). Permanent errors are returned immediately.
fn read_file_content(path: &Path, is_large: bool) -> io::Result<String> {
    if is_network_path(path) {
        // Network path: always use buffered sequential reads regardless of
        // file size.  Mmap over SMB is pathologically slow (serial page faults).
        read_buffered_with_retry(path)
    } else if is_large {
        read_large_file(path)
    } else {
        read_small_file_with_retry(path)
    }
}

/// Read using `memmap2` for large LOCAL files (avoids allocating the full buffer).
///
/// **Not used for network paths** -- see `read_file_content` for rationale.
fn read_large_file(path: &Path) -> io::Result<String> {
    let file = std::fs::File::open(path)?;
    // SAFETY: the file is read-only and we do not mutate the map.
    // We accept the documented risk that external modification of the file
    // during the map's lifetime could produce undefined behaviour, which is
    // acceptable for a log viewer reading already-written files.
    let mmap = unsafe { memmap2::Mmap::map(&file)? };

    // Fast path: valid UTF-8 (most log files).
    if let Ok(s) = std::str::from_utf8(&mmap) {
        return Ok(s.to_string());
    }

    // Encoding fallback: handles UTF-16 LE/BE (Windows system logs) and ANSI.
    decode_bytes(&mmap, path)
}

/// Read a file using a large buffered reader optimised for network I/O.
///
/// Uses `NETWORK_IO_BUFFER_SIZE` (256 KB) to reduce the number of SMB READ
/// round-trips.  Pre-allocates the output String to the file's reported size
/// to avoid re-allocations.
///
/// Retry logic mirrors `read_small_file_with_retry`.
fn read_buffered_with_retry(path: &Path) -> io::Result<String> {
    use crate::util::constants::NETWORK_IO_BUFFER_SIZE;

    let mut last_err: Option<io::Error> = None;

    for (attempt, &delay_ms) in RETRY_DELAYS_MS.iter().enumerate() {
        match read_buffered(path, NETWORK_IO_BUFFER_SIZE) {
            Ok(content) => return Ok(content),
            Err(e) if e.kind() == io::ErrorKind::InvalidData => {
                return decode_non_utf8_file(path);
            }
            Err(e) if is_transient_error(&e) => {
                tracing::debug!(
                    file = %path.display(),
                    attempt = attempt + 1,
                    error = %e,
                    "Transient I/O error on network read, retrying"
                );
                std::thread::sleep(Duration::from_millis(delay_ms));
                last_err = Some(e);
            }
            Err(e) => return Err(e),
        }
    }

    Err(last_err.unwrap_or_else(|| io::Error::other("Unknown read error")))
}

/// Read a file sequentially with the specified buffer capacity.
///
/// Pre-allocates the output String to the file's metadata size to avoid
/// growths during reading (each growth on a network file would trigger
/// a metadata re-fetch on some OS implementations).
fn read_buffered(path: &Path, buf_capacity: usize) -> io::Result<String> {
    let file = std::fs::File::open(path)?;
    let size_hint = file.metadata().map(|m| m.len() as usize).unwrap_or(0);
    let mut reader = std::io::BufReader::with_capacity(buf_capacity, file);
    let mut content = String::with_capacity(size_hint + 1);
    reader.read_to_string(&mut content)?;
    Ok(content)
}

/// Read a small file with transient-error retries.
fn read_small_file_with_retry(path: &Path) -> io::Result<String> {
    let mut last_err: Option<io::Error> = None;

    for (attempt, &delay_ms) in RETRY_DELAYS_MS.iter().enumerate() {
        match std::fs::read_to_string(path) {
            Ok(content) => return Ok(content),
            Err(e) if e.kind() == io::ErrorKind::InvalidData => {
                // File is not valid UTF-8 (e.g. UTF-16 LE Windows system logs).
                // Switch to encoding-aware decoding rather than retrying.
                return decode_non_utf8_file(path);
            }
            Err(e) if is_transient_error(&e) => {
                tracing::debug!(
                    file = %path.display(),
                    attempt = attempt + 1,
                    error = %e,
                    "Transient I/O error, retrying"
                );
                std::thread::sleep(Duration::from_millis(delay_ms));
                last_err = Some(e);
            }
            Err(e) => return Err(e), // Permanent error; do not retry.
        }
    }

    Err(last_err.unwrap_or_else(|| io::Error::other("Unknown read error")))
}

/// Decode a file whose bytes are not valid UTF-8.
///
/// Checks for UTF-16 LE BOM (0xFF 0xFE) and UTF-16 BE BOM (0xFE 0xFF) —
/// both are used by Windows system logs such as CBS.log and WindowsUpdate.log.
/// Falls back to lossy UTF-8 for ANSI / Windows-1252 encoded files.
fn decode_non_utf8_file(path: &Path) -> io::Result<String> {
    let bytes = std::fs::read(path)?;
    decode_bytes(&bytes, path)
}

/// Detect encoding from `bytes` and return the decoded string.
///
/// Checks BOM markers only; does not do statistical charset detection.
/// This is intentionally conservative: it correctly handles the most common
/// non-UTF-8 encodings found in Windows system log directories while avoiding
/// the complexity and potential false-positives of full charset detection.
fn decode_bytes(bytes: &[u8], path: &Path) -> io::Result<String> {
    // UTF-16 LE BOM: 0xFF 0xFE — used by CBS.log, WindowsUpdate.log, etc.
    if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xFE {
        let utf16: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        tracing::debug!(file = %path.display(), "Decoded file as UTF-16 LE");
        return Ok(String::from_utf16_lossy(&utf16));
    }

    // UTF-16 BE BOM: 0xFE 0xFF — uncommon but valid.
    if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
        let utf16: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        tracing::debug!(file = %path.display(), "Decoded file as UTF-16 BE");
        return Ok(String::from_utf16_lossy(&utf16));
    }

    // No recognised BOM: try zero-copy UTF-8 first (most log files are
    // valid UTF-8), falling back to lossy conversion only for genuinely
    // invalid bytes.  The owned Vec→String path avoids a full buffer copy
    // that from_utf8_lossy().into_owned() would always perform.
    match String::from_utf8(bytes.to_vec()) {
        Ok(s) => {
            tracing::debug!(file = %path.display(), "Decoded file as UTF-8 (zero-copy)");
            Ok(s)
        }
        Err(e) => {
            tracing::debug!(file = %path.display(), "Decoded file as lossy UTF-8 (no BOM)");
            Ok(String::from_utf8_lossy(e.as_bytes()).into_owned())
        }
    }
}

/// Returns true for transient I/O errors that are worth retrying.
fn is_transient_error(e: &io::Error) -> bool {
    matches!(
        e.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted | io::ErrorKind::TimedOut
    )
}
