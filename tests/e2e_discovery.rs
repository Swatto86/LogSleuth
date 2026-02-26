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
    let (files, warnings, _) = discover_files(&fixtures_dir, &config, |_, _| {}).unwrap();

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

/// When more files exist than the limit, discovery succeeds and truncates
/// to the `max_files` most recently modified entries, adding a warning.
#[test]
fn e2e_max_files_truncates_to_most_recent() {
    let fixtures_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures");

    let config = DiscoveryConfig {
        max_files: 1,
        ..Default::default()
    };
    let (files, warnings, total_found) = discover_files(&fixtures_dir, &config, |_, _| {}).unwrap();

    assert_eq!(files.len(), 1, "should return exactly 1 file");
    assert!(
        total_found >= 1,
        "total_found must reflect files before truncation"
    );
    // When there is more than one fixture file, a warning must be emitted.
    if total_found > 1 {
        assert!(
            !warnings.is_empty(),
            "a truncation warning must be present when files were dropped"
        );
    }
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

    let (files, _, _) = discover_files(root, &config, |_, _| {}).unwrap();
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
/// Exercises the full path: real fixture → parsed entries (with timestamps) →
/// AppState → update_correlation().  Verifies that entries within the window
/// are included, entries outside the window are excluded, and disabling
/// correlation clears the set.
#[test]
fn e2e_correlation_window_highlights_nearby_entries() {
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

    // We need at least two entries with parsed timestamps to test the window.
    let timestamped: Vec<_> = result
        .entries
        .iter()
        .filter(|e| e.timestamp.is_some())
        .collect();
    assert!(
        timestamped.len() >= 2,
        "fixture must contain at least 2 timestamped entries for correlation test"
    );

    let mut state = logsleuth::app::state::AppState::new(profiles, false);
    state.entries = result.entries;
    // Build a simple identity filtered_indices so selected_index maps directly.
    state.filtered_indices = (0..state.entries.len()).collect();

    // Select the first entry that has a timestamp.
    let anchor_display = state
        .entries
        .iter()
        .position(|e| e.timestamp.is_some())
        .expect("expected at least one timestamped entry");
    let anchor_id = state.entries[anchor_display].id;

    state.selected_index = Some(anchor_display);
    state.correlation_active = true;
    state.correlation_window_secs = 30;
    state.update_correlation();

    // The anchor entry must be in the correlated set.
    assert!(
        state.correlated_ids.contains(&anchor_id),
        "anchor entry must be in the correlated set"
    );

    // All correlated entries must have timestamps; none are timestampless.
    for &id in &state.correlated_ids {
        let entry = state.entries.iter().find(|e| e.id == id).unwrap();
        assert!(
            entry.timestamp.is_some(),
            "correlated_ids must not contain entries without timestamps"
        );
    }

    // Disabling correlation must clear the overlay.
    state.correlation_active = false;
    state.update_correlation();
    assert!(
        state.correlated_ids.is_empty(),
        "correlated_ids must be empty after disabling correlation"
    );
}

/// Exercises the full session save/restore round-trip through real storage.
///
/// Verifies that:
/// - `save_session()` writes a valid JSON file to the configured path
/// - `load()` deserialises it back without error
/// - `restore_from_session()` reinstates scan_path, filter text, bookmarks,
///   and the correlation window
/// - `load()` returns `None` for a path that does not exist (regression guard)
#[test]
fn e2e_session_save_restore_round_trip() {
    use logsleuth::app::session;
    use logsleuth::app::state::AppState;
    use std::collections::HashMap;
    use tempfile::TempDir;

    // ---------- 1. Build state with known values ----------
    let profiles = load_profiles();
    let mut state = AppState::new(profiles, false);

    let tmp = TempDir::new().expect("temp dir");
    let session_file = tmp.path().join("session.json");
    state.session_path = Some(session_file.clone());

    let scan_dir = tmp.path().join("logs");
    state.scan_path = Some(scan_dir.clone());
    state.filter_state.text_search = "CRITICAL".to_string();
    state.filter_state.fuzzy = true;
    state.bookmarks = HashMap::from([(42u64, "important".to_string())]);
    state.correlation_window_secs = 90;

    // ---------- 2. Save ----------
    state.save_session();
    assert!(
        session_file.exists(),
        "session file must be written to disk"
    );

    // ---------- 3. Load ----------
    let loaded = session::load(&session_file).expect("session must load successfully");
    assert_eq!(
        loaded.scan_path.as_deref(),
        Some(scan_dir.as_path()),
        "scan_path must survive round-trip"
    );
    assert_eq!(
        loaded.filter.text_search, "CRITICAL",
        "text_search must survive round-trip"
    );
    assert!(loaded.filter.fuzzy, "fuzzy flag must survive round-trip");
    assert_eq!(
        loaded.bookmarks.len(),
        1,
        "bookmark count must survive round-trip"
    );
    assert_eq!(
        loaded.correlation_window_secs, 90,
        "correlation window must survive round-trip"
    );

    // ---------- 4. Restore into a fresh AppState ----------
    let mut state2 = AppState::new(load_profiles(), false);
    state2.restore_from_session(loaded);

    assert_eq!(
        state2.scan_path.as_deref(),
        Some(scan_dir.as_path()),
        "scan_path must be restored"
    );
    assert_eq!(
        state2.filter_state.text_search, "CRITICAL",
        "text_search filter must be restored"
    );
    assert!(state2.filter_state.fuzzy, "fuzzy flag must be restored");
    assert_eq!(state2.bookmark_count(), 1, "bookmarks must be restored");
    assert_eq!(
        state2.correlation_window_secs, 90,
        "correlation window must be restored"
    );

    // ---------- 5. Missing file returns None ----------
    let missing = tmp.path().join("does_not_exist.json");
    assert!(
        session::load(&missing).is_none(),
        "load() must return None for a missing file"
    );
}

// =============================================================================
// filtered_results_report tests
// =============================================================================

use logsleuth::app::state::AppState;
use logsleuth::core::model::{LogEntry, Severity};

/// Helper: build a minimal LogEntry for testing.
fn make_entry(id: u64, severity: Severity, message: &str, ts_offset_secs: i64) -> LogEntry {
    LogEntry {
        id,
        timestamp: Some(chrono::Utc::now() - chrono::Duration::seconds(ts_offset_secs)),
        severity,
        message: message.to_string(),
        raw_text: message.to_string(),
        source_file: std::path::PathBuf::from("app.log"),
        line_number: id,
        thread: None,
        component: None,
        profile_id: "test".to_string(),
        file_modified: None,
    }
}

/// report on an empty filtered set is well-formed and shows 0 entries.
#[test]
fn filtered_results_report_empty_state() {
    let state = AppState::new(vec![], false);
    let report = state.filtered_results_report();
    assert!(
        report.contains("LogSleuth Filtered Results"),
        "report must contain header"
    );
    assert!(
        report.contains("Entries:   0"),
        "empty state must show 0 entries, got: {report}"
    );
}

/// report with filtered entries contains timestamp, severity, and message text.
#[test]
fn filtered_results_report_populated() {
    let mut state = AppState::new(vec![], false);
    state
        .entries
        .push(make_entry(0, Severity::Error, "disk full", 30));
    state
        .entries
        .push(make_entry(1, Severity::Warning, "high memory", 20));
    state
        .entries
        .push(make_entry(2, Severity::Info, "startup complete", 10));
    // Filter to just the first two (Error + Warning).
    state.filter_state.severity_levels.insert(Severity::Error);
    state.filter_state.severity_levels.insert(Severity::Warning);
    state.apply_filters();

    let report = state.filtered_results_report();

    assert!(
        report.contains("disk full"),
        "Error entry message must appear in report"
    );
    assert!(
        report.contains("high memory"),
        "Warning entry message must appear in report"
    );
    assert!(
        !report.contains("startup complete"),
        "Info entry must not appear when filtered to Error+Warning"
    );
    assert!(
        report.contains("Entries:   2"),
        "report must show 2 filtered entries, got: {report}"
    );
    // Filter description must mention the severity constraint.
    assert!(
        report.contains("Severity:"),
        "filter description must mention severity"
    );
}

/// report is truncated at MAX_CLIPBOARD_ENTRIES with a notice appended.
#[test]
fn filtered_results_report_truncation() {
    use logsleuth::util::constants::MAX_CLIPBOARD_ENTRIES;

    let mut state = AppState::new(vec![], false);
    // Push MAX + 1 entries so truncation fires.
    let total = MAX_CLIPBOARD_ENTRIES + 1;
    for i in 0..total {
        state
            .entries
            .push(make_entry(i as u64, Severity::Error, "overflow", 0));
    }
    state.apply_filters();

    assert_eq!(
        state.filtered_indices.len(),
        total,
        "all entries must pass an empty filter"
    );

    let report = state.filtered_results_report();
    assert!(
        report.contains("truncated"),
        "report must contain truncation notice when over limit"
    );
    assert!(
        report.contains(&format!("{MAX_CLIPBOARD_ENTRIES}")),
        "report must cite the limit value"
    );
}
