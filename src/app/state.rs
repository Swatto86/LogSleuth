// LogSleuth - app/state.rs
//
// Application state management. Holds the current scan results,
// filter state, selection, and profile list.
// Owned by the eframe::App implementation.

use crate::core::filter::{DedupInfo, DedupMode, FilterState};
use crate::core::model::{DiscoveredFile, FormatProfile, LogEntry, ScanSummary};
use crate::util::constants::{DEFAULT_CORRELATION_WINDOW_SECS, MAX_CLIPBOARD_ENTRIES};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::PathBuf;

/// Top-level application state.
#[derive(Debug)]
pub struct AppState {
    /// Currently loaded format profiles.
    pub profiles: Vec<FormatProfile>,

    /// Current scan directory (None if no scan has been started).
    pub scan_path: Option<PathBuf>,

    /// Whether a scan is currently in progress.
    pub scan_in_progress: bool,

    /// Set to `true` by the `pending_scan` and `pending_replace_files` GUI
    /// handlers when the user opens a new directory or file set interactively.
    /// Cleared in `ParsingCompleted`.  Causes `ParsingCompleted` to default
    /// the file list to nothing-checked so the user explicitly selects which
    /// files to view (opt-in model).  Not set for CLI-driven or append scans.
    pub fresh_scan_in_progress: bool,

    /// Files discovered in the current scan.
    pub discovered_files: Vec<DiscoveredFile>,

    /// All parsed log entries from the current scan.
    pub entries: Vec<LogEntry>,

    /// Indices of entries matching the current filter (into `entries`).
    pub filtered_indices: Vec<usize>,

    /// Current filter configuration.
    pub filter_state: FilterState,

    /// Index of the currently selected entry in filtered_indices.
    pub selected_index: Option<usize>,

    /// Set of selected indices into `filtered_indices` for multi-select.
    ///
    /// Populated by Ctrl+Click (toggle) and Shift+Click (range) in the
    /// timeline panel.  When non-empty, the right-click context menu offers
    /// "Copy Selected Lines" which copies the raw text of all selected entries.
    ///
    /// Uses `BTreeSet` so iteration is always in ascending order (matching
    /// the chronological display for ascending sort).
    ///
    /// Cleared by `clear()` and rebuilt by ID across `apply_filters()` so
    /// multi-select survives filter changes.
    pub selected_indices: BTreeSet<usize>,

    /// Scan summary from the most recent completed scan.
    pub scan_summary: Option<ScanSummary>,

    /// Status message for the status bar.
    pub status_message: String,

    /// Non-fatal warnings accumulated during the current scan.
    pub warnings: Vec<String>,

    /// Whether to show the scan summary dialog.
    pub show_summary: bool,

    /// Whether to show the log-entry summary panel.
    pub show_log_summary: bool,

    /// Whether to show the About dialog.
    pub show_about: bool,

    /// UI body font size in points.  Persists across `clear()` calls — it is a
    /// user preference, not scan state.  Applied every frame in `gui.rs` via
    /// `ctx.set_style()` so all text tracks this value.
    pub ui_font_size: f32,

    /// Whether the UI is rendering in dark mode (true) or light mode (false).
    /// Persists across `clear()` calls — it is a user preference, not scan state.
    /// Applied every frame via `ctx.set_visuals()` in `gui.rs`.
    pub dark_mode: bool,

    /// Timeline sort order.  `false` = ascending (oldest first, default);
    /// `true` = descending (newest first).
    /// Persists across `clear()` calls — it is a user preference, not scan state.
    pub sort_descending: bool,

    /// Whether debug mode is enabled.
    pub debug_mode: bool,

    /// Set by a UI panel to request starting a new scan on this path.
    /// Consumed and cleared by `gui.rs` in the update loop each frame.
    pub pending_scan: Option<PathBuf>,

    /// Set by a UI panel to request cancellation of the running scan.
    /// Consumed and cleared by `gui.rs` in the update loop each frame.
    pub request_cancel: bool,

    /// Text typed into the file-list search box in the filters panel.
    /// Filters which filenames are shown in the source-file checklist.
    /// Pure UI state: does not affect the filter logic itself.
    pub file_list_search: String,

    /// Per-file palette colour assignments for the CMTrace-style coloured
    /// stripe shown on every timeline row. Keys are full file paths.
    /// Populated by `assign_file_colour` when files are discovered.
    pub file_colours: HashMap<PathBuf, egui::Color32>,

    /// Set by the UI to request parsing a specific list of files in append
    /// mode (adds to the current session without clearing existing entries).
    /// Consumed and cleared by `gui.rs` in the update loop each frame.
    pub pending_single_files: Option<Vec<PathBuf>>,

    /// Set by the discovery panel to request a follow-up parse of all files
    /// that were skipped due to an active `parse_path_filter` on the last scan.
    /// Consumed and cleared by `gui.rs` each frame.
    pub pending_parse_skipped: bool,

    // -------------------------------------------------------------------------
    // Activity window
    // -------------------------------------------------------------------------
    /// Rolling "activity window" in seconds.  When `Some(n)`, only files whose
    /// OS last-modified time is within the last `n` seconds are shown in the
    /// file list; all entries from files outside that window are hidden from the
    /// timeline.  The cutoff is re-evaluated every second so stale files age
    /// out automatically as the clock advances.
    ///
    /// Uses `DiscoveredFile::modified` (kept live by the dir-watcher mtime
    /// polling) rather than parsed log timestamps, so it works correctly for
    /// plain-text logs and files whose embedded timestamps are unreliable.
    ///
    /// Fail-open: files whose mtime cannot be read are always treated as active
    /// so a metadata failure never silently hides a log file.
    ///
    /// `None` means the feature is off; all files and entries are shown.
    pub activity_window_secs: Option<u64>,

    /// UI text buffer for the custom activity-window duration input.
    /// Stores the value typed by the user (e.g. "90" for 90 seconds).
    pub activity_window_input: String,

    // -------------------------------------------------------------------------
    // Directory watcher state
    // -------------------------------------------------------------------------
    /// Whether the recursive directory watcher background thread is currently
    /// running.  Set to `true` when the watcher is started after a directory
    /// scan completes; cleared on `clear()` / `new_session()`.
    ///
    /// The watcher monitors the scan directory for newly created log files and
    /// automatically adds them to the session in real time.
    pub dir_watcher_active: bool,

    /// True while the watcher's background walk thread is actively scanning
    /// the directory tree for new files.  Shown in the WATCH badge tooltip
    /// so the user can see that the watcher is alive and working on UNC/SMB
    /// shares where each walk can take several minutes.
    pub dir_watcher_scanning: bool,

    // -------------------------------------------------------------------------
    // Live tail state
    // -------------------------------------------------------------------------
    /// Whether the live tail watcher is currently running.
    pub tail_active: bool,

    /// When true, the timeline auto-scrolls to the bottom whenever new tail
    /// entries arrive. The user can toggle this off to scroll back through history.
    pub tail_auto_scroll: bool,

    /// Set by a UI panel to request starting the live tail watcher.
    /// Consumed and cleared by `gui.rs` in the update loop each frame.
    pub request_start_tail: bool,

    /// Set by a UI panel to request stopping the live tail watcher.
    /// Consumed and cleared by `gui.rs` in the update loop each frame.
    pub request_stop_tail: bool,

    // -------------------------------------------------------------------------
    // Bookmarks
    // -------------------------------------------------------------------------
    /// Bookmarked entry IDs mapped to an optional annotation label.
    /// An empty string label means no annotation was added by the user.
    /// Bookmark state survives filter changes but is cleared on `clear()`.
    pub bookmarks: HashMap<u64, String>,

    // -------------------------------------------------------------------------
    // Time correlation
    // -------------------------------------------------------------------------
    /// Whether the time correlation overlay is active.
    ///
    /// When active, selecting a timeline entry recomputes `correlated_ids`
    /// and the timeline renders a teal highlight on every entry whose
    /// timestamp falls within [anchor - window, anchor + window].
    pub correlation_active: bool,

    /// Half-window size in seconds for the correlation overlay.
    /// Bounded to [MIN_CORRELATION_WINDOW_SECS, MAX_CORRELATION_WINDOW_SECS].
    pub correlation_window_secs: i64,

    /// UI text buffer for the window-size input in the filters panel.
    /// Kept in sync with `correlation_window_secs` after each committed edit.
    pub correlation_window_input: String,

    /// Pre-computed set of entry IDs whose timestamps lie within the current
    /// correlation window around the selected entry.
    ///
    /// Populated by `update_correlation()`; used by `timeline.rs` for the
    /// teal highlight overlay. Contains all entries in the window (including
    /// those hidden by the current filter) so context is never silently missing.
    pub correlated_ids: HashSet<u64>,

    // -------------------------------------------------------------------------
    // Session persistence
    // -------------------------------------------------------------------------
    /// Absolute path to the session file.
    /// Set once at startup from `platform_paths.data_dir / SESSION_FILE_NAME`.
    /// Never cleared; persists across `clear()` calls.
    pub session_path: Option<PathBuf>,

    /// Set at startup when restoring a session with a saved scan path.
    /// Consumed by `gui.rs` WITHOUT calling `clear()` first (unlike `pending_scan`)
    /// so the restored filter/colour/bookmark state is preserved during the re-scan.
    pub initial_scan: Option<PathBuf>,

    /// Files added to the current session via "Add File(s)\u2026" that are persisted
    /// so they can be re-added after the next session restore.
    ///
    /// When `save_session()` runs, the contents of this Vec are written to
    /// `SessionData.extra_files`.  On the following launch, after the main
    /// `scan_path` scan completes, `gui.rs` triggers an append scan for any of
    /// these paths not already discovered by the directory scan.
    ///
    /// Cleared by `clear()` so it is scoped to the active scan-directory session.
    pub manually_added_files: Vec<PathBuf>,

    /// Extra files loaded from `SessionData.extra_files` on session restore.
    ///
    /// Populated by `restore_from_session()` and consumed exactly once by
    /// `gui.rs` when the first `ParsingCompleted` arrives after startup — an
    /// append scan is then triggered for any of these paths not already in
    /// `discovered_files`.  Cleared by `clear()` so a mid-session new-directory
    /// open does not accidentally re-add stale paths from the old session.
    pub extra_files_to_restore: Vec<PathBuf>,

