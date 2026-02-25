# LogSleuth - Implementation Progress

## Increment 1: Project Scaffolding & Core Foundation
**Status: COMPLETE**

- [x] Project structure (Cargo.toml, directory layout per Atlas)
- [x] `util::constants` - All named constants and limits
- [x] `util::error` - Full typed error hierarchy with context chains
- [x] `util::logging` - Structured logging with debug mode support
- [x] `core::model` - LogEntry, Severity, FormatProfile, ScanProgress, ScanSummary
- [x] `core::profile` - TOML profile parsing, validation, regex compilation, auto-detection
- [x] `core::parser` - Basic line-by-line parsing with multiline support (needs timestamp parsing)
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

## Increment 2: Discovery Engine (NEXT)
**Status: NOT STARTED**

- [ ] `core::discovery` - Full recursive traversal with walkdir
- [ ] Glob pattern matching (include/exclude)
- [ ] File metadata collection (size, modified time)
- [ ] Format auto-detection integration (sample lines + profile matching)
- [ ] Progress reporting via ScanProgress messages
- [ ] Max depth and max files enforcement
- [ ] E2E tests with real directory structures

## Increment 3: Full Parser Implementation
**Status: NOT STARTED**

- [ ] Timestamp parsing with chrono (all profile formats)
- [ ] Streaming/chunked file reading for large files
- [ ] Memory-mapped I/O for files above threshold
- [ ] Full integration with discovery (scan thread orchestration)
- [ ] Per-file parse error tracking and summarisation
- [ ] E2E tests with real Veeam log samples

## Increment 4: UI Polish
**Status: NOT STARTED**

- [ ] Virtual scrolling for timeline (1M+ entries)
- [ ] Filter debounce
- [ ] Keyboard shortcuts
- [ ] Cross-log correlation view
- [ ] Scan summary with per-file breakdown
- [ ] Export dialog
- [ ] Dark/light theme toggle

## Increment 5: Release Pipeline
**Status: NOT STARTED**

- [ ] NSIS installer script (Windows)
- [ ] GitHub Actions CI workflow
- [ ] GitHub Actions release workflow
- [x] update-application.ps1 release script
- [x] Application icon embedding (ICO for Windows, via winres build.rs; runtime PNG via eframe viewport `with_icon`)
- [ ] DMG builder (macOS)
- [ ] AppImage builder (Linux)
