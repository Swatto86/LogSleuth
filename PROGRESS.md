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

## Increment 15: Sidebar Layout Fixes & Live Tail File-Selection Enforcement
**Status: COMPLETE**

- [x] `ui/theme.rs` - `SIDEBAR_WIDTH` raised from 290 to 320 px to give the filter button row enough width to render without overflow.
- [x] `gui.rs` - Added `.min_width(ui::theme::SIDEBAR_WIDTH)` to `SidePanel::left("sidebar")` so egui cannot restore a narrower width from session memory and the sidebar can never be dragged narrower than the filter buttons require.
- [x] `ui/panels/filters.rs` - Split the single `horizontal_wrapped` button block into **two rows**: Row 1 = severity presets (`Errors only` / `Errors + Warn` / `Err+Warn+15m` / `Clear`); Row 2 = utility actions (`Summary` / `★ Bookmarks` / `× clear bm`). Each row has its own full-width budget so no button is ever squashed.
- [x] `gui.rs` - Live Tail now **respects the source-file filter**: the `TailFileInfo` list is built by applying `hide_all_sources` and `source_files` whitelist semantics before calling `tail_manager.start_tail()`. Previously all discovered files were always tailed regardless of selection. Status bar message now shows the number of files actually being watched.

**Test results: 57 unit tests + 14 E2E tests = 71 total, all passing (no new tests; UI and logic fixes)**

---

## Increment 16: About Dialog
**Status: COMPLETE**

- [x] `ui/panels/about.rs` (NEW) - Centred, non-resizable egui `Window` showing the app icon character, app name, `v{VERSION}` (from `CARGO_PKG_VERSION` at compile time), short description, clickable GitHub repository hyperlink, MIT licence line, and "Built with Rust & egui". Dismissed by the title-bar × button; `open` flag wired to `state.show_about`.
- [x] `ui/panels/mod.rs` - Registered `pub mod about`.
- [x] `app/state.rs` - Added `show_about: bool` field (initialised `false`; not reset on `clear()` — independent of scan state).
- [x] `gui.rs` - Right-aligned greyed `ⓘ` frameless button in the menu bar (rendered before left-anchored `File`/`View` menus so egui allocates its space first). Hover text: "About LogSleuth". Click sets `state.show_about = true`.

**NOTE: The About button placement was subsequently found to be a bug — see Increment 19.**

**Test results: 57 unit tests + 14 E2E tests = 71 total, all passing (no new tests; UI only)**

---

## Increment 17: New Built-in Format Profiles
**Status: COMPLETE**

Expanded the built-in profile library from 9 to 14 profiles. All profiles are embedded at compile time via `include_str!` and are therefore available in the portable EXE without any external files.

- [x] `profiles/sql_server_error.toml` (NEW) - Microsoft SQL Server `ERRORLOG` / `ERRORLOG.N`. Fractional-second timestamp (`.nn`) excluded from captured group so `NaiveDateTime` parsing works cleanly. Component field captures the `Server`/`Logon`/`spidNN` token. Severity inferred from message keywords (explicit SQL severity numbers for Critical). File pattern detection: `ERRORLOG`, `ERRORLOG.[0-9]`, `ERRORLOG.[0-9][0-9]`.
- [x] `profiles/sql_server_agent.toml` (NEW) - SQL Server Agent `SQLAGENT.OUT` / `SQLAGENT.N`. Single-char level field: `!` = Error, `?`/`+` = Info. Message ID captured as thread field. File pattern detection: `SQLAGENT.OUT`, `SQLAGENT.[0-9]`.
- [x] `profiles/apache_combined.toml` (NEW) - Apache httpd and nginx Combined Log Format. Timezone offset matched but excluded from timestamp capture so `NaiveDateTime` parsing works. HTTP status code severity mapping via `severity_override` regex (`5xx` = Error, `4xx` = Warning). File patterns: `access.log`, `access_log`, `*-access.log`, `*_access.log`.
- [x] `profiles/nginx_error.toml` (NEW) - nginx error log (`YYYY/MM/DD HH:MM:SS [level] PID#TID: message`). Full `level` field: `emerg`/`alert`/`crit` = Critical, `error` = Error, `warn` = Warning, `notice`/`info` = Info, `debug` = Debug. Thread field captures `PID#TID`.
- [x] `profiles/windows_dhcp.toml` (NEW) - Windows Server DHCP daily CSV logs (`DhcpSrvLog-*.log`, `DhcpV6SrvLog-*.log`). Date and time columns combined into a single timestamp capture group. Event ID captured as thread. Severity inferred from description keywords (nack/conflict/not authorized = Error; expired/unreachable = Warning).
- [x] `src/core/profile.rs` - All 5 new profiles registered in `builtin_profile_sources()` between `log4j_default.toml` and `generic_timestamp.toml`.

**Test results: 57 unit tests + 14 E2E tests = 71 total, all passing (`test_load_builtin_profiles` validates all 14 profiles compile cleanly)**

---

## Increment 18: Sidebar Width + Intune Profile + Menu Bar Fix
**Status: COMPLETE**

- [x] `ui/theme.rs` - `SIDEBAR_WIDTH` raised from 320 to 380 px so all four Row 1 filter buttons ("Errors only" / "Errors + Warn" / "Err+Warn+15m" / "Clear") fit on a single line without wrapping.
- [x] `gui.rs` - Updated `.default_width` and `.min_width` to use the new `SIDEBAR_WIDTH` constant.
- [x] `profiles/intune_ime.toml` (NEW) - Microsoft Intune Management Extension CMTrace format. Covers `IntuneManagementExtension*.log`, `AgentExecutor.log`, `SidecarExtension.log`, `EventCollector.log`. `type=` attribute drives severity (1=Info, 2=Warning, 3=Error); `component=` attribute populates the component column; `thread=` captures the thread ID. `timestamp` captures `HH:MM:SS` only (the `date=` attribute follows `time=` in the line, making a combined capture impractical).
- [x] `src/core/profile.rs` - `intune_ime.toml` registered in `builtin_profile_sources()` (omitted from prior increment; was never active at runtime).
- [x] `README.md` - Added Intune profile to the built-in profiles table.
- [x] `ATLAS.md` - `intune_ime.toml` entry added to profiles list in repo structure.

**Test results: 57 unit tests + 14 E2E tests = 71 total, all passing**

---

## Increment 19: Menu Bar About Button Placement Bug Fix
**Status: COMPLETE**

Root cause: the `with_layout(right_to_left, ...)` block for the ⓘ About button was allocated first inside `egui::menu::bar`, consuming all remaining horizontal space before File and View were rendered. Both menus were allocated zero width and never appeared.

Fix: move the About button block to **after** the View menu so egui allocates File and View their space first and the right-to-left block fills the remainder.

- [x] `gui.rs` - Removed `with_layout(right_to_left, ...)` block from before `ui.menu_button("File", ...)`. Re-added it after the closing `});` of the View menu, with a code comment explaining why the order is load-bearing. Now File → View → ⓘ render correctly.
- [x] `README.md` - Fixed "File > Scan Summary" (incorrect) → "View > Scan Summary"; removed erroneous `Ctrl+S` shortcut.
- [x] `ATLAS.md` - Updated `about.rs` entry to document correct button placement semantics.