    // -------------------------------------------------------------------------
    // Options / ingest limits
    // -------------------------------------------------------------------------
    /// Maximum number of files to ingest in a single directory scan.
    /// User-configurable via the Options dialog; defaults to DEFAULT_MAX_FILES.
    /// Changes are applied to the next scan.
    pub max_files_limit: usize,

    /// Maximum total log entries to hold in memory across all scanned files.
    /// User-configurable via the Options dialog; defaults to MAX_TOTAL_ENTRIES.
    /// Changes are applied to the next scan.
    pub max_total_entries: usize,

    /// Maximum directory recursion depth for scans and the directory watcher.
    /// User-configurable via the Options dialog; defaults to DEFAULT_MAX_DEPTH.
    /// Changes are applied to the next scan/watch start.
    pub max_scan_depth: usize,

    /// How often the live tail background thread polls watched files (ms).
    /// User-configurable via the Options dialog; defaults to TAIL_POLL_INTERVAL_MS.
    /// Applied when a new tail session is started.
    pub tail_poll_interval_ms: u64,

    /// How often the directory watcher polls for new files (ms).
    /// User-configurable via the Options dialog; defaults to DIR_WATCH_POLL_INTERVAL_MS.
    /// Applied when a new directory watch session is started.
    pub dir_watch_poll_interval_ms: u64,

    /// Whether the Options dialog is currently open.
    pub show_options: bool,

    /// When true the timeline should snap its scroll position to the top on the
    /// next rendered frame.  Set by the tail polling loop in `gui.rs` whenever
    /// new entries arrive while the timeline is in descending (newest-first)
    /// sort order with auto-scroll enabled.  Consumed and cleared immediately
    /// by `timeline.rs` so it fires exactly once per batch of new entries.
    pub scroll_top_requested: bool,

    /// Total files found during the last discovery pass **before** the ingest
    /// limit was applied. Equals `discovered_files.len()` when no truncation
    /// occurred. Used to display "Found N, showing M" in the status bar.
    pub total_files_found: usize,

    /// Set by the `DiscoveryCompleted` handler when the raw file count exceeds
    /// the ingest limit.  Consumed by the `ParsingCompleted` handler to decide
    /// whether to show the truncation hint.  Prevents false positives on append
    /// scans where `total_files_found` is stale from the initial directory scan.
    pub discovery_truncated: bool,

    /// Set by the discovery panel "Open Log(s)..." button to request starting
    /// a fresh session with a user-selected list of individual files.
    /// Consumed and cleared by `gui.rs` (calls `clear()` then `start_scan_files`).
    pub pending_replace_files: Option<Vec<PathBuf>>,

    /// Set by any UI panel to request a full session reset: clears all scan
    /// results **and** the selected directory path, returning to the initial
    /// "no directory selected" state. Consumed and cleared by `gui.rs`.
    pub request_new_session: bool,

    /// Datetime filter typed by the user in the "Open Directory" date-filter box.
    ///
    /// Accepted formats (most-specific first):
    ///   `YYYY-MM-DD HH:MM:SS` — second precision
    ///   `YYYY-MM-DD HH:MM`    — minute precision
    ///   `YYYY-MM-DD`          — day precision (treated as 00:00:00 local time)
    ///
    /// An empty string means no filter.  Parsed by `discovery_modified_since()`
    /// as **local time** (then converted to UTC internally) and passed as
    /// `DiscoveryConfig::modified_since` when a scan is started.
    ///
    /// Persists across scans so the user does not have to re-enter it each time.
    pub discovery_date_input: String,

    /// Which tab is active in the left sidebar.  0 = Files, 1 = Filters.
    /// Pure UI state — not saved to the session file.
    pub sidebar_tab: usize,

    /// Path to the user external-profiles directory (`%APPDATA%\LogSleuth\profiles\`).
    /// Set by main.rs after platform paths are resolved.  `None` only if platform
    /// path resolution failed entirely (should not happen in normal use).
    pub user_profiles_dir: Option<std::path::PathBuf>,

    /// When `true`, `gui.rs` will call `load_all_profiles` on the next frame,
    /// update `self.profiles`, and reset this flag.
    pub request_reload_profiles: bool,

    // -------------------------------------------------------------------------
    // Dir-watcher file queue (Bug fix: prevents cancel race)
    // -------------------------------------------------------------------------
    /// File paths reported by the directory watcher while an append/scan was
    /// already in progress.  Rather than cancelling the running scan (which
    /// loses in-flight entries), new paths are accumulated here and processed
    /// as a single append scan once the current scan completes.  Drained with
    /// `parse_path_filter = Some(empty)` (profile only — opt-in model).
    ///
    /// Cleared by `clear()`.
    pub queued_dir_watcher_files: Vec<PathBuf>,

    /// File paths that the user explicitly checked (ticked) in the file list
    /// while a scan was already in progress.  These are user-initiated requests
    /// to fully parse a file, so they are drained with
    /// `parse_path_filter = None` (parse all entries) when the running scan
    /// completes — unlike `queued_dir_watcher_files` which are profile-only.
    ///
    /// Cleared by `clear()`.
    pub queued_parse_files: Vec<PathBuf>,

    /// Highest entry ID currently assigned.  Maintained incrementally as
    /// entries arrive from scans, tail, and append operations so that
    /// `next_entry_id()` is O(1) instead of scanning all entries.
    ///
    /// Reset to 0 by `clear()`.  Updated by `track_max_entry_id()` which
    /// must be called every time entries are added to `self.entries`.
    max_entry_id: u64,

    // -------------------------------------------------------------------------
    // Derived / cached UI data
    // -------------------------------------------------------------------------
    /// Per-entry deduplication metadata, keyed by the surviving entry's global
    /// index into `self.entries`.  Populated by `apply_filters()` when
    /// `filter_state.dedup_mode != Off`; empty otherwise.
    ///
    /// Each value describes how many duplicate occurrences exist and where
    /// they are located (for the detail panel "Occurrences" section).
    pub dedup_info: HashMap<usize, DedupInfo>,

    /// Sorted, deduplicated list of all `component` values observed across
    /// `self.entries`.  Rebuilt by `apply_filters()` and cleared by `clear()`.
    pub unique_component_values: Vec<String>,

    // -------------------------------------------------------------------------
    // Live-tail ring-buffer state
    // -------------------------------------------------------------------------
    /// Entry count snapshot taken the moment the current live-tail session was
    /// started (`set_tail_base()` is called from the `request_start_tail`
    /// handler in `gui.rs`).
    ///
    /// Entries with index in `[0, tail_base_count)` are from the initial scan
    /// and are NEVER evicted by the ring-buffer logic.  Entries at index
    /// `[tail_base_count, entries.len())` are from the live tail session and are
    /// subject to eviction when `max_tail_buffer_entries` is exceeded.
    ///
    /// Reset to 0 by `clear()`.
    pub tail_base_count: usize,

    /// Maximum number of live-tail entries held in the rolling ring-buffer.
    ///
    /// When appending new tail entries would push `entries[tail_base_count..]`
    /// beyond this limit, the oldest tail entries are drained to make room.
    /// User-configurable via the Options dialog.  Defaults to
    /// `DEFAULT_MAX_TAIL_BUFFER_ENTRIES`.
    pub max_tail_buffer_entries: usize,

    /// Number of entries currently in `self.entries` that have
    /// `entry.timestamp == None`.
    ///
    /// Maintained incrementally by `track_notimestamp_entries()`.  Reset to 0
    /// by `clear()` and recounted from scratch after any eviction inside
    /// `evict_tail_entries()`.
    ///
    /// Used to skip the O(n) per-entry `file_modified` update in the
    /// `FileMtimeUpdates` handler: when this counter is 0 every entry already
    /// has a parsed log timestamp so `file_modified` is never used as a
    /// time-range-filter fallback, and the O(n) scan can be avoided entirely.
    pub notimestamp_entry_count: usize,

    /// Set to `Some(Instant::now())` by text-input handlers when the user edits
    /// the text-search or regex fields.  Cleared by `apply_filters()`.  The UI
    /// loop fires `apply_filters()` once this age exceeds `FILTER_DEBOUNCE_MS`,
    /// preventing an O(n) filter pass on every individual keystroke.
    pub filter_dirty_at: Option<std::time::Instant>,

    /// Raw text typed by the user into the multi-term search input box.
    /// Parsed into include/exclude terms on each edit and compiled into
    /// `filter_state.multi_search`.  Kept as a separate buffer because
    /// `MultiSearch` stores parsed term vectors, not the raw input.
    pub multi_search_input: String,

    // -------------------------------------------------------------------------
    // Troubleshoot mode
    // -------------------------------------------------------------------------
    /// When `true`, only Critical and Error entries are retained during
    /// ingestion (scan batches + live tail).  All other severities are
    /// discarded before they reach `self.entries`, drastically reducing
    /// memory usage on high-volume log directories.
    ///
    /// Persists across `clear()` calls (user preference).  Reset by
    /// `new_session()` so a fresh start always begins in normal mode.
    pub troubleshoot_mode: bool,

    /// When `true`, `gui.rs` will automatically start Live Tail after the
    /// next `ParsingCompleted` arrives.  Set by the Troubleshoot Mode
    /// activation flow; consumed and cleared by the `ParsingCompleted`
    /// handler.
    pub request_start_tail_after_scan: bool,
}

// =============================================================================
// Standalone helpers
// =============================================================================

/// Parse a user-supplied datetime string into a UTC bound for a time filter.
///
/// Accepted formats (most-specific first):
///   `YYYY-MM-DD HH:MM:SS`  -- second precision
///   `YYYY-MM-DD HH:MM`     -- minute precision
///   `YYYY-MM-DD`           -- day precision (midnight in local time)
///
/// All formats are interpreted as **local** time and converted to UTC, matching
/// the behaviour of `AppState::discovery_modified_since`.  An empty or
/// non-parseable input returns `None`.
///
/// Used by `ui/panels/filters.rs` to commit absolute date range inputs directly
/// to `FilterState::time_start` / `FilterState::time_end`.
pub fn parse_filter_datetime(s: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    use chrono::{Local, NaiveDate, NaiveDateTime, TimeZone as _};
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return None;
    }
    // Interpret as LOCAL time, convert to UTC.  Falls back to treating the
    // naive datetime as UTC on DST ambiguity (e.g. clock-back hour).
    let local_to_utc = |ndt: NaiveDateTime| -> chrono::DateTime<chrono::Utc> {
        Local
            .from_local_datetime(&ndt)
            .single()
            .map(|dt| dt.to_utc())
            .unwrap_or_else(|| ndt.and_utc())
    };
    if let Ok(ndt) = NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%d %H:%M:%S") {
        return Some(local_to_utc(ndt));
    }
    if let Ok(ndt) = NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%d %H:%M") {
        return Some(local_to_utc(ndt));
    }
    NaiveDate::parse_from_str(trimmed, "%Y-%m-%d")
        .ok()
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .map(local_to_utc)
}

