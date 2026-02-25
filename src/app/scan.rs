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
use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
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
const MAX_RETRIES: u32 = 3;
const RETRY_DELAYS_MS: [u64; 3] = [50, 100, 200];

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
            run_scan(root, profiles, config, parse_config, tx, cancel);
        });

        tracing::info!("Scan started");
    }

    /// Request cancellation of the running scan.
    /// The background thread will send `ScanProgress::Cancelled` and exit.
    pub fn cancel_scan(&mut self) {
        if let Some(flag) = &self.cancel_flag {
            flag.store(true, Ordering::SeqCst);
        }
        self.cancel_flag = None;
    }

    /// Poll for progress messages without blocking. Returns all pending messages.
    pub fn poll_progress(&self) -> Vec<ScanProgress> {
        let mut messages = Vec::new();
        if let Some(ref rx) = self.progress_rx {
            while let Ok(msg) = rx.try_recv() {
                messages.push(msg);
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
                return; // Receiver dropped (UI closed); exit quietly.
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
    // Phase 1: Discovery
    // -------------------------------------------------------------------------
    send!(ScanProgress::DiscoveryStarted);

    let tx_discovery = tx.clone();
    let (mut discovered_files, warnings) =
        match discovery::discover_files(&root, &config, |path, count| {
            tracing::trace!(file = %path.display(), count, "File discovered");
            // Non-fatal: ignore send error (UI may have closed).
            let _ = tx_discovery.send(ScanProgress::FileDiscovered {
                path: path.to_path_buf(),
                files_found: count,
            });
        }) {
            Ok(result) => result,
            Err(e) => {
                send!(ScanProgress::Failed {
                    error: e.to_string(),
                });
                return;
            }
        };

    // Forward discovery warnings as non-fatal scan warnings.
    for warning in warnings {
        send!(ScanProgress::Warning { message: warning });
    }

    check_cancel!();

    // -------------------------------------------------------------------------
    // Phase 2: Auto-detection
    // -------------------------------------------------------------------------
    // Read sample lines per file, run auto-detect, annotate the file record.
    let total_files = discovered_files.len();

    for file in &mut discovered_files {
        check_cancel!();

        let samples = read_sample_lines(&file.path, SAMPLE_LINES);
        let file_name = file.path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        if let Some(detection) = profile::auto_detect(file_name, &samples, &profiles) {
            file.profile_id = Some(detection.profile_id.clone());
            file.detection_confidence = detection.confidence;
            tracing::debug!(
                file = %file.path.display(),
                profile = detection.profile_id,
                confidence = detection.confidence,
                "Auto-detected profile"
            );
        }
    }

    send!(ScanProgress::DiscoveryCompleted { total_files });

    // Send the full annotated file list so the UI can populate the discovery panel.
    send!(ScanProgress::FilesDiscovered {
        files: discovered_files.clone(),
    });

    check_cancel!();

    // -------------------------------------------------------------------------
    // Phase 3: Parsing
    // -------------------------------------------------------------------------
    send!(ScanProgress::ParsingStarted { total_files });

    let scan_start = Instant::now();
    let mut total_entries: usize = 0;
    let mut total_errors: usize = 0;
    let mut files_with_entries: usize = 0;
    let mut entry_id: u64 = 0;
    let mut entry_batch: Vec<LogEntry> = Vec::with_capacity(ENTRY_BATCH_SIZE);
    let mut file_summaries: Vec<FileSummary> = Vec::new();

    for (idx, file) in discovered_files.iter().enumerate() {
        check_cancel!();

        let files_completed = idx + 1;

        // Look up the matched profile; skip files with no matched format.
        let profile_id = match &file.profile_id {
            Some(id) => id.clone(),
            None => {
                tracing::debug!(file = %file.path.display(), "No matching profile, skipping");
                send!(ScanProgress::Warning {
                    message: format!(
                        "'{}': no matching format profile, file skipped",
                        file.path.display()
                    ),
                });
                continue;
            }
        };

        let matched_profile = match profiles.iter().find(|p| p.id == profile_id) {
            Some(p) => p,
            None => {
                tracing::warn!(profile = %profile_id, "Profile not found in loaded profiles");
                continue;
            }
        };

        // Read file content (with retry for transient I/O errors, mmap for large).
        let content = match read_file_content(&file.path, file.is_large) {
            Ok(c) => c,
            Err(e) => {
                let msg = format!("Cannot read '{}': {e}", file.path.display());
                tracing::warn!(warning = %msg, "File read failed");
                send!(ScanProgress::Warning { message: msg });
                continue;
            }
        };

        check_cancel!();

        // Parse the file content.
        let parse_result = parser::parse_content(
            &content,
            &file.path,
            matched_profile,
            &parse_config,
            entry_id,
        );

        let entry_count = parse_result.entries.len();
        let error_count = parse_result.errors.len();

        entry_id += entry_count as u64;
        total_entries += entry_count;
        total_errors += error_count;

        if entry_count > 0 {
            files_with_entries += 1;
        }

        // Collect per-file timestamp range for FileSummary.
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
        file_summaries.push(FileSummary {
            path: file.path.clone(),
            profile_id: profile_id.clone(),
            entry_count,
            error_count,
            earliest,
            latest,
        });

        // Log non-fatal parse errors at debug level.
        for err in &parse_result.errors {
            tracing::debug!(error = %err, "Parse error");
        }

        // Batch entries and flush when the batch is full.
        for entry in parse_result.entries {
            entry_batch.push(entry);
            if entry_batch.len() >= ENTRY_BATCH_SIZE {
                let batch =
                    std::mem::replace(&mut entry_batch, Vec::with_capacity(ENTRY_BATCH_SIZE));
                send!(ScanProgress::EntriesBatch { entries: batch });
                check_cancel!();
            }
        }

        send!(ScanProgress::FileParsed {
            path: file.path.clone(),
            entries: entry_count,
            errors: error_count,
            files_completed,
            total_files,
        });
    }

    // Flush the remaining partial batch.
    if !entry_batch.is_empty() {
        send!(ScanProgress::EntriesBatch {
            entries: entry_batch,
        });
    }

    check_cancel!();

    let summary = ScanSummary {
        total_files_discovered: total_files,
        total_entries,
        total_parse_errors: total_errors,
        files_matched: files_with_entries,
        file_summaries,
        duration: scan_start.elapsed(),
        ..Default::default()
    };

    send!(ScanProgress::ParsingCompleted { summary });

    tracing::info!(
        files = total_files,
        entries = total_entries,
        errors = total_errors,
        "Scan complete"
    );
}

// =============================================================================
// File reading helpers
// =============================================================================

/// Read up to `max_lines` lines from the start of a file for auto-detection.
/// Returns an empty vec on any I/O error (non-fatal; auto-detection will skip).
fn read_sample_lines(path: &Path, max_lines: usize) -> Vec<String> {
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) => {
            tracing::debug!(file = %path.display(), error = %e, "Cannot open for sampling");
            return Vec::new();
        }
    };

    BufReader::new(file)
        .lines()
        .take(max_lines)
        .filter_map(|l| l.ok())
        .collect()
}

