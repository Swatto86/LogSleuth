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
**Status: COMPLETE**

- [x] `installer/windows/logsleuth.nsi` - NSIS installer with MultiUser.nsh support (per-user and per-machine), Start Menu shortcuts, Add/Remove Programs registry entry, clean uninstaller; version string updated automatically by `update-application.ps1`
- [x] `installer/macos/create-dmg.sh` - Builds a macOS .app bundle (Info.plist, binary, .icns via sips/iconutil) then produces a DMG with `create-dmg` (falls back to plain `hdiutil` if unavailable)
- [x] `installer/linux/create-appimage.sh` - Builds a portable Linux AppImage: constructs AppDir (AppRun, .desktop, icon, binary), downloads `appimagetool` automatically if not in PATH
- [x] `.github/workflows/ci.yml` - Triggered on push/PR to main; matrix build (ubuntu-latest, windows-latest, macos-latest); steps: `cargo build --release`, `cargo fmt -- --check`, `cargo clippy -- -D warnings`, `cargo test`; Linux apt deps for eframe/rfd
- [x] `.github/workflows/release.yml` - Triggered on `v*.*.*` tag push; three parallel build jobs (Windows: NSIS, macOS: DMG via `create-dmg`, Linux: AppImage via `appimagetool`); `create-release` job downloads all artefacts and publishes GitHub Release with `softprops/action-gh-release@v2`
- [x] `update-application.ps1` release script (previously complete)
- [x] Application icon embedding (ICO for Windows, via winres build.rs; runtime PNG via eframe viewport `with_icon`)

**Test results: 40 unit tests + 9 E2E tests = 49 total, all passing**

## Increment 6: Multi-File Merged Timeline, Fuzzy Search & Scalability
**Status: COMPLETE**