impl AppState {
    /// Create initial state with loaded profiles.
    pub fn new(profiles: Vec<FormatProfile>, debug_mode: bool) -> Self {
        Self {
            profiles,
            scan_path: None,
            scan_in_progress: false,
            discovered_files: Vec::new(),
            entries: Vec::new(),
            filtered_indices: Vec::new(),
            filter_state: FilterState::default(),
            selected_index: None,
            selected_indices: BTreeSet::new(),
            scan_summary: None,
            status_message: "Ready. Open a directory to begin scanning.".to_string(),
            warnings: Vec::new(),
            show_summary: false,
            show_log_summary: false,
            show_about: false,
            ui_font_size: crate::util::constants::DEFAULT_FONT_SIZE,
            dark_mode: true,        // default to dark; matches egui's own default
            sort_descending: false, // default ascending (oldest first)
            debug_mode,
            pending_scan: None,
            request_cancel: false,
            file_list_search: String::new(),
            file_colours: HashMap::new(),
            pending_single_files: None,
            pending_parse_skipped: false,
            activity_window_secs: None,
            activity_window_input: String::new(),
            dir_watcher_active: false,
            dir_watcher_scanning: false,
            tail_active: false,
            tail_auto_scroll: true,
            request_start_tail: false,
            request_stop_tail: false,
            bookmarks: HashMap::new(),
            correlation_active: false,
            correlation_window_secs: DEFAULT_CORRELATION_WINDOW_SECS,
            correlation_window_input: DEFAULT_CORRELATION_WINDOW_SECS.to_string(),
            correlated_ids: HashSet::new(),
            session_path: None,
            initial_scan: None,
            manually_added_files: Vec::new(),
            extra_files_to_restore: Vec::new(),
            max_files_limit: crate::util::constants::DEFAULT_MAX_FILES,
            max_total_entries: crate::util::constants::MAX_TOTAL_ENTRIES,
            max_scan_depth: crate::util::constants::DEFAULT_MAX_DEPTH,
            tail_poll_interval_ms: crate::util::constants::TAIL_POLL_INTERVAL_MS,
            dir_watch_poll_interval_ms: crate::util::constants::DIR_WATCH_POLL_INTERVAL_MS,
            show_options: false,
            scroll_top_requested: false,
            total_files_found: 0,
            discovery_truncated: false,
            pending_replace_files: None,
            request_new_session: false,
            discovery_date_input: String::new(),
            sidebar_tab: 0,
            user_profiles_dir: None,
            request_reload_profiles: false,
            queued_dir_watcher_files: Vec::new(),
            queued_parse_files: Vec::new(),
            max_entry_id: 0,
            tail_base_count: 0,
            max_tail_buffer_entries: crate::util::constants::DEFAULT_MAX_TAIL_BUFFER_ENTRIES,
            notimestamp_entry_count: 0,
            filter_dirty_at: None,
            fresh_scan_in_progress: false,
            unique_component_values: Vec::new(),
            dedup_info: HashMap::new(),
            multi_search_input: String::new(),
            troubleshoot_mode: false,
            request_start_tail_after_scan: false,
        }
    }

    /// Returns the UTC cutoff instant for the current activity window, or
    /// `None` if the window is disabled.  Re-evaluated on every call so the
    /// rolling window stays current as the clock advances.
    ///
    /// Uses checked arithmetic to avoid a panic if `activity_window_secs` is
    /// extremely large (e.g. from a malformed session file).  On overflow the
    /// window is treated as disabled (returns `None` -- fail-open).
    pub fn activity_cutoff(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        self.activity_window_secs.and_then(|secs| {
            let clamped = i64::try_from(secs).unwrap_or(i64::MAX);
            chrono::Utc::now().checked_sub_signed(chrono::Duration::seconds(clamped))
        })
    }

    /// Remove all parsed entries for a specific file from memory and mark it as
    /// `parsing_skipped` so the file list shows the unparsed `□` indicator and
    /// re-ticking the checkbox triggers an on-demand re-parse from disk.
    ///
    /// Called when the user unchecks a file row in the Files tab (Tab 1).
    /// The caller is responsible for calling `apply_filters()` afterwards to
    /// refresh the visible-entry list.
    pub fn remove_entries_for_file(&mut self, path: &std::path::PathBuf) {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("?")
            .to_string();
        let before = self.entries.len();
        self.entries.retain(|e| &e.source_file != path);
        let removed = before - self.entries.len();

        // Release the backing allocation for removed entries so RSS drops
        // immediately rather than waiting for the allocator to reclaim the
        // idle capacity (Rule 11 — resource bounds).
        if removed > 0 {
            self.entries.shrink_to_fit();
        }

        // Recount no-timestamp entries after removal.  `track_notimestamp_entries`
        // only increments; to correctly decrement we recount from scratch.
        // This keeps the `FileMtimeUpdates` fast-path skip accurate — if this
        // counter stays non-zero after removal it forces an O(n) entry scan on
        // every mtime tick even when no plain-text entries remain.
        self.notimestamp_entry_count = self
            .entries
            .iter()
            .filter(|e| e.timestamp.is_none())
            .count();

        // Mark the file as unparsed so re-ticking triggers a fresh parse from disk
        // and the file row shows the □ indicator until then.
        for f in &mut self.discovered_files {
            if &f.path == path {
                f.parsing_skipped = true;
                break;
            }
        }
        if removed > 0 {
            tracing::info!(
                path = %path.display(),
                removed,
                "Removed entries for unchecked file"
            );
            self.status_message = format!(
                "Removed {removed} entr{} for \"{name}\".",
                if removed == 1 { "y" } else { "ies" }
            );
        }
    }

    /// Recompute filtered indices from current entries and filter state.
    ///
    /// If a relative time filter is active (`filter_state.relative_time_secs`),
    /// the effective `time_start` is computed here from `Utc::now()`.  This keeps
    /// the core filter layer pure (no side-effects or clock access).
    ///
    /// If the bookmarks-only filter is active, `filter_state.bookmarked_ids` is
    /// populated from `self.bookmarks` so core sees a plain `HashSet<u64>`.
    pub fn apply_filters(&mut self) {
        // Clear any pending debounce marker so a forced apply_filters() call
        // (e.g. from a button click) supersedes the pending text-input debounce.
        self.filter_dirty_at = None;

        // Relative time filter: derive the absolute start bound each call so the
        // rolling window stays current as the clock advances.
        //
        // Uses checked arithmetic to avoid a panic if the value is extremely
        // large (e.g. from a malformed session file).  On overflow, time_start
        // is set to None (no lower bound -- shows all entries; fail-open).
        if let Some(secs) = self.filter_state.relative_time_secs {
            let clamped = i64::try_from(secs).unwrap_or(i64::MAX);
            self.filter_state.time_start =
                chrono::Utc::now().checked_sub_signed(chrono::Duration::seconds(clamped));
            self.filter_state.time_end = None;
        }

        // Bookmark filter: give core a snapshot of the bookmarked IDs so it stays pure.
        if self.filter_state.bookmarks_only {
            self.filter_state.bookmarked_ids = self.bookmarks.keys().copied().collect();
        } else {
            self.filter_state.bookmarked_ids.clear();
        }

        // Capture the ID of the currently selected entry so we can restore the
        // selection by identity after the filter is recomputed.  Without this,
        // selected_index (a display-position integer) could silently drift to a
        // different entry whenever the filtered set changes — causing the detail
        // panel and correlation overlay to show the wrong entry.
        let selected_id = self.selected_entry().map(|e| e.id);

        // Capture the IDs of all multi-selected entries so multi-select survives
        // filter changes (same ID-based preservation as the primary selection).
        let multi_selected_ids: HashSet<u64> = self
            .selected_indices
            .iter()
            .filter_map(|&idx| {
                self.filtered_indices
                    .get(idx)
                    .and_then(|&ei| self.entries.get(ei))
                    .map(|e| e.id)
            })
            .collect();

        self.filtered_indices =
            crate::core::filter::apply_filters(&self.entries, &self.filter_state);

        // Activity window: further filter to only entries whose source file has
        // been modified within the rolling window.  Applied *after* all other
        // filters so it combines with severity, text search, etc.  Uses the live
        // mtime from `discovered_files` (updated by the dir-watcher) rather than
        // any parsed log timestamp, so it works for plain-text and files with
        // unreliable embedded timestamps.
        //
        // Fail-open: a file with no known mtime is treated as active so that a
        // metadata failure never silently hides entries.
        //
        // Performance: only build the active-file HashSet when the feature is
        // enabled and at least one file is discovered.  Building a HashSet of
        // PathBuf references on every filter call was O(discovered_files) and
        // allocated even when the filter matched all files.
        if let Some(cutoff) = self.activity_cutoff() {
            if !self.discovered_files.is_empty() {
                let active_files: std::collections::HashSet<&std::path::PathBuf> = self
                    .discovered_files
                    .iter()
                    .filter(|f| f.modified.map_or(true, |t| t >= cutoff))
                    .map(|f| &f.path)
                    .collect();
                self.filtered_indices.retain(|&idx| {
                    self.entries
                        .get(idx)
                        .is_some_and(|e| active_files.contains(&e.source_file))
                });
            }
        }

        // Deduplication post-filter: collapse duplicate messages (per source file)
        // to show only the latest occurrence of each unique message.  Applied after
        // all other filters so it operates on the already-narrowed result set.
        if self.filter_state.dedup_mode != DedupMode::Off {
            let (deduped, info) = crate::core::filter::apply_dedup(
                &self.entries,
                &self.filtered_indices,
                self.filter_state.dedup_mode,
            );
            self.filtered_indices = deduped;
            self.dedup_info = info;
        } else {
            self.dedup_info.clear();
        }

        // Restore selection: find the new display position of the previously
        // selected entry by ID.  If the entry is no longer in the filtered set
        // (e.g. it was hidden by a new severity filter), clear the selection.
        self.selected_index = selected_id.and_then(|id| {
            self.filtered_indices
                .iter()
                .position(|&entry_idx| self.entries.get(entry_idx).is_some_and(|e| e.id == id))
        });

        // Restore multi-select: rebuild selected_indices from the saved IDs.
        self.selected_indices.clear();
        if !multi_selected_ids.is_empty() {
            for (pos, &entry_idx) in self.filtered_indices.iter().enumerate() {
                if let Some(entry) = self.entries.get(entry_idx) {
                    if multi_selected_ids.contains(&entry.id) {
                        self.selected_indices.insert(pos);
                    }
                }
            }
        }

        // Recompute the correlation overlay whenever the filter changes.
        // Without this, if the selected entry is hidden by the new filter
        // (selected_index → None), correlated_ids would still hold the old
        // window, showing ghost teal highlights with no active selection.
        //
        // Note: this is only reached from the slow path (sort + full rebuild).
        // The tail fast path (extend_filtered_for_range) skips apply_filters
        // entirely, so update_correlation is NOT called for every tail tick —
        // which is correct because a pure tail append never changes the selected
        // entry's identity or position.
        self.update_correlation();

        // Rebuild the unique-thread and unique-component caches so the filter
        // panel checkboxes always reflect the current entry set.  Runs after
        // update_correlation to share the apply_filters call as the natural
        // trigger point without requiring an extra dirty flag.
        self.rebuild_unique_values();
    }

