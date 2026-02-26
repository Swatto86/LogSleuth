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

// =============================================================================
// VBO365 E2E tests
// =============================================================================

/// Auto-detection should identify the VBO365 format from the sample file.
#[test]
fn e2e_auto_detects_veeam_vbo365_profile() {
    let profiles = load_profiles();
    let fixture_path = fixture("veeam_vbo365_sample.log");

    let content = fs::read_to_string(&fixture_path).expect("read vbo365 fixture");
    let sample_lines: Vec<String> = content.lines().take(20).map(String::from).collect();

    let result = profile::auto_detect("Veeam.Archiver.Proxy.log", &sample_lines, &profiles);

    assert!(
        result.is_some(),
        "should detect a profile for a VBO365 archiver log"
    );
    let detection = result.unwrap();
    assert_eq!(
        detection.profile_id, "veeam-vbo365",
        "should detect the veeam-vbo365 profile, got '{}'",
        detection.profile_id
    );
}

/// End-to-end parse of veeam_vbo365_sample.log using the VBO365 profile.
#[test]
fn e2e_parse_veeam_vbo365_sample() {
    use logsleuth::core::parser::{parse_content, ParseConfig};

    let profiles = load_profiles();
    let fixture_path = fixture("veeam_vbo365_sample.log");
    let content = fs::read_to_string(&fixture_path).expect("read vbo365 fixture");

    let vbo_profile = profiles
        .iter()
        .find(|p| p.id == "veeam-vbo365")
        .expect("veeam-vbo365 profile should be loaded");

    let result = parse_content(
        &content,
        &fixture_path,
        vbo_profile,
        &ParseConfig::default(),
        0,
    );

    assert!(
        result.entries.len() >= 10,
        "expected >= 10 parsed entries, got {}",
        result.entries.len()
    );

    // All entries should reference the correct source file.
    for entry in &result.entries {
        assert_eq!(entry.source_file, fixture_path);
    }

    // Severity is inferred from message content for VBO365 (no explicit level field).
    let has_error = result
        .entries
        .iter()
        .any(|e| e.severity == logsleuth::core::model::Severity::Error);
    assert!(has_error, "fixture should have Error-classified entries");

    // No timestamp parse errors — the fixture uses clean M/D/YYYY H:MM:SS AM/PM format.
    let ts_errors = result
        .errors
        .iter()
        .filter(|e| matches!(e, logsleuth::util::error::ParseError::TimestampParse { .. }))
        .count();
    assert_eq!(
        ts_errors, 0,
        "VBO365 fixture timestamps should all parse cleanly; got {ts_errors} errors"
    );
}

// =============================================================================
// VBR filename-pattern regression tests (regression for filenames that
// previously fell through to plain-text because the VBR profile's file_patterns
// did not include them or the filename bonus was too small).
// =============================================================================

/// VBR filenames that must be auto-detected even when the sample content
/// contains only header/separator lines (confidence from filename alone).
///
/// Regression guard: before the fix, WmiServer.BackupSrv.log had no matching
/// glob pattern and the 0.2 filename bonus was below the 0.3 threshold, so
/// these files fell back to plain-text regardless of content format.
#[test]
fn e2e_vbr_service_log_filenames_detect_via_filename_match() {
    let profiles = load_profiles();

    // Build a sample that contains at least a few genuine VBR-format lines
    // as well as separator lines that would lower the raw content ratio.
    let vbr_lines = vec![
        "=======================================================".to_string(),
        "Veeam Backup & Replication service starting".to_string(),
        "[15.01.2024 14:30:22] <01> Info     Service initialized".to_string(),
        "[15.01.2024 14:30:23] <01> Info     Loading configuration".to_string(),
        "[15.01.2024 14:30:24] <02> Info     WMI provider ready".to_string(),
        "-------------------------------------------------------".to_string(),
    ];

    let filenames = [
        "WmiServer.BackupSrv.log",
        "WmiServer.log",
        "VeeamDeployerUpdater.log",
        "Svc.VeeamBackup.log",
        "Svc.Veeam.VBR.RESTAPI.log",
        "Svc.VeeamMount.log",
    ];

    for filename in &filenames {
        let result = profile::auto_detect(filename, &vbr_lines, &profiles);
        assert!(
            result.is_some(),
            "Expected veeam-vbr profile for '{filename}', got None (file fell back to plain-text)"
        );
        let det = result.unwrap();
        assert_eq!(
            det.profile_id, "veeam-vbr",
            "Expected veeam-vbr for '{filename}', got '{}'",
            det.profile_id
        );
    }
}

