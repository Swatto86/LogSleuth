// LogSleuth - app/state.rs
//
// Application state management. Holds the current scan results,
// filter state, selection, and profile list.
// Owned by the eframe::App implementation.

use crate::core::filter::FilterState;
use crate::core::model::{DiscoveredFile, FormatProfile, LogEntry, ScanSummary};
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

    /// Whether debug mode is enabled.
    pub debug_mode: bool,
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
            debug_mode,
        }
    }

    /// Recompute filtered indices from current entries and filter state.
    pub fn apply_filters(&mut self) {
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
        self.status_message = "Ready.".to_string();
    }
}
