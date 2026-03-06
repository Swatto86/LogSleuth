// LogSleuth - core/filter.rs
//
// Composable filter engine for log entries.
// All active filters are AND-combined.
// Core layer: pure logic, no I/O or UI dependencies.

use crate::core::model::{LogEntry, Severity};
use crate::core::multi_search::MultiSearch;
use crate::util::error::FilterError;
use chrono::{DateTime, Utc};
use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::OnceLock;

// =============================================================================
// Deduplication
// =============================================================================

/// Deduplication mode for the post-filter dedup pass.
///
/// When enabled, duplicate log entries (per source file) are collapsed so only
/// the latest occurrence of each unique message is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum DedupMode {
    /// No deduplication (default).
    #[default]
    Off,
    /// Exact character-for-character message match.
    Exact,
    /// Normalised match: variable data (IPs, GUIDs, hex strings, numbers) is
    /// replaced with tokens before comparing.
    Normalized,
}

impl DedupMode {
    /// Human-readable label for UI display.
    pub fn label(self) -> &'static str {
        match self {
            DedupMode::Off => "Off",
            DedupMode::Exact => "Exact match",
            DedupMode::Normalized => "Normalized",
        }
    }

    /// All variants in display order.
    pub fn all() -> &'static [DedupMode] {
        &[DedupMode::Off, DedupMode::Exact, DedupMode::Normalized]
    }
}

/// Metadata for a group of deduplicated entries.
///
/// Attached to the surviving (latest) entry in each dedup group.
#[derive(Debug, Clone)]
pub struct DedupInfo {
    /// Total number of occurrences (including the surviving entry).
    pub count: usize,
    /// Timestamp of the earliest occurrence, if available.
    pub first_timestamp: Option<DateTime<Utc>>,
    /// Global indices into `AppState::entries` for every occurrence in the
    /// group (including the surviving entry).  Ordered by timestamp ascending
    /// (or by entry ID when timestamps are absent).
    pub all_indices: Vec<usize>,
}

/// Compiled regex set for message normalisation.
///
/// Initialised once via `OnceLock` to avoid recompiling on every call.
/// Order matters: GUIDs before hex (GUIDs contain hex), IPs before numbers.
struct NormRegexes {
    guid: Regex,
    ipv6: Regex,
    ipv4: Regex,
    hex_prefixed: Regex,
    number: Regex,
}

fn norm_regexes() -> &'static NormRegexes {
    static INSTANCE: OnceLock<NormRegexes> = OnceLock::new();
    INSTANCE.get_or_init(|| NormRegexes {
        // GUIDs: 8-4-4-4-12 hex pattern
        guid: Regex::new(
            r"(?i)\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b",
        )
        .expect("GUID regex must compile"),
        // IPv6: simplified pattern covering common representations
        ipv6: Regex::new(
            r"(?i)\b(?:[0-9a-f]{1,4}:){2,7}[0-9a-f]{1,4}\b|(?i)\b(?:[0-9a-f]{1,4}:){1,6}:[0-9a-f]{1,4}\b|::1\b",
        )
        .expect("IPv6 regex must compile"),
        // IPv4 with optional port
        ipv4: Regex::new(r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}(:\d+)?\b")
            .expect("IPv4 regex must compile"),
        // Hex strings: 0x-prefixed (8+ hex digits)
        hex_prefixed: Regex::new(r"(?i)\b0x[0-9a-f]{8,}\b")
            .expect("hex-prefixed regex must compile"),
        // Standalone integers and decimals
        number: Regex::new(r"\b\d+(\.\d+)?\b").expect("number regex must compile"),
    })
}

/// Normalise a log message by replacing variable data with fixed tokens.
///
/// Replacement order prevents conflicts:
/// 1. GUIDs (contain hex sequences that the hex pattern would also match)
/// 2. IPv6 addresses
/// 3. IPv4 addresses with optional port (contain digits the number pattern would eat)
/// 4. 0x-prefixed hex strings (8+ hex digits)
/// 5. Standalone numbers (integers and decimals)
///
/// The result is suitable as a dedup grouping key: two messages that differ
/// only in variable data will produce the same normalised string.
pub fn normalize_message(msg: &str) -> String {
    let re = norm_regexes();
    let s = re.guid.replace_all(msg, "<GUID>");
    let s = re.ipv6.replace_all(&s, "<IP>");
    let s = re.ipv4.replace_all(&s, "<IP>");
    let s = re.hex_prefixed.replace_all(&s, "<HEX>");
    let s = re.number.replace_all(&s, "<NUM>");
    s.into_owned()
}

