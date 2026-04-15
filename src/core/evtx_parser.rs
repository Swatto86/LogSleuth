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
    RE.get_or_init(|| Regex::new(r#"<Provider[^>]*\bName=(?:'([^']+)'|"([^"]+)")"#).unwrap())
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
    RE.get_or_init(|| Regex::new(r#"ProcessID=(?:'(\d+)'|"(\d+)")"#).unwrap())
}

/// Extract `ThreadID='NNN'` from the Execution element.
fn thread_id_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"ThreadID=(?:'(\d+)'|"(\d+)")"#).unwrap())
}

/// Extract `<Data Name='key'>value</Data>` pairs from EventData.
/// Supports both single-quoted and double-quoted Name attributes and
/// unnamed `<Data>value</Data>` elements.
fn event_data_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"<Data(?:\s+Name=(?:'([^']*)'|"([^"]*)"))?[^>]*>([^<]*)</Data>"#).unwrap()
    })
}

/// Extract rendered message text if available under `<RenderingInfo>`.
fn rendered_message_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?s)<RenderingInfo[^>]*>.*?<Message>(.*?)</Message>").unwrap())
}

/// Extract the `<UserData>...</UserData>` block when present.
fn user_data_block_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?s)<UserData[^>]*>(.*?)</UserData>").unwrap())
}

/// Extract simple `<Field>value</Field>` items from a UserData block.
fn user_data_field_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?s)<([A-Za-z0-9_:\.-]+)[^>]*>([^<]+)</([A-Za-z0-9_:\.-]+)>"#).unwrap()
    })
}

/// Remove XML tags for last-resort plain text fallback.
fn xml_tag_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?s)<[^>]+>").unwrap())
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
    re.captures(xml).and_then(|caps| {
        caps.iter()
            .skip(1)
            .flatten()
            .map(|m| m.as_str().trim())
            .find(|s| !s.is_empty())
            .map(|s| s.to_string())
    })
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
    if let Some(rendered) = extract_match(rendered_message_re(), xml) {
        let rendered = normalise_whitespace(&decode_xml_entities(&rendered));
        if !rendered.is_empty() {
            return rendered;
        }
    }

    let mut parts = Vec::with_capacity(6);

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

    // Extract structured payload fields for a compact summary.
    let event_data_summary = extract_event_data(xml);
    let user_data_summary = extract_user_data(xml);
    let mut payload_parts = Vec::with_capacity(2);
    if !event_data_summary.is_empty() {
        payload_parts.push(event_data_summary);
    }
    if !user_data_summary.is_empty() {
        payload_parts.push(user_data_summary);
    }
    if !payload_parts.is_empty() {
        parts.push(payload_parts.join(" | "));
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
    let mut unnamed_counter = 1usize;
    let pairs: Vec<String> = event_data_re()
        .captures_iter(xml)
        .take(constants::EVTX_MAX_DATA_PAIRS)
        .map(|cap| {
            let key = cap
                .get(1)
                .or_else(|| cap.get(2))
                .map(|m| m.as_str().trim().to_string())
                .filter(|k| !k.is_empty())
                .unwrap_or_else(|| {
                    let generated_key = format!("Data{unnamed_counter}");
                    unnamed_counter += 1;
                    generated_key
                });
            let field_value =
                normalise_whitespace(&decode_xml_entities(cap.get(3).map_or("", |m| m.as_str())));
            format!("{key}={field_value}")
        })
        .filter(|kv| !kv.ends_with('='))
        .collect();

    pairs.join(", ")
}

/// Extract key=value pairs from the `<UserData>` section.
///
/// Many Windows operational logs place their human-relevant fields under
/// `UserData` instead of `EventData`, so this acts as an important fallback.
fn extract_user_data(xml: &str) -> String {
    let Some(block) = extract_match(user_data_block_re(), xml) else {
        return String::new();
    };

    let mut pairs: Vec<String> = Vec::new();
    for cap in user_data_field_re()
        .captures_iter(block.as_str())
        .take(constants::EVTX_MAX_DATA_PAIRS)
    {
        let key_open = cap.get(1).map_or("", |m| m.as_str());
        let key_close = cap.get(3).map_or("", |m| m.as_str());
        if key_open != key_close {
            continue;
        }
        let key = key_open.rsplit(':').next().unwrap_or(key_open).trim();
        let field_value =
            normalise_whitespace(&decode_xml_entities(cap.get(2).map_or("", |m| m.as_str())));
        if !key.is_empty() && !field_value.is_empty() {
            pairs.push(format!("{key}={field_value}"));
        }
    }

    if !pairs.is_empty() {
        return pairs.join(", ");
    }

    // Final fallback: strip tags and return any remaining text content.
    let plain = normalise_whitespace(&decode_xml_entities(&xml_tag_re().replace_all(&block, " ")));
    plain
}

/// Decode a small subset of XML entities commonly seen in rendered fields.
fn decode_xml_entities(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// Collapse runs of whitespace/newlines into a single space for timeline rows.
fn normalise_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
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
