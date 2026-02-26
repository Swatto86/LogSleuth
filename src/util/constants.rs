// LogSleuth - util/constants.rs
//
// Single source of truth for all named constants, limits, and defaults.
// Referenced by DevWorkflow Part A Rule 11 (explicit named-constant limits).

// =============================================================================
// Application metadata
// =============================================================================

/// Application display name.
pub const APP_NAME: &str = "LogSleuth";

/// Application identifier used for config/data directories.
pub const APP_ID: &str = "LogSleuth";

/// Current application version (updated by release script).
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

// =============================================================================
// Discovery limits
// =============================================================================

/// Maximum directory recursion depth during discovery.
pub const DEFAULT_MAX_DEPTH: usize = 10;

/// Minimum sensible value for the max-files limit (controls must be non-zero).
pub const MIN_MAX_FILES: usize = 1;

/// Maximum number of files to discover in a single scan.
pub const DEFAULT_MAX_FILES: usize = 500;

/// Hard upper bound on max files (prevents configuration mistakes).
pub const ABSOLUTE_MAX_FILES: usize = 10_000;

/// Hard upper bound on max depth (prevents infinite traversal).
pub const ABSOLUTE_MAX_DEPTH: usize = 50;

// =============================================================================
// Parsing limits
// =============================================================================

/// Default read chunk size in bytes for streaming file reads.
pub const DEFAULT_CHUNK_SIZE: usize = 64 * 1024; // 64 KB

/// Maximum size of a single log entry in bytes. Entries exceeding
/// this are truncated to prevent unbounded memory from malformed files.
pub const DEFAULT_MAX_ENTRY_SIZE: usize = 64 * 1024; // 64 KB

/// File size threshold in bytes above which a "large file" warning is shown.
pub const DEFAULT_LARGE_FILE_THRESHOLD: u64 = 100 * 1024 * 1024; // 100 MB

/// Number of lines sampled from the start of a file for format auto-detection.
pub const DEFAULT_CONTENT_DETECTION_LINES: usize = 20;

/// Default number of worker threads for parallel parsing.
/// 0 means auto-detect (use available CPU cores).
pub const DEFAULT_WORKER_THREADS: usize = 0;

/// Maximum number of parse errors tracked per file before suppression.
pub const MAX_PARSE_ERRORS_PER_FILE: usize = 1_000;

/// Maximum total parse errors tracked across all files in a scan.
pub const MAX_TOTAL_PARSE_ERRORS: usize = 10_000;

/// Hard upper bound on the total number of log entries held in memory at once.
///
/// Prevents out-of-memory crashes when scanning directories with many large,
/// high-frequency log files.  When the cap is reached the background scan
/// thread stops ingesting further entries and emits a warning so the user
/// knows data was truncated.  At ~1 KB per entry this caps heap usage at
/// roughly 1 GB for entries alone — well within 64-bit address space limits.
///
/// Users who need more entries should apply date or file-count filters to
/// narrow the scope of the scan.
pub const MAX_TOTAL_ENTRIES: usize = 1_000_000;

/// Minimum user-configurable entry cap.
pub const MIN_MAX_TOTAL_ENTRIES: usize = 10_000;

/// Maximum user-configurable entry cap (same as the absolute hard limit).
pub const ABSOLUTE_MAX_TOTAL_ENTRIES: usize = MAX_TOTAL_ENTRIES;

// =============================================================================
// Live tail limits
// =============================================================================

/// How often the tail watcher polls each watched file for new content (ms).
pub const TAIL_POLL_INTERVAL_MS: u64 = 500;

/// How often the cancel flag is checked within each poll sleep interval (ms).
/// The background thread wakes every this many ms to check for cancellation.
pub const TAIL_CANCEL_CHECK_INTERVAL_MS: u64 = 100;

/// Minimum user-configurable tail poll interval (ms).
pub const MIN_TAIL_POLL_INTERVAL_MS: u64 = 100;

/// Maximum user-configurable tail poll interval (ms).
pub const MAX_TAIL_POLL_INTERVAL_MS: u64 = 10_000; // 10 s

// =============================================================================
// Directory watcher limits
// =============================================================================

/// How often the directory watcher polls for new files (ms).
/// Balances responsiveness against CPU overhead from repeated directory walks.
pub const DIR_WATCH_POLL_INTERVAL_MS: u64 = 2_000;

/// How often the cancel flag is checked within each directory watch poll sleep (ms).
pub const DIR_WATCH_CANCEL_CHECK_INTERVAL_MS: u64 = 100;

/// Minimum user-configurable directory watch poll interval (ms).
pub const MIN_DIR_WATCH_POLL_INTERVAL_MS: u64 = 1_000; // 1 s

/// Maximum user-configurable directory watch poll interval (ms).
pub const MAX_DIR_WATCH_POLL_INTERVAL_MS: u64 = 60_000; // 60 s

/// Maximum bytes read from a single file in one poll tick.
/// Prevents a large burst of new content from stalling the entire poll loop.
pub const MAX_TAIL_READ_BYTES_PER_TICK: usize = 512 * 1_024; // 512 KiB