    /// Recompute the correlation overlay from the currently selected entry.
    ///
    /// Iterates **all** entries (not just filtered ones) so context entries
    /// hidden by an active filter are still included in the teal highlight.
    /// This matches CMTrace behaviour: the window shows what happened around
    /// that moment across all loaded files regardless of the current view.
    ///
    /// Called whenever:
    /// - The selected entry changes (click in the timeline)
    /// - The correlation overlay is toggled on/off
    /// - The window size is committed via the filters panel input
    pub fn update_correlation(&mut self) {
        self.correlated_ids.clear();
        if !self.correlation_active {
            return;
        }
        let Some(entry) = self.selected_entry() else {
            return;
        };
        let Some(anchor_ts) = entry.timestamp else {
            // Entries without a parsed timestamp cannot serve as a correlation
            // anchor because there is no time reference to build a window from.
            return;
        };
        let window = chrono::Duration::seconds(self.correlation_window_secs);
        // Use checked arithmetic to avoid panicking on extreme window values
        // (e.g. from a crafted session file).  Fall back to chrono's min/max
        // representable instants so the window degrades to "everything" rather
        // than crashing.
        let start = anchor_ts
            .checked_sub_signed(window)
            .unwrap_or(chrono::DateTime::<chrono::Utc>::MIN_UTC);
        let end = anchor_ts
            .checked_add_signed(window)
            .unwrap_or(chrono::DateTime::<chrono::Utc>::MAX_UTC);
        self.correlated_ids = self
            .entries
            .iter()
            .filter(|e| {
                e.timestamp
                    .map(|ts| ts >= start && ts <= end)
                    .unwrap_or(false)
            })
            .map(|e| e.id)
            .collect();
    }

    /// Get the currently selected entry, if any.
    pub fn selected_entry(&self) -> Option<&LogEntry> {
        self.selected_index
            .and_then(|idx| self.filtered_indices.get(idx))
            .and_then(|&entry_idx| self.entries.get(entry_idx))
    }

    /// Rebuild the unique-component cache from `self.entries`.
    ///
    /// Iterates the full entry set once, collecting all distinct non-None values
    /// for `component` into a sorted Vec.  Called at the end of `apply_filters()`
    /// so the cache is always consistent with the loaded data.
    fn rebuild_unique_values(&mut self) {
        let mut components: HashSet<String> = HashSet::new();
        for entry in &self.entries {
            if let Some(c) = &entry.component {
                components.insert(c.clone());
            }
        }
        let mut cv: Vec<String> = components.into_iter().collect();
        cv.sort_unstable();
        self.unique_component_values = cv;
    }

    /// Return the next available monotonic entry ID.
    ///
    /// Used when starting the live tail watcher so tail entry IDs continue
    /// from where the scan left off and do not collide with existing IDs.
    ///
    /// Uses the higher of the incrementally tracked `max_entry_id` counter
    /// and the actual maximum ID present in `entries`.  The tracked counter
    /// is maintained by [`track_max_entry_id`] in the normal GUI pipeline;
    /// the full scan provides a safety net when entries are added directly
    /// (e.g. in tests or session restore).  This method is called only at
    /// lifecycle transitions (not per-entry), so the O(n) fallback is
    /// negligible.
    pub fn next_entry_id(&self) -> u64 {
        if self.entries.is_empty() {
            0
        } else {
            let from_entries = self.entries.iter().map(|e| e.id).max().unwrap_or(0);
            self.max_entry_id.max(from_entries) + 1
        }
    }

    /// Update `max_entry_id` from a slice of newly added entries.
    ///
    /// Must be called every time entries are appended to `self.entries`
    /// (scan batches, tail entries, etc.) so that `next_entry_id()` stays
    /// accurate without requiring an O(n) full scan.
    pub fn track_max_entry_id(&mut self, new_entries: &[LogEntry]) {
        if let Some(max) = new_entries.iter().map(|e| e.id).max() {
            self.max_entry_id = self.max_entry_id.max(max);
        }
    }

    /// Update `notimestamp_entry_count` from a slice of newly added entries.
    ///
    /// Must be called every time entries are appended to `self.entries`.  The
    /// counter allows the `FileMtimeUpdates` handler to skip an O(n) entry scan
    /// when every entry already has a parsed timestamp (the common case).
    pub fn track_notimestamp_entries(&mut self, new_entries: &[LogEntry]) {
        for e in new_entries {
            if e.timestamp.is_none() {
                self.notimestamp_entry_count += 1;
            }
        }
    }

    /// Filter a mutable entry vector in-place, retaining only Critical and
    /// Error entries when troubleshoot mode is active.
    ///
    /// Returns the number of entries dropped.  When troubleshoot mode is off
    /// this is a no-op that returns 0.
    pub fn filter_entries_for_ingest(
        &self,
        entries: &mut Vec<crate::core::model::LogEntry>,
    ) -> usize {
        if !self.troubleshoot_mode {
            return 0;
        }
        let before = entries.len();
        entries.retain(|e| {
            matches!(
                e.severity,
                crate::core::model::Severity::Critical | crate::core::model::Severity::Error
            )
        });
        before - entries.len()
    }

    /// Record the current entry count as the live-tail baseline.
    ///
    /// Called from `gui.rs` when the `request_start_tail` flag is processed,
    /// immediately before the `TailManager::start_tail()` call.  All entries
    /// added before this point are from the initial scan and will never be
    /// evicted by the ring-buffer logic.
    pub fn set_tail_base(&mut self) {
        self.tail_base_count = self.entries.len();
    }

    /// Evict the oldest `count` entries from the live-tail section
    /// (`entries[tail_base_count..]`).
    ///
    /// Entries from the initial scan (`entries[..tail_base_count]`) are
    /// NEVER touched.  After draining, `notimestamp_entry_count` is
    /// recounted from scratch (because some evicted entries may have had
    /// `timestamp == None`).
    ///
    /// Returns the number of entries actually removed (may be less than
    /// `count` if the tail section is smaller).  The caller MUST rebuild
    /// `filtered_indices` after calling this (via `apply_filters()` or
    /// `sort_entries_chronologically()`).
    pub fn evict_tail_entries(&mut self, count: usize) -> usize {
        let tail_start = self.tail_base_count;
        let available = self.entries.len().saturating_sub(tail_start);
        let to_evict = count.min(available);
        if to_evict == 0 {
            return 0;
        }
        self.entries.drain(tail_start..tail_start + to_evict);
        // Recount no-timestamp entries after draining, because some evicted
        // entries may have had timestamp == None.  O(n) but infrequent.
        self.notimestamp_entry_count = self
            .entries
            .iter()
            .filter(|e| e.timestamp.is_none())
            .count();
        to_evict
    }

    /// Incrementally extend `filtered_indices` by evaluating the current
    /// filter against `entries[start..]` only.
    ///
    /// This is the fast path used by the live-tail handler when incoming
    /// entries are appended in strictly ascending timestamp order (the
    /// common case).  It costs O(N) where N = new entries, instead of the
    /// O(M) full rebuild that `apply_filters()` performs.
    ///
    /// **Pre-conditions the caller MUST guarantee:**
    /// 1. All entries in `[start..]` have timestamps ≥ the last entry in
    ///    `[..start]`, preserving the ascending `filtered_indices` invariant.
    /// 2. No entries were removed or reordered below `start` since the last
    ///    full `apply_filters()` call.
    ///
    /// **Does NOT call `update_correlation()`.**  The correlation set only
    /// needs updating when the *selected* entry changes, which never happens
    /// on a pure tail append.  The caller is responsible for calling
    /// `update_correlation()` separately if it detects a selection change.
    pub fn extend_filtered_for_range(&mut self, start: usize) {
        if start >= self.entries.len() {
            return;
        }

        // When dedup is active, incremental extension cannot correctly maintain
        // the dedup groups (a new entry may supersede an existing survivor).
        // Fall back to a full rebuild which is correct in all cases.
        if self.filter_state.dedup_mode != DedupMode::Off {
            self.apply_filters();
            return;
        }

        // Mirror the pre-conditions of apply_filters: recompute the rolling
        // time bound and bookmark set so the incremental result is identical
        // to what a full rebuild would produce for the new entries.
        if let Some(secs) = self.filter_state.relative_time_secs {
            let clamped = i64::try_from(secs).unwrap_or(i64::MAX);
            self.filter_state.time_start =
                chrono::Utc::now().checked_sub_signed(chrono::Duration::seconds(clamped));
            self.filter_state.time_end = None;
        }
        if self.filter_state.bookmarks_only {
            self.filter_state.bookmarked_ids = self.bookmarks.keys().copied().collect();
        }

        // Pre-compute the lowercased text-search needle once for the batch
        // (mirrors the same optimisation in apply_filters / matches_all).
        let text_lower = self.filter_state.text_search.to_lowercase();

        // Activity-window cutoff: build the active-file set once for the
        // entire batch (same logic as the post-filter in apply_filters).
        let active_files: Option<std::collections::HashSet<&std::path::PathBuf>> =
            if let Some(cutoff) = self.activity_cutoff() {
                if !self.discovered_files.is_empty() {
                    Some(
                        self.discovered_files
                            .iter()
                            .filter(|f| f.modified.map_or(true, |t| t >= cutoff))
                            .map(|f| &f.path)
                            .collect(),
                    )
                } else {
                    None
                }
            } else {
                None
            };

        // Evaluate core filter + activity window for each new entry and
        // append matching global indices to filtered_indices.
        for (local_idx, entry) in self.entries[start..].iter().enumerate() {
            if !crate::core::filter::entry_matches(entry, &self.filter_state, &text_lower) {
                continue;
            }
            if let Some(ref active) = active_files {
                if !active.contains(&entry.source_file) {
                    continue;
                }
            }
            self.filtered_indices.push(start + local_idx);
        }
    }

