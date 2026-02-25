// LogSleuth - core/profile.rs
//
// Format profile loading, validation, and auto-detection.
// Core layer: accepts TOML strings and file content, never touches the filesystem.
// I/O is handled by the app::profile_mgr which feeds content here.

use crate::core::model::{FormatProfile, MultilineMode, Severity};
use crate::util::constants;
use crate::util::error::ProfileError;
use regex::Regex;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;

// =============================================================================
// TOML deserialization structures (raw input)
// =============================================================================

/// Raw TOML profile definition as deserialized from a .toml file.
/// This is validated and compiled into a `FormatProfile` for runtime use.
#[derive(Debug, Deserialize)]
pub struct ProfileDefinition {
    pub profile: ProfileMeta,
    pub detection: DetectionDef,
    pub parsing: ParsingDef,
    #[serde(default)]
    pub severity_mapping: SeverityMappingDef,
}

#[derive(Debug, Deserialize)]
pub struct ProfileMeta {
    pub id: String,
    pub name: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default)]
    pub description: String,
}

fn default_version() -> String {
    "1.0".to_string()
}

#[derive(Debug, Deserialize)]
pub struct DetectionDef {
    #[serde(default)]
    pub file_patterns: Vec<String>,
    pub content_match: String,
}

#[derive(Debug, Deserialize)]
pub struct ParsingDef {
    pub line_pattern: String,
    pub timestamp_format: String,
    #[serde(default)]
    pub multiline_mode: MultilineMode,
}

#[derive(Debug, Deserialize, Default)]
pub struct SeverityMappingDef {
    #[serde(default)]
    pub critical: Vec<String>,
    #[serde(default)]
    pub error: Vec<String>,
    #[serde(default)]
    pub warning: Vec<String>,
    #[serde(default)]
    pub info: Vec<String>,
    #[serde(default)]
    pub debug: Vec<String>,
}

// =============================================================================
// Profile validation and compilation
// =============================================================================

/// Parse a TOML string into a `ProfileDefinition`.
///
/// `source_path` is used for error messages only (not for I/O).
pub fn parse_profile_toml(
    toml_content: &str,
    source_path: &PathBuf,
) -> Result<ProfileDefinition, ProfileError> {
    toml::from_str(toml_content).map_err(|e| ProfileError::TomlParse {
        path: source_path.clone(),
        source: e,
    })
}

/// Validate a `ProfileDefinition` and compile it into a runtime `FormatProfile`.
///
/// Validates:
/// - Required fields are present and non-empty
/// - Regex patterns are valid and within size limits
/// - Timestamp format is plausible
///
/// Returns a fully compiled `FormatProfile` ready for use.
pub fn validate_and_compile(
    def: ProfileDefinition,
    source_path: &PathBuf,
    is_builtin: bool,
) -> Result<FormatProfile, ProfileError> {
    let id = &def.profile.id;

    // Validate required fields
    if id.is_empty() {
        return Err(ProfileError::MissingField {
            profile_id: "(empty)".to_string(),
            field: "profile.id",
        });
    }
    if def.profile.name.is_empty() {
        return Err(ProfileError::MissingField {
            profile_id: id.clone(),
            field: "profile.name",
        });
    }
    if def.detection.content_match.is_empty() {
        return Err(ProfileError::MissingField {
            profile_id: id.clone(),
            field: "detection.content_match",
        });
    }
    if def.parsing.line_pattern.is_empty() {
        return Err(ProfileError::MissingField {
            profile_id: id.clone(),
            field: "parsing.line_pattern",
        });
    }
    if def.parsing.timestamp_format.is_empty() {
        return Err(ProfileError::MissingField {
            profile_id: id.clone(),
            field: "parsing.timestamp_format",
        });
    }

    // Validate and compile content_match regex
    let content_match = compile_regex(id, "detection.content_match", &def.detection.content_match)?;

    // Validate and compile line_pattern regex
    let line_pattern = compile_regex(id, "parsing.line_pattern", &def.parsing.line_pattern)?;

    // Validate line_pattern has at least a 'message' capture group
    let capture_names: Vec<&str> = line_pattern.capture_names().flatten().collect();

    if !capture_names.contains(&"message") {
        tracing::warn!(
            profile_id = id,
            source = %source_path.display(),
            "Profile line_pattern has no 'message' capture group; \
             entire match will be used as message"
        );
    }

    // Build severity mapping
    let mut severity_mapping = HashMap::new();
    if !def.severity_mapping.critical.is_empty() {
        severity_mapping.insert(Severity::Critical, def.severity_mapping.critical);
    }
    if !def.severity_mapping.error.is_empty() {
        severity_mapping.insert(Severity::Error, def.severity_mapping.error);
    }
    if !def.severity_mapping.warning.is_empty() {
        severity_mapping.insert(Severity::Warning, def.severity_mapping.warning);
    }
    if !def.severity_mapping.info.is_empty() {
        severity_mapping.insert(Severity::Info, def.severity_mapping.info);
    }
    if !def.severity_mapping.debug.is_empty() {
        severity_mapping.insert(Severity::Debug, def.severity_mapping.debug);
    }

    Ok(FormatProfile {
        id: id.clone(),
        name: def.profile.name,
        version: def.profile.version,
        description: def.profile.description,
        file_patterns: def.detection.file_patterns,
        content_match,
        line_pattern,
        timestamp_format: def.parsing.timestamp_format,
        multiline_mode: def.parsing.multiline_mode,
        severity_mapping,
        is_builtin,
    })
}

