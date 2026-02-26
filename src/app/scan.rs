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

    /// Start parsing a pre-selected list of individual files in *append* mode.
    ///
    /// Unlike `start_scan`, this does NOT clear existing state — the caller is
    /// responsible for deciding whether to clear before calling.  The parsed
    /// entries are streamed via `EntriesBatch` and the file list is sent via
    /// `AdditionalFilesDiscovered` so the UI extends rather than replaces its
    /// current file list.
    pub fn start_scan_files(&mut self, files: Vec<PathBuf>, profiles: Vec<FormatProfile>) {
        self.cancel_scan();

        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));

        self.progress_rx = Some(rx);
        self.cancel_flag = Some(Arc::clone(&cancel));

        let parse_config = ParseConfig::default();

        std::thread::spawn(move || {
            run_files_scan(files, profiles, parse_config, tx, cancel);
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
    // Phase 1: Discovery
    // -------------------------------------------------------------------------
    send!(ScanProgress::DiscoveryStarted);

    let tx_discovery = tx.clone();
    let (discovered_files, warnings, total_found) =
        match discovery::discover_files(&root, &config, |path, count| {
            tracing::trace!(file = %path.display(), count, "File discovered");
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
    // Phase 2: Auto-detection
    // -------------------------------------------------------------------------
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
        } else {
            // No specific profile matched: fall back to plain-text.
            if profiles.iter().any(|p| p.id == "plain-text") {
                file.profile_id = Some("plain-text".to_string());
                file.detection_confidence = 0.0;
                tracing::debug!(
                    file = %file.path.display(),
                    "No structured profile matched; falling back to plain-text"
                );
            }
        }
    }

    send!(ScanProgress::DiscoveryCompleted {
        total_files,
        total_found,
    });

    // Send file list. Append mode extends UI list; normal mode replaces it.
    if append {
        send!(ScanProgress::AdditionalFilesDiscovered {
            files: discovered_files.clone(),
        });
    } else {
        send!(ScanProgress::FilesDiscovered {
            files: discovered_files.clone(),
        });
    }

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
    // Collect all entries here; sort chronologically on this background thread
    // before streaming to the UI so the UI thread never blocks on a large sort.
    let mut all_entries: Vec<LogEntry> = Vec::new();
    let mut file_summaries: Vec<FileSummary> = Vec::new();
    // Flag set when MAX_TOTAL_ENTRIES is reached so processing stops cleanly.
    let mut entry_cap_reached = false;

    for (idx, file) in discovered_files.iter().enumerate() {
        check_cancel!();

        // Stop ingesting new files once the hard entry cap is reached (Rule 11).
        // Continuing to parse would allocate unbounded memory, causing an OOM
        // crash with no warning — the original bug this guard prevents.
        if entry_cap_reached {
            break;
        }

        let files_completed = idx + 1;

        let profile_id = match &file.profile_id {
            Some(id) => id.clone(),
            None => {
                tracing::debug!(file = %file.path.display(), "No profile assigned, skipping");
                send!(ScanProgress::Warning {
                    message: format!(
                        "'{}': no format profile could be assigned (even plain-text was unavailable), file skipped",
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

        let mut parse_result = parser::parse_content(
            &content,
            &file.path,
            matched_profile,
            &parse_config,
            entry_id,
        );

        // Stamp the source file's OS last-modified time on every entry.
        // The time-range filter uses file_modified (not the parsed log timestamp)
        // so filtering by "last 15 minutes" correctly includes plain-text entries
        // and any log whose embedded timestamps are missing or malformed.
        let file_mtime = file.modified;
        for entry in &mut parse_result.entries {
            entry.file_modified = file_mtime;
        }

        let entry_count = parse_result.entries.len();
        let error_count = parse_result.errors.len();

        entry_id += entry_count as u64;
        total_entries += entry_count;
        total_errors += error_count;

        if entry_count > 0 {
            files_with_entries += 1;
        }

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

        for err in &parse_result.errors {
            tracing::debug!(error = %err, "Parse error");
        }

        // Enforce the hard entry cap (Rule 11: bounded collections).
        // All entries from this file are still counted in file_summaries so
        // the scan summary accurately reflects what was found vs. what was loaded.
        let remaining_capacity =
            crate::util::constants::MAX_TOTAL_ENTRIES.saturating_sub(all_entries.len());
        if remaining_capacity == 0 {
            // Cap already hit before this file; entries were skipped above.
            // This branch should not be reached now that the loop breaks early,
            // but guard defensively.
        } else if parse_result.entries.len() <= remaining_capacity {
            all_entries.extend(parse_result.entries);
        } else {
            // Partial ingest: take as many entries as fit, then trigger the cap.
            all_entries.extend(parse_result.entries.into_iter().take(remaining_capacity));
            entry_cap_reached = true;
            let cap = crate::util::constants::MAX_TOTAL_ENTRIES;
            let msg = format!(
                "Entry limit reached: {cap} entries loaded. \
                 Remaining files in this scan have been skipped to prevent an out-of-memory crash. \
                 Use the date filter or reduce the max-files limit to target a smaller dataset."
            );
            tracing::warn!("{}", msg);
            send!(ScanProgress::Warning { message: msg });
        }

        send!(ScanProgress::FileParsed {
            path: file.path.clone(),
            entries: entry_count,
            errors: error_count,
            files_completed,
            total_files,
        });
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
    check_cancel!();
    for chunk in all_entries.chunks(ENTRY_BATCH_SIZE) {
        send!(ScanProgress::EntriesBatch {
            entries: chunk.to_vec(),
        });
        check_cancel!();
    }

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

    for path in &paths {
        match std::fs::metadata(path) {
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
    );
}

// =============================================================================
// File reading helpers
// =============================================================================

/// Read up to `max_lines` lines from the start of a file for auto-detection.
/// Returns an empty vec on any I/O error (non-fatal; auto-detection will skip).
///
/// Handles UTF-16 LE/BE encoded files (common in C:\Windows\Logs) by first
/// reading the full content with encoding detection, then splitting into lines.
/// This ensures auto-detection works on Windows system log files.
fn read_sample_lines(path: &Path, max_lines: usize) -> Vec<String> {
    // Try fast UTF-8 path first.
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) => {
            tracing::debug!(file = %path.display(), error = %e, "Cannot open for sampling");
            return Vec::new();
        }
    };

    let lines: Vec<String> = BufReader::new(file)
        .lines()
        .take(max_lines)
        .filter_map(Result::ok)
        .collect();

    // If we got lines, we're done (UTF-8 file).
    if !lines.is_empty() {
        return lines;
    }

    // Empty result may mean UTF-16 LE/BE encoding. Try encoding-aware read.
    match std::fs::read(path) {
        Ok(bytes) if bytes.len() >= 2 => {
            if let Ok(content) = decode_bytes(&bytes, path) {
                return content
                    .lines()
                    .take(max_lines)
                    .map(ToString::to_string)
                    .collect();
            }
            Vec::new()
        }
        _ => Vec::new(),
    }
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

    // Fast path: valid UTF-8 (most log files).
    if let Ok(s) = std::str::from_utf8(&mmap) {
        return Ok(s.to_string());
    }

    // Encoding fallback: handles UTF-16 LE/BE (Windows system logs) and ANSI.
    decode_bytes(&mmap, path)
}

/// Read a small file with transient-error retries.
fn read_small_file_with_retry(path: &Path) -> io::Result<String> {
    let mut last_err: Option<io::Error> = None;

    for attempt in 0..MAX_RETRIES {
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
                std::thread::sleep(Duration::from_millis(RETRY_DELAYS_MS[attempt as usize]));
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

    // No recognised BOM: treat as lossy UTF-8 / ANSI.
    tracing::debug!(file = %path.display(), "Decoded file as lossy UTF-8 (no BOM)");
    Ok(String::from_utf8_lossy(bytes).into_owned())
}

/// Returns true for transient I/O errors that are worth retrying.
fn is_transient_error(e: &io::Error) -> bool {
    matches!(
        e.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted | io::ErrorKind::TimedOut
    )
}
