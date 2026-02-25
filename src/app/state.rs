// LogSleuth - app/state.rs
//
// Application state management. Holds the current scan results,
// filter state, selection, and profile list.
// Owned by the eframe::App implementation.

use crate::core::filter::FilterState;
use crate::core::model::{DiscoveredFile, FormatProfile, LogEntry, ScanSummary};
use std::collections::HashMap;
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

    /// Set to `true` by the gui.rs tail message handler whenever new entries
    /// arrive. Consumed and cleared by `timeline.rs` after it triggers the
    /// scroll-to-bottom, so scroll only fires on the frame new entries appear.
    pub tail_scroll_to_bottom: bool,

    /// Set by a UI panel to request starting the live tail watcher.
    /// Consumed and cleared by `gui.rs` in the update loop each frame.
    pub request_start_tail: bool,

    /// Set by a UI panel to request stopping the live tail watcher.
    /// Consumed and cleared by `gui.rs` in the update loop each frame.
    pub request_stop_tail: bool,
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
            debug_mode,
            pending_scan: None,
            request_cancel: false,
            file_list_search: String::new(),
            file_colours: HashMap::new(),
            pending_single_files: None,
            tail_active: false,
            tail_auto_scroll: true,
            tail_scroll_to_bottom: false,
            request_start_tail: false,
            request_stop_tail: false,
        }
    }

    /// Recompute filtered indices from current entries and filter state.
    ///
    /// If a relative time filter is active (`filter_state.relative_time_secs`),
    /// the effective `time_start` is computed here from `Utc::now()`.  This keeps
    /// the core filter layer pure (no side-effects or clock access).
    pub fn apply_filters(&mut self) {
        // Relative time filter: derive the absolute start bound each call so the
        // rolling window stays current as the clock advances.
        if let Some(secs) = self.filter_state.relative_time_secs {
            self.filter_state.time_start =
                Some(chrono::Utc::now() - chrono::Duration::seconds(secs as i64));
            self.filter_state.time_end = None;
        }

        self.filtered_indices =
            crate::core::filter::apply_filters(&self.entries, &self.filter_state);

        // Clear selection if it is out of range
        if let Some(idx) = self.selected_index {
            if idx >= self.filtered_indices.len() {
                self.selected_index = None;
            }
        }
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
    pub fn next_entry_id(&self) -> u64 {
        self.entries.last().map(|e| e.id + 1).unwrap_or(0)
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
        self.tail_scroll_to_bottom = false;
        self.request_start_tail = false;
        self.request_stop_tail = false;
        // tail_auto_scroll preference is intentionally preserved across clears.
    }
}
