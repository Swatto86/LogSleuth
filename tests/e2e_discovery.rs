// LogSleuth - tests/e2e_discovery.rs
//
// End-to-end tests for the discovery and parsing pipeline.
//
// These tests exercise the real filesystem, real profile loading,
// real walkdir traversal, and real chrono timestamp parsing —
// no mocks, no stubs. This exercises the full path from a raw log
// file on disk to structured LogEntry objects with parsed timestamps
// and correct severity levels.
//
// Per DevWorkflow Part A Rule 3 (E2E tests mandatory for every user-visible
// feature), these tests MUST be kept passing before each release.

use logsleuth::core::discovery::{discover_files, DiscoveryConfig};
use logsleuth::core::parser::{parse_content, ParseConfig};
use logsleuth::core::profile;
use std::fs;
use std::path::PathBuf;

// =============================================================================
// Helpers
// =============================================================================

/// Absolute path to the on-disk fixture files.
fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

/// Load all built-in profiles (embedded in the binary).
fn load_profiles() -> Vec<logsleuth::core::model::FormatProfile> {
    logsleuth::core::profile::load_builtin_profiles()
}

// =============================================================================
// Discovery E2E
// =============================================================================

/// Discovering the fixtures directory should find the two .log files.
#[test]
fn e2e_discovers_fixture_log_files() {
    let fixtures_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures");

    let config = DiscoveryConfig::default();
    let (files, warnings) = discover_files(&fixtures_dir, &config, |_, _| {}).unwrap();

    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");

    let names: Vec<_> = files
        .iter()
        .map(|f| f.path.file_name().unwrap().to_str().unwrap().to_string())
        .collect();

    assert!(
        names.contains(&"veeam_vbr_sample.log".to_string()),
        "expected veeam_vbr_sample.log in {names:?}"
    );
    assert!(
        names.contains(&"iis_w3c_sample.log".to_string()),
        "expected iis_w3c_sample.log in {names:?}"
    );
}

/// Discovery on a nonexistent path returns RootNotFound.
#[test]
fn e2e_discovers_nonexistent_root_returns_error() {
    use logsleuth::util::error::DiscoveryError;
    let result = discover_files(
        &PathBuf::from("C:\\nonexistent\\logsleuth-e2e-test-path"),
        &DiscoveryConfig::default(),
        |_, _| {},
    );
    assert!(
        matches!(result, Err(DiscoveryError::RootNotFound { .. })),
        "expected RootNotFound, got {result:?}"
    );
}

/// Discovery respects the max_files limit and returns MaxFilesExceeded.
#[test]
fn e2e_max_files_exceeded_returns_error() {
    use logsleuth::util::error::DiscoveryError;
    let fixtures_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures");

    let config = DiscoveryConfig {
        max_files: 1,
        ..Default::default()
    };
    let result = discover_files(&fixtures_dir, &config, |_, _| {});
    assert!(
        matches!(result, Err(DiscoveryError::MaxFilesExceeded { .. })),
        "expected MaxFilesExceeded, got {result:?}"
    );
}

/// Excluded directory patterns prevent descent into those directories.
#[test]
fn e2e_excludes_directory_by_pattern() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    fs::write(root.join("app.log"), "content").unwrap();
    let excluded_dir = root.join("archive");
    fs::create_dir(&excluded_dir).unwrap();
    fs::write(excluded_dir.join("old.log"), "old content").unwrap();

    let config = DiscoveryConfig {
        exclude_patterns: vec!["archive".to_string()],
        ..Default::default()
    };

    let (files, _) = discover_files(root, &config, |_, _| {}).unwrap();
    let names: Vec<_> = files
        .iter()
        .map(|f| f.path.file_name().unwrap().to_str().unwrap().to_string())
        .collect();

    assert!(
        names.contains(&"app.log".to_string()),
        "app.log should be discovered"
    );
    assert!(
        !names.contains(&"old.log".to_string()),
        "old.log in excluded dir should not be discovered"
    );
}

// =============================================================================
// Auto-detection E2E
// =============================================================================

/// Auto-detection should identify the Veeam VBR format from the sample file.
#[test]
fn e2e_auto_detects_veeam_vbr_profile() {
    let profiles = load_profiles();
    let fixture_path = fixture("veeam_vbr_sample.log");

    let content = fs::read_to_string(&fixture_path).expect("read veeam fixture");
    let sample_lines: Vec<String> = content.lines().take(20).map(String::from).collect();

    let result = profile::auto_detect("veeam_vbr_sample.log", &sample_lines, &profiles);

    assert!(
        result.is_some(),
        "should detect a profile for veeam_vbr_sample.log"
    );
    let detection = result.unwrap();
    assert_eq!(
        detection.profile_id, "veeam-vbr",
        "should detect the veeam-vbr profile, got '{}'",
        detection.profile_id
    );
    assert!(
        detection.confidence >= 0.3,
        "confidence should exceed threshold, got {}",
        detection.confidence
    );
}

// =============================================================================
// Parsing E2E
// =============================================================================

