// LogSleuth - core/model.rs
//
// Core data model types. Pure data definitions with no I/O, no UI,
// no platform dependencies (Atlas Layer Rule: Core depends on std only).
//
// These types are the shared vocabulary across all layers.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

// =============================================================================
// Log Entry (normalised output of parsing)
// =============================================================================

/// A single parsed log event, normalised across all formats.
///
/// This is the core data unit that flows through filtering, display,
/// and export. Every format profile produces these regardless of the
/// source log's native structure.
#[derive(Debug, Clone, Serialize)]
pub struct LogEntry {
    /// Monotonically increasing unique ID within the scan session.
    pub id: u64,

    /// Parsed timestamp in UTC. `None` if the source line had no
    /// parseable timestamp (sorted to end of timeline with indicator).
    pub timestamp: Option<DateTime<Utc>>,

    /// Normalised severity level.
    pub severity: Severity,

    /// Path to the source log file.
    pub source_file: PathBuf,

    /// Line number in the source file where this entry begins.
    pub line_number: u64,

    /// Thread or process ID extracted from the log line (format-dependent).
    pub thread: Option<String>,

    /// Source component or module name (format-dependent).
    pub component: Option<String>,

    /// Full message text, including any continuation/multi-line content.
    pub message: String,

    /// Original unparsed text from the source file.
    pub raw_text: String,

    /// ID of the format profile used to parse this entry.
    pub profile_id: String,
}

// =============================================================================
// Severity
// =============================================================================

/// Normalised severity levels, ordered from most to least severe.
///
/// All format-specific level strings (Error, ERR, E, error, Failed, etc.)
/// are mapped to one of these variants via the profile's severity_mapping.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Default,
)]
pub enum Severity {
    Critical,
    Error,
    Warning,
    Info,
    Debug,
    #[default]
    Unknown,
}

impl Severity {
    /// Returns all variants in display order (most severe first).
    pub fn all() -> &'static [Severity] {
        &[
            Severity::Critical,
            Severity::Error,
            Severity::Warning,
            Severity::Info,
            Severity::Debug,
            Severity::Unknown,
        ]
    }

    /// Human-readable label for display.
    pub fn label(&self) -> &'static str {
        match self {
            Severity::Critical => "Critical",
            Severity::Error => "Error",
            Severity::Warning => "Warning",
            Severity::Info => "Info",
            Severity::Debug => "Debug",
            Severity::Unknown => "Unknown",
        }
    }

    /// Short label for compact display (e.g. table columns).
    pub fn short_label(&self) -> &'static str {
        match self {
            Severity::Critical => "CRIT",
            Severity::Error => "ERR",
            Severity::Warning => "WARN",
            Severity::Info => "INFO",
            Severity::Debug => "DBG",
            Severity::Unknown => "???",
        }
    }
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

// =============================================================================
// Multiline mode
// =============================================================================

/// How the parser handles lines that do not match the profile's line_pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum MultilineMode {
    /// Append non-matching lines to the previous entry's message.
    /// This is the correct behaviour for stack traces and multi-line messages.
    #[default]
    Continuation,

    /// Skip non-matching lines entirely.
    Skip,

    /// Treat non-matching lines as standalone entries with no parsed fields.
    Raw,
}

// =============================================================================
// Format Profile (runtime representation)
// =============================================================================

/// Runtime representation of a format profile after TOML parsing and
/// regex compilation. This is what the parser uses at scan time.
///
/// Built from `ProfileDefinition` (the raw TOML structure) via validation.
#[derive(Debug, Clone)]
pub struct FormatProfile {
    /// Unique profile identifier (e.g. "veeam-vbr").
    pub id: String,

    /// Human-readable name (e.g. "Veeam Backup & Replication").
    pub name: String,

    /// Profile schema version.
    pub version: String,

    /// Description of what this profile covers.
    pub description: String,

    /// Glob patterns for filename-based detection hints.
    pub file_patterns: Vec<String>,

    /// Compiled regex for content-based format detection.
    /// Applied to the first N lines of a file; match rate determines confidence.
    pub content_match: regex::Regex,

    /// Compiled regex for parsing individual log lines.
    /// Named capture groups: timestamp, level, thread, component, message.
    pub line_pattern: regex::Regex,

    /// chrono format string for parsing the timestamp capture group.
    pub timestamp_format: String,

    /// How to handle lines that do not match line_pattern.
    pub multiline_mode: MultilineMode,

    /// Maps normalised Severity variants to lists of format-specific strings.
    /// Matching is case-insensitive.
    pub severity_mapping: HashMap<Severity, Vec<String>>,

    /// Whether this is a built-in profile (true) or user-defined (false).
    pub is_builtin: bool,
}