/// Compile a regex pattern with length validation to prevent ReDoS.
fn compile_regex(
    profile_id: &str,
    field: &'static str,
    pattern: &str,
) -> Result<Regex, ProfileError> {
    if pattern.len() > constants::MAX_REGEX_PATTERN_LENGTH {
        return Err(ProfileError::RegexTooLong {
            profile_id: profile_id.to_string(),
            field,
            length: pattern.len(),
            max_length: constants::MAX_REGEX_PATTERN_LENGTH,
        });
    }

    Regex::new(pattern).map_err(|e| ProfileError::InvalidRegex {
        profile_id: profile_id.to_string(),
        field,
        pattern: pattern.to_string(),
        source: e,
    })
}

// =============================================================================
// Auto-detection
// =============================================================================

/// Result of attempting to auto-detect a file's format.
#[derive(Debug, Clone)]
pub struct DetectionResult {
    /// Profile ID of the best match.
    pub profile_id: String,
    /// Confidence score (0.0 - 1.0). Ratio of lines matching content_match.
    pub confidence: f64,
}

/// Attempt to auto-detect the format of a file by sampling its first lines.
///
/// Tests each profile's `content_match` regex against the sample lines.
/// Returns the profile with the highest match ratio, or None if no profile
/// exceeds the minimum confidence threshold.
///
/// For profiles with `file_patterns`, a filename match adds a bonus to confidence.
pub fn auto_detect(
    file_name: &str,
    sample_lines: &[String],
    profiles: &[FormatProfile],
) -> Option<DetectionResult> {
    if sample_lines.is_empty() || profiles.is_empty() {
        return None;
    }

    let mut best: Option<DetectionResult> = None;

    for profile in profiles {
        // Skip the plain-text fallback; it matches everything
        if profile.id == "plain-text" {
            continue;
        }

        // Count how many sample lines match the content_match regex
        let matches = sample_lines
            .iter()
            .filter(|line| profile.content_match.is_match(line))
            .count();

        let mut confidence = matches as f64 / sample_lines.len() as f64;

        // Bonus for filename pattern match (adds up to 0.2)
        let filename_match = profile.file_patterns.iter().any(|pattern| {
            glob::Pattern::new(pattern)
                .map(|p| p.matches(file_name))
                .unwrap_or(false)
        });
        if filename_match {
            confidence = (confidence + 0.2).min(1.0);
        }

        if confidence >= constants::AUTO_DETECT_MIN_CONFIDENCE {
            if best.as_ref().map_or(true, |b| confidence > b.confidence) {
                best = Some(DetectionResult {
                    profile_id: profile.id.clone(),
                    confidence,
                });
            }
        }
    }

    tracing::debug!(
        file = file_name,
        result = ?best,
        "Auto-detection complete"
    );

    best
}

// =============================================================================
// Built-in profiles (embedded at compile time)
// =============================================================================

