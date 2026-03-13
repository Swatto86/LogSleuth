// LogSleuth - core/evtx_parser.rs
//
// Parser for Windows Event Log binary files (.evtx).
//
// Uses the `evtx` crate to parse the binary format and maps each event
// record to a LogEntry.  This module is only compiled on Windows per the
// requirement that Event Viewer support is Windows-only.
//
// Architecture note: the `evtx` crate is a pure-Rust binary format parser
// with no Windows API dependencies.  The Windows-only gating is a business
// rule (Event Viewer is a Windows concept), not a technical constraint.

use crate::core::model::{LogEntry, Severity};
use crate::core::parser::ParseResult;
use crate::util::constants;
use crate::util::error::ParseError;
use evtx::EvtxParser;
use regex::Regex;
use std::path::Path;
use std::sync::OnceLock;

// =============================================================================
// XML field extraction regexes (compiled once via OnceLock)
// =============================================================================

/// Extract `<EventID>NNN</EventID>` (may have attributes like Qualifiers).
fn event_id_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"<EventID[^>]*>(\d+)</EventID>").unwrap())
}

/// Extract `<Level>N</Level>`.
fn level_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"<Level>(\d+)</Level>").unwrap())
}

/// Extract `<Provider Name='...'>` or `<Provider Name="...">`.
fn provider_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"<Provider Name='([^']+)'"#).unwrap())
}

/// Extract `<Channel>...</Channel>`.
fn channel_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"<Channel>([^<]+)</Channel>").unwrap())
}

/// Extract `<Computer>...</Computer>`.
fn computer_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"<Computer>([^<]+)</Computer>").unwrap())
}

/// Extract `ProcessID='NNN'` from the Execution element.
fn process_id_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"ProcessID='(\d+)'"#).unwrap())
}

/// Extract `ThreadID='NNN'` from the Execution element.
fn thread_id_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"ThreadID='(\d+)'"#).unwrap())
}

/// Extract `<Data Name='key'>value</Data>` pairs from EventData.
fn event_data_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"<Data Name='([^']+)'>([^<]*)</Data>").unwrap())
}

// =============================================================================
// EVTX parsing
// =============================================================================

/// Parse a `.evtx` file and return entries in the standard `ParseResult`
/// format used by the scan pipeline.
///
/// Each event record in the `.evtx` file becomes a single `LogEntry`:
/// - `timestamp`: from the record's system timestamp (always UTC)
/// - `severity`: mapped from the `<Level>` element (1=Critical .. 5=Verbose)
/// - `component`: the `<Provider Name>` value
/// - `thread`: `ProcessID` (falls back to `ThreadID`)
/// - `message`: "EventID {id} | {provider} | {channel} | {computer} | {data}"
/// - `raw_text`: the full XML of the record
/// - `line_number`: the `EventRecordID` from the event header
///
/// # Arguments
///
/// * `path` - Path to the `.evtx` file.
/// * `max_entry_size` - Maximum raw_text size before truncation (bytes).
/// * `max_parse_errors` - Maximum parse errors to record per file.
/// * `id_start` - First entry ID to assign (normally 0 for temporary IDs
///   that are reassigned sequentially by the scan pipeline).
pub fn parse_evtx_file(
    path: &Path,
    max_entry_size: usize,
    max_parse_errors: usize,
    id_start: u64,
) -> ParseResult {
    let mut entries = Vec::new();
    let mut errors = Vec::new();
    let mut current_id = id_start;
    let mut records_processed: u64 = 0;

    let mut parser = match EvtxParser::from_path(path) {
        Ok(p) => p,
        Err(e) => {
            errors.push(ParseError::Io {
                file: path.to_path_buf(),
                source: std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("Failed to open .evtx file: {e}"),
                ),
            });
            return ParseResult {
                entries,
                errors,
                lines_processed: 0,
            };
        }
    };

    for record_result in parser.records() {
        records_processed += 1;

        let record = match record_result {
            Ok(r) => r,
            Err(e) => {
                if errors.len() < max_parse_errors {
                    errors.push(ParseError::LineParse {
                        file: path.to_path_buf(),
                        line_number: records_processed,
                        reason: format!("Failed to parse event record: {e}"),
                    });
                }
                continue;
            }
        };

        let xml = &record.data;

        // Extract fields from the event XML.
        let event_id = extract_match(event_id_re(), xml);
        let level_str = extract_match(level_re(), xml);
        let provider = extract_match(provider_re(), xml);
        let channel = extract_match(channel_re(), xml);
        let computer = extract_match(computer_re(), xml);
        let process_id = extract_match(process_id_re(), xml);
        let thread_id_val = extract_match(thread_id_re(), xml);

        // Map Windows Event Log Level to LogSleuth Severity.
        // Level values: 0=LogAlways, 1=Critical, 2=Error, 3=Warning,
        //               4=Informational, 5=Verbose.
        let severity = match level_str.as_deref() {
            Some("1") => Severity::Critical,
            Some("2") => Severity::Error,
            Some("3") => Severity::Warning,
            Some("4") | Some("0") => Severity::Info,
            Some("5") => Severity::Debug,
            _ => Severity::Unknown,
        };

        // Build human-readable message from event fields.
        let message = build_message(
            event_id.as_deref(),
            provider.as_deref(),
            channel.as_deref(),
            computer.as_deref(),
            xml,
        );

        // Thread: prefer ProcessID, fall back to ThreadID.
        let thread = process_id.or(thread_id_val);

        // Truncate raw_text if it exceeds the entry size limit.
        let raw_text = if xml.len() > max_entry_size {
            let mut truncated = String::with_capacity(max_entry_size + 20);
            // Truncate at a char boundary to avoid splitting a multi-byte char.
            let end = truncate_to_char_boundary(xml, max_entry_size);
            truncated.push_str(&xml[..end]);
            truncated.push_str("... [truncated]");
            truncated
        } else {
            xml.clone()
        };

        entries.push(LogEntry {
            id: current_id,
            timestamp: Some(record.timestamp),
            severity,
            source_file: path.to_path_buf(),
            line_number: record.event_record_id,
            thread,
            component: provider,
            message,
            raw_text,
            profile_id: constants::EVTX_PROFILE_ID.to_string(),
            file_modified: None, // stamped by the scan pipeline after collection
        });

        current_id += 1;
    }

    tracing::debug!(
        file = %path.display(),
        records = records_processed,
        entries = entries.len(),
        errors = errors.len(),
        "EVTX parsing complete"
    );

    ParseResult {
        entries,
        errors,
        lines_processed: records_processed,
    }
}

