// LogSleuth - core/multi_search.rs
//
// Multi-term search engine.  Allows the user to search for many terms
// simultaneously across loaded log entries using ANY (union) or ALL
// (intersection) match logic, with optional NOT (exclusion) terms.
//
// Performance: uses `regex::RegexSet` for single-pass multi-pattern
// detection and compiled `Regex` objects for per-term highlighting.
//
// Core layer: pure logic, no I/O or UI dependencies.

use regex::{Regex, RegexBuilder, RegexSet, RegexSetBuilder};
use serde::{Deserialize, Serialize};

// =============================================================================
// Constants
// =============================================================================

/// Maximum number of search terms (include + exclude) to prevent excessive
/// regex compilation time and memory use.
pub const MAX_MULTI_SEARCH_TERMS: usize = 200;

// =============================================================================
// Types
// =============================================================================

/// Match mode for multi-term search.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum MultiSearchMode {
    /// Return entries matching ANY of the supplied include terms (OR logic).
    #[default]
    Any,
    /// Return entries only when ALL include terms appear within the same entry.
    All,
}

impl MultiSearchMode {
    /// Human-readable label for UI display.
    pub fn label(self) -> &'static str {
        match self {
            MultiSearchMode::Any => "ANY (OR)",
            MultiSearchMode::All => "ALL (AND)",
        }
    }

    /// All variants in display order.
    pub fn all() -> &'static [MultiSearchMode] {
        &[MultiSearchMode::Any, MultiSearchMode::All]
    }
}

/// Result of compiling multi-search terms into regex engines.
///
/// Holds both a `RegexSet` (fast single-pass multi-pattern detection) and
/// individual compiled `Regex` objects (needed for per-term highlighting
/// because `RegexSet` does not expose match positions).
#[derive(Debug, Clone)]
pub struct CompiledMultiSearch {
    /// Fast multi-pattern matcher for include terms.
    pub include_set: RegexSet,
    /// Individual compiled patterns for include terms (same order as the set).
    pub include_patterns: Vec<Regex>,
    /// Fast multi-pattern matcher for exclude (NOT) terms.
    pub exclude_set: Option<RegexSet>,
    /// Individual compiled patterns for exclude terms.
    pub exclude_patterns: Vec<Regex>,
}

/// Errors produced when compiling multi-search terms.
#[derive(Debug, Clone)]
pub struct MultiSearchError {
    /// Human-readable description of the compilation failure.
    pub message: String,
    /// Zero-based index of the offending term, if applicable.
    pub term_index: Option<usize>,
}

impl std::fmt::Display for MultiSearchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(idx) = self.term_index {
            write!(f, "Term {}: {}", idx + 1, self.message)
        } else {
            write!(f, "{}", self.message)
        }
    }
}

/// Complete multi-term search configuration.
///
/// Stored in `FilterState` and evaluated for every entry in the filter
/// pipeline.  The struct caches compiled regex engines so re-compilation
/// only occurs when the user changes the search terms or options.
#[derive(Debug, Clone)]
pub struct MultiSearch {
    /// Match mode: ANY (OR) or ALL (AND).
    pub mode: MultiSearchMode,
    /// Raw include terms as entered by the user.
    pub include_terms: Vec<String>,
    /// Raw exclude (NOT) terms as entered by the user.
    pub exclude_terms: Vec<String>,
    /// Minimum number of include terms that must match (for threshold mode).
    /// `None` means use the mode default (1 for ANY, all for ALL).
    pub min_match: Option<usize>,
    /// Whether matching is case-insensitive.
    pub case_insensitive: bool,
    /// Whether to wrap terms with word boundaries (`\b`).
    pub whole_word: bool,
    /// When true, terms are treated as regex patterns.
    /// When false, terms are escaped for literal matching.
    pub regex_mode: bool,
    /// Pre-compiled search engines.  `None` when there are no valid terms.
    pub compiled: Option<CompiledMultiSearch>,
    /// Compilation error from the last `compile()` call, if any.
    pub compile_error: Option<MultiSearchError>,
}

impl Default for MultiSearch {
    fn default() -> Self {
        Self {
            mode: MultiSearchMode::default(),
            include_terms: Vec::new(),
            exclude_terms: Vec::new(),
            min_match: None,
            case_insensitive: true,
            whole_word: false,
            regex_mode: false,
            compiled: None,
            compile_error: None,
        }
    }
}