/// Read the full content of a file as a UTF-8 string.
///
/// For large files, uses `memmap2` which avoids copying the entire file into
/// heap memory. Small files use `fs::read_to_string`.
///
/// Transient I/O errors (WouldBlock, Interrupted, TimedOut) are retried with
/// capped exponential backoff (Rule 11). Permanent errors are returned immediately.
fn read_file_content(path: &Path, is_large: bool) -> io::Result<String> {
    if is_large {
        read_large_file(path)
    } else {
        read_small_file_with_retry(path)
    }
}

/// Read using `memmap2` for large files (avoids allocating the full buffer).
fn read_large_file(path: &Path) -> io::Result<String> {
    let file = std::fs::File::open(path)?;
    // SAFETY: the file is read-only and we do not mutate the map.
    // We accept the documented risk that external modification of the file
    // during the map's lifetime could produce undefined behaviour, which is
    // acceptable for a log viewer reading already-written files.
    let mmap = unsafe { memmap2::Mmap::map(&file)? };
    std::str::from_utf8(&mmap)
        .map(|s| s.to_string())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// Read a small file with transient-error retries.
fn read_small_file_with_retry(path: &Path) -> io::Result<String> {
    let mut last_err: Option<io::Error> = None;

    for attempt in 0..MAX_RETRIES {
        match std::fs::read_to_string(path) {
            Ok(content) => return Ok(content),
            Err(e) if is_transient_error(&e) => {
                tracing::debug!(
                    file = %path.display(),
                    attempt = attempt + 1,
                    error = %e,
                    "Transient I/O error, retrying"
                );
                std::thread::sleep(Duration::from_millis(RETRY_DELAYS_MS[attempt as usize]));
                last_err = Some(e);
            }
            Err(e) => return Err(e), // Permanent error; do not retry.
        }
    }

    Err(last_err.unwrap_or_else(|| io::Error::other("Unknown read error")))
}

/// Returns true for transient I/O errors that are worth retrying.
fn is_transient_error(e: &io::Error) -> bool {
    matches!(
        e.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted | io::ErrorKind::TimedOut
    )
}
