// LogSleuth - core/filter.rs
//
// Composable filter engine for log entries.
// All active filters are AND-combined.
// Core layer: pure logic, no I/O or UI dependencies.

use crate::core::model::{LogEntry, Severity};
use crate::util::error::FilterError;
use chrono::{DateTime, Utc};
use regex::Regex;
use std::collections::HashSet;
use std::path::PathBuf;

/// Complete filter state. All fields are AND-combined when applied.
#[derive(Debug, Clone, Default)]
pub struct FilterState {
    /// Severity levels to include (empty = all).
    pub severity_levels: HashSet<Severity>,

    /// Source files to include (empty = all).
    pub source_files: HashSet<PathBuf>,

    /// Start of time range (inclusive). None = no lower bound.
    pub time_start: Option<DateTime<Utc>>,

    /// End of time range (inclusive). None = no upper bound.
    pub time_end: Option<DateTime<Utc>>,

    /// Substring text search (case-insensitive). Empty = no filter.
    pub text_search: String,

    /// Compiled regex search. None = no regex filter.
    pub regex_search: Option<Regex>,
}

impl FilterState {
    /// Returns true if no filters are active.
    pub fn is_empty(&self) -> bool {
        self.severity_levels.is_empty()
            && self.source_files.is_empty()
            && self.time_start.is_none()
            && self.time_end.is_none()
            && self.text_search.is_empty()
            && self.regex_search.is_none()
    }

    /// Set the regex search pattern, compiling it.
    /// Returns an error if the pattern is invalid.
    pub fn set_regex(&mut self, pattern: &str) -> Result<(), FilterError> {
        if pattern.is_empty() {
            self.regex_search = None;
            return Ok(());
        }
        let regex = Regex::new(pattern).map_err(|e| FilterError::InvalidRegex {
            pattern: pattern.to_string(),
            source: e,
        })?;
        self.regex_search = Some(regex);
        Ok(())
    }

    /// Create a quick-filter for errors only.
    pub fn errors_only() -> Self {
        let mut levels = HashSet::new();
        levels.insert(Severity::Critical);
        levels.insert(Severity::Error);
        Self {
            severity_levels: levels,
            ..Default::default()
        }
    }

    /// Create a quick-filter for errors and warnings.
    pub fn errors_and_warnings() -> Self {
        let mut levels = HashSet::new();
        levels.insert(Severity::Critical);
        levels.insert(Severity::Error);
        levels.insert(Severity::Warning);
        Self {
            severity_levels: levels,
            ..Default::default()
        }
    }
}

/// Apply filters to a slice of entries, returning indices of matching entries.
///
/// Returns a Vec of indices into the original entries slice. This avoids
/// copying entries and enables virtual scrolling on the filtered view.
pub fn apply_filters(entries: &[LogEntry], filter: &FilterState) -> Vec<usize> {
    if filter.is_empty() {
        return (0..entries.len()).collect();
    }

    let text_lower = filter.text_search.to_lowercase();

    entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| matches_all(entry, filter, &text_lower))
        .map(|(idx, _)| idx)
        .collect()
}

/// Check if a single entry matches all active filters.
fn matches_all(entry: &LogEntry, filter: &FilterState, text_lower: &str) -> bool {
    // Severity filter
    if !filter.severity_levels.is_empty() && !filter.severity_levels.contains(&entry.severity) {
        return false;
    }

    // Source file filter
    if !filter.source_files.is_empty() && !filter.source_files.contains(&entry.source_file) {
        return false;
    }

    // Time range filter
    if let Some(ref start) = filter.time_start {
        match entry.timestamp {
            Some(ts) if ts < *start => return false,
            None => return false, // Entries without timestamps excluded from time filters
            _ => {}
        }
    }
    if let Some(ref end) = filter.time_end {
        match entry.timestamp {
            Some(ts) if ts > *end => return false,
            None => return false,
            _ => {}
        }
    }

    // Text search (case-insensitive substring)
    if !text_lower.is_empty() && !entry.message.to_lowercase().contains(text_lower) {
        return false;
    }

    // Regex search
    if let Some(ref regex) = filter.regex_search {
        if !regex.is_match(&entry.message) {
            return false;
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::Severity;
    use std::path::PathBuf;

    fn make_entry(id: u64, severity: Severity, message: &str) -> LogEntry {
        LogEntry {
            id,
            timestamp: None,
            severity,
            source_file: PathBuf::from("test.log"),
            line_number: id,
            thread: None,
            component: None,
            message: message.to_string(),
            raw_text: message.to_string(),
            profile_id: "test".to_string(),
        }
    }

    #[test]
    fn test_empty_filter_returns_all() {
        let entries = vec![
            make_entry(1, Severity::Error, "Error 1"),
            make_entry(2, Severity::Info, "Info 1"),
        ];
        let result = apply_filters(&entries, &FilterState::default());
        assert_eq!(result, vec![0, 1]);
    }

    #[test]
    fn test_severity_filter() {
        let entries = vec![
            make_entry(1, Severity::Error, "Error 1"),
            make_entry(2, Severity::Info, "Info 1"),
            make_entry(3, Severity::Warning, "Warning 1"),
        ];
        let result = apply_filters(&entries, &FilterState::errors_only());
        assert_eq!(result, vec![0]); // Only Error (Critical set too but none present)
    }

    #[test]
    fn test_text_search_case_insensitive() {
        let entries = vec![
            make_entry(1, Severity::Error, "Connection FAILED"),
            make_entry(2, Severity::Info, "Connection succeeded"),
        ];
        let filter = FilterState {
            text_search: "failed".to_string(),
            ..Default::default()
        };
        let result = apply_filters(&entries, &filter);
        assert_eq!(result, vec![0]);
    }

    #[test]
    fn test_regex_filter() {
        let entries = vec![
            make_entry(1, Severity::Error, "Error code: 404"),
            make_entry(2, Severity::Error, "Error code: 500"),
            make_entry(3, Severity::Info, "Status OK"),
        ];
        let mut filter = FilterState::default();
        filter.set_regex(r"code:\s*5\d{2}").unwrap();
        let result = apply_filters(&entries, &filter);
        assert_eq!(result, vec![1]);
    }

    #[test]
    fn test_combined_filters() {
        let entries = vec![
            make_entry(1, Severity::Error, "Database connection failed"),
            make_entry(2, Severity::Error, "Network timeout"),
            make_entry(3, Severity::Info, "Database query ok"),
        ];
        let filter = FilterState {
            severity_levels: {
                let mut s = HashSet::new();
                s.insert(Severity::Error);
                s
            },
            text_search: "database".to_string(),
            ..Default::default()
        };
        let result = apply_filters(&entries, &filter);
        assert_eq!(result, vec![0]); // Error + contains "database"
    }

    #[test]
    fn test_invalid_regex() {
        let mut filter = FilterState::default();
        let result = filter.set_regex("[invalid");
        assert!(result.is_err());
    }
}