impl MultiSearch {
    /// Returns true when no terms are configured (the search is inactive).
    pub fn is_empty(&self) -> bool {
        self.include_terms.is_empty() && self.exclude_terms.is_empty()
    }

    /// Returns true when valid compiled patterns exist and at least one
    /// include or exclude term is present.
    pub fn is_active(&self) -> bool {
        self.compiled.is_some()
    }

    /// Parse a raw input string into include and exclude term lists.
    ///
    /// Supports:
    /// - One term per line
    /// - Comma-separated terms on a single line
    /// - Terms prefixed with `-` or `!` are treated as exclude (NOT) terms
    /// - Empty lines and whitespace-only lines are skipped
    /// - Duplicate terms are removed (preserving first occurrence)
    pub fn parse_terms(input: &str) -> (Vec<String>, Vec<String>) {
        let mut include = Vec::new();
        let mut exclude = Vec::new();
        let mut seen_include = std::collections::HashSet::new();
        let mut seen_exclude = std::collections::HashSet::new();

        for line in input.lines() {
            // Split by comma if the line contains commas (unless the whole
            // line is a single regex pattern with a comma in it and regex
            // mode is on -- but we cannot know that here, so we always split).
            let tokens: Vec<&str> = if line.contains(',') {
                line.split(',').collect()
            } else {
                vec![line]
            };

            for token in tokens {
                let trimmed = token.trim();
                if trimmed.is_empty() {
                    continue;
                }

                // Detect NOT prefix
                let (is_exclude, term) = if let Some(rest) = trimmed.strip_prefix('-') {
                    let rest = rest.trim();
                    if rest.is_empty() {
                        continue;
                    }
                    (true, rest.to_string())
                } else if let Some(rest) = trimmed.strip_prefix('!') {
                    let rest = rest.trim();
                    if rest.is_empty() {
                        continue;
                    }
                    (true, rest.to_string())
                } else {
                    (false, trimmed.to_string())
                };

                if is_exclude {
                    if seen_exclude.insert(term.clone()) {
                        exclude.push(term);
                    }
                } else if seen_include.insert(term.clone()) {
                    include.push(term);
                }
            }
        }

        (include, exclude)
    }

    /// Build a regex pattern string for a single term, respecting the
    /// current options (literal vs regex, whole word, case sensitivity).
    fn build_pattern(&self, term: &str) -> String {
        let base = if self.regex_mode {
            term.to_string()
        } else {
            regex::escape(term)
        };
        if self.whole_word {
            format!(r"\b{base}\b")
        } else {
            base
        }
    }

    /// Compile all terms into regex engines.
    ///
    /// Must be called after changing terms or options.  On success, sets
    /// `self.compiled` and clears `self.compile_error`.  On failure, clears
    /// `self.compiled` and sets `self.compile_error`.
    pub fn compile(&mut self) {
        self.compiled = None;
        self.compile_error = None;

        // Enforce term-count limit.
        let total = self.include_terms.len() + self.exclude_terms.len();
        if total > MAX_MULTI_SEARCH_TERMS {
            self.compile_error = Some(MultiSearchError {
                message: format!("Too many terms ({total}). Maximum is {MAX_MULTI_SEARCH_TERMS}."),
                term_index: None,
            });
            return;
        }

        if self.include_terms.is_empty() && self.exclude_terms.is_empty() {
            return;
        }

        // Build include patterns
        let mut include_pattern_strs = Vec::with_capacity(self.include_terms.len());
        let mut include_compiled = Vec::with_capacity(self.include_terms.len());

        for (i, term) in self.include_terms.iter().enumerate() {
            let pat = self.build_pattern(term);
            match RegexBuilder::new(&pat)
                .case_insensitive(self.case_insensitive)
                .build()
            {
                Ok(re) => {
                    include_pattern_strs.push(pat);
                    include_compiled.push(re);
                }
                Err(e) => {
                    self.compile_error = Some(MultiSearchError {
                        message: format!("Invalid pattern '{}': {}", term, e),
                        term_index: Some(i),
                    });
                    return;
                }
            }
        }

        // Build include RegexSet
        let include_set = match RegexSetBuilder::new(&include_pattern_strs)
            .case_insensitive(self.case_insensitive)
            .build()
        {
            Ok(s) => s,
            Err(e) => {
                self.compile_error = Some(MultiSearchError {
                    message: format!("Failed to compile include pattern set: {}", e),
                    term_index: None,
                });
                return;
            }
        };

        // Build exclude patterns
        let mut exclude_pattern_strs = Vec::with_capacity(self.exclude_terms.len());
        let mut exclude_compiled = Vec::with_capacity(self.exclude_terms.len());

        for (i, term) in self.exclude_terms.iter().enumerate() {
            let pat = self.build_pattern(term);
            match RegexBuilder::new(&pat)
                .case_insensitive(self.case_insensitive)
                .build()
            {
                Ok(re) => {
                    exclude_pattern_strs.push(pat);
                    exclude_compiled.push(re);
                }
                Err(e) => {
                    self.compile_error = Some(MultiSearchError {
                        message: format!("Invalid exclude pattern '{}': {}", term, e),
                        term_index: Some(self.include_terms.len() + i),
                    });
                    return;
                }
            }
        }

        let exclude_set = if exclude_pattern_strs.is_empty() {
            None
        } else {
            match RegexSetBuilder::new(&exclude_pattern_strs)
                .case_insensitive(self.case_insensitive)
                .build()
            {
                Ok(s) => Some(s),
                Err(e) => {
                    self.compile_error = Some(MultiSearchError {
                        message: format!("Failed to compile exclude pattern set: {}", e),
                        term_index: None,
                    });
                    return;
                }
            }
        };

        self.compiled = Some(CompiledMultiSearch {
            include_set,
            include_patterns: include_compiled,
            exclude_set,
            exclude_patterns: exclude_compiled,
        });
    }