**Test results: 57 unit tests + 14 E2E tests = 71 total, all passing (no new tests; menu bar regression fix)**

---

## Increment 20: Log Location Tooltips, 3 New Built-in Profiles, Documentation
**Status: COMPLETE**

### Log location tooltips

Added an optional `log_locations` array field to all TOML format profiles. These strings appear as a hover tooltip on the profile confidence label next to each file in the discovery panel — allowing the user to quickly find where to look for more logs of that type.

- [x] `core/model.rs` - Added `log_locations: Vec<String>` to `FormatProfile`.
- [x] `core/profile.rs` - Added `log_locations: Vec<String>` (serde default = empty) to `ProfileMeta`; `validate_and_compile` passes the field through to `FormatProfile`.
- [x] All 18 profiles - Added `log_locations = [...]` to the `[profile]` section with platform-specific paths, `%EnvVar%` notation for Windows paths, and fallback notes for generic/application-defined formats.
- [x] `ui/panels/discovery.rs` - Profile label widget now stores the egui `Response` and calls `.on_hover_text(...)` with "Default log locations:\n{list}" when the matched profile's `log_locations` is non-empty. Profiles are looked up from `state.profiles` by ID (disjoint field borrows; no extra data copying).

### New built-in profiles (18 total, up from 14)

- [x] `profiles/windows_cluster.toml` (NEW) - Windows Failover Cluster service log (`cluster.log`). Format: `HexPID.HexTID::YYYY/MM/DD-HH:MM:SS.mmm  LEVEL  message`. Thread field captures the hex PID.TID; milliseconds excluded from timestamp capture. Severity: INFO / WARN / ERR / DBG.
- [x] `profiles/kubernetes_klog.toml` (NEW) - Kubernetes klog format used by kube-apiserver, kubelet, kube-proxy, etc. Single-char level prefix (I/W/E/F). **Timestamp limitation documented**: klog omits the year; timestamp capture is intentionally omitted so entries degrade gracefully to no-timestamp rather than producing wrong timestamps. Modern Kubernetes with `--logging-format=json` is handled by the existing `json-lines` profile.
- [x] `profiles/exchange_tracking.toml` (NEW) - Microsoft Exchange Server message tracking CSV logs (`MSGTRK*.LOG`). ISO 8601 timestamp (fractional/Z stripped from capture); rest of CSV row is the message. `severity_override` patterns classify FAIL/BADMAIL/QUARANTINE as Error, DELAY/REDIRECT as Warning, RECEIVE/DELIVER/SEND as Info.
- [x] `src/core/profile.rs` - All 4 new/fixed profiles (intune_ime was already added in Inc 18; windows_cluster, kubernetes_klog, exchange_tracking are new) registered in `builtin_profile_sources()` between `windows_dhcp` and `generic_timestamp`.

**Test results: 57 unit tests + 14 E2E tests = 71 total, all passing (`test_load_builtin_profiles` validates all 18 profiles compile cleanly)**

---

## Increment 21: 4 New Built-in Profiles, 3 Bug Fixes
**Status: COMPLETE**

### Bug fixes

- [x] `src/gui.rs` - Fixed wrong filename in the module-level comment (`// LogSleuth - app.rs` → `// LogSleuth - gui.rs`).
- [x] `src/core/filter.rs` - `matches_all()`: Text search and regex search now also check `entry.thread` and `entry.component` metadata fields in addition to `entry.message`. Previously, typing a thread ID or component name in the search box would not match any entries. Both exact-substring and fuzzy-subsequence modes benefit from the fix.
- [x] `src/ui/panels/filters.rs` - Preset buttons "Errors only", "Errors + Warn", and "Err+Warn+15m" now correctly preserve the `hide_all_sources` flag alongside `source_files` when a file filter was active. Previously, clicking a preset after selecting "None" in the file filter would silently reset the source-file filter to "all pass", unexpectedly showing entries from all files.

### New built-in profiles (22 total, up from 18)

- [x] `profiles/postgresql_log.toml` (NEW) - PostgreSQL server log. Default `log_line_prefix '%m [%p] '` format: `YYYY-MM-DD HH:MM:SS[.mmm] [TZ] [PID] LEVEL:  message`. TZ component optional (excluded from timestamp capture). Severity: PANIC → Critical; ERROR/FATAL → Error; WARNING → Warning; LOG/INFO/NOTICE/DETAIL/HINT/CONTEXT/STATEMENT → Info; DEBUG1-5 → Debug. `continuation` multiline mode for embedded stack traces.
- [x] `profiles/tomcat_catalina.toml` (NEW) - Apache Tomcat / Catalina log. `java.util.logging` format: `DD-Mon-YYYY HH:MM:SS.mmm LEVEL [thread] logger.class.Method message`. Severity: SEVERE → Error; WARNING → Warning; INFO/CONFIG → Info; FINE/FINER/FINEST → Debug. `continuation` multiline mode for Java exception traces.
- [x] `profiles/sccm_cmtrace.toml` (NEW) - Microsoft SCCM / ConfigMgr CMTrace-format logs. Same wire format as the intune-ime profile (`<![LOG[...]LOG]!>` envelope) but targets SCCM/MECM client and server log file names (`smsts.log`, `AppEnforce.log`, `CcmExec.log`, `PolicyAgent.log`, `WUAHandler.log`, etc.). File-pattern matching differentiates from intune-ime on Intune log files; on unknown CMTrace files the profile registered first wins.
- [x] `profiles/windows_firewall.toml` (NEW) - Windows Firewall packet log (`pfirewall.log`). W3C-style format: `YYYY-MM-DD HH:MM:SS ACTION PROTOCOL src-ip dst-ip src-port dst-port ...`. `#` header lines discarded by `skip` multiline mode. Severity: DROP/DROP-ICMP → Warning; ALLOW/ALLOW-ICMP → Info.
- [x] `src/core/profile.rs` - All 4 new profiles registered in `builtin_profile_sources()` between `exchange_tracking` and `generic_timestamp`.
- [x] `ATLAS.md` / `README.md` / `PROGRESS.md` - Updated status, profile count, profiles list, and README profile table.

**Test results: 57 unit tests + 14 E2E tests = 71 total, all passing (`test_load_builtin_profiles` validates all 22 profiles compile cleanly)**

---

## Increment 22: 2 Bug Fixes (Explorer /select arg, per-frame clone)
**Status: COMPLETE**

### Bug fixes

- [x] `src/ui/panels/detail.rs` - **"Show in folder" on Windows never highlighted the file.** `explorer /select,<path>` must be passed as a single argument token. The previous code used `.arg("/select,").arg(&entry.source_file)`, which passes two separate argv entries to `explorer.exe`. Windows Explorer does not concatenate them, so it received `/select,` as one argument and the path as another — silently opening the default folder without selecting the file. Fixed by using `.arg(format!("/select,{}", entry.source_file.display()))` to produce one combined arg.

- [x] `src/ui/panels/summary.rs` - **Scan Summary window cloned `ScanSummary` on every render frame (≈60×/sec while open).** The `if let Some(summary) = state.scan_summary.clone()` pattern cloned the full struct (including `Vec<FileSummary>` with one entry per scanned file) on every frame. Changed to `if let Some(ref summary) = state.scan_summary` so the render borrows the data without copying it.