/// Apply deduplication to an already-filtered index set.
///
/// Groups entries by `(source_file, message_key)` where `message_key` is either
/// the raw message (Exact mode) or the normalised message (Normalized mode).
/// Within each group, the entry with the **latest** timestamp is kept; ties are
/// broken by entry ID (higher = newer).
///
/// Returns:
/// - A new filtered-index vector containing only the surviving entries, in the
///   same relative order as the input.
/// - A map from each surviving entry's global index to its [`DedupInfo`].
pub fn apply_dedup(
    entries: &[LogEntry],
    filtered_indices: &[usize],
    mode: DedupMode,
) -> (Vec<usize>, HashMap<usize, DedupInfo>) {
    if mode == DedupMode::Off {
        return (filtered_indices.to_vec(), HashMap::new());
    }

    // Group indices by (source_file, message_key).
    // Use a BTreeMap-like approach but HashMap is fine for grouping.
    let mut groups: HashMap<(&PathBuf, String), Vec<usize>> = HashMap::new();

    for &idx in filtered_indices {
        let Some(entry) = entries.get(idx) else {
            continue;
        };
        let key = match mode {
            DedupMode::Exact => entry.message.clone(),
            DedupMode::Normalized => normalize_message(&entry.message),
            DedupMode::Off => unreachable!(),
        };
        groups
            .entry((&entry.source_file, key))
            .or_default()
            .push(idx);
    }

    // For each group, pick the latest entry and build DedupInfo.
    let mut survivors: HashMap<usize, DedupInfo> = HashMap::with_capacity(groups.len());

    for indices in groups.values() {
        // Find the entry with the latest timestamp (or highest ID as tiebreaker).
        let &latest_idx = indices
            .iter()
            .max_by(|&&a, &&b| {
                let ea = &entries[a];
                let eb = &entries[b];
                ea.timestamp
                    .cmp(&eb.timestamp)
                    .then_with(|| ea.id.cmp(&eb.id))
            })
            .expect("group must be non-empty");

        let first_timestamp = indices.iter().filter_map(|&i| entries[i].timestamp).min();

        // Sort all_indices by timestamp ascending (then ID) for the detail panel.
        let mut all_sorted = indices.clone();
        all_sorted.sort_by(|&a, &b| {
            entries[a]
                .timestamp
                .cmp(&entries[b].timestamp)
                .then_with(|| entries[a].id.cmp(&entries[b].id))
        });

        survivors.insert(
            latest_idx,
            DedupInfo {
                count: indices.len(),
                first_timestamp,
                all_indices: all_sorted,
            },
        );
    }

    // Build the output index vector preserving the original filter order,
    // but only including surviving entries.
    let survivor_set: HashSet<usize> = survivors.keys().copied().collect();
    let deduped_indices: Vec<usize> = filtered_indices
        .iter()
        .copied()
        .filter(|idx| survivor_set.contains(idx))
        .collect();

    (deduped_indices, survivors)
}

/// Complete filter state. All fields are AND-combined when applied.
#[derive(Debug, Clone, Default)]
pub struct FilterState {
    /// Severity levels to include (empty = all).
    pub severity_levels: HashSet<Severity>,

    /// Source files to include (empty = all, unless `hide_all_sources` is true).
    pub source_files: HashSet<PathBuf>,

    /// When true, no source file passes the filter — representing the "none
    /// selected" state.  An empty `source_files` set alone cannot represent
    /// this because empty is interpreted as "all pass".
    pub hide_all_sources: bool,

    /// Start of time range (inclusive). None = no lower bound.
    pub time_start: Option<DateTime<Utc>>,

    /// End of time range (inclusive). None = no upper bound.
    pub time_end: Option<DateTime<Utc>>,

    /// Text search term. When `fuzzy` is false this is a case-insensitive
    /// substring match; when true it uses subsequence fuzzy matching.
    /// Empty = no filter.
    pub text_search: String,

    /// When true, `text_search` uses fuzzy (subsequence) matching instead
    /// of exact case-insensitive substring matching.
    pub fuzzy: bool,

    /// Raw regex pattern string kept for the UI input buffer.
    /// The compiled form is in `regex_search`.
    pub regex_pattern: String,

    /// Compiled regex search. None = no regex filter (or last pattern was invalid).
    pub regex_search: Option<Regex>,

    /// Relative time window in seconds.  When Some(n), only entries timestamped
    /// within the last n seconds are shown.  The actual `time_start` value is
    /// computed from `Utc::now()` by the *app* layer (not core) so that core
    /// remains a pure, side-effect-free function.
    pub relative_time_secs: Option<u64>,

    /// UI text buffer for the custom relative-time input (stores minutes typed
    /// by the user before parsing).  Not used by the filter logic itself.
    pub relative_time_input: String,

    /// When true, only entries whose IDs appear in `bookmarked_ids` pass
    /// the filter.  The set is populated by the app layer before each filter
    /// application so that core remains a pure, state-free function.
    pub bookmarks_only: bool,

    /// The set of bookmarked entry IDs used when `bookmarks_only` is true.
    /// Populated from `AppState::bookmarks` by `apply_filters()` in the app layer.
    pub bookmarked_ids: HashSet<u64>,

    /// Text to EXCLUDE: entries whose message, thread, or component contains this
    /// substring (case-insensitive) are hidden.  The complement of `text_search`.
    /// Empty = no exclusion filter active.
    pub exclude_text: String,

    /// Component / module names to include.  When non-empty, only entries whose
    /// `component` value is present in this set pass.  Entries with no parsed
    /// component value are excluded when this filter is active.
    /// An empty set means all components pass.
    pub component_filter: HashSet<String>,

    /// UI text buffer for the absolute "from" datetime input (e.g. "2026-01-15 14:30").
    /// Parsed by the UI layer on commit and applied directly to `time_start`.
    /// Kept in sync so the widget re-shows the active value after a Clear.
    pub abs_time_start_input: String,

    /// UI text buffer for the absolute "to" datetime input (e.g. "2026-01-15 16:00").
    /// Parsed by the UI layer on commit and applied directly to `time_end`.
    pub abs_time_end_input: String,

    /// When true, entries with no parsed timestamp in their source text are hidden.
    /// Entries that fall back to a file-modification-time estimate are also excluded.
    /// Useful when precise event-time analysis matters and mtime-only entries would
    /// be misleading.  False by default (all entries shown regardless of timestamp
    /// availability).
    pub hide_no_timestamp: bool,

    /// Deduplication mode.  When not `Off`, a post-filter pass collapses
    /// duplicate log messages (per source file) so only the latest occurrence
    /// of each unique message is shown.  `Exact` compares messages character-
    /// for-character; `Normalized` replaces variable data (IPs, GUIDs, hex,
    /// numbers) with tokens before comparing.
    pub dedup_mode: DedupMode,

