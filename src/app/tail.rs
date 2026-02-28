// LogSleuth - app/tail.rs
//
// Live tail: watches loaded files for new lines appended after the initial
// scan and streams them to the UI in real time.
//
// Architecture:
//   - `TailManager` lives on the UI thread; `run_tail_watcher` runs on a
//     background thread polling each file for new content on a fixed interval.
//   - An `Arc<AtomicBool>` cancel flag allows the UI to stop the tail.
//   - New entries are sent as `TailProgress::NewEntries` over an mpsc channel.
//   - The UI thread polls the channel each frame (same pattern as ScanManager).
//
// Encoding: tail reads new bytes and decodes them as lossy UTF-8.
// UTF-16 encoded files (Windows system logs) are generally not appended
// line-by-line by the OS, so this limitation is acceptable and documented.
//
// Rule 11 compliance:
//   - File read/stat errors on a single file are non-fatal: logged as warnings,
//     a FileError message is sent, and the watcher continues to the next file.
//   - Truncated/rotated files (size < last offset) are handled by resetting the
//     offset to 0 so the rewritten content is picked up cleanly.
//   - The poll loop sleeps in small sub-intervals so cancel is checked promptly
//     (within TAIL_CANCEL_CHECK_INTERVAL_MS of the cancel flag being set).
//   - MAX_TAIL_READ_BYTES_PER_TICK caps the bytes consumed per file per tick to
//     prevent a burst of large writes from stalling the entire poll loop.

use crate::core::model::{FormatProfile, TailProgress};
use crate::core::parser::{self, ParseConfig};
use chrono::Utc;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Duration;

// =============================================================================
// Constants (Rule 11: named bounds — defined in util::constants and re-used
// here via the constant names; the actual values live in constants.rs).
// =============================================================================

use crate::util::constants::{
    MAX_TAIL_PARTIAL_BYTES, MAX_TAIL_READ_BYTES_PER_TICK, TAIL_CANCEL_CHECK_INTERVAL_MS,
};

// =============================================================================
// Public types
// =============================================================================

/// Identifies a file to watch in live tail mode together with its resolved
/// format profile for incremental parsing.
pub struct TailFileInfo {
    /// Full path to the file.
    pub path: PathBuf,
    /// Format profile used to parse new lines from this file.
    pub profile: FormatProfile,
    /// Byte offset to start reading from on the first poll tick.
    ///
    /// Should be set to the file size recorded at the end of the initial scan
    /// (`DiscoveredFile::size`) so that any bytes appended between the scan
    /// completing and Live Tail being activated are picked up on the first poll
    /// — eliminating the "gap" where new entries are silently skipped.
    ///
    /// `None` falls back to the old behaviour of seeking to the current EOF at
    /// tail-activation time (safe but lossy when tailing is started late).
    pub initial_offset: Option<u64>,
}

// =============================================================================
// TailManager
// =============================================================================

/// Manages a live tail operation on a background thread.
///
/// The manager lives on the UI thread and exposes a simple start/stop/poll
/// interface that mirrors `ScanManager`.
pub struct TailManager {
    /// Channel receiver for the UI to poll tail progress messages.
    pub progress_rx: Option<mpsc::Receiver<TailProgress>>,
    /// Cancel flag shared with the background thread.
    cancel_flag: Option<Arc<AtomicBool>>,
}

impl TailManager {
    pub fn new() -> Self {
        Self {
            progress_rx: None,
            cancel_flag: None,
        }
    }