    /// Test whether a single text string matches according to the current
    /// multi-search configuration.
    ///
    /// Returns `true` if the text should be included in results.
    /// Returns `true` (pass-through) when no compiled patterns exist.
    ///
    /// The caller should pass the concatenation of all searchable fields
    /// for a log entry (message + thread + component), or call this once
    /// per field and combine results.
    pub fn matches_text(&self, text: &str) -> bool {
        let Some(ref compiled) = self.compiled else {
            return true;
        };

        // Exclude check: if ANY exclude pattern matches, the entry is hidden.
        if let Some(ref excl_set) = compiled.exclude_set {
            if excl_set.is_match(text) {
                return false;
            }
        }

        // If no include terms exist but exclude terms do, everything not
        // excluded passes.
        if compiled.include_set.is_empty() {
            return true;
        }

        // Count how many include patterns match (single pass via RegexSet).
        let matches: Vec<usize> = compiled.include_set.matches(text).into_iter().collect();
        let match_count = matches.len();

        // Determine the required match threshold.
        let required = match self.min_match {
            Some(n) => n.min(compiled.include_set.len()).max(1),
            None => match self.mode {
                MultiSearchMode::Any => 1,
                MultiSearchMode::All => compiled.include_set.len(),
            },
        };

        match_count >= required
    }

    /// Test whether a log entry matches the multi-search by checking all
    /// searchable fields (message, thread, component).
    ///
    /// For ANY mode, a match in any field counts.  For ALL mode, all terms
    /// must appear across the combined text of all fields.
    pub fn matches_entry(
        &self,
        message: &str,
        thread: Option<&str>,
        component: Option<&str>,
    ) -> bool {
        let Some(ref compiled) = self.compiled else {
            return true;
        };

        // Build a combined searchable text from all fields.
        // Using a separator that is unlikely to appear in log text to prevent
        // false cross-field matches.
        let mut combined = String::with_capacity(
            message.len()
                + thread.map_or(0, |t| t.len() + 3)
                + component.map_or(0, |c| c.len() + 3),
        );
        combined.push_str(message);
        if let Some(t) = thread {
            combined.push_str(" | ");
            combined.push_str(t);
        }
        if let Some(c) = component {
            combined.push_str(" | ");
            combined.push_str(c);
        }

        // Exclude check first (fast rejection).
        if let Some(ref excl_set) = compiled.exclude_set {
            if excl_set.is_match(&combined) {
                return false;
            }
        }

        if compiled.include_set.is_empty() {
            return true;
        }

        let matches: Vec<usize> = compiled
            .include_set
            .matches(&combined)
            .into_iter()
            .collect();
        let match_count = matches.len();

        let required = match self.min_match {
            Some(n) => n.min(compiled.include_set.len()).max(1),
            None => match self.mode {
                MultiSearchMode::Any => 1,
                MultiSearchMode::All => compiled.include_set.len(),
            },
        };

        match_count >= required
    }