    /// Assign a palette colour to `path` if it does not already have one.
    /// Uses a round-robin index over the theme palette so each new file gets
    /// a distinct colour (wrapping after 12 files).
    pub fn assign_file_colour(&mut self, path: &std::path::Path) {
        if !self.file_colours.contains_key(path) {
            let idx = self.file_colours.len();
            let colour = crate::ui::theme::file_colour(idx);
            self.file_colours.insert(path.to_path_buf(), colour);
        }
    }

    /// Return the palette colour for `path`, or a neutral grey if not found.
    pub fn colour_for_file(&self, path: &std::path::Path) -> egui::Color32 {
        self.file_colours
            .get(path)
            .copied()
            .unwrap_or(egui::Color32::from_rgb(107, 114, 128))
    }

    // -------------------------------------------------------------------------
    // Bookmark helpers
    // -------------------------------------------------------------------------

    /// Toggle a bookmark on the given entry ID.
    ///
    /// Returns `true` if the entry is now bookmarked, `false` if it was removed.
    /// Does **not** call `apply_filters()`; the caller must do that if a filter
    /// refresh is needed (e.g. when `bookmarks_only` is active).
    pub fn toggle_bookmark(&mut self, entry_id: u64) -> bool {
        if let std::collections::hash_map::Entry::Vacant(e) = self.bookmarks.entry(entry_id) {
            e.insert(String::new());
            true
        } else {
            self.bookmarks.remove(&entry_id);
            false
        }
    }

    /// Returns `true` if the entry with the given ID is currently bookmarked.
    pub fn is_bookmarked(&self, entry_id: u64) -> bool {
        self.bookmarks.contains_key(&entry_id)
    }

    /// Returns the total number of bookmarked entries.
    pub fn bookmark_count(&self) -> usize {
        self.bookmarks.len()
    }

    /// Remove all bookmarks and reset the `bookmarks_only` filter.
    /// Calls `apply_filters()` to refresh the timeline.
    pub fn clear_bookmarks(&mut self) {
        self.bookmarks.clear();
        self.filter_state.bookmarks_only = false;
        self.filter_state.bookmarked_ids.clear();
        self.apply_filters();
    }

    /// Generate a plain-text bookmark report suitable for clipboard export.
    ///
    /// Entries are listed in ID (chronological) order. Each entry shows its
    /// timestamp, severity, source filename, message preview (first 200 chars)
    /// and any annotation label.
    pub fn bookmarks_report(&self) -> String {
        let mut ids: Vec<u64> = self.bookmarks.keys().copied().collect();
        ids.sort_unstable();
        let generated = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
        let mut out = format!(
            "LogSleuth Bookmark Report\nGenerated: {generated}\n{}\n\n",
            "=".repeat(60)
        );
        // Build a HashMap index so each bookmark lookup is O(1) instead of
        // O(entries).  With 1M entries and many bookmarks the previous
        // `entries.iter().find()` approach was O(bookmarks * entries).
        let entry_map: HashMap<u64, &LogEntry> = self.entries.iter().map(|e| (e.id, e)).collect();
        for id in &ids {
            if let Some(entry) = entry_map.get(id) {
                let ts = entry
                    .timestamp
                    .map(|t| t.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                    .unwrap_or_else(|| "no timestamp".to_string());
                let src = entry
                    .source_file
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("?");
                let label = self
                    .bookmarks
                    .get(id)
                    .filter(|s| !s.is_empty())
                    .map(|s| format!(" [{s}]"))
                    .unwrap_or_default();
                // Use raw_text (the verbatim original log line) so the clipboard
                // contains the actual file contents, not a parsed sub-field.
                // Cap at 500 chars to keep individual entries readable; the entry
                // count is already bounded by MAX_CLIPBOARD_ENTRIES.
                let body: String = entry.raw_text.chars().take(500).collect();
                let sev = entry.severity.label();
                out.push_str(&format!("[{ts}] {sev:<8} {src}{label}\n  {body}\n\n"));
            }
        }
        out
    }

    /// Generate a plain-text report of all currently-filtered entries for clipboard export.
    ///
    /// Each entry is rendered as a single-line row: `[timestamp] severity  source  message`.
    /// The report begins with a header that summarises the active filters so the recipient
    /// understands the scope of what they are looking at.
    ///
    /// Bounded to [`MAX_CLIPBOARD_ENTRIES`] to prevent clipboard overload on very large
    /// filtered sets. A truncation notice is appended when the limit is hit.
    pub fn filtered_results_report(&self) -> String {
        let total_filtered = self.filtered_indices.len();
        let take = total_filtered.min(MAX_CLIPBOARD_ENTRIES);
        let generated = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC");

        // Build a concise human-readable description of the active filter so the
        // clipboard recipient understands what criteria were applied.
        let mut filter_parts: Vec<String> = Vec::new();
        if !self.filter_state.severity_levels.is_empty() {
            let mut sevs: Vec<&str> = self
                .filter_state
                .severity_levels
                .iter()
                .map(|s| s.label())
                .collect();
            sevs.sort_unstable();
            filter_parts.push(format!("Severity: {}", sevs.join("+")));
        }
        if let Some(secs) = self.filter_state.relative_time_secs {
            if secs < 60 {
                filter_parts.push(format!("Last {secs}s"));
            } else if secs < 3_600 {
                filter_parts.push(format!("Last {}m", secs / 60));
            } else {
                filter_parts.push(format!("Last {}h", secs / 3_600));
            }
        } else if self.filter_state.time_start.is_some() || self.filter_state.time_end.is_some() {
            filter_parts.push("Time range active".to_string());
        }
        if !self.filter_state.text_search.is_empty() {
            filter_parts.push(format!("Text: \"{}\"", self.filter_state.text_search));
        }
        if self.filter_state.regex_search.is_some() {
            filter_parts.push(format!("Regex: /{}/", self.filter_state.regex_pattern));
        }
        // Bug fix: include source-file, bookmark, and activity-window filters
        // in the report header so the recipient knows the full scope of the
        // filtered result set.
        if self.filter_state.hide_all_sources {
            filter_parts.push("Files: none (all hidden)".to_string());
        } else if !self.filter_state.source_files.is_empty() {
            let n = self.filter_state.source_files.len();
            filter_parts.push(format!("Files: {n} selected"));
        }
        if self.filter_state.bookmarks_only {
            filter_parts.push("Bookmarks only".to_string());
        }
        if let Some(secs) = self.activity_window_secs {
            if secs < 60 {
                filter_parts.push(format!("Activity window: {secs}s"));
            } else if secs < 3_600 {
                filter_parts.push(format!("Activity window: {}m", secs / 60));
            } else {
                filter_parts.push(format!("Activity window: {}h", secs / 3_600));
            }
        }
        let filter_desc = if filter_parts.is_empty() {
            "No filter (all entries)".to_string()
        } else {
            filter_parts.join(" | ")
        };

        let mut out = format!(
            "LogSleuth Filtered Results\nGenerated: {generated}\nFilter:    {filter_desc}\nEntries:   {total_filtered}\n{}\n\n",
            "=".repeat(80)
        );

        for &entry_idx in self.filtered_indices.iter().take(take) {
            if let Some(entry) = self.entries.get(entry_idx) {
                let ts = entry
                    .timestamp
                    .map(|t| t.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                    .unwrap_or_else(|| "no timestamp         ".to_string());
                let src = entry
                    .source_file
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("?");
                let sev = entry.severity.label();
                // Use raw_text (the verbatim original log line) so the clipboard
                // contains the actual file contents, not a parsed sub-field.
                // Cap at 500 chars to keep individual entries readable; the total
                // entry count is already bounded by MAX_CLIPBOARD_ENTRIES.
                let body: String = entry.raw_text.chars().take(500).collect();
                out.push_str(&format!("[{ts}] {sev:<8}  {src}\n  {body}\n\n"));
            }
        }

        if take < total_filtered {
            out.push_str(&format!(
                "--- truncated: {take} of {total_filtered} entries shown (limit: {MAX_CLIPBOARD_ENTRIES}) ---\n"
            ));
        }

        out
    }

    /// Generate a clipboard-ready text block from the multi-selected entries.
    ///
    /// Each selected entry's `raw_text` is emitted on its own line, preserving
    /// the original log file content verbatim.  Entries are iterated in
    /// `selected_indices` order (ascending, matching chronological display).
    ///
    /// Bounded to [`MAX_CLIPBOARD_ENTRIES`] to prevent clipboard overload.
    pub fn selected_entries_report(&self) -> String {
        let total = self.selected_indices.len();
        let take = total.min(MAX_CLIPBOARD_ENTRIES);

        let mut out = String::new();
        for &idx in self.selected_indices.iter().take(take) {
            if let Some(&entry_idx) = self.filtered_indices.get(idx) {
                if let Some(entry) = self.entries.get(entry_idx) {
                    out.push_str(&entry.raw_text);
                    out.push('\n');
                }
            }
        }

        if take < total {
            out.push_str(&format!(
                "--- truncated: {take} of {total} entries shown (limit: {MAX_CLIPBOARD_ENTRIES}) ---\n"
            ));
        }

        out
    }

    /// Sort `entries` chronologically (entries with timestamps first, then
    /// timestampless entries in their original relative order at the end).
    ///
    /// Called after a scan completes or after files are appended to ensure the
    /// merged timeline is in time order regardless of the order files were parsed.
    /// After sorting, filters are re-applied so `filtered_indices` stays valid.
    pub fn sort_entries_chronologically(&mut self) {
        // Stable sort: preserves original order among equal-timestamp (or no-timestamp) entries.
        self.entries
            .sort_by(|a, b| match (a.timestamp, b.timestamp) {
                (Some(ta), Some(tb)) => ta.cmp(&tb),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            });
        self.apply_filters();
    }

    /// Toggle the timeline sort direction between ascending (oldest first) and
    /// descending (newest first).
    ///
    /// `selected_index` is a **stable position into `filtered_indices`** (which
    /// is always kept in ascending chronological order).  It does not change
    /// meaning when the display direction flips, so no remapping is needed here.
    /// `selected_entry()` continues to return the correct entry regardless of
    /// sort direction.
    ///
    /// `sort_descending` is a user preference and is **not** cleared by
    /// `clear()` — it persists across scans, like `dark_mode` and
    /// `tail_auto_scroll`.
    pub fn toggle_sort_direction(&mut self) {
        self.sort_descending = !self.sort_descending;
        // `selected_index` is a stable position into `filtered_indices` (always
        // in ascending chronological order), so it does not need remapping when
        // the display direction flips.  `selected_entry()` continues to return
        // the correct entry because it looks up `filtered_indices[selected_index]`
        // which is independent of `sort_descending`.
    }

    /// Clear all scan results and reset to initial state.
    pub fn clear(&mut self) {
        self.discovered_files.clear();
        self.entries.clear();
        self.filtered_indices.clear();
        self.filter_state = FilterState::default();
        self.selected_index = None;
        self.selected_indices.clear();
        self.scan_summary = None;
        self.warnings.clear();
        self.show_summary = false;
        self.show_log_summary = false;
        self.status_message = "Ready.".to_string();
        self.scan_in_progress = false;
        self.pending_scan = None;
        self.request_cancel = false;
        self.file_list_search.clear();
        self.file_colours.clear();
        self.pending_single_files = None;
        self.pending_parse_skipped = false;
        // Stop tail and dir watcher on clear — a new scan starts fresh.
        self.dir_watcher_active = false;
        self.dir_watcher_scanning = false;
        // Reset activity window on clear so a fresh scan starts unfiltered.
        self.activity_window_secs = None;
        self.activity_window_input.clear();
        self.tail_active = false;
        self.request_start_tail = false;
        self.request_stop_tail = false;
        // Bookmarks are cleared on scan clear.
        self.bookmarks.clear();
        self.filter_state.bookmarks_only = false;
        self.filter_state.bookmarked_ids.clear();
        // Correlation overlay is cleared on scan clear.
        self.correlation_active = false;
        self.correlated_ids.clear();
        self.correlation_window_secs = DEFAULT_CORRELATION_WINDOW_SECS;
        self.correlation_window_input = DEFAULT_CORRELATION_WINDOW_SECS.to_string();
        // tail_auto_scroll preference is intentionally preserved across clears.
        // initial_scan is cleared on each new scan; session_path is never cleared.
        self.initial_scan = None;
        // Extra-files tracking is scoped to the active session.
        self.manually_added_files.clear();
        self.extra_files_to_restore.clear();
        // Reset per-scan discovery counters.
        self.total_files_found = 0;
        self.discovery_truncated = false;
        self.pending_replace_files = None;
        self.request_new_session = false;
        // Discard any queued dir-watcher files from the previous session.
        self.queued_dir_watcher_files.clear();
        // Discard any queued user-parse requests from the previous session.
        self.queued_parse_files.clear();
        // Reset tracked entry ID counter.
        self.max_entry_id = 0;
        // Reset live-tail ring-buffer state.
        self.tail_base_count = 0;
        self.notimestamp_entry_count = 0;
        // max_tail_buffer_entries is a user preference — do not reset.
        // Cancel any pending debounced filter rebuild.
        self.filter_dirty_at = None;
        self.fresh_scan_in_progress = false;
        // Clear derived caches so stale values from the previous session are
        // not shown in the component filter panel.
        self.unique_component_values.clear();
        // Clear dedup metadata.
        self.dedup_info.clear();
        // Clear multi-search input buffer.
        self.multi_search_input.clear();
        // troubleshoot_mode is intentionally NOT cleared here — it is a user
        // preference that persists across rescans.  Reset only by new_session().
        // request_start_tail_after_scan is consumed once; clear it to prevent
        // stale requests from a cancelled scan leaking into the next one.
        self.request_start_tail_after_scan = false;
    }

    /// Reset to the initial blank state: clears everything `clear()` clears
    /// **plus** the selected scan path, leaving the app as if it was freshly
    /// launched with no directory chosen.
    pub fn new_session(&mut self) {
        self.clear();
        self.scan_path = None;
        self.troubleshoot_mode = false;
        self.status_message = "Ready. Open a directory to begin scanning.".to_string();
    }

    /// Parse `discovery_date_input` into a UTC `DateTime`.
    ///
    /// Accepts three levels of precision (most-specific first):
    ///   `YYYY-MM-DD HH:MM:SS`  — exact second boundary
    ///   `YYYY-MM-DD HH:MM`     — minute boundary (seconds = 00)
    ///   `YYYY-MM-DD`           — day boundary   (time  = 00:00:00 UTC)
    ///
    /// Returns `None` when the input is empty or does not match any format.
    /// The caller passes this to `DiscoveryConfig::modified_since` when starting
    /// a scan so only files modified on or after that instant are ingested.
    pub fn discovery_modified_since(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        use chrono::{Local, NaiveDate, NaiveDateTime, TimeZone as _};

        let trimmed = self.discovery_date_input.trim();
        if trimmed.is_empty() {
            return None;
        }

        // Interpret a NaiveDateTime as LOCAL time and convert to UTC.
        //
        // Using local time here is intentional: the quick-fill buttons ("Now",
        // "-1h", etc.) and manual entry all work in wall-clock local time.
        // Previously the string was passed straight to `.and_utc()`, meaning
        // users not in UTC saw the filter cutoff shifted by their UTC offset
        // (up to ±14 h), causing files to be incorrectly included or excluded.
        //
        // Falls back to UTC-interpretation on DST ambiguity (e.g. clocks
        // turning back) so the function never silently returns None.
        let local_to_utc = |ndt: NaiveDateTime| -> chrono::DateTime<chrono::Utc> {
            Local
                .from_local_datetime(&ndt)
                .single()
                .map(|dt| dt.to_utc())
                .unwrap_or_else(|| ndt.and_utc()) // DST-gap fallback
        };

        // 1. Full datetime to the second: "YYYY-MM-DD HH:MM:SS"
        if let Ok(ndt) = NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%d %H:%M:%S") {
            return Some(local_to_utc(ndt));
        }
        // 2. Datetime to the minute: "YYYY-MM-DD HH:MM"
        if let Ok(ndt) = NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%d %H:%M") {
            return Some(local_to_utc(ndt));
        }
        // 3. Date only: "YYYY-MM-DD" — treat as start of day in local time.
        NaiveDate::parse_from_str(trimmed, "%Y-%m-%d")
            .ok()
            .and_then(|d| d.and_hms_opt(0, 0, 0))
            .map(local_to_utc)
    }

