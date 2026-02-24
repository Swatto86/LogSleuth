// LogSleuth - core/parser.rs
//
// Stream-oriented log file parsing using format profiles.
// Core layer: accepts Read trait objects, never touches filesystem directly.
//
// Implementation: next increment.

use crate::core::model::{FormatProfile, LogEntry};
use crate::util::error::ParseError;
use std::path::PathBuf;

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
    file_path: &PathBuf,
    profile: &FormatProfile,
    config: &ParseConfig,
    id_start: u64,
) -> ParseResult {
    // TODO: Implement full streaming parser in next increment.
    // Stub: demonstrate the pattern with basic line-by-line parsing.
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

            // Extract severity: from explicit 'level' field or infer from message
            let severity = if let Some(level_match) = caps.name("level") {
                profile.map_severity(level_match.as_str())
            } else {
                profile.infer_severity_from_message(&message)
            };

            let entry = LogEntry {
                id: current_id,
                timestamp: None, // TODO: parse timestamp in next increment
                severity,
                source_file: file_path.clone(),
                line_number,
                thread: caps.name("thread").map(|m| m.as_str().to_string()),
                component: caps.name("component").map(|m| m.as_str().to_string()),
                message,
                raw_text: line.to_string(),
                profile_id: profile.id.clone(),
            };

            entries.push(entry);
            current_id += 1;
        } else {
            // Line does not match the pattern
            match profile.multiline_mode {
                crate::core::model::MultilineMode::Continuation => {
                    // Append to previous entry if one exists
                    if let Some(last) = entries.last_mut() {
                        last.message.push('\n');
                        last.message.push_str(line);
                        last.raw_text.push('\n');
                        last.raw_text.push_str(line);
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
                        source_file: file_path.clone(),
                        line_number,
                        thread: None,
                        component: None,
                        message: line.to_string(),
                        raw_text: line.to_string(),
                        profile_id: profile.id.clone(),
                    });
                    current_id += 1;
                }
            }

            // Track as parse error if this is a new line (not continuation)
            if errors.len() < config.max_parse_errors_per_file {
                // Only count as error if not handled by multiline
                if profile.multiline_mode != crate::core::model::MultilineMode::Continuation
                    || entries.is_empty()
                {
                    errors.push(ParseError::LineParse {
                        file: file_path.clone(),
                        line_number,
                        reason: "Line does not match profile pattern".to_string(),
                    });
                }
            }
        }

        // Enforce max entry size on the last entry
        if let Some(last) = entries.last_mut() {
            if last.message.len() > config.max_entry_size {
                last.message.truncate(config.max_entry_size);
                last.message.push_str("... [truncated]");
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::Severity;
    use crate::core::profile;

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

        let result = parse_content(
            &content,
            &PathBuf::from("big.log"),
            &profile,
            &config,
            0,
        );

        assert_eq!(result.entries.len(), 1);
        assert!(result.entries[0].message.len() < 1100); // truncated + suffix
        assert!(result.entries[0].message.ends_with("... [truncated]"));
    }
}