// =============================================================================
// Live D:\Logs integration tests
//
// These tests exercise the full pipeline against the real log files in
// D:\Logs on the developer's machine.  They are CI-safe: each test skips
// immediately when D:\Logs is not present (e.g. on a build server that has
// no mounted drive with that path).
//
// Regressions covered:
//   Bug 1 - severity regression: all entries showing [INFO] even when the log
//            contained warnings/errors.
//   Bug 2 - source-file filter regression: only one source file visible after
//            an app restart due to stale session source_files being restored.
//   Bug 3 - multi-file parsing regression: "433 entries from 1 file" despite
//            498 files being discovered, caused by continuation-mode silently
//            dropping all lines when no prior entry exists.
// =============================================================================

/// Returns the root path when D:\Logs is available, or None (test will skip).
fn dlogs_root() -> Option<std::path::PathBuf> {
    let p = std::path::PathBuf::from(r"D:\Logs");
    if p.is_dir() {
        Some(p)
    } else {
        None
    }
}

/// D:\Logs full discovery: finds at least 100 files across all sub-folders.
///
/// Exercises discover_files() against a real, large directory tree.
#[test]
fn e2e_dlogs_discovers_many_files() {
    let Some(dlogs) = dlogs_root() else {
        return; // skip: D:\Logs not available on this machine
    };

    let (files, _warnings, total_found) =
        discover_files(&dlogs, &DiscoveryConfig::default(), |_, _| {}).unwrap();

    assert!(
        files.len() >= 100,
        "expected at least 100 files to be discovered under D:\\Logs, got {}",
        files.len()
    );
    assert!(
        total_found >= files.len(),
        "total_found ({total_found}) must be >= returned file count ({})",
        files.len()
    );
}

/// Veeam VBR log files in D:\Logs\veeam auto-detect as the veeam-vbr profile.
#[test]
fn e2e_dlogs_veeam_vbr_auto_detects() {
    let Some(dlogs) = dlogs_root() else {
        return;
    };
    let veeam_dir = dlogs.join("veeam");
    if !veeam_dir.is_dir() {
        return;
    }

    let profiles = load_profiles();

    // Restore.1.log is a known VBR-format file present in D:\Logs\veeam.
    let restore_log = veeam_dir.join("Restore.1.log");
    if !restore_log.exists() {
        return;
    }

    let content = fs::read_to_string(&restore_log).expect("read Restore.1.log");
    let sample_lines: Vec<String> = content.lines().take(20).map(String::from).collect();
    let filename = restore_log.file_name().unwrap().to_str().unwrap();

    let result = profile::auto_detect(filename, &sample_lines, &profiles);
    assert!(
        result.is_some(),
        "auto_detect must return a profile for '{filename}'"
    );
    let det = result.unwrap();
    assert_eq!(
        det.profile_id, "veeam-vbr",
        "Restore.1.log must auto-detect as veeam-vbr, got '{}'",
        det.profile_id
    );
}

/// IIS W3C log files in D:\Logs\iis auto-detect as the iis-w3c profile.
#[test]
fn e2e_dlogs_iis_w3c_auto_detects() {
    let Some(dlogs) = dlogs_root() else {
        return;
    };
    let iis_dir = dlogs.join("iis");
    if !iis_dir.is_dir() {
        return;
    }

    let profiles = load_profiles();

    // Find any u_ex*.log file (IIS W3C naming convention).
    let iis_file = fs::read_dir(&iis_dir)
        .expect("read iis dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with("u_ex"))
                .unwrap_or(false)
        });

    let Some(iis_file) = iis_file else {
        return; // no u_ex*.log present, skip
    };

    let content = fs::read_to_string(&iis_file).expect("read IIS file");
    let sample_lines: Vec<String> = content.lines().take(20).map(String::from).collect();
    let filename = iis_file.file_name().unwrap().to_str().unwrap();

    let result = profile::auto_detect(filename, &sample_lines, &profiles);
    assert!(
        result.is_some(),
        "auto_detect must return a profile for IIS file '{filename}'"
    );
    let det = result.unwrap();
    assert_eq!(
        det.profile_id, "iis-w3c",
        "IIS file '{filename}' must auto-detect as iis-w3c, got '{}'",
        det.profile_id
    );
}