    /// Multi-term search configuration.  When active (non-empty terms),
    /// entries must also pass the multi-search filter in addition to all
    /// other filters.  Supports ANY/ALL modes, NOT terms, minimum match
    /// thresholds, and per-term highlighting.
    pub multi_search: MultiSearch,
}

impl FilterState {
    /// Returns true if no filters are active.
    /// Note: `fuzzy` is a mode toggle, not a filter value, so it is not counted here.
    /// `abs_time_start_input` / `abs_time_end_input` are UI display buffers; the
    /// semantic truth is in `time_start` / `time_end` which are already checked.
    pub fn is_empty(&self) -> bool {
        self.severity_levels.is_empty()
            && self.source_files.is_empty()
            && !self.hide_all_sources
            && self.time_start.is_none()
            && self.time_end.is_none()
            && self.text_search.is_empty()
            && self.regex_search.is_none()
            && self.relative_time_secs.is_none()
            && !self.bookmarks_only
            && self.exclude_text.is_empty()
            && self.component_filter.is_empty()
            && !self.hide_no_timestamp
            && self.dedup_mode == DedupMode::Off
            && self.multi_search.is_empty()
    }

    /// Set the regex search pattern, compiling it.
    /// Always updates `regex_pattern` (for the UI buffer).
    /// On success updates `regex_search`; on failure clears it and returns Err.
    pub fn set_regex(&mut self, pattern: &str) -> Result<(), FilterError> {
        self.regex_pattern = pattern.to_string();
        if pattern.is_empty() {
            self.regex_search = None;
            return Ok(());
        }
        let regex = RegexBuilder::new(pattern)
            .case_insensitive(true)
            .build()
            .map_err(|e| FilterError::InvalidRegex {
                pattern: pattern.to_string(),
                source: e,
            })?;
        self.regex_search = Some(regex);
        Ok(())
    }

    /// Create a quick-filter for errors only, preserving the current fuzzy mode.
    pub fn errors_only_from(fuzzy: bool) -> Self {
        let mut levels = HashSet::new();
        levels.insert(Severity::Critical);
        levels.insert(Severity::Error);
        Self {
            severity_levels: levels,
            fuzzy,
            ..Default::default()
        }
    }

    /// Create a quick-filter for errors only.
    pub fn errors_only() -> Self {
        Self::errors_only_from(false)
    }

    /// Create a quick-filter for errors and warnings, preserving the current fuzzy mode.
    pub fn errors_and_warnings_from(fuzzy: bool) -> Self {
        let mut levels = HashSet::new();
        levels.insert(Severity::Critical);
        levels.insert(Severity::Error);
        levels.insert(Severity::Warning);
        Self {
            severity_levels: levels,
            fuzzy,
            ..Default::default()
        }
    }
}

/// Test whether a single entry matches all active filters in `filter`.
///
/// Exposed publicly so the app layer can perform incremental `filtered_indices`
/// updates (e.g. for live-tail fast-path appends) without rebuilding the entire
/// index from scratch.
///
/// `text_lower` must be `filter.text_search.to_lowercase()` — the caller is
/// responsible for pre-computing this to avoid redundant allocations on every
/// call.  Pass `&String::new()` (or `""`) when there is no text search active.
///
/// `excl_lower` is computed internally from `filter.exclude_text`.  For the
/// hot-path `apply_filters` bulk scan the pre-computed version from
/// `apply_filters` is used instead; this wrapper is for the live-tail
/// single-entry fast path where the allocation cost is negligible.
#[inline]
pub fn entry_matches(entry: &LogEntry, filter: &FilterState, text_lower: &str) -> bool {
    let excl_lower = filter.exclude_text.to_lowercase();
    matches_all(entry, filter, text_lower, &excl_lower)
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
    // Pre-compute the exclusion pattern so we allocate once per apply_filters
    // call rather than once per entry (critical for 1M-entry data sets).
    let excl_lower = filter.exclude_text.to_lowercase();

    // Pre-allocate with a heuristic capacity.  When a text/regex filter is
    // active only a fraction of entries will match; use 1/4 of total as a
    // rough estimate to avoid the first few realloc doubling cycles while
    // not over-allocating for large entry sets.  When only severity or time
    // filters are active most entries still match, so use full capacity.
    let has_text_filter = !text_lower.is_empty()
        || filter.regex_search.is_some()
        || !excl_lower.is_empty()
        || !filter.component_filter.is_empty();
    let initial_capacity = if has_text_filter {
        entries.len() / 4
    } else {
        entries.len()
    };
    let mut result = Vec::with_capacity(initial_capacity);

    for (idx, entry) in entries.iter().enumerate() {
        if matches_all(entry, filter, &text_lower, &excl_lower) {
            result.push(idx);
        }
    }
    result
}

/// Return true if every character of `query` appears in `text` in order
/// (case-insensitive subsequence / fuzzy match).
///
/// `query` must already be lowercased by the caller.  `text` is compared
/// in its original form using per-char `to_lowercase`, avoiding a whole-
/// string allocation for the common case of ASCII-only log lines.
///
/// Examples:
///   fuzzy_match("cerr", "Connection error") -> true  (c..e..r..r)
///   fuzzy_match("fail", "FAILED")            -> true
///   fuzzy_match("xyz",  "Connection error")  -> false
pub fn fuzzy_match(query: &str, text: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let mut text_chars = text.chars();
    'outer: for qc in query.chars() {
        // query is already lowercased by the caller (apply_filters passes text_lower)
        loop {
            match text_chars.next() {
                Some(tc) => {
                    if tc.to_lowercase().next().unwrap_or(tc) == qc {
                        continue 'outer;
                    }
                }
                None => return false, // query char not found
            }
        }
    }
    true
}