### Font & Encoding
- [x] `main.rs` - `configure_fonts()` on Windows loads `segoeui.ttf`, `seguiemj.ttf`, `seguisym.ttf` from `C:\Windows\Fonts\` and inserts them at the front of the Proportional family, eliminating □ glyph substitution. Called from the `eframe::run_native` creation callback (`cc`).
- [x] `app::scan` - `decode_bytes()` detects UTF-16 LE/BE BOMs (`[0xFF,0xFE]` / `[0xFE,0xFF]`) and decodes via `u16::from_le/be_bytes` before parsing; falls back to lossy UTF-8. Enables reading `C:\Windows\Logs` and other Windows system log directories.
- [x] `app::scan` - `read_small_file_with_retry` and `read_sample_lines` both tolerate UTF-16 encoded files without failing.

### Plain-Text Fallback
- [x] `app::scan` Phase 2 assigns the `plain-text` profile when no structured profile matches, ensuring all files produce entries rather than being silently skipped. Files with fallback assignment are shown as "plain-text (fallback)" in the discovery panel.

### Detail Pane Improvements
- [x] `ui::panels::detail` - Removed 80 px `max_height` cap; message scroll area now fills available panel height.
- [x] `ui::panels::detail` - **Show in Folder** button: opens the OS file manager with the source file selected (Windows: `explorer /select,<path>`; macOS: `open -R <path>`; Linux: `xdg-open <dir>`).

### Fuzzy Text Search
- [x] `core::filter` - Added `fuzzy: bool` field to `FilterState`. `fuzzy_match()` implements a case-insensitive subsequence algorithm (each character of the query must appear in order in the target, but not necessarily contiguously).
- [x] `core::filter` - `matches_all()` branches on `fuzzy` vs exact substring. `errors_only_from(fuzzy)` and `errors_and_warnings_from(fuzzy)` preserve the fuzzy mode across quick-filter clicks.
- [x] `ui::panels::filters` - Added `~` toggle button next to the text search input; button turns blue when fuzzy mode is active. 5 new unit tests for fuzzy matching (54 total).

### CMTrace-Style Multi-File Merged Timeline
- [x] `ui::theme` - `FILE_COLOUR_PALETTE`: 24 visually distinct `Color32` values. Extended from 12 to 24 so files 1–24 all receive unique colours before any wrapping. `file_colour(index)` wraps with `% 24`.
- [x] `core::model` - Added `AdditionalFilesDiscovered { files: Vec<DiscoveredFile> }` `ScanProgress` variant for append mode (extends the UI file list rather than replacing it).
- [x] `app::state` - Added `file_colours: HashMap<PathBuf, egui::Color32>` and `pending_single_files: Option<Vec<PathBuf>>`. New methods: `assign_file_colour()` (round-robin over 24-colour palette), `colour_for_file()` (returns assigned colour or neutral grey), `sort_entries_chronologically()` (kept as utility; sort itself moved to background thread).
- [x] `app::scan` - `run_parse_pipeline`: entries are now collected into a `Vec`, sorted chronologically on the background thread, then streamed in batches via `EntriesBatch` before `ParsingCompleted` is sent. The UI thread never blocks on a large sort.
- [x] `app::scan` - Added `start_scan_files()` to `ScanManager` and `run_files_scan()` pipeline entry for individual-file append scanning. Shares `run_parse_pipeline(append=true)` with directory scans.
- [x] `ui::panels::timeline` - Each row renders a 4 px coloured left stripe using `state.colour_for_file()` before the selectable label row.
- [x] `ui::panels::discovery` - Each file in the discovery list shows an 8×8 coloured dot matching its assigned timeline stripe colour.
- [x] `ui::panels::filters` - Each file in the source-file checklist shows a coloured dot matching its timeline stripe colour.
- [x] `ui::panels::filters` - Added **Solo** button per file: clicking it filters the timeline to that file exclusively; clicking again returns to all files.
- [x] `gui.rs` - **File > Add File(s)…** menu item opens a multi-file picker (`.log`/`.txt` filter); selected files are appended to the current session without clearing existing entries.
- [x] `gui.rs` - `AdditionalFilesDiscovered` handler extends `discovered_files` and assigns colours; `FilesDiscovered` handler assigns colours then replaces the list.
- [x] `gui.rs` - `ParsingCompleted` handler calls `apply_filters()` only (sort already done on background thread).

**Test results: 45 unit tests + 9 E2E tests = 54 total, all passing**

## Increment 7: Live Tail
**Status: COMPLETE**

- [x] `src/core/model.rs` - Added `TailProgress` enum: `Started { file_count }`, `NewEntries { entries }`, `Stopped`, `FileError { path, message }`.
- [x] `src/util/constants.rs` - Added named constants: `TAIL_POLL_INTERVAL_MS = 500`, `TAIL_CANCEL_CHECK_INTERVAL_MS = 100`, `MAX_TAIL_READ_BYTES_PER_TICK = 512 KiB`.
- [x] `src/app/tail.rs` (NEW) - `TailManager` with `start_tail()`, `stop_tail()`, `is_active()`, `poll_progress()`. Background `run_tail_watcher` poll loop:
  - Seeds each file's byte offset to the **current file end** at start — only lines written *after* tail is activated are surfaced.
  - Polls every 500 ms; cancel flag checked every 100 ms within the sleep.
  - Per-file `partial` buffer carries incomplete lines across poll ticks so entries split across reads are assembled correctly.
  - Offset advances by the number of raw bytes read unconditionally — no re-read of carry-over bytes.
  - Detects file rotation/truncation (current size < offset) and resets offset to 0.
  - Reads capped at `MAX_TAIL_READ_BYTES_PER_TICK` per file per tick.
  - Decodes new bytes as lossy UTF-8. UTF-16 files (Windows system logs that are not line-appended) are a known out-of-scope limitation.
  - Calls `parser::parse_content` on complete lines using the file's resolved profile — full profile support including regex, severity, timestamp, multiline.
  - Per-file read/stat errors are non-fatal: sends `TailProgress::FileError` and continues to the next file.
- [x] `src/app/mod.rs` - Exported `pub mod tail`.
- [x] `src/app/state.rs` - Added: `tail_active`, `tail_auto_scroll` (default `true`), `tail_scroll_to_bottom` (one-shot scroll flag), `request_start_tail`, `request_stop_tail`. Added `next_entry_id() -> u64` helper. `clear()` resets tail flags (preserves `tail_auto_scroll` preference).
- [x] `src/gui.rs` - Added `tail_manager: TailManager` field. Each frame: polls `tail_manager.poll_progress()`, appends `NewEntries` to `state.entries`, calls `apply_filters()`, sets `tail_scroll_to_bottom` flag. Handles `request_start_tail` (builds `TailFileInfo` list from discovered files, calls `tail_manager.start_tail`) and `request_stop_tail`. Schedules `request_repaint_after(500ms)` while tail is active. Green `● LIVE` badge in status bar when tail is active.
- [x] `src/ui/panels/discovery.rs` - Live Tail controls shown after a scan when entries are loaded: **● Live Tail** button (green, starts tail) / **■ Stop Tail** button (red, stops tail) + **↓ Auto** toggle (green when active, grey when off).
- [x] `src/ui/panels/timeline.rs` - `ScrollArea::stick_to_bottom(tail_active && tail_auto_scroll)` keeps the view pinned to newest entries while tail runs; user can scroll up freely and re-enable auto-scroll via the toggle.

**Test results: 45 unit tests + 9 E2E tests = 54 total, all passing**

---

## Increment 8: UI Polish, Log Summary, Tooltips & Filter Fixes
**Status: COMPLETE**

### Font & Encoding
- [x] `main.rs` - `configure_fonts()` rewritten: Consolas inserted at position 0 of the monospace family (primary), Segoe UI inserted at position 0 of the proportional family, Segoe UI Symbol + Segoe UI Emoji inserted as Unicode-range fallbacks. HashSet-based loading guard prevents double-insertion.

### Log Summary Panel
- [x] `ui/panels/log_summary.rs` (NEW) - Severity breakdown table listing each severity with count + percentage; collapsible per-severity sections (up to 50 message previews each). Colour-coded with `theme::severity_colour`. `sanitise_preview()` function replaces control characters (including binary/NUL) with U+FFFD and truncates to 140 chars. `Label::truncate()` prevents grid overflow from very long lines.
- [x] `ui/panels/mod.rs` - Exported `pub mod log_summary`
- [x] `app/state.rs` - `show_log_summary: bool` field added, initialized to `false`, cleared in `clear()`
- [x] `ui/panels/filters.rs` - "Summary" quick-button opens `show_log_summary` (disabled while no filtered entries)
- [x] `gui.rs` - View menu "Log Summary" item (disabled while no filtered entries); `log_summary::render()` call after summary render
- [x] `ui/panels/log_summary.rs` - `CollapsingState::load_with_default_open` with stable static string IDs replaces `CollapsingHeader::new(RichText)` — fixes sections that could not be recollapsed after expanding
- [x] `ui/panels/log_summary.rs` - `.open(&mut open)` for title-bar × close; `default_pos` near top of screen prevents window from spawning off-screen; `ScrollArea::max_height = available_height - 80` prevents window growing taller than the screen; `close_clicked` bool pattern avoids double-borrow of `open`

### Sidebar Layout
- [x] `gui.rs` - Sidebar split into two independent `ScrollArea`s: discovery panel (top ~45%), filters panel (remainder). Prevents one panel's growth from squashing the other.
- [x] `ui/panels/discovery.rs` - Discovery file list `max_height` raised from 200 px to 360 px.
- [x] `ui/theme.rs` - `SIDEBAR_WIDTH` raised from 250 to 290 px.

### File Path Tooltips
- [x] `ui/panels/discovery.rs` - `.on_hover_text(file.path.display())` on each file name row.
- [x] `ui/panels/filters.rs` - `.on_hover_text(path.display())` on each source-file checkbox.
- [x] `ui/panels/log_summary.rs` - Full path stored in preview tuple; filename extracted at render time; `.on_hover_text(full_path)` on each message cell.
- [x] `ui/panels/timeline.rs` - `on_hover_ui` shows full timestamp + full source path on hover.

### Source File Filter Fix
- [x] `core/filter.rs` - `hide_all_sources: bool` field added to `FilterState`. Represents the explicit "nothing selected" state that an empty whitelist cannot encode (empty = all pass). `is_empty()` and `matches_all()` both respect it.
- [x] `ui/panels/filters.rs` - None button: clears whitelist, sets `hide_all_sources = true`. Select all button: clears flag. Checkbox display: respects `hide_all_sources`. Individual uncheck: sets flag when unchecking the last file. Individual re-check: clears flag. Solo button: resets flag on both solo and un-solo.

**Test results: 45 unit tests + 9 E2E tests = 54 total, all passing**

---

## Increment 9: Bookmarks & Annotations
**Status: COMPLETE**

- [x] `core/filter.rs` - `bookmarks_only: bool` and `bookmarked_ids: HashSet<u64>` added to `FilterState`. `is_empty()` returns false when `bookmarks_only` is set. `matches_all()` excludes entries whose IDs are not in `bookmarked_ids` when `bookmarks_only` is true. Core remains pure: `bookmarked_ids` is populated by the app layer before each filter call.
- [x] `app/state.rs` - `bookmarks: HashMap<u64, String>` field (entry ID -> annotation label). New methods: `toggle_bookmark(id) -> bool`, `is_bookmarked(id) -> bool`, `bookmark_count() -> usize`, `clear_bookmarks()` (resets filter + refreshes), `bookmarks_report() -> String` (plain-text report sorted by entry ID, showing timestamp, severity, filename, first 200 chars of message, and label). `apply_filters()` populates `filter_state.bookmarked_ids` from `bookmarks` when `bookmarks_only` is true. `clear()` removes all bookmarks and resets `bookmarks_only`.
- [x] `ui/panels/timeline.rs` - Bookmark star button (★ / ☆) added to every row, between the 4 px file stripe and the entry text. Star is amber (★) when bookmarked, dim outline (☆) when not. Bookmarked rows receive a subtle gold background tint. Star click collected outside the `show_rows` closure (safe borrow pattern) and applied after the scroll area. In `bookmarks_only` mode, removing the last bookmark triggers an immediate `apply_filters()` so the row disappears from view.
- [x] `ui/panels/filters.rs` - "★ Bookmarks (N)" quick-filter toggle button: amber when active, dim when off. Activating it sets `bookmarks_only = true` and calls `apply_filters()`. Adjacent "× clear bm" button (shown when N > 0) calls `clear_bookmarks()`. "Clear" quick-filter button already resets `bookmarks_only` via `FilterState::default()`.
- [x] `gui.rs` - View menu "Copy Bookmark Report (N entries)" item (disabled when no bookmarks). Calls `bookmarks_report()`, copies text to clipboard via `ctx.copy_text()`, sets status bar confirmation message.
- [x] 2 new unit tests: `test_bookmark_filter_returns_only_bookmarked_entries`, `test_bookmark_filter_tracked_in_is_empty`

**Test results: 47 unit tests + 9 E2E tests = 56 total, all passing**

---

## Increment 10: Regex-Based Severity Override
**Status: COMPLETE**

- [x] `core/model.rs` - Added `severity_override: HashMap<Severity, Vec<regex::Regex>>` field to `FormatProfile`. Added `apply_severity_override(&self, text: &str) -> Option<Severity>` method that iterates severities in priority order (Critical → Error → Warning → Info → Debug) and returns the first matching result.
- [x] `core/profile.rs` - Added `SeverityOverrideDef` TOML deserialization struct (mirrors `SeverityMappingDef`). Added `#[serde(default)] pub severity_override: SeverityOverrideDef` field to `ProfileDefinition`. Added override compilation block in `validate_and_compile`: iterates all five severity levels and compiles each `Vec<String>` of regex patterns into `Vec<Regex>` using the existing `compile_regex` helper (same `MAX_REGEX_PATTERN_LENGTH` guard; invalid patterns fail compilation with an actionable `ProfileError`).
- [x] `core/parser.rs` - Replaced 4-line flat severity extraction with a 3-tier layered approach:
  1. `level` capture group present → `map_severity()` → if result is Unknown, try `apply_severity_override()` as a second chance → fall back to Unknown.
  2. No `level` capture group → `apply_severity_override()` first (regex precision) → `infer_severity_from_message()` substring fallback.
