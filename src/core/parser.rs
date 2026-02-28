// LogSleuth - core/parser.rs
//
// Stream-oriented log file parsing using format profiles.
// Core layer: accepts Read trait objects, never touches filesystem directly.

use crate::core::model::{FormatProfile, LogEntry};
use crate::util::error::ParseError;
use chrono::{DateTime, Datelike, NaiveDateTime, Utc};
use regex::Regex;
use std::path::Path;
use std::sync::OnceLock;

/// Configuration for parsing operations.
#[derive(Debug, Clone)]
pub struct ParseConfig {
    pub chunk_size: usize,
    pub max_entry_size: usize,
    pub max_parse_errors_per_file: usize,
}

impl Default for ParseConfig {
    fn default() -> Self {
        use crate::util::constants;
        Self {
            chunk_size: constants::DEFAULT_CHUNK_SIZE,
            max_entry_size: constants::DEFAULT_MAX_ENTRY_SIZE,
            max_parse_errors_per_file: constants::MAX_PARSE_ERRORS_PER_FILE,
        }
    }
}

/// Result of parsing a single log file.
#[derive(Debug)]
pub struct ParseResult {
    /// Successfully parsed entries.
    pub entries: Vec<LogEntry>,
    /// Parse errors encountered (capped at max_parse_errors_per_file).
    pub errors: Vec<ParseError>,
    /// Total lines processed.
    pub lines_processed: u64,
}