**Test results: 57 unit tests + 14 E2E tests = 71 total, all passing**

---

## Increment 23: 2 Bug Fixes (CLI --filter-level, dead severity_bg_colour)
**Status: COMPLETE**

### Bug fixes

- [x] `src/main.rs` — **`--filter-level` CLI argument was declared in the `Cli` struct and parsed by clap but the value was never read or applied after `let cli = Cli::parse()`.** Running `logsleuth --filter-level warning` silently ignored the flag and started with no severity filter. Fixed by adding a block after the `cli.path` override that parses the string case-insensitively against `Severity::all()` labels, then populates `state.filter_state.severity_levels` with the requested level and all more-severe variants (e.g. `--filter-level warning` enables Critical + Error + Warning). Unknown strings emit a `tracing::warn!` noting the valid values.

- [x] `src/ui/panels/timeline.rs` + `src/ui/theme.rs` — **`theme::severity_bg_colour()` was defined but never called anywhere in the codebase.** The function returns a subtle semi-transparent background tint for Critical (dark red), Error (red), and Warning (amber) rows, and `None` for all other severities. It was clearly intended for row background colouring but was never wired up. Fixed by calling it at the start of each row's paint phase in `timeline.rs`, painting the severity tint as the lowest-priority background layer (drawn first so the existing correlated-teal and bookmarked-gold tints still take visual precedence over it).

**Test results: 57 unit tests + 14 E2E tests = 71 total, all passing**

---

## Increment 24: 3 Bug Fixes, 2 Dead-Code Removals, 1 Regression Test
**Status: COMPLETE**

### Bug fixes

- [x] `src/gui.rs` — **Append scans broke chronological order in the unified timeline.** When "Add File(s)…" was used to append additional files to an existing session, the `ParsingCompleted` handler called `apply_filters()` instead of `sort_entries_chronologically()`. The background thread sorts entries within the newly appended batch, but `apply_filters()` does not re-sort entries already in `state.entries`. The result was all newly added entries appearing at the bottom of the timeline in a separate sorted block rather than being interleaved chronologically with existing entries. Fixed by calling `state.sort_entries_chronologically()` (which performs a stable sort across all of `state.entries`, then calls `apply_filters()`) so the full combined entry set is always in chronological order after every append.

- [x] `src/app/state.rs` — **`apply_filters()` silently shifted the timeline selection to the wrong entry when a filter change altered display positions.** The code saved `selected_index` as a bare integer display-position before recomputing `filtered_indices`, then bounds-checked it against the new list length and used it directly. Any filter change that removed rows above the selected entry would point `selected_index` at a completely different entry with no indication. Fixed by capturing `selected_id = self.selected_entry().map(|e| e.id)` before the filter recompute and restoring `selected_index` afterwards by re-finding that ID via `.position(|&i| self.entries.get(i).is_some_and(|e| e.id == id))`, preserving the selection by stable entry ID.

- [x] `src/core/parser.rs` — **`MultilineMode::Raw` produced a spurious `LineParse` error for every non-matching line.** Raw mode is designed to emit each line as a separate entry regardless of whether it matches the profile regex; any non-matching line is packaged as a plain-text entry. The old error-tracking condition (`if profile.multiline_mode != Continuation || entries.is_empty()`) treated Raw the same as Skip and recorded a `LineParse` error in addition to creating the entry. This inflated parse error counts and polluted the scan summary. Fixed with an explicit `match` on `multiline_mode`: `Raw => false` (no error), `Skip => true`, `Continuation => entries.is_empty()`.

### Dead-code removal

- [x] `src/app/state.rs` + `src/gui.rs` + `src/ui/panels/timeline.rs` — **`AppState::tail_scroll_to_bottom` was dead state that was never acted upon.** The field was set in `gui.rs` when new tail entries arrived with auto-scroll enabled, and cleared in `timeline.rs` — but `timeline.rs` never used its value to trigger anything. The actual auto-scroll behaviour has always been driven entirely by `ScrollArea::stick_to_bottom(tail_active && tail_auto_scroll)`. Removed the field from the struct, `new()`, and `clear()`; removed the conditional set in `gui.rs`; removed the unused read/clear block in `timeline.rs`.

- [x] `src/core/model.rs` — **`ScanSummary::entries_by_severity: HashMap<Severity, usize>` was a dead field that was never populated or read.** The field was present on the struct (deriving `Default`) but `app::scan` always constructed `ScanSummary` via `..Default::default()`, leaving the map empty. Neither `ui::panels::summary` nor `ui::panels::log_summary` (nor any other UI panel) read it — both compute their own severity counts from `filtered_indices` at render time. Removed the field entirely; the `..Default::default()` construction in `scan.rs` continues to work without change.

### Regression test

- [x] `src/app/state.rs` — Added `test_apply_filters_preserves_selection_by_id`: creates five entries with alternating Info/Error severities, selects entry id=3 at display position 3 (unfiltered), applies an Error-only filter which shifts id=3 to display position 1, and asserts that `selected_entry().id == 3` and `selected_index == Some(1)`. This test fails on the pre-fix code and passes on the corrected `apply_filters()`.

**Test results: 58 unit tests + 14 E2E tests = 72 total, all passing**

---

## Increment 25: Live Tail Timestamp Fix, Date Filter UX, Veeam Profile Improvements
**Status: COMPLETE**

### Bug fixes

- [x] `src/app/tail.rs` — **Live tail entries with no parsed timestamp were silently swallowed by the relative-time filter.** `filter.rs` unconditionally returns `false` for entries whose `timestamp` is `None` when a time filter (`time_start` / `time_end`) is active. Files assigned the `plain-text` fallback profile have no `timestamp` capture group, so every entry arrives with `timestamp: None`. With the "last 15 minutes" filter set, *all* plain-text tail entries were excluded — the user saw "watching N files" in the status bar but zero new entries appeared in the timeline. Fixed by back-filling `entry.timestamp = Some(Utc::now())` in `run_tail_watcher` for any entry where the parser produced `None`. This is accurate: the entry was just appended to the file, so the observation time is correct. The fix also improves sort order (these entries now appear chronologically with other recent events rather than sorted to end-of-timeline). Added `use chrono::Utc;` to imports.

### UI improvements

- [x] `src/ui/panels/discovery.rs` — **Date filter moved above the Open Directory / Open Log(s) buttons.** Previously the date filter appeared below the open buttons, so users would often miss it and skip setting it before triggering a scan. It now appears between the current-path label and the scan controls, matching the "configure before you scan" workflow. Added an explanatory sub-label: *"Only scans files modified on or after this date. Leave blank to scan all files."* Feedback label wording updated from "Files modified on or after… UTC" to "Scanning files modified on or after… UTC" for clarity.

### Profile improvements