/// Embedded TOML content for built-in profiles.
/// Each tuple is (filename, TOML content).
pub fn builtin_profile_sources() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "veeam_vbr.toml",
            include_str!("../../profiles/veeam_vbr.toml"),
        ),
        (
            "veeam_vbo365.toml",
            include_str!("../../profiles/veeam_vbo365.toml"),
        ),
        ("iis_w3c.toml", include_str!("../../profiles/iis_w3c.toml")),
        (
            "syslog_rfc3164.toml",
            include_str!("../../profiles/syslog_rfc3164.toml"),
        ),
        (
            "syslog_rfc5424.toml",
            include_str!("../../profiles/syslog_rfc5424.toml"),
        ),
        (
            "json_lines.toml",
            include_str!("../../profiles/json_lines.toml"),
        ),
        (
            "log4j_default.toml",
            include_str!("../../profiles/log4j_default.toml"),
        ),
        (
            "generic_timestamp.toml",
            include_str!("../../profiles/generic_timestamp.toml"),
        ),
        (
            "plain_text.toml",
            include_str!("../../profiles/plain_text.toml"),
        ),
    ]
}

/// Load and validate all built-in profiles.
///
/// Invalid profiles are logged as warnings and skipped (non-fatal).
/// Returns the successfully loaded profiles.
pub fn load_builtin_profiles() -> Vec<FormatProfile> {
    let mut profiles = Vec::new();
    let mut errors = Vec::new();

    for (filename, content) in builtin_profile_sources() {
        let path = PathBuf::from(format!("<builtin>/{filename}"));
        match parse_profile_toml(content, &path)
            .and_then(|def| validate_and_compile(def, &path, true))
        {
            Ok(profile) => {
                tracing::debug!(profile_id = %profile.id, "Loaded built-in profile");
                profiles.push(profile);
            }
            Err(e) => {
                // Built-in profile failures are bugs, but we still degrade gracefully
                tracing::error!(file = filename, error = %e, "Failed to load built-in profile");
                errors.push(e);
            }
        }
    }

    if !errors.is_empty() {
        tracing::warn!(
            count = errors.len(),
            "Some built-in profiles failed to load"
        );
    }

    profiles
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_PROFILE_TOML: &str = r#"
[profile]
id = "test-profile"
name = "Test Profile"
version = "1.0"
description = "A test profile"

[detection]
file_patterns = ["test*.log"]
content_match = '^\[\d{4}-\d{2}-\d{2}'

[parsing]
line_pattern = '^(?P<timestamp>\d{4}-\d{2}-\d{2}\s\d{2}:\d{2}:\d{2})\s(?P<level>\w+)\s+(?P<message>.+)$'
timestamp_format = "%Y-%m-%d %H:%M:%S"
multiline_mode = "continuation"

[severity_mapping]
error = ["Error", "ERR"]
warning = ["Warning", "WARN"]
info = ["Info", "INFO"]
"#;

    #[test]
    fn test_parse_valid_profile() {
        let path = PathBuf::from("test.toml");
        let def = parse_profile_toml(VALID_PROFILE_TOML, &path).unwrap();
        assert_eq!(def.profile.id, "test-profile");
        assert_eq!(def.profile.name, "Test Profile");
        assert_eq!(def.detection.file_patterns, vec!["test*.log"]);
    }

    #[test]
    fn test_compile_valid_profile() {
        let path = PathBuf::from("test.toml");
        let def = parse_profile_toml(VALID_PROFILE_TOML, &path).unwrap();
        let profile = validate_and_compile(def, &path, false).unwrap();

        assert_eq!(profile.id, "test-profile");
        assert!(!profile.is_builtin);
        assert_eq!(profile.multiline_mode, MultilineMode::Continuation);
    }

    #[test]
    fn test_severity_mapping() {
        let path = PathBuf::from("test.toml");
        let def = parse_profile_toml(VALID_PROFILE_TOML, &path).unwrap();
        let profile = validate_and_compile(def, &path, false).unwrap();

        assert_eq!(profile.map_severity("Error"), Severity::Error);
        assert_eq!(profile.map_severity("ERR"), Severity::Error);
        assert_eq!(profile.map_severity("error"), Severity::Error); // case-insensitive
        assert_eq!(profile.map_severity("Warning"), Severity::Warning);
        assert_eq!(profile.map_severity("UNKNOWN_LEVEL"), Severity::Unknown);
    }

    #[test]
    fn test_missing_required_field() {
        let toml = r#"
[profile]
id = ""
name = "Empty ID"

[detection]
content_match = "test"

[parsing]
line_pattern = "(?P<message>.+)"
timestamp_format = "%Y"
"#;
        let path = PathBuf::from("bad.toml");
        let def = parse_profile_toml(toml, &path).unwrap();
        let result = validate_and_compile(def, &path, false);
        assert!(result.is_err());
        match result.unwrap_err() {
            ProfileError::MissingField { field, .. } => assert_eq!(field, "profile.id"),
            other => panic!("Expected MissingField, got: {other:?}"),
        }
    }

    #[test]
    fn test_invalid_regex() {
        let toml = r#"
[profile]
id = "bad-regex"
name = "Bad Regex"

[detection]
content_match = "[invalid"

[parsing]
line_pattern = "(?P<message>.+)"
timestamp_format = "%Y"
"#;
        let path = PathBuf::from("bad.toml");
        let def = parse_profile_toml(toml, &path).unwrap();
        let result = validate_and_compile(def, &path, false);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ProfileError::InvalidRegex { .. }
        ));
    }

    #[test]
    fn test_regex_too_long() {
        let long_pattern = "a".repeat(constants::MAX_REGEX_PATTERN_LENGTH + 1);
        let toml = format!(
            r#"
[profile]
id = "long-regex"
name = "Long Regex"

[detection]
content_match = '{long_pattern}'

[parsing]
line_pattern = "(?P<message>.+)"
timestamp_format = "%Y"
"#
        );
        let path = PathBuf::from("long.toml");
        let def = parse_profile_toml(&toml, &path).unwrap();
        let result = validate_and_compile(def, &path, false);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ProfileError::RegexTooLong { .. }
        ));
    }

    #[test]
    fn test_auto_detect_matches_best_profile() {
        let path = PathBuf::from("test.toml");
        let def = parse_profile_toml(VALID_PROFILE_TOML, &path).unwrap();
        let profile = validate_and_compile(def, &path, false).unwrap();

        let sample_lines = vec![
            "[2024-01-15 14:30:22 Error Something failed".to_string(),
            "[2024-01-15 14:30:23 Info Normal operation".to_string(),
            "Some unrelated line".to_string(),
        ];

        let result = auto_detect("test.log", &sample_lines, &[profile]);
        assert!(result.is_some());
        let det = result.unwrap();
        assert_eq!(det.profile_id, "test-profile");
        // 2/3 lines match + 0.2 filename bonus
        assert!(det.confidence > 0.5);
    }

    #[test]
    fn test_auto_detect_no_match() {
        let path = PathBuf::from("test.toml");
        let def = parse_profile_toml(VALID_PROFILE_TOML, &path).unwrap();
        let profile = validate_and_compile(def, &path, false).unwrap();

        let sample_lines = vec![
            "no match here".to_string(),
            "also no match".to_string(),
            "nothing at all".to_string(),
        ];

        let result = auto_detect("random.dat", &sample_lines, &[profile]);
        assert!(result.is_none());
    }

    #[test]
    fn test_infer_severity_from_message() {
        let path = PathBuf::from("test.toml");
        let def = parse_profile_toml(VALID_PROFILE_TOML, &path).unwrap();
        let profile = validate_and_compile(def, &path, false).unwrap();

        assert_eq!(
            profile.infer_severity_from_message("An Error occurred in module X"),
            Severity::Error
        );
        assert_eq!(
            profile.infer_severity_from_message("Warning: disk space low"),
            Severity::Warning
        );
        assert_eq!(
            profile.infer_severity_from_message("Everything is fine"),
            Severity::Info // Default when no keyword matches
        );
    }

    #[test]
    fn test_load_builtin_profiles() {
        let profiles = load_builtin_profiles();
        // All built-in profiles should load successfully
        assert!(!profiles.is_empty(), "No built-in profiles loaded");
        // Check that the Veeam VBR profile loaded
        assert!(
            profiles.iter().any(|p| p.id == "veeam-vbr"),
            "veeam-vbr profile not found"
        );
        // All should be marked as built-in
        assert!(profiles.iter().all(|p| p.is_builtin));
    }
}
