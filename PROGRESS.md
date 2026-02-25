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
**Status: NEXT**

- [ ] Virtual scrolling for timeline (1M+ entries)
- [ ] Filter debounce
- [ ] Keyboard shortcuts
- [ ] Cross-log correlation view
- [ ] Scan summary with per-file breakdown (FileSummary population in scan.rs)
- [ ] Export dialog
- [ ] Dark/light theme toggle
- [ ] Streaming/chunked file reading for very large files (supplement current mmap approach)
- [ ] Per-file parse error tracking and summarisation in UI
- [ ] Additional test fixtures (syslog, log4j E2E)

## Increment 5: Release Pipeline
**Status: NOT STARTED**

- [ ] NSIS installer script (Windows)
- [ ] GitHub Actions CI workflow
- [ ] GitHub Actions release workflow
- [x] update-application.ps1 release script
- [x] Application icon embedding (ICO for Windows, via winres build.rs; runtime PNG via eframe viewport `with_icon`)
- [ ] DMG builder (macOS)
- [ ] AppImage builder (Linux)