- [x] `profiles/veeam_vbr.toml` — Expanded `[detection] file_patterns` to cover more Veeam Backup components found in `%ProgramData%\Veeam\Backup\`: added `BackupCopy.*.log`, `EP.*.log`, `Endpoint.*.log`, `Veeam*.log` (catches `VeeamCatalog.log`, `VeeamMount.log`, etc.), and `Mount.*.log`. The filename-pattern +0.2 confidence bonus now triggers for a much wider set of VBR-format files.
- [x] `profiles/veeam_vbr.toml` — Added `[severity_override]` section with regex patterns (`\bFailed\b`, `\bException\b`, `Unable to`, `Cannot `, etc.) as a second-chance classifier. This handles: (a) continuation lines whose message text contains severity indicators, (b) level strings not in `severity_mapping` (e.g. "Success", "Trace"), and (c) Veeam log lines parsed by the plain-text fallback where `infer_severity_from_message` might produce `Info` instead of `Error`.
- [x] `profiles/veeam_vbr.toml` — Moved `"Failed"` out of `severity_mapping.error` (where it could only match if the captured `level` field was literally "Failed", which VBR never emits) and into `severity_override` as a regex pattern `\bFailed\b` (where it correctly fires on message text). Added `"Success"` → Info and `"Trace"` → Debug to the severity mapping for completeness.

**Test results: 58 unit tests + 14 E2E tests = 72 total, all passing**

---

## Increment 26: File-mtime-based Time Filtering
**Status: COMPLETE**

The relative-time filter (15m / 1h / 6h / 24h) now uses the **OS last-modified time of the source file** rather than the parsed log timestamp embedded in each log line.  All other filters (severity, text, regex, source-file) still operate on log content.

- [x] `core/model.rs` — Added `file_modified: Option<DateTime<Utc>>` to `LogEntry` (`#[serde(skip)]`; not exported to CSV/JSON). All construction sites initialize to `None`; app layer stamps the real value post-parse.
- [x] `core/filter.rs` — `matches_all()` time-range guard now compares `entry.file_modified` instead of `entry.timestamp`. An entry passes the time filter iff its source file was modified within the window.
- [x] `core/parser.rs` — Both `LogEntry` construction sites initialize `file_modified: None` (app-layer responsibility, keeps core I/O-free).
- [x] `app/scan.rs` — After `parse_content`, stamps `entry.file_modified = file.modified` (OS mtime captured at discovery time) on every entry for the file.
- [x] `app/tail.rs` — Expands the per-tick `metadata()` call to capture both `.len()` and `.modified()`. All entries from a tail tick receive `entry.file_modified = current_file_mtime`. Timestamp back-fill (`Utc::now()` for `None`-timestamp entries) retained.
- [x] All test `make_entry()` helpers updated; time-filter unit tests updated to set `file_modified`; test renamed `test_time_filter_excludes_entries_without_file_mtime`.

**Test results: 58 unit tests + 14 E2E tests = 72 total, all passing**

---

## Increment 27: Configurable Options Dialog + Documentation Update
**Status: COMPLETE**

Expanded the Options dialog with three sections of user-configurable runtime settings, threaded all values end-to-end through the scan/tail/dir-watch pipeline, and brought README + ATLAS current.

### New settings

| Setting | Default | Where applied |
|---------|---------|---------------|
| Max total entries | 1,000,000 | Next scan — stops ingesting once cap reached |
| Max scan depth | 10 | Next scan + directory watch start |
| Tail poll interval | 500 ms | Next Live Tail session start |
| Dir watch poll interval | 2,000 ms | Next Directory Watch session start |

### File changes

- [x] `src/util/constants.rs` — Added `MIN_MAX_FILES`, `MIN_MAX_TOTAL_ENTRIES`, `ABSOLUTE_MAX_TOTAL_ENTRIES`, `MIN_TAIL_POLL_INTERVAL_MS`, `MAX_TAIL_POLL_INTERVAL_MS`, `MIN_DIR_WATCH_POLL_INTERVAL_MS`, `MAX_DIR_WATCH_POLL_INTERVAL_MS` as named-constant bounds for the new sliders.
- [x] `src/core/discovery.rs` — Added `max_total_entries: usize` field to `DiscoveryConfig`; default initialised to `constants::MAX_TOTAL_ENTRIES`.
- [x] `src/app/scan.rs` — `run_parse_pipeline` now accepts `max_total_entries: usize` parameter (replaces hardcoded constant); threaded through from `run_scan` (using `config.max_total_entries`) and `run_files_scan` (using passed value). `start_scan_files` now accepts `max_total_entries: usize`. `#[allow(clippy::too_many_arguments)]` on `run_parse_pipeline`.
- [x] `src/app/tail.rs` — `start_tail(&mut self, files, entry_id_start, poll_interval_ms: u64)` and `run_tail_watcher(..., poll_interval_ms: u64)`; removed unused `TAIL_POLL_INTERVAL_MS` import.
- [x] `src/app/dir_watcher.rs` — Added `poll_interval_ms: u64` to `DirWatchConfig` (default: `DIR_WATCH_POLL_INTERVAL_MS`); `run_dir_watcher` uses `config.poll_interval_ms` instead of the constant; removed unused `DIR_WATCH_POLL_INTERVAL_MS` import.
- [x] `src/app/state.rs` — Added 4 new option fields: `max_total_entries`, `max_scan_depth`, `tail_poll_interval_ms`, `dir_watch_poll_interval_ms`; all initialised in `new()` from constants; **excluded from `clear()`** (user preferences survive session resets).
- [x] `src/gui.rs` — All `DiscoveryConfig` constructions now set `max_depth` and `max_total_entries` from state; `start_tail` call passes `state.tail_poll_interval_ms`; `dir_watcher.start_watch` passes `DirWatchConfig { poll_interval_ms: state.dir_watch_poll_interval_ms, ..default() }`; repaint intervals use state values instead of constants; all three `start_scan_files` call sites pass `state.max_total_entries`.
- [x] `src/ui/panels/options.rs` — Full rewrite: 3 sections (Ingest Limits, Live Tail, Directory Watch) each with logarithmic sliders and Reset buttons.
- [x] `README.md` — New "Directory Watch" section; new "Options" section with tables for all configurable settings.
- [x] `ATLAS.md` — Status bumped to Increment 27; `options.rs`, `constants.rs`, `state.rs`, `tail.rs`, `dir_watcher.rs` descriptions updated.

**Test results: 61 unit tests + 14 E2E tests = 75 total, all passing. Zero clippy warnings.**

---

## Increment 28: DirWatcher Date Filter, Severity Underline, File List Perf, Select-All Fix
**Status: COMPLETE**

Four targeted improvements following an internal audit.

### Changes

