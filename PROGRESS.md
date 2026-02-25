# LogSleuth - Implementation Progress

## Increment 1: Project Scaffolding & Core Foundation
**Status: COMPLETE**

- [x] Project structure (Cargo.toml, directory layout per Atlas)
- [x] `util::constants` - All named constants and limits
- [x] `util::error` - Full typed error hierarchy with context chains
- [x] `util::logging` - Structured logging with debug mode support
- [x] `core::model` - LogEntry, Severity, FormatProfile, ScanProgress, ScanSummary
- [x] `core::profile` - TOML profile parsing, validation, regex compilation, auto-detection
- [x] `core::parser` - Basic line-by-line parsing with multiline support, timestamp parsing via chrono
- [x] `core::filter` - Composable filter engine with severity, text, regex, time range
- [x] `core::export` - CSV and JSON export to Write trait objects
- [x] `core::discovery` - Stub with config and validation
- [x] `platform::config` - Platform path resolution
- [x] `platform::fs` - File reading helpers
- [x] `app::profile_mgr` - Built-in + user profile loading with overrides
- [x] `app::state` - Application state management
- [x] `app::scan` - Scan manager stub with channel-based progress
- [x] `ui::theme` - Severity colours, layout constants
- [x] `ui::panels::*` - All panel stubs (discovery, timeline, detail, summary, filters)
- [x] `gui.rs` - eframe::App implementation wiring all panels
- [x] `main.rs` - CLI parsing, logging init, profile loading, GUI launch
- [x] 9 built-in format profiles (Veeam VBR, VBO365, IIS, syslog x2, JSON, Log4j, generic, plain-text)
- [x] Application icon (SVG + PNG + ICO assets, 'LS' brand mark, all sizes regenerated via `cargo run --example gen_icons`)
- [x] Unit tests for profile loading, parsing, filtering, export
- [x] config.example.toml
- [x] LogSleuth-Specification.md
- [x] ATLAS.md (Project Atlas)

## Increment 2: Discovery Engine
**Status: COMPLETE**

- [x] `core::discovery` - Full recursive traversal with walkdir, glob include/exclude patterns, directory descent short-circuiting via `filter_entry`
- [x] File metadata collection (size, modified time, `is_large` flag)
- [x] Format auto-detection integration (sample lines + `profile::auto_detect`)
- [x] Progress reporting via `ScanProgress::FileDiscovered` callback per file
- [x] Max depth and max files enforcement (clamped to `ABSOLUTE_MAX_*` constants)
- [x] `core::parser` - Timestamp parsing via chrono (`NaiveDateTime::parse_from_str`, RFC 3339 fallback, date-only fallback); `ParseError::TimestampParse` on failure (non-fatal, entry kept)
- [x] `app::scan` - Full background scan thread: discovery → auto-detection → parsing; `Arc<AtomicBool>` cancel; transient I/O retry (50/100/200 ms); `memmap2` for large files; entry batching (`ENTRY_BATCH_SIZE = 500`)
- [x] `gui.rs` - Handles all `ScanProgress` variants (`FilesDiscovered`, `EntriesBatch`, `FileParsed`, `ParsingStarted`, `Cancelled`); passes profiles + `DiscoveryConfig` to `start_scan`; `ctx.request_repaint()` during active scans
- [x] `src/lib.rs` - Added library crate entry point to enable integration tests
- [x] E2E test suite in `tests/e2e_discovery.rs`: 9 tests covering discovery, error cases, auto-detection, parsing, timestamps, severity mapping, entry IDs

**Test results: 36 unit tests + 9 E2E tests = 45 total, all passing**

## Increment 3: UI Polish & Full Parser Implementation
**Status: COMPLETE**