/// Maximum number of log entries included in a single "Copy Filtered Results"
/// clipboard export.  Prevents multi-second clipboard operations and excessive
/// memory allocation when the filtered set is very large.
pub const MAX_CLIPBOARD_ENTRIES: usize = 10_000;

// =============================================================================
// Per-frame UI message budgets (Rule 11: growing-collection bounds)
// =============================================================================

/// Maximum number of scan-progress messages processed by the UI update loop
/// per frame.  Any remaining messages are left in the channel and processed
/// on subsequent frames, preventing a burst from stalling the render loop.
pub const MAX_SCAN_MESSAGES_PER_FRAME: usize = 500;

/// Maximum number of live-tail messages processed per UI frame.
/// Tail messages arrive at the tail-poll cadence; bursty writes can queue
/// many messages before the next repaint.  This cap keeps frame times stable.
pub const MAX_TAIL_MESSAGES_PER_FRAME: usize = 200;

/// Maximum number of directory-watch messages processed per UI frame.
/// Directory events are rare; a small cap is sufficient.
pub const MAX_DIR_WATCH_MESSAGES_PER_FRAME: usize = 20;

/// Maximum number of non-fatal warnings accumulated across a single scan
/// session.  Prevents the warnings Vec from growing without bound when a
/// large directory contains many unreadable or unparseable files.
pub const MAX_WARNINGS: usize = 1_000;

// =============================================================================
// Profile limits
// =============================================================================

/// Maximum number of format profiles that can be loaded (built-in + user).
pub const MAX_PROFILES: usize = 100;

/// Maximum size of a profile TOML file in bytes.
pub const MAX_PROFILE_FILE_SIZE: u64 = 64 * 1024; // 64 KB

/// Maximum regex pattern length to prevent ReDoS.
pub const MAX_REGEX_PATTERN_LENGTH: usize = 4_096;

/// Minimum confidence threshold (0.0-1.0) for auto-detection to accept a match.
pub const AUTO_DETECT_MIN_CONFIDENCE: f64 = 0.3;

/// Confidence bonus added when a file's name matches one of a profile's
/// `file_patterns` globs.  Set to 0.3 so that an explicit filename match alone
/// is sufficient to assign the profile even if the sampled content lines do not
/// meet the content_match threshold (e.g. because the first N lines are
/// separator/header lines).  The patterns in built-in profiles are product-
/// specific enough (e.g. `Svc.Veeam*.log`) that false-positive assignments from
/// filename alone are negligible.
pub const AUTO_DETECT_FILENAME_BONUS: f64 = 0.3;

// =============================================================================
// UI defaults
// =============================================================================

/// Default time window (seconds) for cross-log correlation.
pub const DEFAULT_CORRELATION_WINDOW_SECS: i64 = 30;

/// Minimum configurable correlation window (seconds).
/// Prevents the window from collapsing to zero, which would only ever
/// match the anchor entry itself and provides no useful context.
pub const MIN_CORRELATION_WINDOW_SECS: i64 = 1;

/// Maximum configurable correlation window (seconds).
/// 1 hour is a generous upper bound; beyond this the "context" becomes
/// too broad to be meaningful for most log correlation workflows.
pub const MAX_CORRELATION_WINDOW_SECS: i64 = 3_600;

/// Debounce delay in milliseconds for text filter input.
pub const DEFAULT_FILTER_DEBOUNCE_MS: u64 = 300;

/// Number of entries above which an export warning is displayed.
pub const DEFAULT_LARGE_EXPORT_THRESHOLD: usize = 100_000;

// =============================================================================
// File discovery patterns
// =============================================================================

/// Default include glob patterns for log file discovery.
pub const DEFAULT_INCLUDE_PATTERNS: &[&str] = &["*.log", "*.log.[0-9]*", "*.txt"];

/// Default exclude glob patterns for log file discovery.
pub const DEFAULT_EXCLUDE_PATTERNS: &[&str] = &[
    "*.gz",
    "*.zip",
    "*.bak",
    "*.tmp",
    "node_modules",
    ".git",
    "__pycache__",
];

// =============================================================================
// Logging
// =============================================================================

/// Default log level.
pub const DEFAULT_LOG_LEVEL: &str = "info";

/// Maximum length of a log line included in debug output.
/// Prevents accidental exposure of sensitive data in long lines.
pub const DEBUG_MAX_LINE_PREVIEW: usize = 200;

// =============================================================================
// Export
// =============================================================================

/// Maximum number of entries that can be exported in a single operation.
pub const MAX_EXPORT_ENTRIES: usize = 5_000_000;

// =============================================================================
// Configuration
// =============================================================================

/// Configuration file name.
pub const CONFIG_FILE_NAME: &str = "config.toml";

/// Session persistence file name (stored in the platform data directory).
pub const SESSION_FILE_NAME: &str = "session.json";

/// User profiles subdirectory name.
pub const PROFILES_DIR_NAME: &str = "profiles";