- [x] `src/app/dir_watcher.rs` — `DirWatchConfig` gains `modified_since: Option<DateTime<Utc>>` (default `None`). `walk_for_new_files()` applies the same fail-open OS-mtime gate as `core::discovery::discover_files`: any newly detected file whose mtime predates `modified_since` is skipped; files whose mtime cannot be read are included. `gui.rs` passes `state.discovery_modified_since()` when starting the watcher so the live watcher always honours the date filter set in the discovery panel.
- [x] `tests` — New `test_walk_respects_modified_since` regression test covering: future cutoff (file rejected), past cutoff (file accepted), `None` (file accepted). Existing tests updated for new 6-arg `walk_for_new_files` signature.
- [x] `src/ui/panels/timeline.rs` — Replaced the full-row severity background tint (Critical/Error/Warning) with a **2 px underline** drawn at the bottom of the row in the row's `sev_colour`. Removes the visually heavy wash while preserving a clear per-row severity indicator.
- [x] `src/ui/theme.rs` — Removed `severity_bg_colour()` function (only caller was `timeline.rs`). `severity_colour()` is now also used for the underline accent.
- [x] `src/ui/panels/filters.rs` — Source-file checklist switched from `ScrollArea::show()` (lays out all N widgets every frame) to `ScrollArea::show_rows()` (virtual scroll). Per-frame rendering cost is now O(visible rows ≈ 9) regardless of how many files are loaded — a significant improvement when 1 000+ files are present.
- [x] `src/ui/panels/filters.rs` — Fixed "Select All" button: it was calling `.remove()` on the whitelist `source_files` (treating it as a blacklist). Now builds a new whitelist of all visible files plus any previously-selected non-visible files, symmetric with the existing "None" handler. Compact all-pass form is used when the result covers all files.
- [x] `ATLAS.md`, `README.md` — Updated status, Directory Watch domain concept, `dir_watcher.rs` / `timeline.rs` / `theme.rs` / `filters.rs` descriptions and the Directory Watch section in the README.

**Test results: 62 unit tests + 17 E2E tests = 79 total, all passing. Zero clippy warnings.**

---

## Increment 29: Sidebar Restructure — Tab-Based, Resizable, Unified File List
**Status: COMPLETE**

Replaced the cramped fixed-height 45/55 split sidebar with a tab-based, resizable layout that scales cleanly from small to large file sets.

### Layout changes

- [x] `src/ui/theme.rs` — `SIDEBAR_WIDTH` raised to **460 px** (default open width); sidebar is now resizable from 300 px to 800 px via `egui::SidePanel::left.default_width(460).min_width(300).max_width(800)`.
- [x] `src/gui.rs` — Sidebar converted to a **tab bar** (`Files` | `Filters`). Each tab has a single `ScrollArea` filling the tab body — no more dual-scroll 45/55 height split. The `Filters` tab label displays a **●** bullet dot when any filter is active.
- [x] `src/ui/panels/discovery.rs` — Files tab content restructured:
  - Scan controls (path label, date-filter row, quick-fill buttons, Open Directory / Open Log(s) / Clear buttons) moved inside a **`CollapsingHeader`** (`default_open = true`) so the file list dominates once a scan has run.
  - Duplicate source-file listing **eliminated** — the Files tab now shows a **single unified file list** with inline dot, checkbox, filename, solo button and profile label. Hovering a row shows full path + formatted size.
  - `All` / `Live Tail` / search-box / `Select All` / `None` controls placed above the virtual-scroll list.
  - Source-file filter state driven directly from this list (whitelist semantics unchanged).
- [x] `src/ui/panels/filters.rs` — **Source-file filter section removed** (now lives in discovery.rs Files tab). Filters tab now contains only: quick-filter preset buttons, severity checkboxes, text/regex inputs, relative time section, correlation section, and entry-count footer.
- [x] `src/app/state.rs` — Added `sidebar_tab: usize` field (0 = Files, 1 = Filters). Pure UI state; **not persisted in session.json** and **not cleared in `clear()`** so the user's tab position survives scans.

### Date-input parsing

- [x] `src/app/state.rs` — `discovery_modified_since() -> Option<DateTime<Utc>>` refactored to parse four input forms: *YYYY-MM-DD HH:MM:SS*, *YYYY-MM-DD HH:MM*, *YYYY-MM-DD* (interpreted as 00:00:00 UTC), and empty string (returns `None`). Returns `None` on any parse failure so bad input never silently corrupts the filter.
- [x] 5 new unit tests in `src/app/state.rs`:
  - `test_discovery_modified_since_date_only_midnight_utc`
  - `test_discovery_modified_since_date_and_hour_minute`
  - `test_discovery_modified_since_full_datetime_to_second`
  - `test_discovery_modified_since_empty_returns_none`
  - `test_discovery_modified_since_invalid_returns_none`

### New E2E tests (10)

Added a suite of `e2e_dlogs_*` tests exercising multi-file discovery, auto-detection, and timeline assembly:

- `e2e_dlogs_discovers_many_files`
- `e2e_dlogs_full_pipeline_smoke`
- `e2e_dlogs_fresh_state_has_no_source_filter`
- `e2e_dlogs_veeam_vbr_auto_detects`
- `e2e_dlogs_vbr_severity_is_not_uniform_info`
- `e2e_dlogs_vbr_multiple_files_each_contribute_entries`
- `e2e_dlogs_vbo365_auto_detects`
- `e2e_dlogs_syslog_auto_detects`
- `e2e_dlogs_iis_w3c_auto_detects`
- `e2e_dlogs_sql_agent_auto_detects`

Also `e2e_vbr_service_log_filenames_detect_via_filename_match` — filename-bonus auto-detection regression.

**Test results: 68 unit tests + 27 E2E tests = 95 total, all passing. Zero clippy warnings.**

---

## Increment 30: 5 Bug Fixes & Regression Tests
**Status: COMPLETE**

Five defects identified and fixed following the sidebar restructure, each with a regression test where testable.

### Bug fixes

1. **`File → Open Directory` ignored `discovery_date_input` date filter.** The menu-bar handler called `state.clear()` (which blanked `discovery_date_input`) and then read the date field — always reading an empty string. Fixed by reading `discovery_date_input` **before** calling `clear()`, then assigning it back after.
   - Regression test: `e2e_discovery_date_filter_is_honoured_by_menu_open_directory` — opens a temp directory via the same code path and asserts only files modified on or after the supplied cutoff are discovered.

2. **Append scans produced duplicate entry IDs.** `ScanManager::start_scan_files` called `run_parse_pipeline` with `entry_id_start = 0` — resetting the ID counter to zero on every append. Fixed by threading `entry_id_start` through both `start_scan_files` and `run_files_scan`, and having gui.rs pass `state.next_entry_id()`.
   - Regression test: `e2e_append_scan_entry_ids_are_unique_across_parses` — runs two sequential parse pipelines with the correct `entry_id_start` value for the second call and asserts no ID is present in both result sets.

3. **Ghost correlation highlights after filter changes.** `state.apply_filters()` recomputed `filtered_indices` but did not refresh `correlated_ids`, so entries removed from the filter continued to show a teal tint on the next render frame. Fixed by calling `self.update_correlation()` at the end of `apply_filters()`.
   - Regression test: `test_apply_filters_clears_correlation_when_selected_entry_is_hidden` (unit) — selects an entry, activates correlation, applies a severity filter that hides the anchor entry, and asserts `correlated_ids` is empty.

4. **`discovery_date_input` not persisted in session.** The field was set by the user in the discovery panel but never written to or read from `SessionData`, so filtering by modification date was silently lost on restart. Fixed by adding `discovery_date_input: String` (serde default `""`) to `SessionData`, serialising it in `save_session()`, and restoring it in `restore_from_session()`.
   - Covered by the existing `e2e_session_save_restore_round_trip` test (updated to include the field).

5. **Menu bar `File` items remained enabled during an active scan.** `File → Open Directory`, `File → Open Log(s)…`, and `File → Add File(s)…` were rendered unconditionally. Starting a new scan while one was already running could corrupt `state.entries`. Fixed with `ui.add_enabled(!scanning, ...)` wrappers; hover text explains why the items are disabled.
   - UI-only change; no automated test.