/// Parse a log file using the given format profile.
///
/// Reads the file content and applies the profile's line_pattern to extract
/// structured fields from each line. Multi-line entries are handled according
/// to the profile's multiline_mode setting.
///
/// # Arguments
/// * `content` - File content as a string (the app layer handles reading)
/// * `file_path` - Path to the source file (for LogEntry metadata)
/// * `profile` - The format profile to use for parsing
/// * `config` - Parsing configuration (limits)
/// * `id_start` - Starting ID for entries (for global uniqueness across files)
pub fn parse_content(
    content: &str,
    file_path: &Path,
    profile: &FormatProfile,
    config: &ParseConfig,
    id_start: u64,
) -> ParseResult {
    // TODO: Implement full streaming parser in next increment.
    tracing::debug!(
        file = %file_path.display(),
        profile = %profile.id,
        "Parsing started (stub)"
    );

    let mut entries = Vec::new();
    let mut errors = Vec::new();
    let mut current_id = id_start;
    let mut lines_processed: u64 = 0;

    for (line_idx, line) in content.lines().enumerate() {
        lines_processed += 1;
        let line_number = (line_idx as u64) + 1;

        // Skip empty lines
        if line.trim().is_empty() {
            continue;
        }

        // Attempt to match the line against the profile's line_pattern
        if let Some(caps) = profile.line_pattern.captures(line) {
            let message = caps
                .name("message")
                .map(|m| m.as_str().to_string())
                .unwrap_or_else(|| line.to_string());

            // Extract severity:
            //   1. If the profile captured a `level` field, map it via
            //      severity_mapping (case-insensitive exact match).
            //   2. If step 1 returns Unknown (unrecognised level string),
            //      fall back to regex override patterns on the message.
            //   3. If there is no `level` capture group, try regex override
            //      patterns first, then plain-keyword message inference.
            // This layered approach means: structured profiles get precise
            // level-based classification; plain-text and fallback profiles can
            // classify entries via [WARN]-style embedded markers.
            let severity = if let Some(level_match) = caps.name("level") {
                let mapped = profile.map_severity(level_match.as_str());
                if mapped == crate::core::model::Severity::Unknown {
                    // Level field present but value not in severity_mapping --
                    // try regex override on the message as a second chance.
                    profile
                        .apply_severity_override(&message)
                        .unwrap_or(crate::core::model::Severity::Unknown)
                } else {
                    mapped
                }
            } else {
                // No level capture -- regex override takes priority over
                // keyword substring matching so patterns like \[WARN\] win
                // before the generic substring fallback fires.
                profile
                    .apply_severity_override(&message)
                    .unwrap_or_else(|| profile.infer_severity_from_message(&message))
            };

            // Parse timestamp using the profile's format string.
            // On failure: record a non-fatal parse error and keep timestamp as None
            // so the entry is still visible in the timeline (sorted to the end).
            let timestamp: Option<DateTime<Utc>> =
                if let Some(raw_ts) = caps.name("timestamp").map(|m| m.as_str()) {
                    match parse_timestamp(raw_ts, &profile.timestamp_format) {
                        Ok(ts) => Some(ts),
                        Err(_msg) => {
                            if errors.len() < config.max_parse_errors_per_file {
                                errors.push(ParseError::TimestampParse {
                                    file: file_path.to_path_buf(),
                                    line_number,
                                    raw_timestamp: raw_ts.to_string(),
                                    format: profile.timestamp_format.clone(),
                                });
                            }
                            None
                        }
                    }
                } else {
                    None
                };

            let entry = LogEntry {
                id: current_id,
                timestamp,
                severity,
                source_file: file_path.to_path_buf(),
                line_number,
                thread: caps.name("thread").map(|m| m.as_str().to_string()),
                component: caps.name("component").map(|m| m.as_str().to_string()),
                message,
                raw_text: line.to_string(),
                profile_id: profile.id.clone(),
                file_modified: None, // set by app layer after parsing
            };

            entries.push(entry);
            current_id += 1;
        } else {
            // Line does not match the pattern
            match profile.multiline_mode {
                crate::core::model::MultilineMode::Continuation => {
                    // Append to previous entry if one exists.
                    // Skip the append when the entry has already been truncated
                    // to avoid repeated grow-then-truncate cycles that waste CPU
                    // and temporarily spike memory for pathological files.
                    if let Some(last) = entries.last_mut() {
                        if last.message.len() <= config.max_entry_size {
                            last.message.push('\n');
                            last.message.push_str(line);
                        }
                        if last.raw_text.len() <= config.max_entry_size {
                            last.raw_text.push('\n');
                            last.raw_text.push_str(line);
                        }
                    }
                }
                crate::core::model::MultilineMode::Skip => {
                    // Ignore the line
                }
                crate::core::model::MultilineMode::Raw => {
                    // Create an unparsed entry
                    entries.push(LogEntry {
                        id: current_id,
                        timestamp: None,
                        severity: crate::core::model::Severity::Unknown,
                        source_file: file_path.to_path_buf(),
                        line_number,
                        thread: None,
                        component: None,
                        message: line.to_string(),
                        raw_text: line.to_string(),
                        profile_id: profile.id.clone(),
                        file_modified: None, // set by app layer after parsing
                    });
                    current_id += 1;
                }
            }

            // Track as parse error for un-handled non-matching lines.
            // - Continuation mode: only an error when there is no previous entry to
            //   append to (the first line should always match the pattern).
            // - Skip mode: the line is silently discarded — counts as a parse error.
            // - Raw mode: an unparsed entry was successfully created above, so the
            //   line is handled; do NOT record an error or the scan summary will
            //   show inflated error counts for every Raw-mode entry.
            if errors.len() < config.max_parse_errors_per_file {
                let is_error = match profile.multiline_mode {
                    crate::core::model::MultilineMode::Continuation => entries.is_empty(),
                    crate::core::model::MultilineMode::Skip => true,
                    crate::core::model::MultilineMode::Raw => false,
                };
                if is_error {
                    errors.push(ParseError::LineParse {
                        file: file_path.to_path_buf(),
                        line_number,
                        reason: "Line does not match profile pattern".to_string(),
                    });
                }
            }
        }

        // Enforce max entry size on the last entry.
        // Both `message` and `raw_text` are capped so that a pathological
        // file with millions of continuation lines cannot grow an entry
        // without bound (memory safety — Rule 11 resource bounds).
        if let Some(last) = entries.last_mut() {
            if last.message.len() > config.max_entry_size {
                last.message.truncate(config.max_entry_size);
                last.message.push_str("... [truncated]");
            }
            if last.raw_text.len() > config.max_entry_size {
                last.raw_text.truncate(config.max_entry_size);
                last.raw_text.push_str("... [truncated]");
            }
        }
    }

    // -------------------------------------------------------------------------
    // Timestamp sniff fallback
    //
    // Any entry whose structured capture (or timestamp_format parse) produced
    // timestamp: None gets one more chance.  We scan the raw_text for any of a
    // wide set of common timestamp patterns and use the first match.
    //
    // This covers:
    //   - Plain-text (raw-mode) files that have embedded timestamps but no
    //     structured profile.
    //   - Continuation-mode files where the profile timestamp_format did not
    //     match the actual text (e.g. optional milliseconds not in the format).
    //   - Any other entry where the primary parse failed.
    //
    // Best-effort only: if sniff also finds nothing the entry keeps
    // timestamp: None and sorts to the end of the timeline.
    // -------------------------------------------------------------------------
    for entry in &mut entries {
        if entry.timestamp.is_none() {
            entry.timestamp = sniff_timestamp(&entry.raw_text);
        }
    }

    tracing::debug!(
        file = %file_path.display(),
        entries = entries.len(),
        errors = errors.len(),
        lines = lines_processed,
        "Parsing complete"
    );

    ParseResult {
        entries,
        errors,
        lines_processed,
    }
}

// =============================================================================
// Timestamp sniffing
// =============================================================================