- [x] `profiles/plain_text.toml` - Added `[severity_override]` section with patterns for common bracket-style markers (`[CRIT]`, `[ERR]`, `[WARN]`) and Java exception class names that substring matching misses or over-broadly catches.
- [x] `profiles/generic_timestamp.toml` - Added `[severity_override]` section with the same bracket-style marker patterns.
- [x] 2 new unit tests in `core/profile.rs`:
  - `test_severity_override_regex_matching`: verifies `\[ERROR\]` requires literal brackets ("ERROR code 123" → None), `\bFAILED\b` uses word boundaries, correct severity mapping, no-match returns None.
  - `test_severity_override_absent_when_not_configured`: verifies `apply_severity_override` returns None when the map is empty.

**Test results: 49 unit tests + 9 E2E tests = 58 total, all passing**

---

## Increment 11: Time Correlation Window
**Status: COMPLETE**

- [x] `util/constants.rs` - Added `MIN_CORRELATION_WINDOW_SECS = 1` and `MAX_CORRELATION_WINDOW_SECS = 3600` named bounds (Rule 11 compliance).
- [x] `app/state.rs` - Added four new fields: `correlation_active: bool`, `correlation_window_secs: i64` (default 30), `correlation_window_input: String` (UI buffer), `correlated_ids: HashSet<u64>` (pre-computed overlay set). Added `update_correlation()` method that iterates **all** entries (not just filtered) to populate `correlated_ids` with IDs of entries whose timestamps fall within `[anchor - window, anchor + window]`. Entries with no timestamp cannot be anchors and are never included. Fields reset in `clear()`.
- [x] `ui/panels/timeline.rs` - Each row checks `state.correlated_ids.contains(&entry_id)` and renders a teal (`rgba(20,184,166,28)`) background tint for correlated rows. The tint is drawn before the gold bookmark tint so that bookmarked+correlated rows show the gold colour at higher visual priority. A `correlation_update_needed: bool` flag is collected inside `show_rows` and `state.update_correlation()` is called after the scroll area closes to avoid a `&mut self` conflict with the immutable entry borrow (same deferred-action pattern as the bookmark toggle).
- [x] `ui/panels/filters.rs` - Added **Correlation** section above the entry-count footer (only shown when entries are loaded). Contains: a toggle button (shows `+/-Ns` in teal when active, `Off` when inactive) that calls `update_correlation()` on toggle; an entry count badge (`N entries` in teal) visible when the overlay is populated; a `Window: [  30  ] sec` text input with Enter-to-commit and clamp to `[MIN, MAX]` bounds, resetting the buffer to the current valid value on bad input.
- [x] 3 new unit tests in `app/state.rs`:
  - `test_correlation_window_identifies_nearby_entries` — verifies exact in/out boundary (entries at -35s, -20s, 0s, +20s, +35s with 30s window).
  - `test_correlation_clears_when_disabled` — verifies `correlated_ids` is empty immediately after disabling.
  - `test_correlation_no_timestamp_entry_yields_empty_set` — verifies an anchor with no timestamp produces an empty set.