**Test results: 69 unit tests + 29 E2E tests = 98 total, all passing. Zero clippy warnings.**

---

## Increment 31: Font Pre-Load Fix (DevWorkflow Rule 16 Compliance)
**Status: COMPLETE**

Font file I/O was occurring inside the `eframe::run_native` creator closure — **after** the OS window was already open and displaying a white background — violating DevWorkflow Rule 16 ("All expensive initialisation MUST complete before calling `eframe::run_native()`").

- [x] `src/main.rs` — Renamed `configure_fonts(ctx: &egui::Context)` → `build_font_definitions() -> egui::FontDefinitions`. All `std::fs::read` calls for Consolas, Segoe UI, Segoe UI Symbol, and Segoe UI Emoji now execute **before** `eframe::run_native`. The font byte data is loaded into a `FontDefinitions` struct and moved into the closure via capture.
- [x] `src/main.rs` — The creator closure is now trivial: `cc.egui_ctx.set_fonts(font_defs)` followed by `Ok(Box::new(gui::LogSleuthApp::new(state)))`. No I/O, no computation — eliminates the white-flash startup artifact on Windows.

**Test results: 69 unit tests + 29 E2E tests = 98 total, all passing. Zero clippy warnings.**

---

## Increment 32: Time Range Filter Bug Fix
**Status: COMPLETE**

**Bug**: The relative-time filter (15m / 1h / 6h / 24h buttons and custom minute input) was filtering on `entry.file_modified` — the OS last-modified time of the *source file* — rather than the parsed log event timestamp inside each entry. This meant:
- "Last 15 minutes" showed **all entries from files whose mtime was recent**, not entries whose log event timestamp fell in the last 15 minutes.
- A large log file continuously written to (e.g. a live application log) would always show its entire history when any time filter was active.
- Structured log formats with accurate parsed timestamps (Veeam, IIS, SQL Server, syslog, etc.) were all affected.

**Fix** (`src/core/filter.rs`):
- The time range predicate now resolves an **effective time** for each entry: `entry.timestamp` (parsed log event time) is used when available; `entry.file_modified` (OS mtime) is used as a fallback only for plain-text/no-timestamp entries.
- Entries with neither a parsed timestamp nor a file mtime are excluded from any time-bounded view (unchanged).
- The two existing time-filter unit tests were updated to reflect the corrected semantics; `test_time_filter_excludes_entries_without_file_mtime` was renamed `test_time_filter_uses_file_mtime_fallback_when_no_timestamp` and extended to verify all three cases: timestamp present, mtime fallback, and neither.
- `ATLAS.md` `filter.rs` description updated.

**Test results: 69 unit tests + 29 E2E tests = 98 total, all passing. Zero clippy warnings.**

---

## Increment 33: Timeline Sort Order (Ascending / Descending)
**Status: COMPLETE**

**Feature**: Added a compact sort-order toggle button above the timeline scroll area, allowing the user to switch between **ascending (oldest first)** and **descending (newest first)** display order without altering the underlying data structures.

**Design decisions**:
- `state.entries` and `state.filtered_indices` always remain in **ascending chronological order** — the sort toggle is purely a display-layer concern, avoiding expensive data mutations on every toggle.
- `selected_index` retains its existing semantic: a **stable position into `filtered_indices`** (ascending order), independent of display direction.  `selected_entry()` therefore remains correct without modification.
- When descending, `display_idx 0` maps to the **last** element of `filtered_indices` (newest entry) via `actual_idx = n - 1 - display_idx`; this reversal is applied only inside the `show_rows` loop in `timeline.rs`.
- `stick_to_bottom` is disabled in descending order (newest is already at the top; sticky-bottom would fight live tail scroll).
- `sort_descending: bool` is a **user preference** — it is **not** cleared by `clear()`, consistent with `dark_mode` and `tail_auto_scroll`.  Default is `false` (ascending).

**Changes**:
- `src/app/state.rs`:
  - Added `pub sort_descending: bool` field (default `false`).
  - Added `toggle_sort_direction(&mut self)` — flips `sort_descending`; `selected_index` requires no remapping.
  - `clear()` intentionally leaves `sort_descending` untouched (user preference).
- `src/ui/panels/timeline.rs`:
  - Added compact sort toolbar (`↑ Oldest first` / `↓ Newest first` button + `ui.separator()`) above the `ScrollArea`.
  - Inside `show_rows`: computes `actual_idx` from `display_idx` using the descending reversal when `state.sort_descending`.
  - `is_selected` and the click handler both use `actual_idx` (stable `filtered_indices` position) instead of `display_idx`.
  - `stick_to_bottom` gated on `&& !state.sort_descending`.

**New regression tests** (both in `src/app/state.rs`):
- `test_toggle_sort_direction_preserves_selected_entry` — toggles twice; asserts `sort_descending` flips, `selected_index` is unchanged, and `selected_entry()` returns the same entry in all three states (ascending → descending → ascending).
- `test_sort_descending_preserved_across_clear` — sets `sort_descending = true`, calls `clear()`, asserts the preference survives.

**Test results: 71 unit tests + 29 E2E tests = 100 total, all passing. Zero clippy warnings.**

---

## Increment 34: Timestamp Robustness — Profile Fixes, parse_timestamp Fallbacks, sniff_timestamp
**Status: COMPLETE**

Comprehensive hardening of timestamp parsing to handle the wide variety of real-world log formats that either have no year, use non-standard separators, or fall outside the profile's declared `timestamp_format`.

### Profile bugs fixed

1. **`profiles/veeam_vbr.toml`** — `Svc.VeeamNFS.log` lines use `[DD.MM.YYYY HH:MM:SS.mmm] < thread>` (milliseconds present, thread ID space-padded). The original regex required no milliseconds and no space before the thread ID, so every NFS line folded silently as continuation text onto the previous entry. Fixed: `(?:\.\d+)?` for optional ms, `<\s*...\s*>` for space-padded thread, level keyword group made optional, `"Err"` added to `severity_mapping`, `(?i)` added to all `severity_override` patterns.

2. **`profiles/sccm_cmtrace.toml` + `profiles/intune_ime.toml`** — These profiles captured `time="HH:MM:SS"` only; `NaiveDateTime` always failed (no date component), so every SCCM/Intune entry displayed `--:--:--`. Fixed: regex now captures `date="M-D-YYYY"` attribute, `timestamp_format = "%m-%d-%Y"` — giving day-level precision.

3. **`profiles/syslog_rfc3164.toml`** — `%b %d %H:%M:%S` has no year; `NaiveDateTime` always failed. Handled by the new year-injection fallback in `parse_timestamp`; profile updated with a limitation comment.

### parse_timestamp fallback chain extended