// =============================================================================
// Helpers
// =============================================================================

/// Extract the first capture group from a regex match against `xml`.
fn extract_match(re: &Regex, xml: &str) -> Option<String> {
    re.captures(xml)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().to_string())
}

/// Build a human-readable message from extracted event fields.
///
/// Format: "EventID {id} | {provider} | {channel} | {computer} | {data_pairs}"
/// where data_pairs are key=value from the EventData section.
fn build_message(
    event_id: Option<&str>,
    provider: Option<&str>,
    channel: Option<&str>,
    computer: Option<&str>,
    xml: &str,
) -> String {
    let mut parts = Vec::with_capacity(5);

    if let Some(id) = event_id {
        parts.push(format!("EventID {id}"));
    }
    if let Some(prov) = provider {
        parts.push(prov.to_string());
    }
    if let Some(ch) = channel {
        parts.push(ch.to_string());
    }
    if let Some(comp) = computer {
        parts.push(comp.to_string());
    }

    // Extract EventData key=value pairs for a compact summary.
    let data_summary = extract_event_data(xml);
    if !data_summary.is_empty() {
        parts.push(data_summary);
    }

    if parts.is_empty() {
        // Fallback: return a truncated snippet of the XML.
        let snippet_len = xml.len().min(200);
        let end = truncate_to_char_boundary(xml, snippet_len);
        return xml[..end].to_string();
    }

    parts.join(" | ")
}

/// Extract key=value pairs from the `<EventData>` section.
///
/// Returns a compact summary like "Key1=Value1, Key2=Value2".
/// Limits to `EVTX_MAX_DATA_PAIRS` to prevent extremely long messages.
fn extract_event_data(xml: &str) -> String {
    let pairs: Vec<String> = event_data_re()
        .captures_iter(xml)
        .take(constants::EVTX_MAX_DATA_PAIRS)
        .map(|cap| {
            let key = cap.get(1).map_or("", |m| m.as_str());
            let val = cap.get(2).map_or("", |m| m.as_str());
            format!("{key}={val}")
        })
        .collect();

    pairs.join(", ")
}

/// Find the largest byte index <= `max_bytes` that is a valid char boundary.
fn truncate_to_char_boundary(s: &str, max_bytes: usize) -> usize {
    if max_bytes >= s.len() {
        return s.len();
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    end
}