- [x] E2E test `e2e_correlation_window_highlights_nearby_entries` in `tests/e2e_discovery.rs` — loads the veeam_vbr fixture, parses real entries into `AppState`, activates correlation, verifies anchor is included, all correlated entries have timestamps, and disabling clears the set.

**Test results: 52 unit tests + 10 E2E tests = 62 total, all passing**

---

## Increment 12 — Persistent Sessions

**Goal**: Save and restore the current scan path, filter state, colour assignments, bookmarks, and correlation window across application restarts.

**Design decisions**:
- Session data stored as JSON in the platform data directory (`%APPDATA%\LogSleuth\session.json` on Windows).
- Log *entries* are intentionally **not** persisted — files are always re-parsed on restore so the view reflects current file contents.
- `initial_scan: Option<PathBuf>` field added to `AppState` — set at startup for session restores and consumed by `gui.rs` WITHOUT calling `clear()`, preserving the restored filter/colour/bookmark state during the re-scan. This is distinct from `pending_scan` which always calls `clear()` first (user-initiated scans).
- `Color32` serialised as `[u8; 4]` (RGBA bytes) since `egui::Color32` does not impl `Serialize`.
- Atomic write: session written to `.json.tmp` then renamed to `.json` — prevents corrupt state on crash mid-write.
- `load()` returns `Option<SessionData>` — all errors (missing file, malformed JSON, version mismatch) silently return `None` so app always starts cleanly.
- `SESSION_VERSION = 1` constant guards forward-compatibility; version mismatches discard the session with a warning log.