/// Try to find and parse any recognisable timestamp embedded anywhere in
/// `raw_line`, returning the first successful result.
///
/// Used as a best-effort fallback for entries whose structured `timestamp`
/// capture group either did not exist (plain-text, raw-mode) or failed to
/// parse (format mismatch).  The function never returns an error; it either
/// produces a timestamp or returns `None`.
///
/// Patterns are tried from most-precise (RFC 3339 with explicit timezone)
/// to least-precise (year-less BSD syslog), so higher-confidence results
/// take priority over looser matches on the same line.
pub(crate) fn sniff_timestamp(raw_line: &str) -> Option<DateTime<Utc>> {
    /// A sniff candidate: a regex that finds a timestamp substring, plus a
    /// parsing closure that converts the matched text to `DateTime<Utc>`.
    struct Sniffer {
        re: Regex,
        parse: fn(&str) -> Option<DateTime<Utc>>,
    }

    static SNIFFERS: OnceLock<Vec<Sniffer>> = OnceLock::new();

    let sniffers = SNIFFERS.get_or_init(|| {
        // Helper to compile a regex without panicking at runtime.
        // Patterns are tested in the unit tests below, so any mistake there
        // shows up as a failing test rather than a runtime panic.
        fn re(pat: &str) -> Regex {
            Regex::new(pat).expect("sniff_timestamp: invalid regex")
        }

        vec![
            // ------------------------------------------------------------------
            // Tier 1 — RFC 3339 / ISO 8601 with explicit timezone
            // Examples:
            //   2024-01-15T14:30:22Z
            //   2024-01-15T14:30:22.123456Z
            //   2024-01-15T14:30:22+05:30
            //   2024-01-15T14:30:22.999+05:30
            // ------------------------------------------------------------------
            Sniffer {
                re: re(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:[.,]\d+)?(?:Z|[+-]\d{2}:?\d{2})"),
                parse: |s| {
                    // Normalise `+0530` -> `+05:30` so parse_from_rfc3339 accepts it.
                    let fixed = if s.len() > 20 {
                        let tail = &s[s.len().saturating_sub(5)..];
                        if !tail.contains(':') && (tail.starts_with('+') || tail.starts_with('-')) {
                            format!(
                                "{}{}",
                                &s[..s.len() - 4],
                                &format!("{}:{}", &tail[..3], &tail[3..])
                            )
                        } else {
                            s.to_owned()
                        }
                    } else {
                        s.to_owned()
                    };
                    DateTime::parse_from_rfc3339(&fixed)
                        .ok()
                        .map(|dt| dt.into())
                },
            },
            // ------------------------------------------------------------------
            // Tier 2 — ISO 8601 with comma milliseconds (log4j style)
            // Example: 2024-01-15 14:30:22,123
            // ------------------------------------------------------------------
            Sniffer {
                re: re(r"\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2},\d+"),
                parse: |s| {
                    // Replace comma with dot so chrono's %.f specifier accepts it.
                    let s = s.replace(',', ".").replace('T', " ");
                    NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S%.f")
                        .ok()
                        .map(|ndt| ndt.and_utc())
                },
            },
            // ------------------------------------------------------------------
            // Tier 3 — ISO 8601 without timezone, optional dot-millis
            // Examples:
            //   2024-01-15 14:30:22
            //   2024-01-15 14:30:22.123
            //   2024-01-15T14:30:22
            //   2024-01-15T14:30:22.123456
            // ------------------------------------------------------------------
            Sniffer {
                re: re(r"\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}(?:\.\d+)?"),
                parse: |s| {
                    let s = s.replace('T', " ");
                    // Try with fractional seconds first, then without.
                    NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S%.f")
                        .or_else(|_| NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S"))
                        .ok()
                        .map(|ndt| ndt.and_utc())
                },
            },
            // ------------------------------------------------------------------
            // Tier 4 — Slash year-first: YYYY/MM/DD HH:MM:SS[.mmm]
            // Example: 2024/01/15 14:30:22
            // ------------------------------------------------------------------
            Sniffer {
                re: re(r"\d{4}/\d{2}/\d{2}[ T]\d{2}:\d{2}:\d{2}(?:\.\d+)?"),
                parse: |s| {
                    let s = s.replace('/', "-").replace('T', " ");
                    NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S%.f")
                        .or_else(|_| NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S"))
                        .ok()
                        .map(|ndt| ndt.and_utc())
                },
            },
            // ------------------------------------------------------------------
            // Tier 5 — Dot day-first: DD.MM.YYYY HH:MM:SS[.mmm]  (Veeam style)
            // Example: 26.02.2026 22:07:56.535
            // ------------------------------------------------------------------
            Sniffer {
                re: re(r"\d{2}\.\d{2}\.\d{4} \d{2}:\d{2}:\d{2}(?:\.\d+)?"),
                parse: |s| {
                    NaiveDateTime::parse_from_str(s, "%d.%m.%Y %H:%M:%S%.f")
                        .or_else(|_| NaiveDateTime::parse_from_str(s, "%d.%m.%Y %H:%M:%S"))
                        .ok()
                        .map(|ndt| ndt.and_utc())
                },
            },
            // ------------------------------------------------------------------
            // Tier 6 — Apache combined log: DD/Mon/YYYY:HH:MM:SS +ZZZZ
            // Example: 15/Jan/2024:14:30:22 +0000
            // ------------------------------------------------------------------
            Sniffer {
                re: re(r"\d{2}/[A-Za-z]{3}/\d{4}:\d{2}:\d{2}:\d{2} [+-]\d{4}"),
                parse: |s| {
                    DateTime::parse_from_str(s, "%d/%b/%Y:%H:%M:%S %z")
                        .ok()
                        .map(|dt| dt.into())
                },
            },
            // ------------------------------------------------------------------
            // Tier 7 — Slash-delimited date + time: handles both MM/DD/YYYY
            //          (US) and DD/MM/YYYY (GB/EU).
            //
            // Disambiguation strategy (same logic used in Tier 8):
            //   first field > 12  → unambiguously DD/MM/YYYY  (e.g. 15/01/2024)
            //   second field > 12 → unambiguously MM/DD/YYYY  (e.g. 01/15/2024)
            //   both ≤ 12         → ambiguous; try MM/DD/YYYY (US) first,
            //                       then DD/MM/YYYY (GB) as fallback.
            //
            // NOTE: truly ambiguous dates such as 01/02/2024 cannot be resolved
            // without knowing the source locale.  The US interpretation is used
            // as the default.  If the source is consistently GB/EU, use a
            // profile with an explicit timestamp_format = "%d/%m/%Y %H:%M:%S".
            // ------------------------------------------------------------------
            Sniffer {
                re: re(r"\d{2}/\d{2}/\d{4} \d{2}:\d{2}:\d{2}"),
                parse: |s| {
                    // Extract the two leading numeric fields.
                    let mut parts = s.splitn(3, '/');
                    let (first, second) = match (
                        parts.next().and_then(|p| p.parse::<u32>().ok()),
                        parts.next().and_then(|p| p.parse::<u32>().ok()),
                    ) {
                        (Some(a), Some(b)) => (a, b),
                        _ => return None,
                    };

                    if first > 12 {
                        // Unambiguously DD/MM/YYYY (day cannot be a month).
                        NaiveDateTime::parse_from_str(s, "%d/%m/%Y %H:%M:%S")
                            .ok()
                            .map(|ndt| ndt.and_utc())
                    } else if second > 12 {
                        // Unambiguously MM/DD/YYYY (second field cannot be a month).
                        NaiveDateTime::parse_from_str(s, "%m/%d/%Y %H:%M:%S")
                            .ok()
                            .map(|ndt| ndt.and_utc())
                    } else {
                        // Ambiguous: US default, GB fallback.
                        NaiveDateTime::parse_from_str(s, "%m/%d/%Y %H:%M:%S")
                            .or_else(|_| NaiveDateTime::parse_from_str(s, "%d/%m/%Y %H:%M:%S"))
                            .ok()
                            .map(|ndt| ndt.and_utc())
                    }
                },
            },
            // ------------------------------------------------------------------
            // Tier 8 — Windows DHCP two-digit year: MM/DD/YY,HH:MM:SS or
            //          DD/MM/YY,HH:MM:SS.  Same disambiguation as Tier 7.
            // Examples:
            //   01/15/24,14:30:22  → US  (second > 12, unambiguous)
            //   15/01/24,14:30:22  → GB  (first > 12, unambiguous)
            // ------------------------------------------------------------------
            Sniffer {
                re: re(r"\d{2}/\d{2}/\d{2},\d{2}:\d{2}:\d{2}"),
                parse: |s| {
                    let mut parts = s.splitn(3, '/');
                    let (first, second) = match (
                        parts.next().and_then(|p| p.parse::<u32>().ok()),
                        parts.next().and_then(|p| p.parse::<u32>().ok()),
                    ) {
                        (Some(a), Some(b)) => (a, b),
                        _ => return None,
                    };

                    if first > 12 {
                        NaiveDateTime::parse_from_str(s, "%d/%m/%y,%H:%M:%S")
                            .ok()
                            .map(|ndt| ndt.and_utc())
                    } else if second > 12 {
                        NaiveDateTime::parse_from_str(s, "%m/%d/%y,%H:%M:%S")
                            .ok()
                            .map(|ndt| ndt.and_utc())
                    } else {
                        // Ambiguous: US default, GB fallback.
                        NaiveDateTime::parse_from_str(s, "%m/%d/%y,%H:%M:%S")
                            .or_else(|_| NaiveDateTime::parse_from_str(s, "%d/%m/%y,%H:%M:%S"))
                            .ok()
                            .map(|ndt| ndt.and_utc())
                    }
                },
            },
            // ------------------------------------------------------------------
            // Tier 9 — Month-name with 4-digit year
            // Examples:
            //   Jan 15 2024 14:30:22
            //   January 15, 2024 14:30:22
            //   Jan 15, 2024 14:30:22
            // ------------------------------------------------------------------
            Sniffer {
                re: re(r"[A-Z][a-z]{2,8} \d{1,2},? \d{4} \d{2}:\d{2}:\d{2}"),
                parse: |s| {
                    // Normalise: remove optional comma, collapse multiple spaces.
                    let s = s.replace(',', " ");
                    let s: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
                    NaiveDateTime::parse_from_str(&s, "%b %d %Y %H:%M:%S")
                        .ok()
                        .map(|ndt| ndt.and_utc())
                },
            },
            // ------------------------------------------------------------------
            // Tier 10 — BSD syslog year-less: Mon DD HH:MM:SS
            // Example: Jan 15 14:30:22  (space-padded single digit: Jan  5)
            // Year is injected from current UTC year (best-effort).
            // ------------------------------------------------------------------
            Sniffer {
                re: re(r"[A-Z][a-z]{2} [ \d]\d \d{2}:\d{2}:\d{2}"),
                parse: |s| {
                    let year = Utc::now().year();
                    let with_year = format!("{year} {s}");
                    NaiveDateTime::parse_from_str(&with_year, "%Y %b %e %H:%M:%S")
                        .ok()
                        .map(|ndt| ndt.and_utc())
                },
            },
            // ------------------------------------------------------------------
            // Tier 11 — Compact ISO: YYYYMMDDTHHMMSS or YYYYMMDD HHMMSS
            // Example: 20240115T143022
            // ------------------------------------------------------------------
            Sniffer {
                re: re(r"\d{8}[T ]\d{6}"),
                parse: |s| {
                    let s = s.replace(' ', "T");
                    NaiveDateTime::parse_from_str(&s, "%Y%m%dT%H%M%S")
                        .ok()
                        .map(|ndt| ndt.and_utc())
                },
            },
            // ------------------------------------------------------------------
            // Tier 12 — Unix epoch seconds (10 digits, only at line start to
            // avoid matching large port numbers / PIDs mid-line).
            // Example: 1705329022 ... or 1705329022.123 ...
            // ------------------------------------------------------------------
            Sniffer {
                re: re(r"^\d{10}(?:\.\d+)?"),
                parse: |s| {
                    let (secs_str, _) = s.split_once('.').unwrap_or((s, ""));
                    secs_str
                        .parse::<i64>()
                        .ok()
                        .and_then(|secs| DateTime::from_timestamp(secs, 0))
                },
            },
        ]
    });

    for sniffer in sniffers {
        if let Some(m) = sniffer.re.find(raw_line) {
            if let Some(dt) = (sniffer.parse)(m.as_str()) {
                return Some(dt);
            }
        }
    }
    None
}

// =============================================================================
// Timestamp parsing
// =============================================================================

/// Parse a raw timestamp string using a chrono format string.
///
/// Attempts multiple parse strategies in order so that common real-world
/// timestamp variations succeed even when the profile format string is not
/// an exact match.
///
/// Strategy:
///   1. Direct `NaiveDateTime` parse with the given format.
///   2. `NaiveDate`-only parse (date-only formats such as `%Y-%m-%d`).
///   3. RFC 3339 / ISO 8601 with timezone (e.g. `2024-01-15T14:30:22+05:30`).
///   4. Normalised separators: `/`→`-` and `T`→` `, then retry as NaiveDateTime.
///      Handles `generic-timestamp` variants (`2024/01/15` or `2024-01-15T14:30:22`).
///   5. Current-year injection for year-less formats (e.g. BSD syslog
///      `%b %d %H:%M:%S`).  Prepends the current UTC year so entries land at
///      the correct position in the live timeline.  Best-effort: log files
///      spanning a year boundary will show incorrect dates for entries from
///      the previous year.
///
/// Returns `Ok(DateTime<Utc>)` on success, or `Err(description)` on failure.
fn parse_timestamp(raw: &str, format: &str) -> Result<DateTime<Utc>, String> {
    let trimmed = raw.trim();

    // First try: parse as a full NaiveDateTime with the format string.
    if let Ok(ndt) = NaiveDateTime::parse_from_str(trimmed, format) {
        return Ok(ndt.and_utc());
    }

    // Second try: parse as NaiveDate only (for date-only formats like "%Y-%m-%d").
    // Treat as midnight UTC.
    if let Ok(nd) = chrono::NaiveDate::parse_from_str(trimmed, format) {
        if let Some(ndt) = nd.and_hms_opt(0, 0, 0) {
            return Ok(ndt.and_utc());
        }
    }

    // Third try: parse as RFC 3339 / ISO 8601 (includes timezone offset).
    if let Ok(dt) = DateTime::parse_from_rfc3339(trimmed) {
        return Ok(dt.into());
    }

    // Fourth try: normalise common separator variants.
    //   - Slash-separated dates: "2024/01/15 14:30:22" -> "2024-01-15 14:30:22"
    //   - ISO-8601 T separator: "2024-01-15T14:30:22"  -> "2024-01-15 14:30:22"
    // Both mismatches arise when the generic-timestamp profile captures dates
    // that use `/` or `T` but the profile format uses `-` and ` `.
    let normalised = trimmed.replace('/', "-").replace('T', " ");
    if normalised != trimmed {
        if let Ok(ndt) = NaiveDateTime::parse_from_str(&normalised, format) {
            return Ok(ndt.and_utc());
        }
        if let Ok(nd) = chrono::NaiveDate::parse_from_str(&normalised, format) {
            if let Some(ndt) = nd.and_hms_opt(0, 0, 0) {
                return Ok(ndt.and_utc());
            }
        }
    }

    // Fifth try: current-year injection for year-less formats.
    // BSD syslog (RFC 3164) timestamps are "Mon DD HH:MM:SS" with no year.
    // Prepend the current UTC year so the entry is placed correctly in the
    // live timeline.  This is a best-effort heuristic only.
    if !format.contains("%Y") && !format.contains("%y") && !format.contains("%C") {
        let year = Utc::now().year();
        let with_year = format!("{year} {trimmed}");
        let year_format = format!("%Y {format}");
        if let Ok(ndt) = NaiveDateTime::parse_from_str(&with_year, &year_format) {
            return Ok(ndt.and_utc());
        }
    }

    Err(format!("cannot parse '{trimmed}' with format '{format}'"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::Severity;
    use crate::core::profile;
    use std::path::PathBuf;

    fn make_test_profile() -> FormatProfile {
        let toml = r#"
[profile]
id = "test"
name = "Test"

[detection]
content_match = '^\['

[parsing]
line_pattern = '^\[(?P<timestamp>[^\]]+)\]\s(?P<level>\w+)\s+(?P<message>.+)$'
timestamp_format = "%Y-%m-%d %H:%M:%S"
multiline_mode = "continuation"

[severity_mapping]
error = ["Error"]
warning = ["Warning"]
info = ["Info"]
"#;
        let path = PathBuf::from("test.toml");
        let def = profile::parse_profile_toml(toml, &path).unwrap();
        profile::validate_and_compile(def, &path, false).unwrap()
    }

    #[test]
    fn test_parse_basic_lines() {
        let profile = make_test_profile();
        let content = "[2024-01-15 14:30:22] Error Something failed\n\
                        [2024-01-15 14:30:23] Info Normal operation\n";

        let result = parse_content(
            content,
            &PathBuf::from("test.log"),
            &profile,
            &ParseConfig::default(),
            0,
        );

        assert_eq!(result.entries.len(), 2);
        assert_eq!(result.entries[0].severity, Severity::Error);
        assert_eq!(result.entries[0].message, "Something failed");
        assert_eq!(result.entries[1].severity, Severity::Info);
    }

    #[test]
    fn test_parse_multiline_continuation() {
        let profile = make_test_profile();
        let content = "[2024-01-15 14:30:22] Error Connection failed\n\
                        at com.example.Client.connect(Client.java:42)\n\
                        at com.example.Main.run(Main.java:10)\n\
                        [2024-01-15 14:30:23] Info Retry succeeded\n";

        let result = parse_content(
            content,
            &PathBuf::from("test.log"),
            &profile,
            &ParseConfig::default(),
            0,
        );

        assert_eq!(result.entries.len(), 2);
        assert!(result.entries[0].message.contains("Client.java:42"));
        assert!(result.entries[0].message.contains("Main.java:10"));
    }

    #[test]
    fn test_parse_empty_content() {
        let profile = make_test_profile();
        let result = parse_content(
            "",
            &PathBuf::from("empty.log"),
            &profile,
            &ParseConfig::default(),
            0,
        );
        assert_eq!(result.entries.len(), 0);
        assert_eq!(result.errors.len(), 0);
    }

    #[test]
    fn test_parse_entry_truncation() {
        let profile = make_test_profile();
        let long_msg = "x".repeat(100_000);
        let content = format!("[2024-01-15 14:30:22] Error {long_msg}");

        let config = ParseConfig {
            max_entry_size: 1000,
            ..ParseConfig::default()
        };

        let result = parse_content(&content, &PathBuf::from("big.log"), &profile, &config, 0);

        assert_eq!(result.entries.len(), 1);
        assert!(result.entries[0].message.len() < 1100); // truncated + suffix
        assert!(result.entries[0].message.ends_with("... [truncated]"));
    }

    // -------------------------------------------------------------------------
    // Timestamp parsing tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_parse_timestamp_naive_datetime() {
        let ts = parse_timestamp("2024-01-15 14:30:22", "%Y-%m-%d %H:%M:%S").unwrap();
        assert_eq!(
            ts.format("%Y-%m-%d %H:%M:%S").to_string(),
            "2024-01-15 14:30:22"
        );
    }

    #[test]
    fn test_parse_timestamp_with_milliseconds() {
        let ts = parse_timestamp("2024-01-15 14:30:22.123", "%Y-%m-%d %H:%M:%S%.f").unwrap();
        assert_eq!(
            ts.format("%Y-%m-%d %H:%M:%S").to_string(),
            "2024-01-15 14:30:22"
        );
    }

    #[test]
    fn test_parse_timestamp_rfc3339() {
        let ts = parse_timestamp("2024-01-15T14:30:22+00:00", "%Y-%m-%dT%H:%M:%S%z").unwrap();
        assert_eq!(
            ts.format("%Y-%m-%d %H:%M:%S").to_string(),
            "2024-01-15 14:30:22"
        );
    }

    #[test]
    fn test_parse_timestamp_invalid_returns_error() {
        let result = parse_timestamp("not-a-date", "%Y-%m-%d %H:%M:%S");
        assert!(result.is_err(), "invalid timestamp should return Err");
    }

    // -------------------------------------------------------------------------
    // Separator-normalisation fallback (fourth try)
    // -------------------------------------------------------------------------

    /// Regression: slash-separated date (generic-timestamp profile variant)
    /// was silently producing None timestamps because the profile format uses
    /// "-" separators but the actual log line uses "/".
    #[test]
    fn test_parse_timestamp_slash_separated_date() {
        let ts = parse_timestamp("2024/01/15 14:30:22", "%Y-%m-%d %H:%M:%S").unwrap();
        assert_eq!(
            ts.format("%Y-%m-%d %H:%M:%S").to_string(),
            "2024-01-15 14:30:22"
        );
    }

    /// Regression: ISO-8601 T separator (generic-timestamp profile variant)
    /// was failing because "%Y-%m-%d %H:%M:%S" expects a space before the hour.
    #[test]
    fn test_parse_timestamp_t_separator() {
        let ts = parse_timestamp("2024-01-15T14:30:22", "%Y-%m-%d %H:%M:%S").unwrap();
        assert_eq!(
            ts.format("%Y-%m-%d %H:%M:%S").to_string(),
            "2024-01-15 14:30:22"
        );
    }

    // -------------------------------------------------------------------------
    // Current-year injection fallback (fifth try)
    // -------------------------------------------------------------------------

    /// Regression: BSD syslog RFC 3164 format "Jan 15 14:30:22" has no year.
    /// Previously always produced None; now injects the current year.
    #[test]
    fn test_parse_timestamp_syslog_yearless() {
        let ts = parse_timestamp("Jan 15 14:30:22", "%b %d %H:%M:%S")
            .expect("syslog year-less timestamp should succeed with year injection");
        // The year will be the current UTC year — just verify it is reasonable.
        let year = ts.format("%Y").to_string().parse::<i32>().unwrap();
        assert!(
            year >= 2024,
            "injected year {year} should be recent (>= 2024)"
        );
        assert_eq!(ts.format("%m-%d %H:%M:%S").to_string(), "01-15 14:30:22");
    }

    // =========================================================================
    // sniff_timestamp tests
    // =========================================================================

    fn sniff(s: &str) -> String {
        sniff_timestamp(s)
            .expect(&format!(
                "sniff_timestamp should find a timestamp in: {s:?}"
            ))
            .format("%Y-%m-%d %H:%M:%S")
            .to_string()
    }

    /// Tier 1: RFC 3339 with Z suffix embedded mid-line.
    #[test]
    fn test_sniff_rfc3339_z() {
        assert_eq!(
            sniff("event 2024-01-15T14:30:22Z done"),
            "2024-01-15 14:30:22"
        );
    }

    /// Tier 1: RFC 3339 with +HH:MM offset.
    #[test]
    fn test_sniff_rfc3339_offset() {
        assert_eq!(
            sniff("2024-01-15T14:30:22+05:30 something"),
            "2024-01-15 09:00:22", // converted to UTC
        );
    }

    /// Tier 2: log4j comma-milliseconds.
    #[test]
    fn test_sniff_log4j_comma_millis() {
        assert_eq!(
            sniff("2024-01-15 14:30:22,999 ERROR Something"),
            "2024-01-15 14:30:22"
        );
    }

    /// Tier 3: ISO without timezone, dot-milliseconds.
    #[test]
    fn test_sniff_iso_dot_millis() {
        assert_eq!(
            sniff("[2024-01-15 14:30:22.123] INFO msg"),
            "2024-01-15 14:30:22"
        );
    }

    /// Tier 3: ISO with T separator, no millis.
    #[test]
    fn test_sniff_iso_t_no_millis() {
        assert_eq!(
            sniff("ts=2024-01-15T14:30:22 level=info"),
            "2024-01-15 14:30:22"
        );
    }

    /// Tier 4: slash year-first.
    #[test]
    fn test_sniff_slash_year_first() {
        assert_eq!(
            sniff("2024/01/15 14:30:22 - Started"),
            "2024-01-15 14:30:22"
        );
    }

    /// Tier 5: dot day-first (Veeam NFS format).
    #[test]
    fn test_sniff_dot_day_first() {
        assert_eq!(
            sniff("[26.02.2026 22:07:56.535] < 10580> nfstcps | ERR |msg"),
            "2026-02-26 22:07:56"
        );
    }

    /// Tier 6: Apache combined log format.
    #[test]
    fn test_sniff_apache_combined() {
        assert_eq!(
            sniff("127.0.0.1 - - [15/Jan/2024:14:30:22 +0000] \"GET /\""),
            "2024-01-15 14:30:22"
        );
    }

    /// Tier 7: US slash MM/DD/YYYY — second field > 12, unambiguously US.
    #[test]
    fn test_sniff_us_slash_unambiguous() {
        assert_eq!(
            sniff("01/15/2024 14:30:22 Connection"),
            "2024-01-15 14:30:22"
        );
    }

    /// Tier 7: GB slash DD/MM/YYYY — first field > 12, unambiguously GB.
    #[test]
    fn test_sniff_gb_slash_unambiguous() {
        assert_eq!(
            sniff("15/01/2024 14:30:22 Connection"),
            "2024-01-15 14:30:22"
        );
    }

    /// Tier 7: ambiguous date (both fields ≤ 12) — US interpretation used.
    #[test]
    fn test_sniff_slash_ambiguous_defaults_to_us() {
        // 01/02/2024 is ambiguous: US=Jan 2, GB=Feb 1.
        // The sniffer documents US as the default for ambiguous cases.
        assert_eq!(
            sniff("01/02/2024 14:30:22 msg"),
            "2024-01-02 14:30:22" // US: January 2nd
        );
    }

    /// Tier 8: Windows DHCP comma-separated date — US (second > 12).
    #[test]
    fn test_sniff_dhcp_comma_us() {
        assert_eq!(sniff("20,01/15/24,14:30:22,ASSIGN"), "2024-01-15 14:30:22");
    }

    /// Tier 8: Windows DHCP comma-separated date — GB (first > 12).
    #[test]
    fn test_sniff_dhcp_comma_gb() {
        assert_eq!(sniff("20,15/01/24,14:30:22,ASSIGN"), "2024-01-15 14:30:22");
    }

    /// Tier 9: month-name with 4-digit year.
    #[test]
    fn test_sniff_month_name_year() {
        assert_eq!(
            sniff("Started Jan 15 2024 14:30:22 service"),
            "2024-01-15 14:30:22"
        );
    }

    /// Tier 9: month-name with comma.
    #[test]
    fn test_sniff_month_name_comma() {
        assert_eq!(sniff("Jan 15, 2024 14:30:22 - msg"), "2024-01-15 14:30:22");
    }

    /// Tier 10: BSD syslog year-less (current year injected).
    #[test]
    fn test_sniff_bsd_syslog_yearless() {
        let ts = sniff_timestamp("Jan 15 14:30:22 hostname sshd[1]: msg")
            .expect("BSD syslog should sniff");
        let year = ts.format("%Y").to_string().parse::<i32>().unwrap();
        assert!(year >= 2024, "injected year {year} should be recent");
        assert_eq!(ts.format("%m-%d %H:%M:%S").to_string(), "01-15 14:30:22");
    }

    /// Tier 11: compact ISO.
    #[test]
    fn test_sniff_compact_iso() {
        assert_eq!(sniff("20240115T143022 event"), "2024-01-15 14:30:22");
    }

    /// Tier 12: Unix epoch at line start.
    #[test]
    fn test_sniff_unix_epoch() {
        // 1705329022 = 2024-01-15 14:30:22 UTC
        assert_eq!(
            sniff("1705329022 some event happened"),
            "2024-01-15 14:30:22"
        );
    }

    /// No recognisable timestamp — sniff should return None.
    #[test]
    fn test_sniff_no_timestamp_returns_none() {
        assert!(sniff_timestamp("hello world, no date here").is_none());
        assert!(sniff_timestamp("").is_none());
    }

    /// Plain-text raw-mode: entries whose raw_text contains a timestamp should
    /// be sniffed and ordered correctly after the post-parse pass.
    #[test]
    fn test_plain_text_entries_get_sniffed_timestamps() {
        let profile = {
            let toml = r#"
[profile]
id = "plain-text"
name = "Plain Text"
[detection]
content_match = '\S'
[parsing]
line_pattern = '^(?P<message>.+)$'
timestamp_format = "%Y-%m-%d %H:%M:%S"
multiline_mode = "raw"
[severity_mapping]
"#;
            let path = PathBuf::from("plain-text.toml");
            let def = profile::parse_profile_toml(toml, &path).unwrap();
            profile::validate_and_compile(def, &path, false).unwrap()
        };

        let content = "2024-01-15 14:30:21 service started\n\
                        2024-01-15 14:30:22 connection accepted\n\
                        2024-01-15 14:30:23 job completed\n";

        let result = parse_content(
            content,
            &PathBuf::from("app.log"),
            &profile,
            &ParseConfig::default(),
            0,
        );

        assert_eq!(result.entries.len(), 3, "should have 3 entries");
        for entry in &result.entries {
            assert!(
                entry.timestamp.is_some(),
                "plain-text entry should have a sniffed timestamp: {:?}",
                entry.raw_text
            );
        }
        // Verify ordering: first entry's timestamp < third entry's timestamp.
        assert!(
            result.entries[0].timestamp < result.entries[2].timestamp,
            "entries should be in chronological order after sniffing"
        );
    }

    #[test]
    fn test_parse_timestamp_errors_recorded_in_result() {
        let profile = make_test_profile();
        // The profile has format "%Y-%m-%d %H:%M:%S" so "BADTS" will fail
        let content = "[BADTS] Error Bad timestamp line\n\
                        [2024-01-15 14:30:22] Info Good timestamp\n";

        let result = parse_content(
            content,
            &PathBuf::from("test.log"),
            &profile,
            &ParseConfig::default(),
            0,
        );

        // Both lines match — one has a bad timestamp, one is good
        assert_eq!(result.entries.len(), 2, "both lines should produce entries");
        assert!(
            result.entries[0].timestamp.is_none(),
            "bad ts entry should have None"
        );
        assert!(
            result.entries[1].timestamp.is_some(),
            "good ts entry should have Some"
        );
        assert_eq!(
            result.errors.len(),
            1,
            "should record exactly one timestamp error"
        );
        assert!(matches!(
            result.errors[0],
            ParseError::TimestampParse { .. }
        ));
    }

    /// Regression test: a continuation-mode profile whose line_pattern does not
    /// match ANY line in the file must produce zero entries.
    ///
    /// This test documents the trigger condition for the plain-text fallback in
    /// `app::scan::run_parse_pipeline`: when `parse_result.entries.is_empty()`
    /// and the file has content, the scan layer re-parses with the plain-text
    /// profile so the file always contributes visible entries to the timeline.
    ///
    /// Without the fallback, files whose first line doesn't match a structured
    /// profile's pattern are silently dropped because `continuation` mode
    /// calls `entries.last_mut()` (returns None → no-op) before any entry
    /// has been created.
    #[test]
    fn test_continuation_mode_with_no_matching_lines_yields_zero_entries() {
        let profile = make_test_profile(); // multiline_mode = "continuation"
                                           // Lines that deliberately don't match `^\[timestamp\] level  message`
        let content = "=== Job Log Started ===\n\
                        Some header line without bracket prefix\n\
                        Another non-matching line\n";

        let result = parse_content(
            content,
            &PathBuf::from("vbr.log"),
            &profile,
            &ParseConfig::default(),
            0,
        );

        assert!(
            result.entries.is_empty(),
            "continuation mode with no matching first line must yield 0 entries, \
             confirming the trigger condition for the plain-text fallback in scan.rs"
        );
        // Content was non-empty so the fallback in scan.rs would kick in here
        // and re-parse with plain-text, producing at least 3 entries.
        assert!(
            !content.trim().is_empty(),
            "content is non-empty (fallback eligible)"
        );
    }
}