- **4th fallback** — Separator normalisation: replaces `/` with `-` and `T` with ` ` in the timestamp string, then retries `NaiveDateTime` and `NaiveDate` parse. Fixes slash-separated dates (`2024/01/15`) and ISO `T`-separator variants when the profile format uses `-` and space.
- **5th fallback** — Year injection: if the format string contains no `%Y`, prepends the current UTC year and retries. Fixes BSD syslog RFC 3164 (`Jan 15 14:30:22`), klog (`MMDD HH:MM:SS.mmm`), and similar year-less formats across all profiles.
- `use chrono::Datelike` added to imports.
- 3 new regression tests: `test_parse_timestamp_slash_separated_date`, `test_parse_timestamp_t_separator`, `test_parse_timestamp_syslog_yearless`.

### sniff_timestamp — universal post-parse fallback (12 tiers)

A new `pub(crate) fn sniff_timestamp(raw_line: &str) -> Option<DateTime<Utc>>` uses a `OnceLock<Vec<Sniffer>>` (lazily compiled) to try 12 common timestamp patterns against any raw log line, returning the first match. Applied in `parse_content()` as a post-parse sweep over all entries with `timestamp: None` before `ParseResult` is returned.

| Tier | Pattern | Example |
|------|---------|---------|
| 1 | RFC 3339 / ISO 8601 + explicit timezone | `2024-01-15T14:30:22Z`, `+05:30` |
| 2 | log4j comma-milliseconds | `2024-01-15 14:30:22,999` |
| 3 | ISO space/T, optional dot-millis | `2024-01-15 14:30:22.123` |
| 4 | Slash year-first | `2024/01/15 14:30:22` |
| 5 | Dot day-first (Veeam) | `26.02.2026 22:07:56.535` |
| 6 | Apache combined log | `15/Jan/2024:14:30:22 +0000` |
| 7 | Slash YYYY (US/GB disambiguation — see Inc 35) | `01/15/2024 14:30:22` |
| 8 | Windows DHCP two-digit year (disambiguation) | `01/15/24,14:30:22` |
| 9 | Month-name 4-digit year | `Jan 15, 2024 14:30:22` |
| 10 | BSD syslog year-less (year injected) | `Jan 15 14:30:22` |
| 11 | Compact ISO | `20240115T143022` |
| 12 | Unix epoch at line start | `1705329022` |

Benefits: plain-text (raw-mode) files with embedded timestamps now have them extracted automatically; continuation-mode entries where the profile timestamp_format didn't match also get timestamps; any profile where the primary parse failed gets a second chance.

17 new regression tests covering each tier plus negative and integration cases. `test_plain_text_entries_get_sniffed_timestamps` verifies the post-parse pass on a synthetic plain-text profile.

**Test results: 91 unit tests + 29 E2E tests = 120 total, all passing. Zero clippy warnings.**

---

## Increment 35: US vs GB Date Disambiguation in Timestamp Sniffer
**Status: COMPLETE**

Tiers 7 and 8 of `sniff_timestamp` previously assumed US `MM/DD/YYYY` ordering unconditionally, misinterpreting unambiguous GB dates like `15/01/2024` (15 January) as invalid or wrong dates.

**Disambiguation logic** (applied identically to both tiers):
- First numeric field > 12 → unambiguously `DD/MM` (day cannot be a month)
- Second numeric field > 12 → unambiguously `MM/DD` (second field cannot be a month)
- Both ≤ 12 → ambiguous; US `MM/DD` tried first, GB `DD/MM` as fallback. Documented in code: for truly ambiguous dates (e.g. `01/02/2024` could be Jan 2 or Feb 1) a profile with an explicit `timestamp_format` is the reliable solution.

**Changes**:
- `src/core/parser.rs` — Tier 7 and Tier 8 `parse` closures replaced with field-extraction + conditional logic.
- Test suite: `test_sniff_us_slash` split into `test_sniff_us_slash_unambiguous` (second field 15 > 12), `test_sniff_gb_slash_unambiguous` (first field 15 > 12), and `test_sniff_slash_ambiguous_defaults_to_us` (both fields ≤ 12, Jan 2 result). DHCP tier: `test_sniff_dhcp_comma` split into `test_sniff_dhcp_comma_us` and `test_sniff_dhcp_comma_gb`.

**Test results: 94 unit tests + 29 E2E tests = 123 total, all passing. Zero clippy warnings.**

---

## Increment 36: Live mtime Display in File List
**Status: COMPLETE**

The file list in the Files tab now shows a live last-modified timestamp next to each file, updated dynamically as the directory watcher detects writes — no rescan required.

### Changes

- [x] `src/core/model.rs` — Added `FileMtimeUpdates(Vec<(PathBuf, DateTime<Utc>)>)` variant to `DirWatchProgress`.  Each element is `(path, new_mtime)`.

- [x] `src/app/dir_watcher.rs` — Background thread seeds `tracked_mtimes: HashMap<PathBuf, SystemTime>` from `known_paths` at startup. After each poll cycle (walk for new files), stats every known path and collects those whose `SystemTime` mtime differs from the tracked value. Changed entries are sent as `DirWatchProgress::FileMtimeUpdates`; `tracked_mtimes` is updated immediately so the next cycle compares against the new baseline. New files added to `known_paths` during the walk are seeded into `tracked_mtimes` via `entry().or_insert(mtime)` to prevent a spurious change event on the very next poll. Per-file stat errors are silently skipped (Rule 11).

- [x] `src/gui.rs` — New `DirWatchProgress::FileMtimeUpdates` arm in the dir-watch message handler: for each `(path, mtime)`, finds the matching `DiscoveredFile` in `state.discovered_files` and updates `f.modified = Some(mtime)` in-place. The UI repaints at the dir-watch poll cadence anyway, so the display refreshes automatically.

- [x] `src/ui/panels/discovery.rs` — Added `mtime_text: String` (6th field) to the `file_entries` pre-collected tuple. Computed by a new `format_mtime(Option<DateTime<Utc>>) -> String` helper:
  - Same day (local): `HH:MM:SS`
  - Same calendar year: `D Mon HH:MM` (e.g. `26 Feb 14:30`)
  - Prior year: `YYYY-MM-DD`
  - `None`: empty string (nothing displayed)

  In each row the mtime is rendered as small dim text in the `right_to_left` layout block, to the left of the profile label. Hover tooltip extended with `Modified: <mtime_text>`.

**Test results: 94 unit tests + 29 E2E tests = 123 total, all passing. Zero clippy warnings.**

---

## Increment 37: External (User) Profiles
**Status: COMPLETE**