    // -------------------------------------------------------------------------
    // Session persistence helpers
    // -------------------------------------------------------------------------

    /// Snapshot the current state into a session file on disk.
    ///
    /// Silently does nothing if `session_path` has not been set (e.g. in tests).
    /// All errors are logged as warnings but never surfaced to the user.
    pub fn save_session(&self) {
        let Some(session_path) = &self.session_path else {
            return;
        };
        let filter = crate::app::session::PersistedFilter {
            severity_levels: self.filter_state.severity_levels.iter().copied().collect(),
            source_files: self.filter_state.source_files.iter().cloned().collect(),
            hide_all_sources: self.filter_state.hide_all_sources,
            text_search: self.filter_state.text_search.clone(),
            regex_pattern: self.filter_state.regex_pattern.clone(),
            fuzzy: self.filter_state.fuzzy,
            relative_time_secs: self.filter_state.relative_time_secs,
            bookmarks_only: self.filter_state.bookmarks_only,
            exclude_text: self.filter_state.exclude_text.clone(),
            component_filter: {
                let mut v: Vec<String> =
                    self.filter_state.component_filter.iter().cloned().collect();
                v.sort_unstable();
                v
            },
            hide_no_timestamp: self.filter_state.hide_no_timestamp,
            dedup_mode: self.filter_state.dedup_mode,
            multi_search_input: self.multi_search_input.clone(),
            multi_search_mode: self.filter_state.multi_search.mode,
            multi_search_min_match: self.filter_state.multi_search.min_match,
            multi_search_case_insensitive: self.filter_state.multi_search.case_insensitive,
            multi_search_whole_word: self.filter_state.multi_search.whole_word,
            multi_search_regex_mode: self.filter_state.multi_search.regex_mode,
        };
        let file_colours = self
            .file_colours
            .iter()
            .map(|(path, colour)| (path.clone(), colour.to_array()))
            .collect();
        let bookmarks = self
            .bookmarks
            .iter()
            .map(|(id, label)| (*id, label.clone()))
            .collect();
        let data = crate::app::session::SessionData {
            version: crate::app::session::SESSION_VERSION,
            scan_path: self.scan_path.clone(),
            extra_files: self.manually_added_files.to_vec(),
            filter,
            file_colours,
            bookmarks,
            correlation_window_secs: self.correlation_window_secs,
            discovery_date_input: self.discovery_date_input.clone(),
            ui_font_size: self.ui_font_size,
            dark_mode: self.dark_mode,
            sort_descending: self.sort_descending,
            tail_auto_scroll: self.tail_auto_scroll,
            max_files_limit: self.max_files_limit,
            max_total_entries: self.max_total_entries,
            max_scan_depth: self.max_scan_depth,
            tail_poll_interval_ms: self.tail_poll_interval_ms,
            dir_watch_poll_interval_ms: self.dir_watch_poll_interval_ms,
            max_tail_buffer_entries: self.max_tail_buffer_entries,
            troubleshoot_mode: self.troubleshoot_mode,
        };
        if let Err(e) = crate::app::session::save(&data, session_path) {
            tracing::warn!(error = %e, "Failed to save session");
        }
    }