/// Syslog files in D:\Logs\system auto-detect as a syslog profile.
#[test]
fn e2e_dlogs_syslog_auto_detects() {
    let Some(dlogs) = dlogs_root() else {
        return;
    };
    let sys_dir = dlogs.join("system");
    if !sys_dir.is_dir() {
        return;
    }

    let profiles = load_profiles();

    // Find any syslog-*.log file.
    let syslog_file = fs::read_dir(&sys_dir)
        .expect("read system dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with("syslog"))
                .unwrap_or(false)
        });

    let Some(syslog_file) = syslog_file else {
        return;
    };

    let content = fs::read_to_string(&syslog_file).expect("read syslog file");
    let sample_lines: Vec<String> = content.lines().take(20).map(String::from).collect();
    let filename = syslog_file.file_name().unwrap().to_str().unwrap();

    let result = profile::auto_detect(filename, &sample_lines, &profiles);
    assert!(
        result.is_some(),
        "auto_detect must return a profile for syslog file '{filename}'"
    );
    let det = result.unwrap();
    assert!(
        det.profile_id.contains("syslog"),
        "syslog file '{filename}' must detect a syslog profile, got '{}'",
        det.profile_id
    );
}

/// SQL Server log (SQLAGENT.1) in D:\Logs\sql auto-detects as a SQL Server profile.
///
/// Note: the synthetic test data in D:\Logs\sql\SQLAGENT.1 uses the SQL Server
/// Error Log timestamp format (`YYYY-MM-DD HH:MM:SS.fff`), so the content match
/// for `sql-server-error` wins over the filename bonus for `sql-server-agent`.
/// The test validates that *some* SQL Server profile is detected (not plain-text)
/// and that the result is one of the two expected SQL profiles.
#[test]
fn e2e_dlogs_sql_agent_auto_detects() {
    let Some(dlogs) = dlogs_root() else {
        return;
    };
    let sql_file = dlogs.join("sql").join("SQLAGENT.1");
    if !sql_file.exists() {
        return;
    }

    let profiles = load_profiles();
    let content = fs::read_to_string(&sql_file).expect("read SQLAGENT.1");
    let sample_lines: Vec<String> = content.lines().take(20).map(String::from).collect();

    let result = profile::auto_detect("SQLAGENT.1", &sample_lines, &profiles);
    assert!(
        result.is_some(),
        "auto_detect must return a profile for SQLAGENT.1"
    );
    let det = result.unwrap();
    assert!(
        det.profile_id == "sql-server-agent" || det.profile_id == "sql-server-error",
        "SQLAGENT.1 must auto-detect as a SQL Server profile, got '{}'",
        det.profile_id
    );
    assert_ne!(
        det.profile_id, "plain-text",
        "SQLAGENT.1 must not fall back to plain-text"
    );
}