/// Case-insensitive substring search that avoids allocating a new String.
///
/// The needle (`query`) must already be lowercased by the caller.  Each
/// character of `haystack` is lowered on the fly.  For the typical case of
/// ASCII-only log lines this is equivalent in speed to an allocation-free
/// memchr scan; for Unicode content it is slightly slower but still avoids
/// the heap allocation that `haystack.to_lowercase().contains(query)` requires.
fn contains_ci(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    if haystack.len() < needle.len() {
        return false;
    }
    // Fast path: if both are ASCII-only, use a simple sliding window.
    // This avoids the char-boundary complexity and is branch-predictor friendly.
    let hb = haystack.as_bytes();
    let nb = needle.as_bytes();
    if nb.iter().all(|b| b.is_ascii()) && hb.iter().all(|b| b.is_ascii()) {
        return hb.windows(nb.len()).any(|w| w.eq_ignore_ascii_case(nb));
    }
    // Slow path: Unicode — fall back to allocating a lowercased haystack.
    haystack.to_lowercase().contains(needle)
}

/// Check if a single entry matches all active filters.
///
/// `text_lower` is `filter.text_search.to_lowercase()` pre-computed by caller.
/// `excl_lower` is `filter.exclude_text.to_lowercase()` pre-computed by caller.
fn matches_all(entry: &LogEntry, filter: &FilterState, text_lower: &str, excl_lower: &str) -> bool {
    // Severity filter
    if !filter.severity_levels.is_empty() && !filter.severity_levels.contains(&entry.severity) {
        return false;
    }

    // Source file filter
    if filter.hide_all_sources {
        return false;
    }
    if !filter.source_files.is_empty() && !filter.source_files.contains(&entry.source_file) {
        return false;
    }

    // Time range filter — uses the **parsed log timestamp** when available so the
    // filter reflects when each log event *occurred*, not when the file was last
    // written.  Falls back to the source file's OS last-modified time only for
    // plain-text / fallback entries that have no parsed timestamp, so those
    // entries can still be included based on file recency rather than being
    // permanently hidden whenever a time bound is active.
    // Entries with neither a parsed timestamp nor a file mtime are excluded.
    //
    // hide_no_timestamp: always exclude entries that have no parsed timestamp,
    // regardless of whether any time-range bounds are active.  Checked here so
    // it applies uniformly to both the time-range path and the unconditional path.
    if filter.hide_no_timestamp && entry.timestamp.is_none() {
        return false;
    }
    if filter.time_start.is_some() || filter.time_end.is_some() {
        // Prefer the parsed log-event timestamp; use file mtime as a fallback.
        let effective_time = entry.timestamp.or(entry.file_modified);
        match effective_time {
            None => return false, // No time reference — exclude from bounded views
            Some(t) => {
                if let Some(ref start) = filter.time_start {
                    if t < *start {
                        return false;
                    }
                }
                if let Some(ref end) = filter.time_end {
                    if t > *end {
                        return false;
                    }
                }
            }
        }
    }

    // Text search: fuzzy subsequence or exact case-insensitive substring.
    // Searches message, thread, and component metadata fields (any match passes).
    //
    // Performance: avoid allocating lowercased Strings for every entry by using
    // a byte-level case-fold comparison for the ASCII-only common case.  The
    // `contains_ci` helper uses `str::to_ascii_lowercase` on a borrowed slice
    // rather than allocating a new String, but for entries with purely ASCII
    // content (the overwhelming majority of log lines) we can use an even
    // cheaper approach: only allocate when a non-ASCII byte is present.
    if !text_lower.is_empty() {
        let hit = if filter.fuzzy {
            fuzzy_match(text_lower, &entry.message)
                || entry
                    .thread
                    .as_deref()
                    .is_some_and(|t| fuzzy_match(text_lower, t))
                || entry
                    .component
                    .as_deref()
                    .is_some_and(|c| fuzzy_match(text_lower, c))
        } else {
            contains_ci(&entry.message, text_lower)
                || entry
                    .thread
                    .as_deref()
                    .is_some_and(|t| contains_ci(t, text_lower))
                || entry
                    .component
                    .as_deref()
                    .is_some_and(|c| contains_ci(c, text_lower))
        };
        if !hit {
            return false;
        }
    }

    // Regex search: also matches thread and component metadata fields.
    if let Some(ref regex) = filter.regex_search {
        let matches = regex.is_match(&entry.message)
            || entry.thread.as_deref().is_some_and(|t| regex.is_match(t))
            || entry
                .component
                .as_deref()
                .is_some_and(|c| regex.is_match(c));
        if !matches {
            return false;
        }
    }

    // Bookmark filter
    if filter.bookmarks_only && !filter.bookmarked_ids.contains(&entry.id) {
        return false;
    }

    // Exclusion text filter: hide entries that match the exclusion term in any
    // searchable field (message, thread, component).  Uses the same
    // case-insensitive substring engine as `text_search` but inverts the gate.
    // `excl_lower` is pre-lowercased by the caller to avoid per-entry allocation.
    if !excl_lower.is_empty() {
        let hit = contains_ci(&entry.message, excl_lower)
            || entry
                .thread
                .as_deref()
                .is_some_and(|t| contains_ci(t, excl_lower))
            || entry
                .component
                .as_deref()
                .is_some_and(|c| contains_ci(c, excl_lower));
        if hit {
            return false;
        }
    }

    // Component filter: membership-gate for the
    // component / module field.  Entries with no component value are excluded.
    if !filter.component_filter.is_empty() {
        match entry.component.as_deref() {
            Some(c) if filter.component_filter.contains(c) => {}
            _ => return false,
        }
    }

    // Multi-term search: delegates to the MultiSearch engine which uses
    // RegexSet for efficient single-pass multi-pattern matching.
    if filter.multi_search.is_active()
        && !filter.multi_search.matches_entry(
            &entry.message,
            entry.thread.as_deref(),
            entry.component.as_deref(),
        )
    {
        return false;
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
            file_modified: None,
        }
    }

    #[test]
    fn test_fuzzy_match_basic_subsequence() {
        assert!(fuzzy_match("cerr", "Connection error"));
        assert!(fuzzy_match("fail", "FAILED"));
        assert!(fuzzy_match("err", "error"));
        assert!(!fuzzy_match("xyz", "Connection error"));
    }

    #[test]
    fn test_fuzzy_match_empty_query_always_matches() {
        assert!(fuzzy_match("", "anything"));
        assert!(fuzzy_match("", ""));
    }

    #[test]
    fn test_fuzzy_match_exact_still_works() {
        assert!(fuzzy_match("error", "error"));
        assert!(!fuzzy_match("errors", "error")); // query longer than text
    }

    #[test]
    fn test_fuzzy_filter_applied_when_flag_set() {
        let entries = vec![
            make_entry(1, Severity::Error, "Connection error"),
            make_entry(2, Severity::Info, "All systems ok"),
        ];
        let filter = FilterState {
            text_search: "cerr".to_string(), // won't substring-match but WILL fuzzy-match
            fuzzy: true,
            ..Default::default()
        };
        let result = apply_filters(&entries, &filter);
        assert_eq!(result, vec![0]);
    }

    #[test]
    fn test_substring_mode_does_not_fuzzy_match() {
        let entries = vec![make_entry(1, Severity::Error, "Connection error")];
        let filter = FilterState {
            text_search: "cerr".to_string(),
            fuzzy: false,
            ..Default::default()
        };
        // "cerr" is not a substring of "Connection error"
        assert!(apply_filters(&entries, &filter).is_empty());
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
    fn test_regex_filter_is_case_insensitive() {
        let entries = vec![
            make_entry(1, Severity::Error, "Error CODE: 500"),
            make_entry(2, Severity::Info, "Status OK"),
        ];
        let mut filter = FilterState::default();
        filter.set_regex(r"code:\s*5\d{2}").unwrap();
        let result = apply_filters(&entries, &filter);
        // Regex should match case-insensitively (lowercase pattern vs uppercase text)
        assert_eq!(result, vec![0]);
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

    #[test]
    fn test_time_range_start_bound() {
        use chrono::TimeZone;
        let base = chrono::Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap();
        let mut old_entry = make_entry(1, Severity::Info, "old message");
        let mut new_entry = make_entry(2, Severity::Info, "new message");
        // Time filter uses the parsed log timestamp (entry.timestamp).
        old_entry.timestamp = Some(base);
        new_entry.timestamp = Some(base + chrono::Duration::hours(1));
        let entries = vec![old_entry, new_entry];

        let filter = FilterState {
            time_start: Some(base + chrono::Duration::minutes(30)),
            ..Default::default()
        };
        let result = apply_filters(&entries, &filter);
        // Only the newer log timestamp is after the start bound
        assert_eq!(result, vec![1]);
    }

    #[test]
    fn test_time_filter_uses_file_mtime_fallback_when_no_timestamp() {
        use chrono::TimeZone;
        let base = chrono::Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap();
        // Entry with a parsed timestamp: should be filtered by timestamp
        let mut ts_entry = make_entry(1, Severity::Info, "has parsed timestamp");
        ts_entry.timestamp = Some(base + chrono::Duration::hours(1));
        // Entry with no parsed timestamp but a file mtime: fallback to mtime
        let mut mtime_only = make_entry(2, Severity::Info, "no timestamp, has mtime");
        mtime_only.timestamp = None;
        mtime_only.file_modified = Some(base + chrono::Duration::hours(1));
        // Entry with neither: must be excluded
        let mut neither = make_entry(3, Severity::Info, "no timestamp no mtime");
        neither.timestamp = None;
        neither.file_modified = None;
        let entries = vec![ts_entry, mtime_only, neither];

        let filter = FilterState {
            time_start: Some(base + chrono::Duration::minutes(30)),
            ..Default::default()
        };
        let result = apply_filters(&entries, &filter);
        // First two entries pass (timestamp and fallback mtime are both after start);
        // third is excluded (no time reference at all)
        assert_eq!(result, vec![0, 1]);
    }

    #[test]
    fn test_source_file_filter() {
        let mut entry_a = make_entry(1, Severity::Info, "file a entry");
        let mut entry_b = make_entry(2, Severity::Info, "file b entry");
        entry_a.source_file = PathBuf::from("a.log");
        entry_b.source_file = PathBuf::from("b.log");
        let entries = vec![entry_a, entry_b];

        let mut source_files = HashSet::new();
        source_files.insert(PathBuf::from("a.log"));
        let filter = FilterState {
            source_files,
            ..Default::default()
        };
        let result = apply_filters(&entries, &filter);
        assert_eq!(result, vec![0]);
    }

    #[test]
    fn test_relative_time_field_tracked_in_is_empty() {
        let mut filter = FilterState::default();
        assert!(filter.is_empty());
        filter.relative_time_secs = Some(900);
        assert!(!filter.is_empty());
        filter.relative_time_secs = None;
        assert!(filter.is_empty());
    }

    #[test]
    fn test_bookmark_filter_returns_only_bookmarked_entries() {
        let entries = vec![
            make_entry(0, Severity::Info, "entry zero"),
            make_entry(1, Severity::Error, "entry one"),
            make_entry(2, Severity::Info, "entry two"),
        ];
        let mut bookmarked_ids = HashSet::new();
        bookmarked_ids.insert(0u64);
        bookmarked_ids.insert(2u64);
        let filter = FilterState {
            bookmarks_only: true,
            bookmarked_ids,
            ..Default::default()
        };
        let result = apply_filters(&entries, &filter);
        assert_eq!(result, vec![0, 2]);
    }

    #[test]
    fn test_bookmark_filter_tracked_in_is_empty() {
        let mut filter = FilterState::default();
        assert!(filter.is_empty());
        filter.bookmarks_only = true;
        assert!(!filter.is_empty());
        filter.bookmarks_only = false;
        assert!(filter.is_empty());
    }

    // -------------------------------------------------------------------------
    // Exclude-text tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_exclude_text_hides_matching_entries() {
        let entries = vec![
            make_entry(1, Severity::Error, "heartbeat check ok"),
            make_entry(2, Severity::Error, "Connection refused"),
            make_entry(3, Severity::Info, "HEARTBEAT response"),
        ];
        let filter = FilterState {
            exclude_text: "heartbeat".to_string(),
            ..Default::default()
        };
        let result = apply_filters(&entries, &filter);
        // Entries 0 and 2 contain "heartbeat" (case-insensitive) -- hidden.
        assert_eq!(result, vec![1]);
    }

    #[test]
    fn test_exclude_text_tracked_in_is_empty() {
        let mut filter = FilterState::default();
        assert!(filter.is_empty());
        filter.exclude_text = "noise".to_string();
        assert!(!filter.is_empty());
        filter.exclude_text.clear();
        assert!(filter.is_empty());
    }

    #[test]
    fn test_exclude_text_combined_with_include_text() {
        let entries = vec![
            make_entry(1, Severity::Error, "database error: timeout"),
            make_entry(2, Severity::Error, "database error: heartbeat"),
            make_entry(3, Severity::Error, "network timeout"),
        ];
        // Include entries containing "error", exclude those also containing "heartbeat".
        let filter = FilterState {
            text_search: "error".to_string(),
            exclude_text: "heartbeat".to_string(),
            ..Default::default()
        };
        let result = apply_filters(&entries, &filter);
        assert_eq!(result, vec![0]); // entry 1 excluded by exclude_text
    }

    // -------------------------------------------------------------------------
    // Component-filter tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_component_filter_keeps_only_selected_components() {
        let mut e1 = make_entry(1, Severity::Error, "auth failed");
        let mut e2 = make_entry(2, Severity::Error, "db query failed");
        let mut e3 = make_entry(3, Severity::Info, "health ok");
        e1.component = Some("auth".to_string());
        e2.component = Some("db".to_string());
        e3.component = None;
        let entries = vec![e1, e2, e3];

        let mut component_filter = HashSet::new();
        component_filter.insert("auth".to_string());
        let filter = FilterState {
            component_filter,
            ..Default::default()
        };
        let result = apply_filters(&entries, &filter);
        assert_eq!(result, vec![0]);
    }

    #[test]
    fn test_component_filter_tracked_in_is_empty() {
        let mut filter = FilterState::default();
        assert!(filter.is_empty());
        filter.component_filter.insert("scheduler".to_string());
        assert!(!filter.is_empty());
        filter.component_filter.clear();
        assert!(filter.is_empty());
    }

    #[test]
    fn test_component_filter_excludes_non_matching_component() {
        let mut e1 = make_entry(1, Severity::Error, "msg1");
        let mut e2 = make_entry(2, Severity::Error, "msg2");
        let mut e3 = make_entry(3, Severity::Error, "msg3");
        e1.component = Some("auth".to_string());
        e2.component = Some("db".to_string());
        e3.component = Some("auth".to_string());
        let entries = vec![e1, e2, e3];

        let mut component_filter = HashSet::new();
        component_filter.insert("auth".to_string());
        let filter = FilterState {
            component_filter,
            ..Default::default()
        };
        // Entries 0 and 2 are "auth"; entry 1 is "db" and is excluded.
        let result = apply_filters(&entries, &filter);
        assert_eq!(result, vec![0, 2]);
    }

    /// Regression: `errors_and_warnings_from` must include Critical, Error,
    /// and Warning but exclude Info and Debug.  This is the canonical quick-filter
    /// factory used by the Filters panel "Errors + Warnings" preset button.
    #[test]
    fn test_errors_and_warnings_from_includes_exactly_three_severities() {
        let entries = vec![
            make_entry(1, Severity::Critical, "critical msg"),
            make_entry(2, Severity::Error, "error msg"),
            make_entry(3, Severity::Warning, "warning msg"),
            make_entry(4, Severity::Info, "info msg"),
            make_entry(5, Severity::Debug, "debug msg"),
        ];
        let filter = FilterState::errors_and_warnings_from(false);
        let result = apply_filters(&entries, &filter);
        assert_eq!(
            result,
            vec![0, 1, 2],
            "Critical/Error/Warning must pass; Info/Debug must not"
        );
    }

    /// Regression: `errors_and_warnings_from(true)` must propagate `fuzzy = true`
    /// into the produced FilterState so fuzzy text search still works when the
    /// quick-filter preset and a text search are combined.
    #[test]
    fn test_errors_and_warnings_from_preserves_fuzzy_flag() {
        let filter_no_fuzzy = FilterState::errors_and_warnings_from(false);
        let filter_fuzzy = FilterState::errors_and_warnings_from(true);
        assert!(!filter_no_fuzzy.fuzzy);
        assert!(filter_fuzzy.fuzzy);
    }

    /// hide_no_timestamp=true must remove entries with timestamp:None while
    /// keeping entries that have a parsed timestamp.
    #[test]
    fn test_hide_no_timestamp_excludes_untimed_entries() {
        let mut e_with_ts = make_entry(1, Severity::Info, "has timestamp");
        e_with_ts.timestamp = Some(chrono::Utc::now());
        let e_no_ts = make_entry(2, Severity::Info, "no timestamp");

        let entries = vec![e_with_ts, e_no_ts];

        // Without the flag both entries pass.
        let result_off = apply_filters(&entries, &FilterState::default());
        assert_eq!(result_off, vec![0, 1]);

        // With the flag only the entry with a timestamp passes.
        let filter = FilterState {
            hide_no_timestamp: true,
            ..Default::default()
        };
        let result_on = apply_filters(&entries, &filter);
        assert_eq!(
            result_on,
            vec![0],
            "entry with no timestamp must be excluded"
        );
    }

    /// hide_no_timestamp is counted by is_empty() so the filter-active indicator works.
    #[test]
    fn test_hide_no_timestamp_tracked_in_is_empty() {
        let mut filter = FilterState::default();
        assert!(filter.is_empty());
        filter.hide_no_timestamp = true;
        assert!(!filter.is_empty());
        filter.hide_no_timestamp = false;
        assert!(filter.is_empty());
    }

    // =========================================================================
    // Deduplication tests
    // =========================================================================

    #[test]
    fn test_dedup_mode_tracked_in_is_empty() {
        let mut filter = FilterState::default();
        assert!(filter.is_empty());
        filter.dedup_mode = DedupMode::Exact;
        assert!(!filter.is_empty());
        filter.dedup_mode = DedupMode::Normalized;
        assert!(!filter.is_empty());
        filter.dedup_mode = DedupMode::Off;
        assert!(filter.is_empty());
    }

    // -- normalize_message tests --

    #[test]
    fn test_normalize_replaces_ipv4() {
        let msg = "Connection from 192.168.1.100:8080 refused";
        let norm = normalize_message(msg);
        assert_eq!(norm, "Connection from <IP> refused");
    }

    #[test]
    fn test_normalize_replaces_guid() {
        let msg = "Session 550e8400-e29b-41d4-a716-446655440000 expired";
        let norm = normalize_message(msg);
        assert_eq!(norm, "Session <GUID> expired");
    }

    #[test]
    fn test_normalize_replaces_hex_prefixed() {
        let msg = "Error at address 0xDEADBEEF01234567";
        let norm = normalize_message(msg);
        assert_eq!(norm, "Error at address <HEX>");
    }

    #[test]
    fn test_normalize_replaces_numbers() {
        let msg = "Processed 42 items in 3.14 seconds";
        let norm = normalize_message(msg);
        assert_eq!(norm, "Processed <NUM> items in <NUM> seconds");
    }

    #[test]
    fn test_normalize_combined() {
        let msg = "Host 10.0.0.1 job 550e8400-e29b-41d4-a716-446655440000 processed 99 records at 0xCAFEBABE01020304";
        let norm = normalize_message(msg);
        assert_eq!(
            norm,
            "Host <IP> job <GUID> processed <NUM> records at <HEX>"
        );
    }

    #[test]
    fn test_normalize_empty_string() {
        assert_eq!(normalize_message(""), "");
    }

    #[test]
    fn test_normalize_no_variable_data() {
        let msg = "Connection error: timeout expired";
        assert_eq!(normalize_message(msg), msg);
    }

    // -- apply_dedup tests --

    fn make_entry_with_file(
        id: u64,
        severity: Severity,
        message: &str,
        file: &str,
        ts: Option<chrono::DateTime<chrono::Utc>>,
    ) -> LogEntry {
        LogEntry {
            id,
            timestamp: ts,
            severity,
            source_file: PathBuf::from(file),
            line_number: id,
            thread: None,
            component: None,
            message: message.to_string(),
            raw_text: message.to_string(),
            profile_id: "test".to_string(),
            file_modified: None,
        }
    }

    #[test]
    fn test_dedup_off_returns_original() {
        let entries = vec![
            make_entry(1, Severity::Info, "msg A"),
            make_entry(2, Severity::Info, "msg A"),
        ];
        let indices = vec![0, 1];
        let (result, info) = apply_dedup(&entries, &indices, DedupMode::Off);
        assert_eq!(result, vec![0, 1]);
        assert!(info.is_empty());
    }

    #[test]
    fn test_dedup_exact_collapses_identical_messages() {
        use chrono::TimeZone;
        let base = chrono::Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap();
        let entries = vec![
            make_entry_with_file(
                1,
                Severity::Error,
                "Connection refused",
                "a.log",
                Some(base),
            ),
            make_entry_with_file(
                2,
                Severity::Error,
                "Connection refused",
                "a.log",
                Some(base + chrono::Duration::minutes(5)),
            ),
            make_entry_with_file(
                3,
                Severity::Error,
                "Connection refused",
                "a.log",
                Some(base + chrono::Duration::minutes(10)),
            ),
            make_entry_with_file(4, Severity::Info, "OK", "a.log", Some(base)),
        ];
        let indices = vec![0, 1, 2, 3];
        let (result, info) = apply_dedup(&entries, &indices, DedupMode::Exact);
        // Entry 2 (id=3, latest timestamp) survives for "Connection refused"; entry 3 (id=4, "OK") survives alone.
        assert_eq!(result.len(), 2);
        assert!(result.contains(&2)); // latest "Connection refused"
        assert!(result.contains(&3)); // "OK"
        let dedup = info.get(&2).expect("entry 2 should have dedup info");
        assert_eq!(dedup.count, 3);
        assert_eq!(dedup.first_timestamp, Some(base));
        assert_eq!(dedup.all_indices.len(), 3);
    }

    #[test]
    fn test_dedup_per_source_file() {
        use chrono::TimeZone;
        let base = chrono::Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap();
        let entries = vec![
            make_entry_with_file(1, Severity::Error, "timeout", "a.log", Some(base)),
            make_entry_with_file(2, Severity::Error, "timeout", "b.log", Some(base)),
        ];
        let indices = vec![0, 1];
        let (result, info) = apply_dedup(&entries, &indices, DedupMode::Exact);
        // Same message in different files: both should survive (per-file scoping).
        assert_eq!(result.len(), 2);
        // Each has count=1 so no meaningful dedup info (or both have info with count=1)
        for &idx in &result {
            if let Some(d) = info.get(&idx) {
                assert_eq!(d.count, 1);
            }
        }
    }

    #[test]
    fn test_dedup_normalized_groups_variable_data() {
        use chrono::TimeZone;
        let base = chrono::Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap();
        let entries = vec![
            make_entry_with_file(
                1,
                Severity::Error,
                "Failed to connect to 10.0.0.1:8080",
                "a.log",
                Some(base),
            ),
            make_entry_with_file(
                2,
                Severity::Error,
                "Failed to connect to 10.0.0.2:9090",
                "a.log",
                Some(base + chrono::Duration::minutes(1)),
            ),
            make_entry_with_file(
                3,
                Severity::Error,
                "Failed to connect to 192.168.1.1:443",
                "a.log",
                Some(base + chrono::Duration::minutes(2)),
            ),
        ];
        let indices = vec![0, 1, 2];
        let (result, info) = apply_dedup(&entries, &indices, DedupMode::Normalized);
        // All three normalise to "Failed to connect to <IP>" -- collapsed to 1.
        assert_eq!(result.len(), 1);
        let dedup = info
            .get(&result[0])
            .expect("survivor should have dedup info");
        assert_eq!(dedup.count, 3);
    }

    #[test]
    fn test_dedup_normalized_different_guids_collapse() {
        use chrono::TimeZone;
        let base = chrono::Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap();
        let entries = vec![
            make_entry_with_file(
                1,
                Severity::Info,
                "Job 550e8400-e29b-41d4-a716-446655440000 started",
                "a.log",
                Some(base),
            ),
            make_entry_with_file(
                2,
                Severity::Info,
                "Job a1b2c3d4-e5f6-7890-abcd-ef1234567890 started",
                "a.log",
                Some(base + chrono::Duration::minutes(1)),
            ),
        ];
        let indices = vec![0, 1];
        let (result, info) = apply_dedup(&entries, &indices, DedupMode::Normalized);
        assert_eq!(result.len(), 1);
        let dedup = info
            .get(&result[0])
            .expect("survivor should have dedup info");
        assert_eq!(dedup.count, 2);
    }

    #[test]
    fn test_dedup_exact_does_not_collapse_different_variable_data() {
        use chrono::TimeZone;
        let base = chrono::Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap();
        let entries = vec![
            make_entry_with_file(
                1,
                Severity::Error,
                "Error on host 10.0.0.1",
                "a.log",
                Some(base),
            ),
            make_entry_with_file(
                2,
                Severity::Error,
                "Error on host 10.0.0.2",
                "a.log",
                Some(base + chrono::Duration::minutes(1)),
            ),
        ];
        let indices = vec![0, 1];
        let (result, _info) = apply_dedup(&entries, &indices, DedupMode::Exact);
        // Different messages in exact mode: both survive.
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_dedup_latest_wins() {
        use chrono::TimeZone;
        let base = chrono::Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap();
        let entries = vec![
            make_entry_with_file(
                1,
                Severity::Error,
                "dup msg",
                "a.log",
                Some(base + chrono::Duration::minutes(10)),
            ),
            make_entry_with_file(
                2,
                Severity::Error,
                "dup msg",
                "a.log",
                Some(base + chrono::Duration::minutes(5)),
            ),
            make_entry_with_file(3, Severity::Error, "dup msg", "a.log", Some(base)),
        ];
        let indices = vec![0, 1, 2];
        let (result, info) = apply_dedup(&entries, &indices, DedupMode::Exact);
        assert_eq!(result.len(), 1);
        // Entry 0 (id=1) has the latest timestamp (base+10m) and should survive.
        assert_eq!(result[0], 0);
        let dedup = info.get(&0).unwrap();
        assert_eq!(dedup.count, 3);
        assert_eq!(dedup.first_timestamp, Some(base));
    }

    #[test]
    fn test_dedup_no_timestamp_uses_id_tiebreaker() {
        let entries = vec![
            make_entry_with_file(1, Severity::Error, "same msg", "a.log", None),
            make_entry_with_file(2, Severity::Error, "same msg", "a.log", None),
            make_entry_with_file(3, Severity::Error, "same msg", "a.log", None),
        ];
        let indices = vec![0, 1, 2];
        let (result, info) = apply_dedup(&entries, &indices, DedupMode::Exact);
        assert_eq!(result.len(), 1);
        // Highest ID (3) wins when all timestamps are None.
        assert_eq!(result[0], 2);
        let dedup = info.get(&2).unwrap();
        assert_eq!(dedup.count, 3);
    }

    #[test]
    fn test_dedup_preserves_filter_order() {
        use chrono::TimeZone;
        let base = chrono::Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap();
        let entries = vec![
            make_entry_with_file(1, Severity::Error, "alpha", "a.log", Some(base)),
            make_entry_with_file(
                2,
                Severity::Error,
                "alpha",
                "a.log",
                Some(base + chrono::Duration::minutes(1)),
            ),
            make_entry_with_file(
                3,
                Severity::Info,
                "beta",
                "a.log",
                Some(base + chrono::Duration::minutes(2)),
            ),
            make_entry_with_file(
                4,
                Severity::Warning,
                "gamma",
                "a.log",
                Some(base + chrono::Duration::minutes(3)),
            ),
        ];
        let indices = vec![0, 1, 2, 3];
        let (result, _) = apply_dedup(&entries, &indices, DedupMode::Exact);
        // "alpha" at idx 1 (latest), "beta" at idx 2, "gamma" at idx 3
        assert_eq!(result, vec![1, 2, 3]);
    }
}