LogSleuth now loads user-supplied format profiles from `%APPDATA%\LogSleuth\profiles\` on every startup and whenever the user clicks **Options > Reload Profiles**. External profiles follow the same TOML schema as built-in profiles and override any built-in with a matching `id`.

### Design decisions
- Profile directory is `%APPDATA%\LogSleuth\profiles\` (one level above the `config\` directory) — consistent with where a user would expect app data alongside the session file.
- The directory is created automatically at first launch so the user never needs to create it manually.
- Override-by-ID means a power user can replace a built-in with a corrected or extended version by dropping a `.toml` with the same `id` into the folder.
- Reload is **immediate** — the `request_reload_profiles` flag is consumed by `gui.rs` on the next frame; no restart required.

### Changes

- [x] `src/platform/config.rs` — `user_profiles_dir` now derived from `config_dir.parent()` so it correctly resolves to `%APPDATA%\LogSleuth\profiles\` instead of the previous (wrong) `%APPDATA%\LogSleuth\config\profiles\`.
- [x] `src/app/state.rs` — Added `pub user_profiles_dir: Option<PathBuf>` and `pub request_reload_profiles: bool` fields to `AppState` (both non-clearing user-preference fields).
- [x] `src/main.rs` — After `AppState::new()`, sets `state.user_profiles_dir` from `platform_paths.user_profiles_dir` and calls `std::fs::create_dir_all` to ensure the directory exists.
- [x] `src/gui.rs` — Added `request_reload_profiles` handler: calls `load_all_profiles(state.user_profiles_dir.as_deref())`, replaces `state.profiles`, sets a status message with counts (total / external), and logs the reload at info level.
- [x] `src/ui/panels/options.rs` — New **Section 4 — External Profiles**: displays the profile folder path (monospace), a `{total} profiles loaded ({builtin} built-in, {external} external)` summary line, and two buttons — **Open Folder** (opens the directory in the platform file manager) and **Reload Profiles** (sets `state.request_reload_profiles = true`). Open Folder is disabled when `user_profiles_dir` is `None`. Footer text updated: `"Profile changes take effect immediately."`
- [x] `scripts/New-LogSleuthProfile.ps1` — New PowerShell generator script. Accepts `-LogDirectory`, `-ProfileId`, `-ProfileName`, `-OutputPath`, `-SampleLines`, `-Force` parameters. Samples up to 5 representative files per filename-prefix group (up to 50 lines each), infers timestamp format via a ranked regex ladder (ISO 8601, log4j, Veeam dot-date, Apache, US-slash, DHCP, BSD syslog, month-name), finds a content anchor, detects severity keywords, emits a commented `.toml` with all low-confidence fields marked for review. Default output: `%APPDATA%\LogSleuth\profiles\<ProfileId>.toml`.
- [x] `ATLAS.md` — Extension Points table updated, `FormatProfile` concept updated, `options.rs` description updated, `scripts/` directory added to repo structure.

**Test results: 94 unit tests + 29 E2E tests = 123 total, all passing. Zero clippy warnings.**

---

## Increment 41 — Incremental walk streaming for fast new-file discovery (bug fix)

**Problem**: `walk_for_new_files` collected all results across the entire UNC directory tree before sending a single bulk message. On large SMB shares this meant new files (e.g., a Veeam error log created mid-session or a file excluded from the initial scan by the date gate) didn't appear in the file list until the complete 60-120 second walk finished. Reproducing an error in Veeam and looking for the resulting log update was unreliable.

**Fix**: Changed `walk_for_new_files` to stream results incrementally via the existing inner channel in batches of `WALK_BATCH_SIZE = 20`. The main poll loop now drains all available batches each 2-second cycle using a `loop { try_recv }` pattern, and interprets `TryRecvError::Disconnected` (channel closed when the walk thread returns) as the "walk complete" signal instead of receiving a single final `Vec`. A `walk_new_count` accumulator tracks the total across all batches so `WalkComplete{new_count}` is still sent correctly when the walk finishes.

Effect: a new file appears in the file list at most 2 seconds after the walker reaches it in the directory tree, regardless of how large the tree is or how long the remaining walk takes.

- [x] `src/util/constants.rs` — `WALK_BATCH_SIZE: usize = 20` added.
- [x] `src/app/dir_watcher.rs` — `walk_for_new_files` signature changed: returns `()`, takes `batch_tx: &mpsc::Sender<Vec<PathBuf>>`, streams batches and exits early on send error. Main loop: `try_recv` section replaced with draining loop; WalkComplete sent on Disconnected; `walk_new_count: usize` accumulator added and reset on each new walk start. Test helper `walk_collect` added to wrap streaming API into synchronous Vec for tests.
- [x] `ATLAS.md` — Dir-watcher component entry updated to document streaming walk.

**Test results: 94 unit tests + 29 E2E tests = 123 total, all passing. Zero clippy warnings.**

---

## Increment 40 — Fix `LogEntry::file_modified` staleness in time-range filter (bug fix)

**Problem**: `LogEntry::file_modified` is stamped once at parse time and never updated. The time-range filter uses it as a fallback effective timestamp for plain-text / no-timestamp entries (`effective_time = entry.timestamp.or(entry.file_modified)`). Over time those entries aged out of rolling windows such as "Last 1m" even while the file was actively being written to — producing a confusing "0/N entries" display while the activity window and file list still showed the file as live.

**Fix**: `src/gui.rs` — In the existing `DirWatchProgress::FileMtimeUpdates` handler, after updating `DiscoveredFile::modified`, also iterate `state.entries` and update `entry.file_modified = Some(mtime)` for every entry whose `source_file` matches the updated path. This keeps the fallback timestamp in sync with each poll cycle's live mtime so plain-text entries stay within the rolling time window for as long as the file is being written to.

- [x] `src/gui.rs` — `FileMtimeUpdates` handler now also refreshes `LogEntry::file_modified` for all matching entries.
- [x] `ATLAS.md` — Directory Watch concept updated to document the dual update.

**Test results: 94 unit tests + 29 E2E tests = 123 total, all passing. Zero clippy warnings.**

---

### High Priority
- [x] **Persistent sessions** -- Save and restore the current set of loaded files, filter state, and colour assignments so a session can be resumed after reopening the application. *(Increment 12)*

### Medium Priority
- [ ] **Column visibility toggles** -- Allow the user to show/hide columns in the timeline (e.g. hide the filepath column when viewing a single file).
- [ ] **Export retains filter** -- Optionally export the full entry set (pre-filter) alongside the filtered export.
- [ ] **Profile editor UI** -- In-app wizard to create and test a new TOML profile without leaving LogSleuth.
- [x] **Additional built-in profiles** -- Increments 17/18/20/21 added SQL Server, Apache, nginx, DHCP, Intune IME, Windows Cluster, Kubernetes klog, Exchange tracking, PostgreSQL, Tomcat/Catalina, SCCM CMTrace, Windows Firewall. Remaining candidates: Windows Event Log XML exports, Docker/podman JSON logs (use json-lines profile), systemd journal exports (use syslog profile).
- [x] **Configurable max files / max entries** -- Max files, max total entries, max scan depth, tail poll interval, and directory watch poll interval are all now user-configurable via Edit > Options… and apply on the next scan/session start. *(Increment 27)*

### Low Priority / Research
- [x] **Parallel file parsing** -- Rayon-parallelised merged auto-detect + parse pipeline. Each file is read once (eliminating the previous double-I/O for sample lines + full content), auto-detected from in-memory content, parsed, and results collected in parallel. Entry IDs are assigned sequentially post-collection. Also: 128 KB BufReader buffers for network-efficient reads, optimised UTF-8 decode path (zero-copy for valid UTF-8). Major performance improvement on UNC/network paths where I/O latency dominates.
- [ ] **Full-text index** -- Build a Tantivy (or similar) in-memory index on scan completion to enable fast regex and phrase searching across millions of entries.
- [ ] **Plugin / WASM profile extensions** -- Allow format profiles to include embedded WASM functions for custom timestamp or severity extraction logic beyond what regex alone can express.
- [ ] **Network / remote log sources** -- Pull logs from a remote host via SSH or HTTP (structured log APIs).