**Files changed**:
- [x] `src/app/session.rs` — **new file**. `SessionData` + `PersistedFilter` serde structs; `session_path()`, `save()`, `load()` public functions; `SESSION_VERSION: pub const u32 = 1`. 5 unit tests.
- [x] `src/util/constants.rs` — added `SESSION_FILE_NAME: &str = "session.json"`.
- [x] `src/app/mod.rs` — added `pub mod session;`.
- [x] `src/app/state.rs` — added `session_path: Option<PathBuf>` and `initial_scan: Option<PathBuf>` fields; initialised to `None` in `new()`; `initial_scan` cleared in `clear()` (session_path is never cleared); added `save_session()` (snapshots state to disk, silently fires-and-forgets errors) and `restore_from_session()` (reinstates scan_path, all filter fields, file_colours, bookmarks, and correlation window from a `SessionData`).
- [x] `src/main.rs` — after `AppState::new()`: sets `state.session_path` from `platform_paths.data_dir`; loads previous session via `session::load()`; calls `restore_from_session()` if successful; sets `state.initial_scan` when a saved scan_path exists; CLI `--path` argument overrides session path (sets both `scan_path` and `initial_scan`).
- [x] `src/gui.rs` — added `initial_scan` handler in `update()` loop (no `clear()` call); added `self.state.save_session()` after `ParsingCompleted` (saves state after each scan); added `fn on_exit()` override that calls `save_session()` when the window closes.
- [x] E2E test `e2e_session_save_restore_round_trip` in `tests/e2e_discovery.rs` — writes a session to a temp dir, reloads it, calls `restore_from_session()`, verifies scan_path / filter / bookmarks / correlation_window; also asserts `load()` returns `None` for a missing path.