    /// Apply a previously loaded `SessionData` snapshot to this state.
    ///
    /// Entries are intentionally **not** restored here.  The user must click
    /// "Open Directory..." or "Open Log(s)..." to trigger a fresh parse so
    /// the view always reflects current on-disk state.
    pub fn restore_from_session(&mut self, data: crate::app::session::SessionData) {
        self.scan_path = data.scan_path;
        let f = &data.filter;
        self.filter_state.severity_levels = f.severity_levels.iter().copied().collect();
        // NOTE: source_files and hide_all_sources are intentionally NOT
        // restored from the session.  File-path whitelists are tightly coupled
        // to a particular scan directory and become silently stale when a new
        // scan runs over a different (or updated) directory.  Restoring them
        // caused confusing states where only one file was visible after restart
        // even though 400+ files had been discovered.
        // The values ARE still serialised by PersistedFilter so old sessions
        // round-trip without schema breakage, but we simply discard them here.
        self.filter_state.text_search = f.text_search.clone();
        self.filter_state.fuzzy = f.fuzzy;
        self.filter_state.relative_time_secs = f.relative_time_secs;
        self.filter_state.relative_time_input = f
            .relative_time_secs
            .map(|s| (s / 60).to_string())
            .unwrap_or_default();
        self.filter_state.bookmarks_only = f.bookmarks_only;
        // Restore the component filter.  component_filter references component
        // strings that are re-discovered on the next scan parse, so it remains
        // semantically valid on restore.
        self.filter_state.exclude_text = f.exclude_text.clone();
        self.filter_state.component_filter = f.component_filter.iter().cloned().collect();
        self.filter_state.hide_no_timestamp = f.hide_no_timestamp;
        self.filter_state.dedup_mode = f.dedup_mode;
        if !f.regex_pattern.is_empty() && self.filter_state.set_regex(&f.regex_pattern).is_err() {
            tracing::warn!(
                pattern = %f.regex_pattern,
                "Session restore: saved regex pattern is invalid, discarding"
            );
        }
        // Queue extra files for a secondary append scan after the initial
        // scan_path scan completes (handled in gui.rs::ParsingCompleted).
        self.extra_files_to_restore = data.extra_files;
        self.file_colours = data
            .file_colours
            .into_iter()
            .map(|(path, arr)| {
                let colour = egui::Color32::from_rgba_premultiplied(arr[0], arr[1], arr[2], arr[3]);
                (path, colour)
            })
            .collect();
        self.bookmarks = data.bookmarks.into_iter().collect();
        self.correlation_window_secs = data.correlation_window_secs;
        self.correlation_window_input = data.correlation_window_secs.to_string();
        // Restore the date filter so the re-scan triggered by initial_scan
        // applies the same modified_since cutoff as the original scan.
        self.discovery_date_input = data.discovery_date_input;
        self.ui_font_size = data.ui_font_size;
        // Restore user preferences so they survive application restarts.
        self.dark_mode = data.dark_mode;
        self.sort_descending = data.sort_descending;
        self.tail_auto_scroll = data.tail_auto_scroll;
        // Restore options / ingest limits.
        self.max_files_limit = data.max_files_limit;
        self.max_total_entries = data.max_total_entries;
        self.max_scan_depth = data.max_scan_depth;
        self.tail_poll_interval_ms = data.tail_poll_interval_ms;
        self.dir_watch_poll_interval_ms = data.dir_watch_poll_interval_ms;
        self.max_tail_buffer_entries = data.max_tail_buffer_entries;
        self.troubleshoot_mode = data.troubleshoot_mode;

        // Restore multi-term search state.
        self.multi_search_input = f.multi_search_input.clone();
        if !self.multi_search_input.is_empty() {
            let (include, exclude) =
                crate::core::multi_search::MultiSearch::parse_terms(&self.multi_search_input);
            self.filter_state.multi_search.include_terms = include;
            self.filter_state.multi_search.exclude_terms = exclude;
            self.filter_state.multi_search.mode = f.multi_search_mode;
            self.filter_state.multi_search.min_match = f.multi_search_min_match;
            self.filter_state.multi_search.case_insensitive = f.multi_search_case_insensitive;
            self.filter_state.multi_search.whole_word = f.multi_search_whole_word;
            self.filter_state.multi_search.regex_mode = f.multi_search_regex_mode;
            self.filter_state.multi_search.compile();
            if let Some(ref err) = self.filter_state.multi_search.compile_error {
                tracing::warn!(
                    error = %err,
                    "Session restore: saved multi-search terms failed to compile, discarding"
                );
                self.multi_search_input.clear();
                self.filter_state.multi_search = crate::core::multi_search::MultiSearch::default();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::Severity;
    use chrono::{TimeZone, Utc};

    /// Build a minimal LogEntry with a fixed timestamp for unit testing.
    fn make_entry(id: u64, offset_secs: i64) -> LogEntry {
        let base = Utc.with_ymd_and_hms(2024, 1, 15, 12, 0, 0).unwrap();
        let ts = base + chrono::Duration::seconds(offset_secs);
        LogEntry {
            id,
            timestamp: Some(ts),
            severity: Severity::Info,
            source_file: std::path::PathBuf::from("test.log"),
            line_number: 1,
            thread: None,
            component: None,
            message: String::new(),
            raw_text: String::new(),
            profile_id: "test".to_string(),
            file_modified: None,
        }
    }

    /// Correlation must identify all entries within the window and exclude
    /// those outside it, across all loaded entries (not just filtered ones).
    #[test]
    fn test_correlation_window_identifies_nearby_entries() {
        let mut state = AppState::new(vec![], false);
        // Anchor at t=0; entries at -35s, -20s, 0s, +20s, +35s.
        // With a 30-second window: -20, 0, +20 are in; -35 and +35 are out.
        state.entries = vec![
            make_entry(10, -35),
            make_entry(20, -20),
            make_entry(30, 0),
            make_entry(40, 20),
            make_entry(50, 35),
        ];
        // filtered_indices maps display position 0 -> entries[2] (the anchor)
        state.filtered_indices = vec![2];
        state.selected_index = Some(0);
        state.correlation_active = true;
        state.correlation_window_secs = 30;
        state.update_correlation();

        assert!(
            !state.correlated_ids.contains(&10),
            "-35s must be outside the 30s window"
        );
        assert!(
            state.correlated_ids.contains(&20),
            "-20s must be inside the 30s window"
        );
        assert!(
            state.correlated_ids.contains(&30),
            "anchor (0s) must be in the window"
        );
        assert!(
            state.correlated_ids.contains(&40),
            "+20s must be inside the 30s window"
        );
        assert!(
            !state.correlated_ids.contains(&50),
            "+35s must be outside the 30s window"
        );
    }

    /// Disabling correlation must clear the set immediately.
    #[test]
    fn test_correlation_clears_when_disabled() {
        let mut state = AppState::new(vec![], false);
        state.entries = vec![make_entry(1, 0), make_entry(2, 5)];
        state.filtered_indices = vec![0];
        state.selected_index = Some(0);
        state.correlation_active = true;
        state.correlation_window_secs = 30;
        state.update_correlation();

        // Sanity: both entries should be in the window.
        assert!(!state.correlated_ids.is_empty());

        // Now disable and recompute.
        state.correlation_active = false;
        state.update_correlation();
        assert!(
            state.correlated_ids.is_empty(),
            "correlated_ids must be empty when correlation is disabled"
        );
    }

    /// An entry with no timestamp cannot be an anchor; correlated_ids stays empty.
    #[test]
    fn test_correlation_no_timestamp_entry_yields_empty_set() {
        let mut state = AppState::new(vec![], false);
        let mut entry = make_entry(1, 0);
        entry.timestamp = None; // no timestamp
        state.entries = vec![entry];
        state.filtered_indices = vec![0];
        state.selected_index = Some(0);
        state.correlation_active = true;
        state.update_correlation();

        assert!(
            state.correlated_ids.is_empty(),
            "an anchor with no timestamp must produce an empty correlation set"
        );
    }

    /// Regression test for Bug #2: `apply_filters()` must preserve the
    /// selected entry by **entry ID**, not by display-position integer.
    ///
    /// Before the fix, applying a filter that shifted display positions would
    /// silently point `selected_index` at a different (wrong) entry because
    /// the old code only bounds-checked the integer index rather than
    /// re-finding the previously selected entry by its stable ID.
    #[test]
    fn test_apply_filters_preserves_selection_by_id() {
        let mut state = AppState::new(vec![], false);

        // Five entries with alternating Info / Error severities.
        // IDs: 0=Info, 1=Error, 2=Info, 3=Error, 4=Info
        for id in 0u64..5 {
            let mut e = make_entry(id, id as i64 * 60);
            e.severity = if id % 2 == 0 {
                Severity::Info
            } else {
                Severity::Error
            };
            state.entries.push(e);
        }

        // With no active filter every entry passes; display positions 0..4.
        state.apply_filters();
        assert_eq!(state.filtered_indices, vec![0, 1, 2, 3, 4]);

        // Select display position 3 → entries[3] → id = 3 (Error).
        state.selected_index = Some(3);
        assert_eq!(
            state.selected_entry().map(|e| e.id),
            Some(3),
            "pre-filter: selected entry must be id=3"
        );

        // Apply an Error-only filter.
        // After filtering, filtered_indices = [1, 3] (the two Error entries),
        // so entry id=3 is now at display position 1.
        state.filter_state.severity_levels.clear();
        state.filter_state.severity_levels.insert(Severity::Error);
        state.apply_filters();

        assert_eq!(
            state.selected_entry().map(|e| e.id),
            Some(3),
            "after filter: selected entry must still be id=3 (not shifted to a different entry)"
        );
        assert_eq!(
            state.selected_index,
            Some(1),
            "after filter: selected_index must be 1 (id=3 is now at display position 1)"
        );
    }

    // -------------------------------------------------------------------------
    // discovery_modified_since — datetime parsing precision tests
    // -------------------------------------------------------------------------

    fn state_with_input(s: &str) -> AppState {
        let mut st = AppState::new(vec![], false);
        st.discovery_date_input = s.to_string();
        st
    }

    #[test]
    fn test_discovery_modified_since_empty_returns_none() {
        let st = state_with_input("");
        assert!(st.discovery_modified_since().is_none());
    }

    #[test]
    fn test_discovery_modified_since_date_only_midnight_local() {
        use chrono::{Local, NaiveDate, TimeZone as _};
        let st = state_with_input("2025-03-14");
        let dt = st.discovery_modified_since().expect("should parse");
        // Input is interpreted as local midnight; verify the round-trip back to
        // the local NaiveDate equals the input date, regardless of timezone.
        let ndt = NaiveDate::from_ymd_opt(2025, 3, 14)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        let expected = Local
            .from_local_datetime(&ndt)
            .single()
            .map(|d| d.to_utc())
            .unwrap_or_else(|| ndt.and_utc());
        assert_eq!(dt, expected);
    }

    #[test]
    fn test_discovery_modified_since_date_and_hour_minute() {
        use chrono::{Local, NaiveDateTime, TimeZone as _};
        let st = state_with_input("2025-03-14 09:30");
        let dt = st.discovery_modified_since().expect("should parse");
        // Verify round-trip: UTC result converts back to the local time the
        // user typed (timezone-agnostic — works on any machine).  We check via
        // the expected UTC value built the same way the function builds it.
        let ndt =
            NaiveDateTime::parse_from_str("2025-03-14 09:30:00", "%Y-%m-%d %H:%M:%S").unwrap();
        let expected = Local
            .from_local_datetime(&ndt)
            .single()
            .map(|d| d.to_utc())
            .unwrap_or_else(|| ndt.and_utc());
        assert_eq!(dt, expected);
    }

    #[test]
    fn test_discovery_modified_since_full_datetime_to_second() {
        use chrono::{Local, NaiveDateTime, TimeZone as _};
        let st = state_with_input("2025-03-14 09:30:45");
        let dt = st.discovery_modified_since().expect("should parse");
        let ndt =
            NaiveDateTime::parse_from_str("2025-03-14 09:30:45", "%Y-%m-%d %H:%M:%S").unwrap();
        let expected = Local
            .from_local_datetime(&ndt)
            .single()
            .map(|d| d.to_utc())
            .unwrap_or_else(|| ndt.and_utc());
        assert_eq!(dt, expected);
    }

    #[test]
    fn test_discovery_modified_since_invalid_returns_none() {
        for bad in &["not-a-date", "2025/03/14", "14-03-2025", "2025-03-14T09:30"] {
            let st = state_with_input(bad);
            assert!(
                st.discovery_modified_since().is_none(),
                "'{bad}' should not parse"
            );
        }
    }

    /// Regression — Bug: `apply_filters()` did not call `update_correlation()`
    /// after recomputing `selected_index`.  When a filter change excluded the
    /// selected entry, `selected_index` became `None` but `correlated_ids` kept
    /// its old content, leaving phantom teal highlights on entries with no
    /// active selection.
    #[test]
    fn test_apply_filters_clears_correlation_when_selected_entry_is_hidden() {
        let mut state = AppState::new(vec![], false);

        // Three entries all at the same second (all fall inside a 30s window
        // relative to any of them).
        for id in 0u64..3 {
            let mut e = make_entry(id, 0);
            e.severity = if id == 1 {
                Severity::Error
            } else {
                Severity::Info
            };
            state.entries.push(e);
        }

        // Start with no filter — all three pass.
        state.apply_filters();
        assert_eq!(state.filtered_indices.len(), 3);

        // Select the Info entry at display position 0 (entries[0], id=0).
        state.selected_index = Some(0);
        state.correlation_active = true;
        state.correlation_window_secs = 30;
        state.update_correlation();

        // All three entries share the same timestamp so all should be correlated.
        assert_eq!(
            state.correlated_ids.len(),
            3,
            "all entries are within the 30s window before any filter"
        );

        // Now apply an Error-only filter.  entries[0] (Info, id=0) is hidden.
        // apply_filters() must:
        //   (a) move selected_index to None (selected entry no longer visible), and
        //   (b) clear correlated_ids via update_correlation() so no ghost highlights remain.
        state.filter_state.severity_levels.clear();
        state.filter_state.severity_levels.insert(Severity::Error);
        state.apply_filters();

        assert!(
            state.selected_index.is_none(),
            "selected_index must be None after the selected entry is hidden by a filter"
        );
        assert!(
            state.correlated_ids.is_empty(),
            "correlated_ids must be cleared when the selected entry is no longer visible \
             (otherwise phantom teal highlights appear with no active selection)"
        );
    }

    // -------------------------------------------------------------------------
    // toggle_sort_direction — sort order flag and selection stability
    // -------------------------------------------------------------------------

    /// `toggle_sort_direction` must flip `sort_descending` and leave
    /// `selected_index` unchanged (it is a stable position into
    /// `filtered_indices`, not a display-position integer that varies with
    /// sort order).  `selected_entry()` must therefore return the same entry
    /// both before and after the toggle.
    #[test]
    fn test_toggle_sort_direction_preserves_selected_entry() {
        let mut state = AppState::new(vec![], false);

        // Three entries at t+0, t+60, t+120 (ascending chronological order).
        state.entries = vec![make_entry(1, 0), make_entry(2, 60), make_entry(3, 120)];
        state.apply_filters(); // filtered_indices = [0, 1, 2]

        // Select the middle entry — filtered_indices[1] → entries[1] → id=2.
        state.selected_index = Some(1);
        let id_before = state.selected_entry().map(|e| e.id);
        assert_eq!(
            id_before,
            Some(2),
            "pre-toggle: selected entry must be id=2"
        );
        assert!(!state.sort_descending, "default sort must be ascending");

        // Toggle to descending.
        state.toggle_sort_direction();

        assert!(
            state.sort_descending,
            "after first toggle: sort must be descending"
        );
        assert_eq!(
            state.selected_index,
            Some(1),
            "selected_index must not change (stable position into filtered_indices)"
        );
        assert_eq!(
            state.selected_entry().map(|e| e.id),
            id_before,
            "selected_entry() must still return id=2 after toggling to descending"
        );

        // Toggle back to ascending.
        state.toggle_sort_direction();
        assert!(
            !state.sort_descending,
            "after second toggle: sort must be ascending again"
        );
        assert_eq!(
            state.selected_entry().map(|e| e.id),
            id_before,
            "selected_entry() must still return id=2 after toggling back to ascending"
        );
    }

    /// `sort_descending` must survive `clear()` unchanged, because it is a
    /// user preference like `dark_mode` and `tail_auto_scroll`.
    #[test]
    fn test_sort_descending_preserved_across_clear() {
        let mut state = AppState::new(vec![], false);
        assert!(!state.sort_descending, "default must be ascending (false)");
        state.sort_descending = true;
        state.clear();
        assert!(
            state.sort_descending,
            "sort_descending must not be reset by clear() - it is a user preference"
        );
    }

    // -------------------------------------------------------------------------
    // extra_files session persistence (Bug 7 regression)
    // -------------------------------------------------------------------------

    /// `save_session()` must persist `manually_added_files` into
    /// `SessionData.extra_files`.  Before the fix, `extra_files` was always
    /// serialised as `vec![]` so files added via "Add File(s)..." were silently
    /// lost when the app restarted.
    #[test]
    fn test_manually_added_files_round_trip_through_save_session() {
        use tempfile::TempDir;
        let dir = TempDir::new().unwrap();
        let session_file = dir.path().join("session.json");

        let mut state = AppState::new(vec![], false);
        state.session_path = Some(session_file.clone());
        state.manually_added_files = vec![
            std::path::PathBuf::from("/var/log/extra1.log"),
            std::path::PathBuf::from("/var/log/extra2.log"),
        ];

        state.save_session();

        let data = crate::app::session::load(&session_file)
            .expect("session file must exist after save_session");
        assert_eq!(
            data.extra_files.len(),
            2,
            "extra_files in session must contain both manually-added paths"
        );
        assert!(
            data.extra_files
                .contains(&std::path::PathBuf::from("/var/log/extra1.log")),
            "extra1.log must be present in saved extra_files"
        );
    }

    /// `restore_from_session()` must populate `extra_files_to_restore` from
    /// `SessionData.extra_files`.  Before the fix, `extra_files` was always
    /// silently discarded so extra files could never be re-added on restore.
    #[test]
    fn test_extra_files_restored_into_queue_after_restore_from_session() {
        let mut state = AppState::new(vec![], false);

        let data = crate::app::session::SessionData {
            version: crate::app::session::SESSION_VERSION,
            scan_path: Some(std::path::PathBuf::from("/var/log")),
            extra_files: vec![
                std::path::PathBuf::from("/tmp/a.log"),
                std::path::PathBuf::from("/tmp/b.log"),
            ],
            filter: crate::app::session::PersistedFilter::default(),
            file_colours: vec![],
            bookmarks: vec![],
            correlation_window_secs: crate::util::constants::DEFAULT_CORRELATION_WINDOW_SECS,
            discovery_date_input: String::new(),
            ui_font_size: crate::util::constants::DEFAULT_FONT_SIZE,
            dark_mode: true,
            sort_descending: false,
            tail_auto_scroll: true,
            max_files_limit: crate::util::constants::DEFAULT_MAX_FILES,
            max_total_entries: crate::util::constants::MAX_TOTAL_ENTRIES,
            max_scan_depth: crate::util::constants::DEFAULT_MAX_DEPTH,
            tail_poll_interval_ms: crate::util::constants::TAIL_POLL_INTERVAL_MS,
            dir_watch_poll_interval_ms: crate::util::constants::DIR_WATCH_POLL_INTERVAL_MS,
            max_tail_buffer_entries: crate::util::constants::DEFAULT_MAX_TAIL_BUFFER_ENTRIES,
            troubleshoot_mode: false,
        };

        state.restore_from_session(data);

        assert_eq!(
            state.extra_files_to_restore.len(),
            2,
            "extra_files_to_restore must hold both paths after restore_from_session"
        );
        assert!(
            state
                .extra_files_to_restore
                .contains(&std::path::PathBuf::from("/tmp/a.log")),
            "/tmp/a.log must be queued for restore"
        );
    }

    /// `clear()` must wipe both extra-file Vec fields so a new-directory scan
    /// does not inherit stale lists from the previous session.
    #[test]
    fn test_clear_removes_extra_file_lists() {
        let mut state = AppState::new(vec![], false);
        state.manually_added_files = vec![std::path::PathBuf::from("/a.log")];
        state.extra_files_to_restore = vec![std::path::PathBuf::from("/b.log")];

        state.clear();

        assert!(
            state.manually_added_files.is_empty(),
            "manually_added_files must be empty after clear()"
        );
        assert!(
            state.extra_files_to_restore.is_empty(),
            "extra_files_to_restore must be empty after clear()"
        );
    }
}