impl FormatProfile {
    /// Determines the normalised severity for a raw level string.
    ///
    /// Checks each severity mapping (case-insensitive). Returns `Severity::Unknown`
    /// if no mapping matches.
    pub fn map_severity(&self, raw_level: &str) -> Severity {
        let raw_lower = raw_level.to_lowercase();
        for (severity, patterns) in &self.severity_mapping {
            for pattern in patterns {
                if raw_lower == pattern.to_lowercase() {
                    return *severity;
                }
            }
        }
        Severity::Unknown
    }

    /// Determines severity by scanning the message text for keywords.
    ///
    /// Used for formats that have no explicit severity field (e.g. VBO365).
    /// Checks severity mappings as substring matches against the message.
    /// Returns the highest (most severe) match found.
    pub fn infer_severity_from_message(&self, message: &str) -> Severity {
        let msg_lower = message.to_lowercase();

        // Check in order of severity (highest first) so the most severe match wins
        for severity in Severity::all() {
            if let Some(patterns) = self.severity_mapping.get(severity) {
                for pattern in patterns {
                    if msg_lower.contains(&pattern.to_lowercase()) {
                        return *severity;
                    }
                }
            }
        }

        Severity::Info // Default to Info for unclassified messages
    }
}

// =============================================================================
// Discovered File (output of discovery phase)
// =============================================================================

/// Metadata about a file found during directory scanning, before parsing.
#[derive(Debug, Clone)]
pub struct DiscoveredFile {
    /// Full path to the file.
    pub path: PathBuf,

    /// File size in bytes.
    pub size: u64,

    /// Last modification timestamp.
    pub modified: Option<DateTime<Utc>>,

    /// Detected format profile ID (None if no profile matched).
    pub profile_id: Option<String>,

    /// Auto-detection confidence score (0.0-1.0).
    pub detection_confidence: f64,

    /// Whether this file exceeds the large file threshold.
    pub is_large: bool,
}

// =============================================================================
// Scan Summary
// =============================================================================

/// Summary statistics for a completed scan operation.
#[derive(Debug, Clone, Default)]
pub struct ScanSummary {
    /// Total files discovered (before filtering by format).
    pub total_files_discovered: usize,

    /// Files that matched a format profile.
    pub files_matched: usize,

    /// Files that could not be read (permissions, encoding, etc.).
    pub files_with_errors: usize,

    /// Total log entries parsed across all files.
    pub total_entries: usize,

    /// Entries by severity level.
    pub entries_by_severity: HashMap<Severity, usize>,

    /// Total parse errors (lines that could not be parsed).
    pub total_parse_errors: usize,

    /// Per-file breakdown.
    pub file_summaries: Vec<FileSummary>,

    /// Wall-clock scan duration.
    pub duration: std::time::Duration,
}

/// Per-file scan statistics.
#[derive(Debug, Clone)]
pub struct FileSummary {
    /// File path.
    pub path: PathBuf,

    /// Format profile ID used.
    pub profile_id: String,

    /// Number of entries parsed from this file.
    pub entry_count: usize,

    /// Number of parse errors in this file.
    pub error_count: usize,

    /// Earliest timestamp found (if any).
    pub earliest: Option<DateTime<Utc>>,

    /// Latest timestamp found (if any).
    pub latest: Option<DateTime<Utc>>,
}

// =============================================================================
// Scan Progress (for UI updates)
// =============================================================================

/// Progress messages sent from the scan thread to the UI thread.
#[derive(Debug, Clone)]
pub enum ScanProgress {
    /// Discovery phase started.
    DiscoveryStarted,

    /// A file was discovered.
    FileDiscovered { path: PathBuf, files_found: usize },

    /// Discovery phase completed.
    DiscoveryCompleted { total_files: usize },

    /// Parsing phase started.
    ParsingStarted { total_files: usize },

    /// A file has been parsed.
    FileParsed {
        path: PathBuf,
        entries: usize,
        errors: usize,
        files_completed: usize,
        total_files: usize,
    },

    /// Parsing phase completed.
    ParsingCompleted { summary: ScanSummary },

    /// A non-fatal warning occurred during scanning.
    Warning { message: String },

    /// Scan failed with a fatal error.
    Failed { error: String },

    /// Scan was cancelled by the user before completion.
    Cancelled,

    /// All discovered file metadata, sent once after auto-detection completes
    /// so the UI can populate the discovery panel before parsing begins.
    FilesDiscovered { files: Vec<DiscoveredFile> },

    /// Additional files discovered when "Add File(s)" appends to an existing
    /// session. Unlike `FilesDiscovered` (which replaces the list), this
    /// message extends the UI's discovered-file list.
    AdditionalFilesDiscovered { files: Vec<DiscoveredFile> },

    /// A batch of parsed log entries, streamed to the UI during parsing.
    ///
    /// Batched (see ENTRY_BATCH_SIZE in app::scan) to amortise channel overhead
    /// while still allowing the UI to display partial results before the scan
    /// finishes.
    EntriesBatch { entries: Vec<LogEntry> },
}