**Test results: 57 unit tests + 11 E2E tests = 68 total, all passing**

---

---

## Increment 13: Troubleshoot Presets, Copy Filtered Results & Rolling Window Indicator
**Status: COMPLETE**

- [x] `src/util/constants.rs` - Added `MAX_CLIPBOARD_ENTRIES: usize = 10_000` — named bound for clipboard export operations (Rule 11 resource bounds compliance).
- [x] `src/app/state.rs` - Added `filtered_results_report() -> String` method: generates a plain-text clipboard report of all currently-filtered entries. Header includes generated timestamp and a human-readable filter description (severity, time window, text/regex terms). Each entry is rendered as `[timestamp] Severity  source\n  message`. Bounded to `MAX_CLIPBOARD_ENTRIES` with a truncation notice appended when the limit is reached.
- [x] `src/ui/panels/filters.rs` - Added **Err+Warn+15m** quick-preset button between "Errors + Warn" and "Clear". A single click sets severity to Critical+Error+Warning AND activates the 15-minute rolling relative-time window. Hover text explains that the window auto-advances during Live Tail.
- [x] `src/ui/panels/filters.rs` - Added green **● Rolling window (live)** label in the Time Range section, visible only when Live Tail is active and a relative-time window is set. Confirms to the user that new tail entries entering the window will appear automatically.
- [x] `src/ui/panels/filters.rs` - Added **📋 Copy** button in the entry-count footer row (next to the N/total label). Disabled when no filtered entries exist (Rule 16). Calls `filtered_results_report()`, copies to clipboard via `ctx.copy_text()`, sets status bar confirmation.
- [x] `src/gui.rs` - Added **View → Copy Filtered Results (N entries)** menu item. Disabled when filtered set is empty (Rule 16). Same report + clipboard + status pattern as the sidebar button.
- [x] `tests/e2e_discovery.rs` - 3 new tests:
  - `filtered_results_report_empty_state`: verifies empty state produces a well-formed report header with 0 entries.
  - `filtered_results_report_populated`: verifies Error+Warning entries appear, Info entries are excluded, count is correct, filter description mentions severity.
  - `filtered_results_report_truncation`: verifies `MAX_CLIPBOARD_ENTRIES + 1` entries produces a truncation notice citing the limit.