    /// Collect all match ranges in `text` for highlighting purposes.
    ///
    /// Returns a sorted, non-overlapping list of `(start, end)` byte offsets
    /// where include patterns matched.  Used by the detail panel to render
    /// highlighted spans.
    pub fn highlight_matches(&self, text: &str) -> Vec<(usize, usize)> {
        let Some(ref compiled) = self.compiled else {
            return Vec::new();
        };

        let mut ranges: Vec<(usize, usize)> = Vec::new();
        for re in &compiled.include_patterns {
            for m in re.find_iter(text) {
                ranges.push((m.start(), m.end()));
            }
        }

        if ranges.is_empty() {
            return ranges;
        }

        // Sort by start position, then merge overlapping ranges.
        ranges.sort_unstable();
        let mut merged: Vec<(usize, usize)> = Vec::with_capacity(ranges.len());
        let mut current = ranges[0];
        for &(start, end) in &ranges[1..] {
            if start <= current.1 {
                current.1 = current.1.max(end);
            } else {
                merged.push(current);
                current = (start, end);
            }
        }
        merged.push(current);
        merged
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create a MultiSearch from raw input, compile, and return it.
    fn make_search(
        input: &str,
        mode: MultiSearchMode,
        case_insensitive: bool,
        whole_word: bool,
        regex_mode: bool,
        min_match: Option<usize>,
    ) -> MultiSearch {
        let (include, exclude) = MultiSearch::parse_terms(input);
        let mut ms = MultiSearch {
            mode,
            include_terms: include,
            exclude_terms: exclude,
            min_match,
            case_insensitive,
            whole_word,
            regex_mode,
            ..Default::default()
        };
        ms.compile();
        ms
    }

    // -------------------------------------------------------------------------
    // parse_terms tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_parse_terms_newline_separated() {
        let (inc, exc) = MultiSearch::parse_terms("error\nwarning\ntimeout");
        assert_eq!(inc, vec!["error", "warning", "timeout"]);
        assert!(exc.is_empty());
    }

    #[test]
    fn test_parse_terms_comma_separated() {
        let (inc, exc) = MultiSearch::parse_terms("error, warning, timeout");
        assert_eq!(inc, vec!["error", "warning", "timeout"]);
        assert!(exc.is_empty());
    }

    #[test]
    fn test_parse_terms_mixed_newline_and_comma() {
        let (inc, exc) = MultiSearch::parse_terms("error, warning\ntimeout");
        assert_eq!(inc, vec!["error", "warning", "timeout"]);
        assert!(exc.is_empty());
    }

    #[test]
    fn test_parse_terms_exclude_with_dash_prefix() {
        let (inc, exc) = MultiSearch::parse_terms("error\n-heartbeat\n-noise");
        assert_eq!(inc, vec!["error"]);
        assert_eq!(exc, vec!["heartbeat", "noise"]);
    }

    #[test]
    fn test_parse_terms_exclude_with_bang_prefix() {
        let (inc, exc) = MultiSearch::parse_terms("error\n!heartbeat");
        assert_eq!(inc, vec!["error"]);
        assert_eq!(exc, vec!["heartbeat"]);
    }

    #[test]
    fn test_parse_terms_skips_empty_and_whitespace() {
        let (inc, exc) = MultiSearch::parse_terms("  \nerror\n\n  \nwarning\n");
        assert_eq!(inc, vec!["error", "warning"]);
        assert!(exc.is_empty());
    }

    #[test]
    fn test_parse_terms_deduplicates() {
        let (inc, exc) = MultiSearch::parse_terms("error\nerror\nwarning\nerror");
        assert_eq!(inc, vec!["error", "warning"]);
        assert!(exc.is_empty());
    }

    #[test]
    fn test_parse_terms_dash_only_is_skipped() {
        let (inc, exc) = MultiSearch::parse_terms("-\nerror\n!");
        assert_eq!(inc, vec!["error"]);
        assert!(exc.is_empty());
    }