    /// Start tailing the given files from their *current end* (new content only).
    ///
    /// Spawns a background poll thread immediately. If a tail is already running
    /// it is stopped first.
    ///
    /// `entry_id_start` is the next available monotonic entry ID so that tail
    /// entries do not collide with IDs assigned during the initial scan.
    /// `poll_interval_ms` controls how often each watched file is checked for
    /// new content.  Lower values give faster tail updates at the cost of more
    /// I/O; higher values reduce CPU/disk overhead.
    pub fn start_tail(
        &mut self,
        files: Vec<TailFileInfo>,
        entry_id_start: u64,
        poll_interval_ms: u64,
    ) {
        self.stop_tail();

        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));

        self.progress_rx = Some(rx);
        self.cancel_flag = Some(Arc::clone(&cancel));

        let file_count = files.len();
        std::thread::spawn(move || {
            run_tail_watcher(files, entry_id_start, tx, cancel, poll_interval_ms);
        });

        tracing::info!(files = file_count, poll_interval_ms, "Live tail started");
    }

    /// Request the background tail thread to stop.
    ///
    /// The thread will exit within `TAIL_CANCEL_CHECK_INTERVAL_MS` and send
    /// `TailProgress::Stopped` before terminating.
    pub fn stop_tail(&mut self) {
        if let Some(flag) = &self.cancel_flag {
            flag.store(true, Ordering::SeqCst);
        }
        self.cancel_flag = None;
        self.progress_rx = None;
    }

    /// Returns `true` if a tail background thread is currently active.
    pub fn is_active(&self) -> bool {
        self.cancel_flag.is_some()
    }

    /// Poll for pending tail progress messages without blocking.
    ///
    /// Drains at most `max` messages per call.  Any messages beyond the budget
    /// remain in the channel and are picked up on the next call.  This keeps
    /// frame times stable when a burst of tail entries arrives (Rule 11).
    pub fn poll_progress(&self, max: usize) -> Vec<TailProgress> {
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

impl Default for TailManager {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Per-file state (private to the background thread)
// =============================================================================

struct FileState {
    path: PathBuf,
    profile: FormatProfile,
    /// Byte position of the last byte examined in the file.
    /// Always advances by exactly the number of bytes read each tick,
    /// whether those bytes produced complete lines or not.
    offset: u64,
    /// Bytes from the most recent read that followed the final newline —
    /// they represent an in-progress (incomplete) log line. Prepended to the
    /// next tick's decoded bytes before searching for newlines.
    partial: String,
}

// =============================================================================
// Background tail watcher
// =============================================================================

/// Background poll loop. Checks each file every `poll_interval_ms` ms for
/// new content and sends parsed entries back to the UI via `tx`.
fn run_tail_watcher(
    files: Vec<TailFileInfo>,
    entry_id_start: u64,
    tx: mpsc::Sender<TailProgress>,
    cancel: Arc<AtomicBool>,
    poll_interval_ms: u64,
) {
    macro_rules! send {
        ($msg:expr) => {
            if tx.send($msg).is_err() {
                // UI channel closed — exit silently.
                return;
            }
        };
    }

    let parse_config = ParseConfig::default();

    // Initialise per-file state.
    //
    // Offset priority:
    //   1. `initial_offset` supplied by the caller (the file size at scan-end).
    //      Using the scan-end size means the first poll tick catches any bytes
    //      appended between scan completion and tail activation — the "gap".
    //   2. Fallback: current EOF (stat the file now).  Used when no scan-end
    //      offset is available (e.g. files opened via Open Log(s)...).
    let mut states: Vec<FileState> = files
        .into_iter()
        .map(|info| {
            let offset = info
                .initial_offset
                .unwrap_or_else(|| std::fs::metadata(&info.path).map(|m| m.len()).unwrap_or(0));
            tracing::debug!(
                file = %info.path.display(),
                offset,
                from_scan = info.initial_offset.is_some(),
                "Tail: seeding initial offset"
            );
            FileState {
                path: info.path,
                profile: info.profile,
                offset,
                partial: String::new(),
            }
        })
        .collect();

    let file_count = states.len();
    send!(TailProgress::Started { file_count });

    // Single monotonically increasing ID counter shared across all watched files.
    let mut next_id = entry_id_start;

    // Sub-divide each poll interval into cancel-check slices.
    // Total sleep = poll_interval_ms; wake every TAIL_CANCEL_CHECK_INTERVAL_MS.
    let slices = (poll_interval_ms / TAIL_CANCEL_CHECK_INTERVAL_MS).max(1);

    loop {
        // Interruptible sleep: check cancel flag between slices.
        for _ in 0..slices {
            std::thread::sleep(Duration::from_millis(TAIL_CANCEL_CHECK_INTERVAL_MS));
            if cancel.load(Ordering::SeqCst) {
                send!(TailProgress::Stopped);
                return;
            }
        }

        for state in &mut states {
            if cancel.load(Ordering::SeqCst) {
                send!(TailProgress::Stopped);
                return;
            }

            // -----------------------------------------------------------------
            // 1. Check current file size and last-modified time.
            //    Both come from a single metadata() call to avoid TOCTOU.
            // -----------------------------------------------------------------
            let (current_size, file_mtime) = match std::fs::metadata(&state.path) {
                Ok(m) => {
                    let size = m.len();
                    let mtime: Option<chrono::DateTime<Utc>> = m.modified().ok().map(|t| t.into());
                    (size, mtime)
                }
                Err(e) => {
                    let msg = format!("Cannot stat: {e}");
                    tracing::warn!(file = %state.path.display(), error = %e, "Tail: stat error");
                    send!(TailProgress::FileError {
                        path: state.path.clone(),
                        message: msg,
                    });
                    continue;
                }
            };

            // -----------------------------------------------------------------
            // 2. Handle rotation / truncation.
            // -----------------------------------------------------------------
            if current_size < state.offset {
                tracing::info!(
                    file = %state.path.display(),
                    old_offset = state.offset,
                    new_size = current_size,
                    "Tail: file truncated or rotated — resetting offset to 0"
                );
                state.offset = 0;
                state.partial.clear();
            }

            // -----------------------------------------------------------------
            // 3. Nothing new.
            // -----------------------------------------------------------------
            if current_size == state.offset {
                continue;
            }

            // -----------------------------------------------------------------
            // 4. Read new bytes (capped per tick).
            // -----------------------------------------------------------------
            let bytes_available =
                usize::try_from(current_size - state.offset).unwrap_or(usize::MAX);
            let read_limit = bytes_available.min(MAX_TAIL_READ_BYTES_PER_TICK);

            let new_bytes = match read_bytes_at(&state.path, state.offset, read_limit) {
                Ok(b) => b,
                Err(e) => {
                    let msg = format!("Read error: {e}");
                    tracing::warn!(file = %state.path.display(), error = %e, "Tail: read error");
                    send!(TailProgress::FileError {
                        path: state.path.clone(),
                        message: msg,
                    });
                    continue;
                }
            };

            let n = new_bytes.len();
            if n == 0 {
                continue;
            }

            // Advance offset unconditionally — we have consumed these bytes
            // whether they produce complete lines or not.
            state.offset += n as u64;

            // -----------------------------------------------------------------
            // 5. Decode (lossy UTF-8) and append to the partial-line buffer.
            // -----------------------------------------------------------------
            let decoded = String::from_utf8_lossy(&new_bytes);
            state.partial.push_str(&decoded);

            // Bound the partial buffer (Rule 11 — resource bounds on growing
            // collections).  A file that never emits newlines — binary content,
            // an extremely long structured-log line, or a non-text file opened
            // by mistake — would otherwise cause the buffer to grow at
            // MAX_TAIL_READ_BYTES_PER_TICK per tick without any upper limit,
            // eventually exhausting heap.  When the bound is hit we discard the
            // stale fragment, emit a warning, and skip to the next file; the
            // watcher remains alive and will resume on the next newline.
            if state.partial.len() > MAX_TAIL_PARTIAL_BYTES {
                tracing::warn!(
                    file = %state.path.display(),
                    partial_bytes = state.partial.len(),
                    limit = MAX_TAIL_PARTIAL_BYTES,
                    "Tail: partial buffer exceeded limit — discarding fragment \
                     (binary content or extremely long line?)"
                );
                send!(TailProgress::FileError {
                    path: state.path.clone(),
                    message: format!(
                        "Partial-line buffer exceeded {MAX_TAIL_PARTIAL_BYTES}B; \
                         content may be binary or contain very long lines."
                    ),
                });
                state.partial.clear();
                continue;
            }

            // -----------------------------------------------------------------
            // 6. Split at the last newline.
            //    Everything up to and including the final '\n' can be parsed.
            //    Bytes after the final '\n' are an in-progress line — carry forward.
            // -----------------------------------------------------------------
            let complete_text = match state.partial.rfind('\n') {
                Some(nl_pos) => {
                    let complete = state.partial[..=nl_pos].to_string();
                    // Keep the tail after the last newline for the next tick.
                    state.partial = state.partial[nl_pos + 1..].to_string();
                    complete
                }
                None => {
                    // No newline yet — the entire buffer is an in-progress line.
                    continue;
                }
            };

            // -----------------------------------------------------------------
            // 7. Parse complete lines through the file's format profile.
            // -----------------------------------------------------------------
            let mut result = parser::parse_content(
                &complete_text,
                &state.path,
                &state.profile,
                &parse_config,
                next_id,
            );

            if result.entries.is_empty() {
                continue;
            }

            // Stamp entries with the file's OS mtime and back-fill a parsed
            // timestamp if the profile has no timestamp capture group.
            //
            // file_modified: used by the time-range filter (e.g. last 15 min).
            //   Files being actively written have a fresh mtime, so they always
            //   pass a relative-time filter regardless of log line content.
            //
            // timestamp: back-filled with now() only when the profile produced
            //   None (plain-text fallback). This keeps timeline sort order
            //   correct and prevents tail entries from sinking to the bottom.
            let now = Utc::now();
            let effective_mtime = file_mtime.unwrap_or(now);
            for entry in &mut result.entries {
                entry.file_modified = Some(effective_mtime);
                if entry.timestamp.is_none() {
                    entry.timestamp = Some(now);
                }
            }

            tracing::debug!(
                file = %state.path.display(),
                count = result.entries.len(),
                "Tail: new entries"
            );

            next_id += result.entries.len() as u64;

            send!(TailProgress::NewEntries {
                entries: result.entries,
            });
        }
    }
}

/// Read exactly `limit` bytes from `path` starting at byte position `offset`.
///
/// Returns fewer bytes than `limit` if the file ends before `limit` is reached.
fn read_bytes_at(path: &std::path::Path, offset: u64, limit: usize) -> std::io::Result<Vec<u8>> {
    let mut file = std::fs::File::open(path)?;
    file.seek(SeekFrom::Start(offset))?;
    let mut buf = vec![0u8; limit];
    let n = file.read(&mut buf)?;
    buf.truncate(n);
    Ok(buf)
}