**Test results: 57 unit tests + 14 E2E tests = 71 total, all passing**

---

## Increment 14: Portable Windows EXE in Release Pipeline
**Status: COMPLETE**

- [x] `.github/workflows/release.yml` - Added `build-windows-portable` parallel job alongside the existing `build-windows` installer job:
  - Builds with `RUSTFLAGS="-C target-feature=+crt-static"` so the MSVC CRT is statically embedded in the binary — no Visual C++ Redistributable required on the target machine.
  - Renames output to `LogSleuth-{VERSION}-windows-portable.exe` (strips `v` prefix to match installer naming convention using PowerShell string replace).
  - Uploads as the `windows-portable` artifact.
- [x] `.github/workflows/release.yml` - `create-release` job updated: `needs` now includes `build-windows-portable`; `files` glob adds `LogSleuth-*-windows-portable.exe` so each GitHub Release publishes 4 artefacts: installer, portable EXE, macOS DMG, Linux AppImage.
- [x] `ATLAS.md` - Updated release.yml description to document the portable job and its static CRT rationale. Updated status to Increment 14.

**Test results: 57 unit tests + 14 E2E tests = 71 total, all passing (no new tests; pipeline change only)**

---

## Future Enhancements

### High Priority
- [x] **Persistent sessions** -- Save and restore the current set of loaded files, filter state, and colour assignments so a session can be resumed after reopening the application. *(Increment 12)*

### Medium Priority
- [ ] **Column visibility toggles** -- Allow the user to show/hide columns in the timeline (e.g. hide the filepath column when viewing a single file).
- [ ] **Export retains filter** -- Optionally export the full entry set (pre-filter) alongside the filtered export.
- [ ] **Profile editor UI** -- In-app wizard to create and test a new TOML profile without leaving LogSleuth.
- [ ] **Additional built-in profiles** -- Windows Event Log XML exports, Apache/nginx access logs, Docker/podman JSON logs, systemd journal exports.
- [ ] **Configurable max files / max entries** -- Surface `DEFAULT_MAX_FILES` and any entry cap as editable config values in the UI rather than compile-time constants only.

### Low Priority / Research
- [ ] **Parallel file parsing** -- Use rayon to parse multiple files concurrently on the background thread; the sort step already handles out-of-order results.
- [ ] **Full-text index** -- Build a Tantivy (or similar) in-memory index on scan completion to enable fast regex and phrase searching across millions of entries.
- [ ] **Plugin / WASM profile extensions** -- Allow format profiles to include embedded WASM functions for custom timestamp or severity extraction logic beyond what regex alone can express.
- [ ] **Network / remote log sources** -- Pull logs from a remote host via SSH or HTTP (structured log APIs).
