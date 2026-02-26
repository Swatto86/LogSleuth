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

    /// Whether the Options dialog is currently open.
    pub show_options: bool,

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

    /// Date filter typed by the user in the "Open Directory" date-filter box.
    ///
    /// Expected format: `YYYY-MM-DD`.  An empty string means no date filter.
    /// Parsed into a UTC midnight boundary by `discovery_modified_since()` and
    /// passed as `DiscoveryConfig::modified_since` when a scan is started.
    ///
    /// Persists across scans so the user does not have to re-enter it each time.
    pub discovery_date_input: String,
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
            dark_mode: true, // default to dark; matches egui's own default
            debug_mode,
            pending_scan: None,
            request_cancel: false,
            file_list_search: String::new(),
            file_colours: HashMap::new(),
            pending_single_files: None,
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
            show_options: false,
            total_files_found: 0,
            pending_replace_files: None,
            request_new_session: false,
            discovery_date_input: String::new(),
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

        // Restore selection: find the new display position of the previously
        // selected entry by ID.  If the entry is no longer in the filtered set
        // (e.g. it was hidden by a new severity filter), clear the selection.
        self.selected_index = selected_id.and_then(|id| {
            self.filtered_indices
                .iter()
                .position(|&entry_idx| self.entries.get(entry_idx).is_some_and(|e| e.id == id))
        });
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
    pub fn assign_file_colour(&mut self, path: &PathBuf) {
        if !self.file_colours.contains_key(path) {
            let idx = self.file_colours.len();
            let colour = crate::ui::theme::file_colour(idx);
            self.file_colours.insert(path.clone(), colour);
        }
    }

    /// Return the palette colour for `path`, or a neutral grey if not found.
    pub fn colour_for_file(&self, path: &PathBuf) -> egui::Color32 {
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
        // Stop tail on clear — a new scan starts fresh.
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

    /// Parse `discovery_date_input` into a UTC `DateTime` representing the
    /// start of that calendar day (00:00:00 UTC).
    ///
    /// Returns `None` when the input is empty or cannot be parsed as YYYY-MM-DD.
    /// The caller passes this to `DiscoveryConfig::modified_since` when starting
    /// a scan so only files modified on or after that UTC midnight are ingested.
    pub fn discovery_modified_since(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        let trimmed = self.discovery_date_input.trim();
        if trimmed.is_empty() {
            return None;
        }
        chrono::NaiveDate::parse_from_str(trimmed, "%Y-%m-%d")
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
        let f = &data.filter;
        self.filter_state.severity_levels = f.severity_levels.iter().copied().collect();
        self.filter_state.source_files = f.source_files.iter().cloned().collect();
        self.filter_state.hide_all_sources = f.hide_all_sources;
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
}
