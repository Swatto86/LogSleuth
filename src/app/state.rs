// LogSleuth - app/state.rs
//
// Application state management. Holds the current scan results,
// filter state, selection, and profile list.
// Owned by the eframe::App implementation.

use crate::core::filter::FilterState;
use crate::core::model::{DiscoveredFile, FormatProfile, LogEntry, ScanSummary};
use crate::util::constants::{DEFAULT_CORRELATION_WINDOW_SECS, MAX_CLIPBOARD_ENTRIES};
use std::collections::{HashMap, HashSet};
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
    ///   `YYYY-MM-DD`          — day precision (treated as 00:00:00 UTC)
    ///
    /// An empty string means no filter.  Parsed by `discovery_modified_since()`
    /// and passed as `DiscoveryConfig::modified_since` when a scan is started.
    ///
    /// Persists across scans so the user does not have to re-enter it each time.
    pub discovery_date_input: String,

    /// Which tab is active in the left sidebar.  0 = Files, 1 = Filters.
    /// Pure UI state — not saved to the session file.
    pub sidebar_tab: usize,

    /// Text typed into the "path" input box in the scan controls.  Supports
    /// local paths, mapped drives, and UNC paths (\\server\share\logs).
    /// Pure UI state — not saved to the session file.
    pub directory_path_input: String,
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
            scan_summary: None,
            status_message: "Ready. Open a directory to begin scanning.".to_string(),
            warnings: Vec::new(),
            show_summary: false,
            show_log_summary: false,
            show_about: false,
            dark_mode: true,        // default to dark; matches egui's own default
            sort_descending: false, // default ascending (oldest first)
            debug_mode,
            pending_scan: None,
            request_cancel: false,
            file_list_search: String::new(),
            file_colours: HashMap::new(),
            pending_single_files: None,
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
            max_files_limit: crate::util::constants::DEFAULT_MAX_FILES,
            max_total_entries: crate::util::constants::MAX_TOTAL_ENTRIES,
            max_scan_depth: crate::util::constants::DEFAULT_MAX_DEPTH,
            tail_poll_interval_ms: crate::util::constants::TAIL_POLL_INTERVAL_MS,
            dir_watch_poll_interval_ms: crate::util::constants::DIR_WATCH_POLL_INTERVAL_MS,
            show_options: false,
            scroll_top_requested: false,
            total_files_found: 0,
            pending_replace_files: None,
            request_new_session: false,
            discovery_date_input: String::new(),
            sidebar_tab: 0,
            directory_path_input: String::new(),
        }
    }

    /// Returns the UTC cutoff instant for the current activity window, or
    /// `None` if the window is disabled.  Re-evaluated on every call so the
    /// rolling window stays current as the clock advances.
    pub fn activity_cutoff(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        self.activity_window_secs
            .map(|secs| chrono::Utc::now() - chrono::Duration::seconds(secs as i64))
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
        // Relative time filter: derive the absolute start bound each call so the
        // rolling window stays current as the clock advances.
        if let Some(secs) = self.filter_state.relative_time_secs {
            self.filter_state.time_start =
                Some(chrono::Utc::now() - chrono::Duration::seconds(secs as i64));
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
        if let Some(cutoff) = self.activity_cutoff() {
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

        // Restore selection: find the new display position of the previously
        // selected entry by ID.  If the entry is no longer in the filtered set
        // (e.g. it was hidden by a new severity filter), clear the selection.
        self.selected_index = selected_id.and_then(|id| {
            self.filtered_indices
                .iter()
                .position(|&entry_idx| self.entries.get(entry_idx).is_some_and(|e| e.id == id))
        });

        // Recompute the correlation overlay whenever the filter changes.
        // Without this, if the selected entry is hidden by the new filter
        // (selected_index → None), correlated_ids would still hold the old
        // window, showing ghost teal highlights with no active selection.
        self.update_correlation();
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
        let start = anchor_ts - window;
        let end = anchor_ts + window;
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

    /// Return the next available monotonic entry ID.
    ///
    /// Used when starting the live tail watcher so tail entry IDs continue
    /// from where the scan left off and do not collide with existing IDs.
    ///
    /// Note: entries are sorted chronologically after a scan, so
    /// `entries.last()` is the most recent by *timestamp*, not the
    /// highest by *ID*. We must iterate all entries to find the true
    /// maximum ID, otherwise tail entries can collide with scan entries.
    pub fn next_entry_id(&self) -> u64 {
        self.entries
            .iter()
            .map(|e| e.id)
            .max()
            .map(|m| m + 1)
            .unwrap_or(0)
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
        for id in &ids {
            if let Some(entry) = self.entries.iter().find(|e| e.id == *id) {
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
                let preview: String = entry.message.chars().take(200).collect();
                let sev = format!("{:?}", entry.severity);
                out.push_str(&format!("[{ts}] {sev:<8} {src}{label}\n  {preview}\n\n"));
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
            let mut sevs: Vec<String> = self
                .filter_state
                .severity_levels
                .iter()
                .map(|s| format!("{s:?}"))
                .collect();
            sevs.sort();
            filter_parts.push(format!("Severity: {}", sevs.join("+")));
        }
        if let Some(secs) = self.filter_state.relative_time_secs {
            if secs < 3_600 {
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
                let sev = format!("{:?}", entry.severity);
                let preview: String = entry.message.chars().take(200).collect();
                out.push_str(&format!("[{ts}] {sev:<8}  {src}\n  {preview}\n\n"));
            }
        }

        if take < total_filtered {
            out.push_str(&format!(
                "--- truncated: {take} of {total_filtered} entries shown (limit: {MAX_CLIPBOARD_ENTRIES}) ---\n"
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
        self.scan_summary = None;
        self.warnings.clear();
        self.show_summary = false;
        self.show_log_summary = false;
        self.status_message = "Ready.".to_string();
        self.pending_scan = None;
        self.request_cancel = false;
        self.file_list_search.clear();
        self.file_colours.clear();
        self.pending_single_files = None;
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
        // Reset per-scan discovery counters.
        self.total_files_found = 0;
        self.pending_replace_files = None;
        self.request_new_session = false;
    }

    /// Reset to the initial blank state: clears everything `clear()` clears
    /// **plus** the selected scan path, leaving the app as if it was freshly
    /// launched with no directory chosen.
    pub fn new_session(&mut self) {
        self.clear();
        self.scan_path = None;
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
        use chrono::NaiveDate;
        use chrono::NaiveDateTime;

        let trimmed = self.discovery_date_input.trim();
        if trimmed.is_empty() {
            return None;
        }

        // 1. Full datetime to the second: "YYYY-MM-DD HH:MM:SS"
        if let Ok(ndt) = NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%d %H:%M:%S") {
            return Some(ndt.and_utc());
        }
        // 2. Datetime to the minute: "YYYY-MM-DD HH:MM"
        if let Ok(ndt) = NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%d %H:%M") {
            return Some(ndt.and_utc());
        }
        // 3. Date only: "YYYY-MM-DD" — treat as start of day (00:00:00 UTC).
        NaiveDate::parse_from_str(trimmed, "%Y-%m-%d")
            .ok()
            .and_then(|d| d.and_hms_opt(0, 0, 0))
            .map(|ndt| ndt.and_utc())
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
            extra_files: vec![],
            filter,
            file_colours,
            bookmarks,
            correlation_window_secs: self.correlation_window_secs,
            discovery_date_input: self.discovery_date_input.clone(),
        };
        if let Err(e) = crate::app::session::save(&data, session_path) {
            tracing::warn!(error = %e, "Failed to save session");
        }
    }

    /// Apply a previously loaded `SessionData` snapshot to this state.
    ///
    /// Entries are intentionally **not** restored here — the log files will be
    /// re-parsed from `scan_path` via `initial_scan` so the view always reflects
    /// current file contents.
    pub fn restore_from_session(&mut self, data: crate::app::session::SessionData) {
        self.scan_path = data.scan_path;
        // Pre-populate the path text box so the user sees (and can edit)
        // the restored path immediately on startup without needing to retype it.
        self.directory_path_input = self
            .scan_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
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
        if !f.regex_pattern.is_empty() {
            let _ = self.filter_state.set_regex(&f.regex_pattern);
        }
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
    fn test_discovery_modified_since_date_only_midnight_utc() {
        let st = state_with_input("2025-03-14");
        let dt = st.discovery_modified_since().expect("should parse");
        assert_eq!(
            dt.format("%Y-%m-%d %H:%M:%S").to_string(),
            "2025-03-14 00:00:00"
        );
    }

    #[test]
    fn test_discovery_modified_since_date_and_hour_minute() {
        let st = state_with_input("2025-03-14 09:30");
        let dt = st.discovery_modified_since().expect("should parse");
        assert_eq!(
            dt.format("%Y-%m-%d %H:%M:%S").to_string(),
            "2025-03-14 09:30:00"
        );
    }

    #[test]
    fn test_discovery_modified_since_full_datetime_to_second() {
        let st = state_with_input("2025-03-14 09:30:45");
        let dt = st.discovery_modified_since().expect("should parse");
        assert_eq!(
            dt.format("%Y-%m-%d %H:%M:%S").to_string(),
            "2025-03-14 09:30:45"
        );
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
            "sort_descending must not be reset by clear() — it is a user preference"
        );
    }
}