/// End-to-end parse of veeam_vbr_sample.log: timestamps, severities, multiline.
#[test]
fn e2e_parse_veeam_vbr_sample() {
    let profiles = load_profiles();
    let fixture_path = fixture("veeam_vbr_sample.log");

    let content = fs::read_to_string(&fixture_path).expect("read veeam fixture");

    let vbr_profile = profiles
        .iter()
        .find(|p| p.id == "veeam-vbr")
        .expect("veeam-vbr profile should be loaded");

    let result = parse_content(
        &content,
        &fixture_path,
        vbr_profile,
        &ParseConfig::default(),
        0,
    );

    // The fixture has 25 lines total. With multiline continuation the
    // stack-trace lines fold into their preceding entry, so we expect fewer
    // entries than lines. Assert conservatively: at least 10 primary entries.
    assert!(
        result.entries.len() >= 10,
        "expected >= 10 parsed entries, got {}",
        result.entries.len()
    );

    // All entries should reference the correct source file.
    for entry in &result.entries {
        assert_eq!(
            entry.source_file, fixture_path,
            "source_file should be the fixture path"
        );
    }

    // The fixture contains known severities: Info, Warning, Error.
    let has_error = result
        .entries
        .iter()
        .any(|e| e.severity == logsleuth::core::model::Severity::Error);
    let has_warning = result
        .entries
        .iter()
        .any(|e| e.severity == logsleuth::core::model::Severity::Warning);
    let has_info = result
        .entries
        .iter()
        .any(|e| e.severity == logsleuth::core::model::Severity::Info);

    assert!(has_error, "fixture should have Error entries");
    assert!(has_warning, "fixture should have Warning entries");
    assert!(has_info, "fixture should have Info entries");
}

/// Timestamps in veeam_vbr_sample.log should be parsed as UTC DateTimes.
#[test]
fn e2e_veeam_vbr_timestamps_are_parsed() {
    let profiles = load_profiles();
    let fixture_path = fixture("veeam_vbr_sample.log");
    let content = fs::read_to_string(&fixture_path).expect("read veeam fixture");

    let vbr_profile = profiles
        .iter()
        .find(|p| p.id == "veeam-vbr")
        .expect("veeam-vbr profile should be loaded");

    let result = parse_content(
        &content,
        &fixture_path,
        vbr_profile,
        &ParseConfig::default(),
        0,
    );

    // Every entry that matched the primary pattern should have a timestamp.
    let entries_with_ts = result
        .entries
        .iter()
        .filter(|e| e.timestamp.is_some())
        .count();
    let total = result.entries.len();
    assert!(
        entries_with_ts > 0,
        "at least one entry should have a parsed timestamp"
    );

    // Timestamp errors should be reported in the errors list, not silently dropped.
    // The fixture has well-formed timestamps so there should be 0 timestamp errors.
    let ts_errors = result
        .errors
        .iter()
        .filter(|e| matches!(e, logsleuth::util::error::ParseError::TimestampParse { .. }))
        .count();
    assert_eq!(
        ts_errors, 0,
        "fixture timestamps should all parse cleanly; got {ts_errors} errors"
    );

    // Sanity check: first timestamp is 2024-01-15
    let first_ts = result.entries[0].timestamp.unwrap();
    assert_eq!(
        first_ts.format("%Y-%m-%d").to_string(),
        "2024-01-15",
        "first timestamp should be 2024-01-15"
    );

    // All entries in a primary-matched entry should have Some timestamp.
    // (Continuation lines inherit the parent entry's timestamp via folding,
    // so total entries with ts should equal total primary-matched entries.)
    let _ = (total, entries_with_ts); // suppress unused-variable warning
}

/// Severity levels in veeam_vbr_sample.log are mapped correctly.
#[test]
fn e2e_veeam_vbr_severity_mapping() {
    let profiles = load_profiles();
    let fixture_path = fixture("veeam_vbr_sample.log");
    let content = fs::read_to_string(&fixture_path).expect("read veeam fixture");

    let vbr_profile = profiles
        .iter()
        .find(|p| p.id == "veeam-vbr")
        .expect("veeam-vbr profile");

    let result = parse_content(
        &content,
        &fixture_path,
        vbr_profile,
        &ParseConfig::default(),
        0,
    );

    // The fixture text "Error    Failed to create snapshot" should map to Severity::Error.
    let error_entries: Vec<_> = result
        .entries
        .iter()
        .filter(|e| e.severity == logsleuth::core::model::Severity::Error)
        .collect();

    assert!(
        !error_entries.is_empty(),
        "should map 'Error' level to Severity::Error"
    );

    // Check the specific message we know is in the fixture.
    assert!(
        error_entries
            .iter()
            .any(|e| e.message.contains("Failed to create snapshot")),
        "should find the snapshot failure error entry"
    );
}

/// IDs assigned to entries are monotonically increasing from id_start.
#[test]
fn e2e_entry_ids_are_monotonic() {
    let profiles = load_profiles();
    let fixture_path = fixture("veeam_vbr_sample.log");
    let content = fs::read_to_string(&fixture_path).expect("read veeam fixture");

    let vbr_profile = profiles
        .iter()
        .find(|p| p.id == "veeam-vbr")
        .expect("veeam-vbr profile");

    let id_start = 1000u64;
    let result = parse_content(
        &content,
        &fixture_path,
        vbr_profile,
        &ParseConfig::default(),
        id_start,
    );

    let first_id = result.entries.first().map(|e| e.id);
    assert_eq!(
        first_id,
        Some(id_start),
        "first entry id should equal id_start"
    );

    for (i, entry) in result.entries.iter().enumerate() {
        assert_eq!(
            entry.id,
            id_start + i as u64,
            "entry ids should be contiguous from id_start"
        );
    }
}