- [x] `core::filter` - Added `regex_pattern: String` buffer field to `FilterState`; `set_regex()` always updates both the buffer and the compiled `Option<Regex>` (enables UI text binding)
- [x] `core::model` - `FileSummary` struct: `path`, `profile_id`, `entry_count`, `error_count`, `earliest: Option<DateTime<Utc>>`, `latest: Option<DateTime<Utc>>`
- [x] `core::model` - `ScanSummary` extended: `file_summaries: Vec<FileSummary>`, `duration: std::time::Duration`
- [x] `app::state` - Added `pending_scan: Option<PathBuf>` and `request_cancel: bool` flags (set by panels, consumed each frame by `gui.rs`; decouples UI layer from ScanManager)
- [x] `app::scan` - Per-file `FileSummary` populated with earliest/latest timestamps; `ScanSummary` includes `file_summaries` and `duration`
- [x] `ui::panels::filters` - Severity checkboxes with `theme::severity_colour()` colour coding; regex input bound to `filter_state.regex_pattern`; live compile-error feedback (\u2713 green / \u2717 red); entry count badge; quick-filter buttons
- [x] `ui::panels::timeline` - Virtual scrolling via `ScrollArea::show_rows` (O(1) rendered rows); `[SEV ] HH:MM:SS | filename | first line` row format; severity-coloured monospace text; tooltip showing full `YYYY-MM-DD HH:MM:SS UTC` timestamp on hover
- [x] `ui::panels::discovery` - File list with name, profile+confidence%, formatted size; spinner + Cancel button during scan; `pending_scan` flag replaces direct scan trigger; warnings badge
- [x] `ui::panels::detail` - Severity label coloured with `severity_colour()`; header row (severity | filename | timestamp); metadata grid (File, Line, Profile, Thread, Component); message area with Copy button (`ctx.copy_text`)
- [x] `ui::panels::summary` - Per-file breakdown `egui::Grid` table (File, Profile, Entries, Errors, Time range); scrollable file table (max 260px); warnings section; overall stats grid with error highlighting
- [x] `gui.rs` - Pending scan flag consumed to trigger `start_scan`; `request_cancel` flag consumed to call `cancel_scan`; File menu Open Directory fixed to `clear()` before setting `scan_path`; Export submenu (CSV + JSON, enabled when entries loaded, `rfd` save dialog, status message); Cancel button in status bar during scan

**Test results: 36 unit tests + 9 E2E tests = 45 total, all passing**

## Increment 4: Time Range & Source File Filters
**Status: COMPLETE**

- [x] `core::filter` - Added `relative_time_secs: Option<u64>` to `FilterState` (stores rolling window in seconds; source of truth for relative time filter); added `relative_time_input: String` (UI text buffer for custom minutes input); updated `is_empty()` to include `relative_time_secs`
- [x] `app::state` - `apply_filters()` now computes `filter_state.time_start = Utc::now() - Duration::seconds(secs)` from `relative_time_secs` before calling the core filter; core layer remains pure (no clock access)
- [x] `ui::panels::filters` - Time range section: quick-select toggle buttons (15m / 1h / 6h / 24h), custom "Last ___ min" text input committed on Enter, clear (✕) button, live feedback label showing computed "After HH:MM:SS"
- [x] `ui::panels::filters` - Source file checklist: per-discovered-file checkbox with correct whitelist semantics (empty set = all pass; uncheck populates other files into set; re-check removes from set; if all re-checked, clears back to empty); "All" reset button; scrollable list (max 140px)
- [x] `gui.rs` - When relative time filter is active, calls `apply_filters()` each frame and schedules `ctx.request_repaint_after(1s)` so the rolling window boundary stays current as the clock advances
- [x] Unit tests (4 new): `test_time_range_start_bound`, `test_time_filter_excludes_entries_without_timestamps`, `test_source_file_filter`, `test_relative_time_field_tracked_in_is_empty`

**Test results: 40 unit tests + 9 E2E tests = 49 total, all passing**

## Increment 4a: Source File Filter UX — Large File Lists
**Status: COMPLETE**

- [x] `app::state` - Added `file_list_search: String` field to `AppState` for the file-list search box buffer
- [x] `ui::panels::filters` - Search box appears automatically when the discovered file list exceeds 8 entries; filters the visible checkboxes in real time
- [x] `ui::panels::filters` - **Select All / Select None** buttons operate only on the currently visible (filtered) subset; allows bulk-selecting within a search result without affecting hidden entries
- [x] `ui::panels::filters` - Fixed scroll area height to 180 px (up from 140 px) to accommodate larger, denser lists
- [x] `ui::panels::filters` - Blue `N / total files` header counter shows how many files are visible vs. total, giving orientation when search is active

**Test results: 40 unit tests + 9 E2E tests = 49 total, all passing**

## Increment 5: Release Pipeline
**Status: NOT STARTED**

- [ ] NSIS installer script (Windows)
- [ ] GitHub Actions CI workflow
- [ ] GitHub Actions release workflow
- [x] update-application.ps1 release script
- [x] Application icon embedding (ICO for Windows, via winres build.rs; runtime PNG via eframe viewport `with_icon`)
- [ ] DMG builder (macOS)
- [ ] AppImage builder (Linux)