    // -------------------------------------------------------------------------
    // ANY mode tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_any_mode_matches_single_term() {
        let ms = make_search("error", MultiSearchMode::Any, true, false, false, None);
        assert!(ms.matches_text("Connection error occurred"));
        assert!(!ms.matches_text("All systems ok"));
    }

    #[test]
    fn test_any_mode_matches_any_of_multiple_terms() {
        let ms = make_search(
            "error\nwarning\ntimeout",
            MultiSearchMode::Any,
            true,
            false,
            false,
            None,
        );
        assert!(ms.matches_text("Connection error"));
        assert!(ms.matches_text("Warning: low memory"));
        assert!(ms.matches_text("Request timeout"));
        assert!(!ms.matches_text("All systems ok"));
    }

    // -------------------------------------------------------------------------
    // ALL mode tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_all_mode_requires_all_terms() {
        let ms = make_search(
            "error\ndatabase",
            MultiSearchMode::All,
            true,
            false,
            false,
            None,
        );
        assert!(ms.matches_text("database connection error"));
        assert!(!ms.matches_text("database connection ok"));
        assert!(!ms.matches_text("network error"));
    }

    // -------------------------------------------------------------------------
    // Minimum match threshold tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_min_match_threshold() {
        // 3 terms, require at least 2 to match.
        let ms = make_search(
            "error\ntimeout\ndatabase",
            MultiSearchMode::Any,
            true,
            false,
            false,
            Some(2),
        );
        assert!(ms.matches_text("database error occurred")); // 2 of 3
        assert!(ms.matches_text("database timeout error")); // 3 of 3
        assert!(!ms.matches_text("database query ok")); // 1 of 3
    }

    #[test]
    fn test_min_match_clamped_to_term_count() {
        // min_match=10 but only 2 terms -> effectively ALL mode
        let ms = make_search(
            "error\ntimeout",
            MultiSearchMode::Any,
            true,
            false,
            false,
            Some(10),
        );
        assert!(ms.matches_text("error timeout"));
        assert!(!ms.matches_text("error only"));
    }

    #[test]
    fn test_min_match_at_least_one() {
        // min_match=0 is clamped to 1
        let ms = make_search(
            "error\ntimeout",
            MultiSearchMode::Any,
            true,
            false,
            false,
            Some(0),
        );
        assert!(ms.matches_text("error only"));
        assert!(!ms.matches_text("all clear"));
    }

    // -------------------------------------------------------------------------
    // NOT term exclusion tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_not_terms_exclude_matches() {
        let ms = make_search(
            "error\n-heartbeat",
            MultiSearchMode::Any,
            true,
            false,
            false,
            None,
        );
        assert!(ms.matches_text("Connection error"));
        assert!(!ms.matches_text("heartbeat error")); // excluded
        assert!(!ms.matches_text("heartbeat check ok")); // excluded, no include match
    }

    #[test]
    fn test_exclude_only_no_include() {
        // Only exclude terms: everything passes except excluded matches.
        let ms = make_search(
            "-heartbeat\n-noise",
            MultiSearchMode::Any,
            true,
            false,
            false,
            None,
        );
        assert!(ms.matches_text("Connection error"));
        assert!(!ms.matches_text("heartbeat check"));
        assert!(!ms.matches_text("background noise"));
    }

    // -------------------------------------------------------------------------
    // Case sensitivity tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_case_insensitive_matching() {
        let ms = make_search("ERROR", MultiSearchMode::Any, true, false, false, None);
        assert!(ms.matches_text("connection error"));
        assert!(ms.matches_text("CONNECTION ERROR"));
        assert!(ms.matches_text("Error occurred"));
    }

    #[test]
    fn test_case_sensitive_matching() {
        let ms = make_search("ERROR", MultiSearchMode::Any, false, false, false, None);
        assert!(ms.matches_text("CONNECTION ERROR"));
        assert!(!ms.matches_text("connection error"));
        assert!(!ms.matches_text("Error occurred"));
    }

    // -------------------------------------------------------------------------
    // Literal vs regex mode tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_literal_mode_escapes_regex_chars() {
        let ms = make_search(
            "error[0]",
            MultiSearchMode::Any,
            true,
            false,
            false, // literal
            None,
        );
        assert!(ms.matches_text("found error[0] in log"));
        assert!(!ms.matches_text("found error0 in log")); // without brackets
    }

    #[test]
    fn test_regex_mode_allows_patterns() {
        let ms = make_search(
            r"error\[\d+\]",
            MultiSearchMode::Any,
            true,
            false,
            true, // regex
            None,
        );
        assert!(ms.matches_text("found error[42] in log"));
        assert!(ms.matches_text("error[0] occurred"));
        assert!(!ms.matches_text("error occurred")); // no brackets
    }

    // -------------------------------------------------------------------------
    // Whole word tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_whole_word_matching() {
        let ms = make_search("error", MultiSearchMode::Any, true, true, false, None);
        assert!(ms.matches_text("an error occurred"));
        assert!(!ms.matches_text("errorhandler started")); // not a whole word
        assert!(!ms.matches_text("myerror")); // not a whole word
    }

    // -------------------------------------------------------------------------
    // Invalid regex handling tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_invalid_regex_produces_error() {
        let ms = make_search(
            r"error\n[invalid",
            MultiSearchMode::Any,
            true,
            false,
            true, // regex mode: the pattern is treated as a real regex
            None,
        );
        assert!(ms.compile_error.is_some());
        assert!(ms.compiled.is_none());
    }

    #[test]
    fn test_invalid_regex_in_literal_mode_is_escaped() {
        // In literal mode, regex special chars are escaped, so this should work.
        let ms = make_search("[invalid", MultiSearchMode::Any, true, false, false, None);
        assert!(ms.compile_error.is_none());
        assert!(ms.compiled.is_some());
        assert!(ms.matches_text("found [invalid pattern"));
    }

    // -------------------------------------------------------------------------
    // Duplicate and empty term handling tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_empty_input_produces_inactive_search() {
        let ms = make_search("", MultiSearchMode::Any, true, false, false, None);
        assert!(ms.is_empty());
        assert!(!ms.is_active());
        // Pass-through: no filter applied
        assert!(ms.matches_text("anything"));
    }

    #[test]
    fn test_whitespace_only_lines_produce_inactive_search() {
        let ms = make_search("  \n  \n  ", MultiSearchMode::Any, true, false, false, None);
        assert!(ms.is_empty());
    }

    // -------------------------------------------------------------------------
    // Multi-field entry matching tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_matches_entry_searches_all_fields() {
        let ms = make_search("worker-5", MultiSearchMode::Any, true, false, false, None);
        // Term appears in thread field, not message
        assert!(ms.matches_entry("connection ok", Some("worker-5"), None));
        // Term appears in component field
        assert!(ms.matches_entry("connection ok", None, Some("worker-5")));
        // Term not present anywhere
        assert!(!ms.matches_entry("connection ok", Some("main"), Some("auth")));
    }

    // -------------------------------------------------------------------------
    // Highlighting tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_highlight_matches_returns_byte_ranges() {
        let ms = make_search("error", MultiSearchMode::Any, true, false, false, None);
        let ranges = ms.highlight_matches("an error occurred, another error");
        assert_eq!(ranges.len(), 2);
        assert_eq!(
            &"an error occurred, another error"[ranges[0].0..ranges[0].1],
            "error"
        );
        assert_eq!(
            &"an error occurred, another error"[ranges[1].0..ranges[1].1],
            "error"
        );
    }

    #[test]
    fn test_highlight_merges_overlapping_ranges() {
        // Two patterns that overlap in the same text region.
        let ms = make_search(
            "database error\nerror",
            MultiSearchMode::Any,
            true,
            false,
            false,
            None,
        );
        let text = "database error occurred";
        let ranges = ms.highlight_matches(text);
        // "database error" covers bytes 0..14, "error" covers 9..14
        // These overlap, so they should merge into one range.
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].0, 0);
        assert_eq!(ranges[0].1, 14);
    }

    #[test]
    fn test_highlight_no_matches_returns_empty() {
        let ms = make_search("error", MultiSearchMode::Any, true, false, false, None);
        let ranges = ms.highlight_matches("all systems ok");
        assert!(ranges.is_empty());
    }

    // -------------------------------------------------------------------------
    // Term count limit test
    // -------------------------------------------------------------------------

    #[test]
    fn test_too_many_terms_produces_error() {
        let terms: Vec<String> = (0..MAX_MULTI_SEARCH_TERMS + 1)
            .map(|i| format!("term{i}"))
            .collect();
        let input = terms.join("\n");
        let (include, exclude) = MultiSearch::parse_terms(&input);
        let mut ms = MultiSearch {
            include_terms: include,
            exclude_terms: exclude,
            ..Default::default()
        };
        ms.compile();
        assert!(ms.compile_error.is_some());
        assert!(ms.compiled.is_none());
    }
}