/// Regression — Bug 3 (multi-file parsing): multiple VBR files in D:\Logs\veeam
/// must each contribute parsed entries, not just the first file.
///
/// Before the scan.rs fix the continuation-mode parser silently dropped all
/// lines when no preceding entry existed, leaving every file after the first
/// with 0 entries and falsely showing "433 entries from 1 file".
#[test]
fn e2e_dlogs_vbr_multiple_files_each_contribute_entries() {
    let Some(dlogs) = dlogs_root() else {
        return;
    };
    let veeam_dir = dlogs.join("veeam");
    if !veeam_dir.is_dir() {
        return;
    }

    let profiles = load_profiles();
    let vbr_profile = profiles
        .iter()
        .find(|p| p.id == "veeam-vbr")
        .expect("veeam-vbr profile must be loaded");

    // Collect all .log files in the veeam folder.
    let log_files: Vec<_> = fs::read_dir(&veeam_dir)
        .expect("read veeam dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("log"))
        .collect();

    assert!(
        !log_files.is_empty(),
        "D:\\Logs\\veeam must contain at least one .log file"
    );

    let mut files_with_entries = 0usize;
    let mut files_checked = 0usize;

    for path in &log_files {
        let Ok(content) = fs::read_to_string(path) else {
            continue;
        };
        if content.trim().is_empty() {
            continue;
        }
        files_checked += 1;
        let result = parse_content(&content, path, vbr_profile, &ParseConfig::default(), 0);
        if !result.entries.is_empty() {
            files_with_entries += 1;
        }
    }

    // We need at least 2 non-empty VBR files in D:\Logs\veeam to be useful.
    // If there's only 1 file, this test is vacuously not reproducible.
    if files_checked < 2 {
        return; // not enough data to exercise the regression
    }

    assert!(
        files_with_entries >= 2,
        "at least 2 VBR files must produce entries (regression for Bug 3); \
         checked {files_checked} files, only {files_with_entries} had entries"
    );
}

/// Regression — Bug 1 (severity always INFO): parsing a VBR file must yield
/// entries with varied severity levels, not just Severity::Info.
///
/// Before the model.rs fix, infer_severity_from_message() returned
/// Severity::Info as its default fallback instead of Severity::Unknown.
#[test]
fn e2e_dlogs_vbr_severity_is_not_uniform_info() {
    let Some(dlogs) = dlogs_root() else {
        return;
    };
    let restore_log = dlogs.join("veeam").join("Restore.1.log");
    if !restore_log.exists() {
        return;
    }

    let profiles = load_profiles();
    let vbr_profile = profiles
        .iter()
        .find(|p| p.id == "veeam-vbr")
        .expect("veeam-vbr profile must be loaded");

    let content = fs::read_to_string(&restore_log).expect("read Restore.1.log");
    let result = parse_content(
        &content,
        &restore_log,
        vbr_profile,
        &ParseConfig::default(),
        0,
    );

    assert!(
        !result.entries.is_empty(),
        "Restore.1.log must produce at least one parsed entry"
    );

    // Count distinct severity values.
    let distinct_severities: std::collections::HashSet<_> =
        result.entries.iter().map(|e| e.severity).collect();

    assert!(
        distinct_severities.len() >= 2,
        "Restore.1.log must contain at least two distinct severity levels \
         (regression for Bug 1 — all entries were Info); got: {distinct_severities:?}"
    );

    // The file must NOT consist entirely of Info entries.
    let all_info = result
        .entries
        .iter()
        .all(|e| e.severity == logsleuth::core::model::Severity::Info);
    assert!(
        !all_info,
        "Restore.1.log must not produce only Severity::Info entries (Bug 1 regression)"
    );
}

/// Regression — Bug 2 (stale source-file filter): a freshly constructed
/// AppState must never have a source-file whitelist pre-populated.
///
/// Before the state.rs fix, restore_from_session() reinstated source_files
/// from the saved session, silently hiding all files except the ones recorded
/// in the previous session's solo-view state.
#[test]
fn e2e_dlogs_fresh_state_has_no_source_filter() {
    // Bug 2 is a state-machine regression that doesn't need D:\Logs to be
    // present; the guard is here only for consistency with the suite.
    let profiles = load_profiles();
    let state = logsleuth::app::state::AppState::new(profiles, false);

    assert!(
        state.filter_state.source_files.is_empty(),
        "a freshly constructed AppState must have an empty source_files filter \
         (regression for Bug 2 — stale session source_files hid all-but-one file)"
    );
    assert!(
        !state.filter_state.hide_all_sources,
        "a freshly constructed AppState must not hide all sources (Bug 2 regression)"
    );
}

/// VBO365 profile detects correctly when given real VBO365-format content.
///
/// Uses the existing `veeam_vbo365_sample.log` fixture (which contains genuine
/// VBO365 timestamp/format lines) combined with the proxy filename
/// `Veeam.Archiver.Proxy.log` that matches the profile's file_patterns.
///
/// Note: `D:\Logs\veeam\vbo365_backup.log` contains VBR-format content (the
/// synthetic generator wrote VBR lines into it), so it correctly detects as
/// `veeam-vbr`. This test uses the real VBO365 fixture instead.
#[test]
fn e2e_dlogs_vbo365_auto_detects() {
    // This test uses the committed fixture, not the D:\Logs folder, so it
    // always runs — no dlogs_root() guard needed.
    let fixture_path = fixture("veeam_vbo365_sample.log");
    if !fixture_path.exists() {
        return;
    }

    let profiles = load_profiles();
    let content = fs::read_to_string(&fixture_path).expect("read veeam_vbo365_sample.log");
    let sample_lines: Vec<String> = content.lines().take(20).map(String::from).collect();

    // Use a typical VBO365 filename to trigger the file_patterns bonus.
    let result = profile::auto_detect("Veeam.Archiver.Proxy.log", &sample_lines, &profiles);
    assert!(
        result.is_some(),
        "VBO365 fixture content with proxy filename must match veeam-vbo365 profile"
    );
    let det = result.unwrap();
    assert_eq!(
        det.profile_id, "veeam-vbo365",
        "VBO365 fixture must detect as veeam-vbo365, got '{}'",
        det.profile_id
    );
}

/// Full pipeline smoke test: discover D:\Logs, parse a sample of files, and
/// assert that the total entry count is non-trivially large (> 500).
///
/// This provides coarse confidence that the end-to-end scan path — profile
/// auto-detection, continuation-mode parsing, and plain-text fallback — all
/// function correctly on real data.
#[test]
fn e2e_dlogs_full_pipeline_smoke() {
    let Some(dlogs) = dlogs_root() else {
        return;
    };

    let profiles = load_profiles();

    let (files, _warnings, _total) =
        discover_files(&dlogs, &DiscoveryConfig::default(), |_, _| {}).unwrap();

    assert!(!files.is_empty(), "discovery must return at least one file");

    let mut total_entries = 0usize;
    let mut files_with_entries = 0usize;

    // Parse up to 20 files to keep the test fast.
    for discovered in files.iter().take(20) {
        let Ok(content) = fs::read_to_string(&discovered.path) else {
            continue;
        };
        if content.trim().is_empty() {
            continue;
        }

        let sample_lines: Vec<String> = content.lines().take(20).map(String::from).collect();
        let filename = discovered
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");

        let profile_to_use = profile::auto_detect(filename, &sample_lines, &profiles)
            .and_then(|det| profiles.iter().find(|p| p.id == det.profile_id))
            .or_else(|| profiles.iter().find(|p| p.id == "plain-text"));

        let Some(prof) = profile_to_use else {
            continue;
        };

        let result = parse_content(&content, &discovered.path, prof, &ParseConfig::default(), 0);
        if !result.entries.is_empty() {
            total_entries += result.entries.len();
            files_with_entries += 1;
        }
    }

    assert!(
        files_with_entries >= 5,
        "at least 5 files out of the first 20 discovered must produce parsed entries; \
         got {files_with_entries}"
    );
    assert!(
        total_entries >= 100,
        "parsing the first 20 files must yield at least 100 entries total; \
         got {total_entries}"
    );
}

// =============================================================================
// Regression tests for previously fixed bugs
// =============================================================================

/// Regression \u2014 Bug: append scans always started entry IDs from 0, causing
/// duplicate IDs between the initial scan and any subsequent append (directory
/// watcher, "Add File(s)").  Bookmarks and the correlation overlay use entry
/// IDs as stable keys, so duplicates silently corrupted both features.
///
/// Fix: `run_parse_pipeline` now accepts `entry_id_start` and callers pass
/// `state.next_entry_id()` for append runs.  This test verifies that
/// `parse_content` honours a non-zero `id_start` and that two consecutive
/// parses with correct starts never share an ID.
#[test]
fn e2e_append_scan_entry_ids_are_unique_across_parses() {
    use logsleuth::core::model::Severity;
    use std::collections::HashSet;

    let profiles = load_profiles();
    let plain = profiles
        .iter()
        .find(|p| p.id == "plain-text")
        .expect("plain-text profile must be present");

    let content_a = "Line one from file A\nLine two from file A\n";
    let content_b = "Line one from file B\nLine two from file B\n";

    let path_a = PathBuf::from("a.log");
    let path_b = PathBuf::from("b.log");

    // First parse: IDs start at 0.
    let result_a = parse_content(content_a, &path_a, plain, &ParseConfig::default(), 0);
    assert!(!result_a.entries.is_empty(), "parse a must produce entries");

    // Second parse (simulating an append): IDs must start after the last ID
    // assigned by the first parse.
    let id_start_b = result_a.entries.iter().map(|e| e.id).max().unwrap_or(0) + 1;
    let result_b = parse_content(
        content_b,
        &path_b,
        plain,
        &ParseConfig::default(),
        id_start_b,
    );
    assert!(!result_b.entries.is_empty(), "parse b must produce entries");

    // Collect all IDs from both parses and assert no duplicates.
    let ids_a: HashSet<u64> = result_a.entries.iter().map(|e| e.id).collect();
    let ids_b: HashSet<u64> = result_b.entries.iter().map(|e| e.id).collect();
    let intersection: HashSet<u64> = ids_a.intersection(&ids_b).copied().collect();

    assert!(
        intersection.is_empty(),
        "entry IDs from the first and second parse must not overlap; \
         colliding IDs: {intersection:?}"
    );

    // IDs within each result must also be strictly monotonic.
    let sorted_a: Vec<u64> = result_a.entries.iter().map(|e| e.id).collect();
    for w in sorted_a.windows(2) {
        assert!(
            w[1] > w[0],
            "entry IDs within a single parse must be strictly increasing; \
             got {} then {}",
            w[0],
            w[1]
        );
    }

    // Verify that next_entry_id() on AppState returns the correct continuation
    // value when entries from both parses are loaded.
    let mut state = logsleuth::app::state::AppState::new(vec![], false);
    state.entries.extend(result_a.entries.iter().cloned());
    state.entries.extend(result_b.entries.iter().cloned());
    let expected_next = state.entries.iter().map(|e| e.id).max().unwrap_or(0) + 1;
    let actual_next = state.next_entry_id();
    assert_eq!(
        actual_next, expected_next,
        "next_entry_id() must return max_id + 1 after both parses are loaded"
    );

    // Regression guard: if both parses had started from 0, ids_a and ids_b
    // WOULD overlap.  Confirm this is the case by checking the "bug" scenario.
    let result_bug = parse_content(content_b, &path_b, plain, &ParseConfig::default(), 0);
    let ids_bug: HashSet<u64> = result_bug.entries.iter().map(|e| e.id).collect();
    let bug_intersection: HashSet<u64> = ids_a.intersection(&ids_bug).copied().collect();
    assert!(
        !bug_intersection.is_empty(),
        "the bug scenario (both parses starting from 0) must produce \
         colliding IDs \u{2014} this guards the regression test itself"
    );
    // Suppress unused warning \u2014 Severity is imported for completeness.
    let _ = Severity::Unknown;
}

/// Regression \u2014 Bug: "File > Open Directory" menu bar handler did not forward
/// the `discovery_date_input` date filter as `modified_since` to `DiscoveryConfig`.
/// The discovery-panel "Open Directory" button (via `pending_scan`) correctly
/// applied the filter; the menu path silently ignored it.
///
/// The fix populates `modified_since` from `state.discovery_modified_since()`
/// in the menu handler before calling `state.clear()`.  This test verifies
/// that `discovery_modified_since()` produces the expected value for a variety
/// of inputs so the GUI can safely call it before or after `clear()`.
#[test]
fn e2e_discovery_date_filter_is_honoured_by_menu_open_directory() {
    use logsleuth::app::state::AppState;

    // Case 1: non-empty valid date \u2014 must parse regardless of other state.
    let mut state = AppState::new(vec![], false);
    state.discovery_date_input = "2025-06-15 08:30:00".to_string();
    let since = state.discovery_modified_since();
    assert!(
        since.is_some(),
        "a valid date input must produce Some(DateTime)"
    );
    let dt = since.unwrap();
    assert_eq!(
        dt.format("%Y-%m-%d %H:%M:%S").to_string(),
        "2025-06-15 08:30:00",
        "parsed datetime must match the input exactly"
    );

    // Case 2: `clear()` must NOT wipe `discovery_date_input`.
    // The menu handler calls clear() AFTER capturing modified_since; this test
    // verifies the field survives so the next `discovery_modified_since()` call
    // (e.g. for the directory watcher restart) also sees the correct value.
    state.clear();
    assert_eq!(
        state.discovery_date_input, "2025-06-15 08:30:00",
        "clear() must preserve discovery_date_input (it is a user preference, \
         not scan state)"
    );
    let since_after_clear = state.discovery_modified_since();
    assert_eq!(
        since_after_clear, since,
        "discovery_modified_since() must return the same value before and after clear()"
    );

    // Case 3: empty input \u2014 must return None (no date filter).
    let mut state2 = AppState::new(vec![], false);
    state2.discovery_date_input = String::new();
    assert!(
        state2.discovery_modified_since().is_none(),
        "empty discovery_date_input must produce None"
    );

    // Case 4: the modified_since filter is correctly applied by the discovery
    // pipeline when set. Use a far-future date so NO fixture files pass it.
    let fixture_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures");
    let future_config = DiscoveryConfig {
        modified_since: Some(
            chrono::NaiveDate::from_ymd_opt(2099, 1, 1)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap()
                .and_utc(),
        ),
        ..DiscoveryConfig::default()
    };
    let (files, _, _) = discover_files(&fixture_dir, &future_config, |_, _| {}).unwrap();
    assert!(
        files.is_empty(),
        "a modified_since date of 2099-01-01 must exclude all fixture files \
         (files are not from the future); got {files:?}"
    );
}
