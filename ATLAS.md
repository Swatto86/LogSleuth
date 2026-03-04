# LogSleuth -- Project Atlas

> **Status**: Increment 49 complete -- (49) **Deduplication filter**: new post-filter step that collapses repeated messages into a single representative entry. Three modes via dropdown in Filters panel: Off (default), Exact match, Normalized. Normalized mode replaces GUIDs, IPv4/IPv6 addresses, 0x-hex strings, and bare numbers with placeholder tokens (`<GUID>`, `<IP>`, `<HEX>`, `<NUM>`) before comparison using `OnceLock`-cached compiled regexes. Dedup is scoped per source file; latest-timestamp entry survives per group. New types: `DedupMode` enum, `DedupInfo` struct, `NormRegexes`. New functions: `normalize_message()`, `apply_dedup()`. Purple `(xN)` count badge in timeline rows. Collapsible "Occurrences" section in detail panel showing all collapsed entries. Session-persisted via `PersistedFilter.dedup_mode`. Live-tail fast path falls back to full rebuild when dedup active. 17 new unit tests (normalize_message patterns, apply_dedup exact/normalized/per-file/latest-wins/no-timestamp/order, DedupMode in is_empty, session round-trip). 163 tests passing (133 unit + 30 E2E), zero clippy warnings, clean fmt. Previously: (48) Bug fix: `filtered_results_report()` and `bookmarks_report()` use `entry.raw_text` instead of `entry.message.chars().take(200)` for clipboard body. Previously: (47) Three new `sniff_timestamp` tiers added to `core/parser.rs`: **Tier 13** -- Unix epoch milliseconds (13-digit at line start, common in Node.js/browser logs; e.g. `1705329022123`); **Tier 14** -- dot-separated date-only `DD.MM.YYYY` or `D.M.YYYY` with no time component (treated as midnight UTC; placed after all date+time tiers so Tier 5 still handles `DD.MM.YYYY HH:MM:SS`); **Tier 15** -- time-only `HH:MM:SS[.mmm]` with no date (today's UTC date injected; lowest-confidence tier, last in chain so any line containing a full date+time is matched by an earlier tier first). 7 new unit tests added (`test_sniff_unix_epoch_millis`, `test_sniff_epoch_seconds_not_confused_with_millis`, `test_sniff_dot_date_only`, `test_sniff_dot_date_only_single_digit`, `test_sniff_time_only`, `test_sniff_time_only_with_millis`, `test_sniff_full_datetime_preempts_time_only`). 159 tests passing (129 unit + 30 E2E), zero clippy warnings, clean fmt. Previously: (46) Added "Hide rows with no timestamp" filter: `hide_no_timestamp: bool` field on `FilterState`; when true, any entry whose source text contains no parseable date/time is excluded from the timeline (entries that would fall back to a file-modification-time estimate are also excluded). Logic added in `matches_all()` before the time-range check. `is_empty()` updated. `PersistedFilter` extended with `#[serde(default)] hide_no_timestamp: bool` (no SESSION_VERSION bump). `save_session()` and `restore_from_session()` wired. UI checkbox "Hide rows with no timestamp" added in the time-range section of `filters.rs` with a full plain-English tooltip. 2 new unit tests (`test_hide_no_timestamp_excludes_untimed_entries`, `test_hide_no_timestamp_tracked_in_is_empty`). 152 tests passing (122 unit + 30 E2E), zero clippy warnings, clean fmt. Previously: (45) Four new filter types added end-to-end (core logic, app state, session persistence, UI): (1) **NOT/Exclusion text filter** -- hides any entry whose message/raw line contains the exclusion term (case-insensitive substring, inverted match); pre-computed as `excl_lower` in `apply_filters` to avoid per-entry allocation on the 1 M-entry hot path, computed inline in `entry_matches` for the live-tail single-entry path. (2) **Thread filter** -- `HashSet<String>` include-list; empty set passes all entries; populated from unique `LogEntry::thread` values cached in `AppState::unique_thread_values` (rebuilt once per `apply_filters` call, not every frame). (3) **Component filter** -- identical pattern against `LogEntry::component`; cache in `AppState::unique_component_values`. (4) **Absolute date/time range** -- "From" and "To" inputs in `YYYY-MM-DD HH:MM` format; parsed by new free function `parse_filter_datetime(s) -> Option<DateTime<Utc>>` in `app/state.rs`; valid entry writes to `filter_state.time_start` / `time_end` and clears `relative_time_secs`; invalid entry resets input buffer to currently-set value; empty entry clears the bound; inline tick/cross validity indicator shown. `PersistedFilter` extended with `exclude_text: String`, `thread_filter: Vec<String>`, `component_filter: Vec<String>` (all `#[serde(default)]` -- no SESSION_VERSION bump, old session files forward-compatible). New `AppState` fields: `unique_thread_values: Vec<String>`, `unique_component_values: Vec<String>`. New private method `AppState::rebuild_unique_values()` called at end of `apply_filters()`. 11 new unit tests added to `core/filter.rs` (exclude_text x3, thread_filter x3, component_filter x2, thread+component combined x1, abs date range x2). 150 tests passing (120 unit + 30 E2E), zero clippy warnings, clean fmt. Previously: (44) AI Operating Contract full compliance audit (Part 3): fixed Rule 2 violations in executable string literals across 7 source files -- replaced all em-dash (U+2014), horizontal ellipsis (U+2026), and midline ellipsis (U+22EF) characters in string literals and assert messages in `src/gui.rs` (10 fixes), `src/ui/panels/discovery.rs` (9 fixes), `src/app/session.rs` (2 fixes), `src/app/state.rs` (1 fix), `tests/e2e_discovery.rs` (2 fixes), `examples/gen_icons.rs` (1 fix), `scripts/New-LogSleuthProfile.ps1` (1 fix) with ASCII equivalents (`-`, `...`); non-ASCII remaining in those files is exclusively in `//` comments (permitted by Rule 2). Verified #[cfg(test)] module placement (Rule 3): all 8 test modules in `core/filter.rs`, `core/profile.rs`, `core/parser.rs`, `core/export.rs`, `core/discovery.rs`, `app/state.rs`, `app/session.rs`, `app/dir_watcher.rs` are placed at the end of their files with no production code following. 139 tests passing (109 unit + 30 E2E), zero clippy warnings, clean fmt. Previously: (43) AI Operating Contract full compliance audit (Part 2): fixed 10 profile TOML files that contained non-ASCII bytes (Rule 2 Config/CI file encoding: U+2500 box-drawing, U+2014 em-dash, U+2013 en-dash, U+2192 right-arrow replaced with ASCII equivalents in `apache_combined.toml`, `exchange_tracking.toml`, `intune_ime.toml`, `kubernetes_klog.toml`, `postgresql_log.toml`, `sccm_cmtrace.toml`, `tomcat_catalina.toml`, `veeam_vbr.toml`, `windows_cluster.toml`, `windows_firewall.toml`); resolved INV-10 ATLAS/PROGRESS contradiction by adding missing Increment 42 entry to PROGRESS.md. Previously implemented (Inc.42): AI Operating Contract full compliance audit (Part 1): (a) Rule 18 -- fixed `Invoke-Git` in `update-application.ps1` to use the automatic `$args` variable (not a named `[string[]]$Arguments` parameter); captures `2>&1`, includes exit code and output in the throw message, and returns output; updated all 10 call sites to pass individual arguments instead of `@(array)` literals; (b) Part E -- added `workflow_dispatch:` trigger to both `ci.yml` and `release.yml` so workflows can be manually triggered and verified; (c) Rule 5 / INV-10 -- implemented log-file support in `util::logging::init()` using a two-layer tracing-subscriber registry (stderr always active; optional `Mutex<File>` layer added when `[logging] file` is configured); resolves Atlas/code contradiction where ATLAS said file logging was active but code silently ignored the parameter with a TODO. 139 tests passing (109 unit + 30 E2E), zero clippy warnings, clean fmt.
> **Last updated**: 2026-03-05

---

## 1. System Purpose

LogSleuth is a cross-platform log file viewer and analyser that discovers, parses, and presents application log files from a directory tree in a unified, filterable timeline. It uses extensible TOML-based format profiles to handle diverse vendor log formats without code changes.

**Primary value proposition**: Point at a directory, automatically find and parse all log files regardless of format, and surface errors with cross-log correlation -- replacing manual grep/Notepad++ workflows.

**Part of the Swatto Tools collection** alongside EventSleuth (Windows Event Log viewer) and DiskSleuth.

---

## 2. Domain Concepts

| Concept | Definition |
|---------|-----------|
| **Log Entry** | A single parsed event from a log file, normalised into common fields (timestamp, severity, message, source file, etc.) |
| **Format Profile** | A TOML definition describing how to detect and parse a specific log format (regex patterns, timestamp format, severity mappings, optional `log_locations` displayed as a hover tooltip in the discovery panel). Profiles are either **built-in** (embedded in the binary from `profiles/`) or **external** (`.toml` files in `%APPDATA%\LogSleuth\profiles\`). External profiles override built-ins when their `id` matches. |
| **Discovery** | Recursive scan of a directory tree to find candidate log files using glob patterns |
| **Auto-detection** | Matching discovered files to format profiles by sampling the first N lines against each profile's content regex |
| **Severity** | Normalised enum: Critical, Error, Warning, Info, Debug, Unknown |
| **Unified Timeline** | Merged, chronologically sorted view of all parsed entries across all discovered files. Sort is performed on the background scan thread before entries are streamed to the UI. |
| **File Colour** | Each source file is assigned a unique colour from a 24-entry palette; a coloured stripe on every timeline row and a coloured dot in the file list identify the origin file visually (CMTrace-style) |
| **Correlation** | Viewing entries across multiple log files within a time window centred on a selected entry |
| **Scan** | A complete discovery + parse operation on a target directory |
| **Directory Watch** | Background polling of the scan directory (`DIR_WATCH_POLL_INTERVAL_MS = 2 s`) by `app::dir_watcher::DirWatcher`. Detects newly created log files and triggers an append scan so they appear in the live timeline automatically. Newly discovered files must satisfy the `modified_since` date filter from `DirWatchConfig` (same fail-open mtime gate as the initial scan); files with an OS mtime earlier than the filter date are silently skipped. Only active for directory-based sessions (not for file-only "Open Log(s)..." sessions). A blue **👁 WATCH** button is shown in the status bar while the watcher is running; clicking it pauses the watcher. When paused, the badge is shown dimmed and clicking it resumes the watcher from the current file set. Each poll cycle also stats all **known** files and sends a `DirWatchProgress::FileMtimeUpdates` message for any whose OS mtime has changed; the UI applies these to both `DiscoveredFile::modified` (so the file list shows a live last-modified time) and to `LogEntry::file_modified` for every entry whose `source_file` matches (so the time-range filter fallback for plain-text / no-timestamp entries stays in sync with live writes and entries do not age out of "Last 1m" while the file is actively written to). |

---

## 3. Architectural Boundaries

### Layer Rules

| Layer | May depend on | Must NOT depend on |
|-------|--------------|-------------------|
| **Core** (`core::*`) | Standard library only | UI, platform, I/O, app |
| **App** (`app::*`) | Core | UI, platform specifics |
| **UI** (`ui::*`) | App, Core (read-only models) | Platform, direct I/O |
| **Platform** (`platform::*`) | Standard library | Core, App, UI |
| **Util** (`util::*`) | Standard library | Core, App, UI, Platform |

### Cross-Cutting Concern Isolation

| Concern | Isolation mechanism |
|---------|-------------------|
| Logging | `util::logging` module behind `tracing` facade; no direct `println!` in core/app |
| Configuration | `platform::config` loads TOML; core receives typed config structs, never reads files directly |
| File I/O | `platform::fs` trait abstracts file reading; core parsers accept `Read` trait objects |
| Error handling | `util::error` defines typed error hierarchy; all propagation via `Result<T, LogSleuthError>` |

---

## 4. Repository Structure

```
LogSleuth/
+-- src/
|   +-- main.rs                  # Entry point, CLI parsing, logging init, GUI launch; **build_font_definitions()** pre-loads Consolas (primary monospace), Segoe UI (primary proportional), Segoe UI Symbol + Emoji (Unicode fallbacks) from C:\Windows\Fonts\ **before** eframe::run_native so no font I/O occurs inside the creator closure (DevWorkflow Rule 16 — eliminates white-flash startup); creator closure is trivial: set_fonts + AppState construction only; --filter-level CLI arg populates severity_levels with the requested level and all more-severe variants before eframe launch
|   +-- lib.rs                   # Library crate entry point (exposes modules for integration tests)
|   +-- gui.rs                   # eframe::App implementation, scan progress routing, panel wiring; sidebar is tab-based (Files | Filters tabs), resizable (default_width=460, min=300, max=800), single ScrollArea per tab — no more dual-scroll 45/55 split; Filters tab label shows a bullet dot when any filter is active; **ParsingCompleted handler**: sets scan_in_progress=false; if fresh_scan_in_progress (interactive scan), sets hide_all_sources=true + clears source_files (opt-in model — nothing shown until user ticks files) and for directory scans overrides status with "N files discovered — tick files to load"; then calls sort_entries_chronologically(); after ParsingCompleted on a directory session, DirWatcher (re)started; if tail was active before append, tail restarted; **pending_scan handler** (Open Directory): passes parse_path_filter=Some(empty HashSet) so all files are discovered and profiled but parsing_skipped=true (zero memory); sets fresh_scan_in_progress=true; **pending_replace_files handler** (Open Log(s)): parses all selected files but sets fresh_scan_in_progress=true so entries are hidden until ticked; **request_start_tail**: file list sorted by mtime desc and truncated to MAX_TAIL_WATCH_FILES (100) before handing to TailManager
|   +-- app/
|   |   +-- mod.rs
|   |   +-- dir_watcher.rs       # Recursive directory watcher: DirWatcher struct (start_watch/stop_watch/poll_progress), DirWatchConfig (include/exclude glob patterns + max_depth + **poll_interval_ms** — default DIR_WATCH_POLL_INTERVAL_MS, user-configurable via Options; **modified_since: Option<DateTime<Utc>>** — when Some, walk_for_new_files() skips any file whose OS mtime predates the value, mirroring the initial scan's date filter; fail-open when mtime is unreadable), background run_dir_watcher() polling thread uses config.poll_interval_ms; **tracked_mtimes: HashMap<PathBuf, SystemTime>** seeded from known_paths at thread startup, checked every poll cycle — files whose mtime changed since last poll are batched into a `DirWatchProgress::FileMtimeUpdates` message; walk_for_new_files() uses walkdir with filter_entry to prune excluded subtrees; new files streamed to a `mpsc::Sender<Vec<PathBuf>>` in batches of `WALK_BATCH_SIZE = 20` as they are found (not collected and sent at end) so new files appear within a single 2-second poll cycle rather than after the entire tree traversal completes; main loop drains all available batches per cycle (loop + try_recv) and sends WalkComplete when channel closes (Disconnected = walk thread returned); known_paths updated immediately after each batch send to prevent re-reporting on next poll cycle
|   |   +-- profile_mgr.rs       # Profile loading (built-in + user), override logic
|   |   +-- scan.rs              # Scan lifecycle: background thread, cancel (AtomicBool), retry backoff, UTF-16 BOM detection, plain-text fallback, background chronological sort before streaming batches; **parallel merged auto-detect+parse pipeline** via rayon -- each file is read once (single I/O pass), auto-detected from in-memory content, and parsed in parallel; entry IDs assigned sequentially post-collection; timeout-guarded file reads (FILE_READ_TIMEOUT_SECS) protect rayon workers from stalled I/O; **parse_path_filter: Option<HashSet<PathBuf>>** parameter on `start_scan` and `run_parse_pipeline` — when Some, files not in the set skip full I/O and receive filename-only profile detection with `parsing_skipped=true` set on the `DiscoveredFile` sent to the UI; `run_files_scan` always passes `None` (explicit file lists parse everything)
|   |   +-- session.rs           # Session persistence: SessionData + PersistedFilter structs (serde JSON); session_path(), save() (atomic write via .json.tmp rename), load() (returns None on missing/corrupt/version-mismatch -- never errors to user); SESSION_VERSION const for forward-compat; **PersistedFilter new fields** (all `#[serde(default)]` for forward-compat): `exclude_text: String`, `thread_filter: Vec<String>` (sorted before save, restored as HashSet), `component_filter: Vec<String>` (same), `hide_no_timestamp: bool`, `dedup_mode: DedupMode`
|   |   +-- state.rs             # Application state; sidebar_tab: usize (0=Files, 1=Filters -- pure UI state, not persisted, not cleared on clear()); tail flags (tail_active, tail_auto_scroll, request_start_tail, request_stop_tail); dir_watcher_active: bool (set when directory watcher is running); **user-preference option fields (not cleared on clear())**: max_files_limit, max_total_entries (entry cap), max_scan_depth, tail_poll_interval_ms, dir_watch_poll_interval_ms, **sort_descending: bool** (false=ascending/oldest-first default) -- all initialised from constants/defaults in new(), configurable or togglable at runtime; show_log_summary; show_about; bookmarks: HashMap<u64,String>; correlation_active, correlation_window_secs, correlated_ids: HashSet<u64>; session_path: Option<PathBuf> (never cleared); initial_scan: Option<PathBuf> (startup re-scan without clear()); **fresh_scan_in_progress: bool** -- set true by pending_scan/pending_replace_files GUI handlers; cleared in ParsingCompleted; signals the opt-in model (default nothing-checked after interactive scan); toggle_bookmark(), is_bookmarked(), bookmark_count(), clear_bookmarks(), bookmarks_report(), filtered_results_report() (bounded to MAX_CLIPBOARD_ENTRIES), update_correlation(), next_entry_id(), save_session(), restore_from_session(), **toggle_sort_direction()** (flips sort_descending; selected_index is a stable filtered_indices position so no remapping needed); apply_filters() preserves the selected entry by stable entry ID (not by display-position integer) before and after filter recompute; sort_entries_chronologically() performs a stable sort across all entries then calls apply_filters(); **remove_entries_for_file(path)**: removes all entries where `source_file == path`, marks the DiscoveredFile as `parsing_skipped=true`, updates status_message -- called when the user unchecks a file row in the Files tab so entries are freed from memory immediately and re-ticking triggers a fresh on-demand parse; **new fields**: `unique_thread_values: Vec<String>` and `unique_component_values: Vec<String>` -- sorted caches rebuilt once per `apply_filters()` call by private `rebuild_unique_values()` (O(n) scan of `self.entries`); both cleared in `clear()`; used by the thread/component filter UI checkboxes to enumerate available values without scanning entries every frame; **dedup_info: HashMap<usize, DedupInfo>** -- populated by `apply_dedup()` after the filter pipeline in `apply_filters()`; cleared in `clear()`; consulted by timeline (count badge) and detail panel (occurrences list); when dedup is active, `extend_filtered_for_range()` falls back to full `apply_filters()` rebuild; **new free function** `parse_filter_datetime(s: &str) -> Option<DateTime<Utc>>` (defined before `impl AppState`) -- parses "YYYY-MM-DD HH:MM" (and "YYYY-MM-DD HH:MM:SS") strings to `DateTime<Utc>`; used by the absolute date/time range UI inputs in `filters.rs`
|   |   +-- tail.rs              # Live tail: TailManager + run_tail_watcher poll loop (**poll_interval_ms parameter**, default TAIL_POLL_INTERVAL_MS=500 ms, user-configurable via Options), per-file byte-offset tracking, partial-line buffer, rotation/truncation detection, TailFileInfo; file-selection filter applied before start (respects hide_all_sources + source_files whitelist); start_tail() accepts poll_interval_ms: u64; **MAX_TAIL_WATCH_FILES cap (100)**: gui.rs sorts candidate files by mtime descending and truncates to 100 before passing to start_tail — most-recently-modified files are always included; status message notes when cap was applied
|   +-- core/
|   |   +-- mod.rs
|   |   +-- model.rs             # LogEntry, Severity, FormatProfile structs; FormatProfile includes severity_override: HashMap<Severity,Vec<Regex>> + apply_severity_override() method; **DirWatchProgress** enum: `NewFiles(Vec<PathBuf>)` (newly discovered files) + `FileMtimeUpdates(Vec<(PathBuf, DateTime<Utc>)>)` (mtime changes to known files sent each poll cycle)
|   |   +-- discovery.rs         # Recursive traversal (walkdir), glob include/exclude, filter_entry dir exclusion, metadata; **pre-flight check** uses `fs::metadata()` (not `exists()`/`is_dir()`) to correctly distinguish PermissionDenied from NotFound; runs in a background thread with PREFLIGHT_TIMEOUT_SECS=10
|   |   +-- export.rs            # CSV/JSON serialisation
|   |   +-- filter.rs            # Composable filter engine: severity, text (exact or fuzzy subsequence), regex, **parsed-timestamp-based** time window (uses `LogEntry::timestamp` -- the parsed log event time -- as the primary comparison; falls back to `LogEntry::file_modified` OS mtime only for plain-text/no-timestamp entries; entries with neither are excluded from time-bounded views), source file whitelist (hide_all_sources flag for explicit "none" state); bookmark filter (bookmarks_only + bookmarked_ids populated by app layer); **NOT/exclusion text filter** (`exclude_text: String` -- case-insensitive substring, inverted; entries whose message or raw line contains the term are hidden; `excl_lower` pre-computed in `apply_filters` hot path, computed inline in `entry_matches` single-entry path); **thread filter** (`thread_filter: HashSet<String>` -- include-allow-list; empty set passes all; filters on `LogEntry::thread`); **component filter** (`component_filter: HashSet<String>` -- same pattern against `LogEntry::component`); **absolute time bounds** stored as `filter_state.time_start` / `filter_state.time_end` (existing fields reused); UI input buffers `abs_time_start_input: String` and `abs_time_end_input: String` held on `FilterState` so the panel can debounce user typing without round-tripping through `Option<DateTime<Utc>>`; **hide_no_timestamp: bool** -- when true, any entry with `LogEntry::timestamp == None` is excluded regardless of other filters (entries that would fall back to file-mtime are also excluded; checked in `matches_all()` before the time-range path); **deduplication** (`dedup_mode: DedupMode` enum -- Off/Exact/Normalized; default Off); `DedupMode` implements `Serialize`/`Deserialize`/`PartialEq`/`Clone`/`Copy`; `DedupInfo` struct (count, first_timestamp, all_indices); `NormRegexes` struct with `OnceLock` lazy-init compiled regex set; `normalize_message(&str) -> String` replaces GUIDs, IPv6, IPv4, 0x-hex, numbers with tokens; `apply_dedup(entries, indices, mode) -> (Vec<usize>, HashMap<usize, DedupInfo>)` groups by (source_file, message_key), latest-timestamp entry survives per group; `is_empty()` updated to include `exclude_text`, `thread_filter`, `component_filter`, `hide_no_timestamp`, `dedup_mode`; `matches_all()` signature extended with `excl_lower: &str` parameter for the bulk path
|   |   +-- profile.rs           # TOML profile parsing, validation, auto-detection scoring; SeverityOverrideDef TOML struct; override patterns compiled via compile_regex in validate_and_compile
|   |   +-- parser.rs            # Stream-oriented log parsing, multi-line handling, chrono timestamp parsing; MultilineMode::Raw emits every line as an entry and records no parse error; MultilineMode::Skip records an error for every non-matching line; MultilineMode::Continuation records an error only when no prior entry exists to attach the line to; **parse_timestamp() 5-fallback chain**: (1) NaiveDateTime direct, (2) NaiveDate-only (midnight), (3) RFC 3339/ISO 8601 with timezone, (4) separator normalisation (`/`→`-`, `T`→` `) then retry, (5) year injection (current UTC year prepended) for year-less formats like BSD syslog; **sniff_timestamp(line) -> Option<DateTime<Utc>>**: 15-tier OnceLock post-parse fallback — (1) RFC 3339+tz, (2) log4j comma-millis, (3) ISO space/T optional dot-millis, (4) slash year-first, (5) dot day-first (Veeam DD.MM.YYYY HH:MM:SS), (6) Apache combined DD/Mon/YYYY:HH:MM:SS ±ZZZZ, (7) slash MM/DD or DD/MM YYYY with disambiguation (first field > 12 → DD/MM; second > 12 → MM/DD; ambiguous both-≤12 defaults to US MM/DD), (8) Windows DHCP two-digit year with same disambiguation, (9) month-name 4-digit year, (10) BSD syslog year-less (year injected), (11) compact ISO YYYYMMDDTHHMMSS, (12) Unix epoch seconds at line start (10 digits), (13) Unix epoch milliseconds at line start (13 digits; Node.js/browser logs), (14) dot date-only DD.MM.YYYY with no time (midnight UTC; 1-2 digit day/month accepted), (15) time-only HH:MM:SS[.mmm] with no date (today's UTC date injected; lowest confidence — only reached when no date appears on the line); applied as a post-parse sweep in parse_content over all entries with timestamp: None before ParseResult is returned
|   +-- ui/
|   |   +-- mod.rs
|   |   +-- panels/
|   |   |   +-- mod.rs
|   |   +-- about.rs         # About dialog: centred modal window (version from CARGO_PKG_VERSION, GitHub link, MIT licence); show_about flag on AppState; ⓘ button right-aligned in menu bar (placed AFTER File/View menus so layout allocation is correct)
|   |   |   +-- discovery.rs     # Files tab renderer: (1) collapsible scan-controls header (CollapsingHeader, default_open=true) containing path label, date filter (YYYY-MM-DD HH:MM:SS + quick-fill buttons), Open Directory / Open Log(s) / Clear Session buttons; (2) unified file list with count badge, All/Live-Tail/search-box/Select-All-None controls, virtual-scroll via show_rows at ROW_HEIGHT — each row: dot + checkbox + filename + solo + reveal button + right-aligned compact **mtime** (`HH:MM:SS` today, `D Mon HH:MM` this year, `YYYY-MM-DD` prior year) + profile label; hover shows full path + size + profile + `Modified: <mtime>`; mtime refreshes live when the directory watcher sends `FileMtimeUpdates`; `format_mtime(Option<DateTime<Utc>>) -> String` helper; source-file filter state driven directly from the file list (replaces separate duplicate list that was in filters.rs); **source_files is always explicitly enumerated** — Select All and individual checkbox-tick NEVER collapse the set to empty (the empty=all shortcut was removed); this ensures new dir-watcher files always start unchecked and require explicit opt-in
|   |   +-- options.rs       # Options dialog: 4 sections — (1) Ingest Limits: max_files_limit (logarithmic slider, ABSOLUTE_MAX_FILES), max_total_entries (logarithmic, MIN_MAX_TOTAL_ENTRIES–ABSOLUTE_MAX_TOTAL_ENTRIES), max_scan_depth (linear, 1–ABSOLUTE_MAX_DEPTH); (2) Live Tail: tail_poll_interval_ms (logarithmic, MIN–MAX_TAIL_POLL_INTERVAL_MS); (3) Directory Watch: dir_watch_poll_interval_ms (logarithmic, MIN–MAX_DIR_WATCH_POLL_INTERVAL_MS); (4) External Profiles: profile folder path (monospace), loaded profile count (built-in vs external), Open Folder button (opens explorer/open/xdg-open), Reload Profiles button (sets state.request_reload_profiles). Each slider row has a Reset button; opened via Edit > Options...; all limits from util::constants
|   |   |   +-- timeline.rs      # Virtual-scrolling unified timeline; purple `(xN)` dedup count badge appended to row text via LayoutJob when dedup_info is present for the row; compact **sort order toolbar** (↑ Oldest first / ↓ Newest first button + separator) above the ScrollArea — calls `state.toggle_sort_direction()`; display reversal in `show_rows` via `actual_idx = if sort_descending { n-1-display_idx } else { display_idx }` — data structures stay ascending; `is_selected` and click handler use `actual_idx` (stable filtered_indices position); `stick_to_bottom` gated on `&& !state.sort_descending`; 4 px coloured left stripe per row; severity 2 px underline accent (Critical/Error/Warning) drawn at the bottom of the row in the row's severity colour — replaces the former full-row background tint; amber star button (★/☆) per row for bookmarking; gold tint on bookmarked rows; teal tint on correlated rows; bookmark toggle applied after ScrollArea to avoid borrow conflict; **LayoutJob** splits each row into a severity-coloured badge ([CRIT]/[ERR ] etc.) and a high-contrast body (white in dark mode, near-black in light mode via theme::row_text_colour()); **three distinct filtered==0 empty states**: (1) discovered_files.is_empty() → welcome screen; (2) hide_all_sources → "N files discovered, tick to load"; (3) otherwise → "no entries match filters"
|   |   |   +-- detail.rs        # Entry detail pane (no height cap); Show in Folder button (Windows: explorer /select,; macOS: open -R; Linux: xdg-open); when dedup is active, shows a collapsible purple "Occurrences (N)" section above the message body listing timestamp, file, and line number of all collapsed entries in a striped grid
|   |   |   +-- summary.rs       # Scan summary dialog (overall statistics + per-file breakdown)
|   |   |   +-- log_summary.rs   # Log-entry summary panel: severity breakdown table + collapsible message preview lists (max 50 rows/severity), colour-coded; opened via View menu or Filters "Summary" button
|   |   |   +-- filters.rs       # Filters tab renderer: two button rows (Row 1: severity presets -- Errors only/Errors+Warn/Err+Warn+15m/Clear; Row 2: Summary/Bookmarks/clear bm); severity checkboxes; text/regex inputs; fuzzy ~ toggle; relative time quick-buttons (15m/1h/6h/24h) + custom input + rolling-window live indicator; **source-file filter section removed** (now lives in discovery.rs Files tab); **NOT/exclusion text filter** -- TextEdit input after the regex row with debounce + inline x clear button + red "NOT active" label indicator when the field is non-empty; **deduplication dropdown** -- ComboBox ("Deduplicate:") between exclusion filter and time range; Off/Exact match/Normalized modes from `DedupMode`; purple stats line when active ("N unique (M duplicates hidden)"); **absolute date/time range** -- two TextEdit inputs ("From:" / "To:", hint "YYYY-MM-DD HH:MM") in the time section below the rolling window; parsed via `crate::app::state::parse_filter_datetime` on lost_focus; valid parse writes to `filter_state.time_start`/`time_end` and clears `relative_time_secs`; invalid parse resets the input buffer; empty clears the bound; inline tick/cross validity indicator; "Clear abs. range" button shown when absolute bounds are active; **"Hide rows with no timestamp" checkbox** -- shown below the absolute range in the time section; maps directly to `filter_state.hide_no_timestamp`; hides entries that have no parseable date/time in their source text; **thread filter** -- checkbox list rendered only when `state.unique_thread_values` is non-empty, one checkbox per discovered thread value, x clear-all button; **component filter** -- identical pattern against `state.unique_component_values`; both rendered inside the `!state.entries.is_empty()` block; borrow conflict avoided by cloning the Vec before iterating; correlation overlay toggle + window input; entry-count footer with "Copy" clipboard button (disabled when empty)
|   |   +-- theme.rs             # Colours, severity mapping, layout constants; 24-entry FILE_COLOUR_PALETTE for per-file stripes; SIDEBAR_WIDTH=460 (default_width for resizable SidePanel, min=300, max=800); **row_text_colour(dark_mode) -> Color32** returns WHITE in dark mode and Slate-950 in light mode for timeline body text; **severity_colour(severity, dark_mode)** used for both the severity badge text and the row underline accent (no separate bg-colour function)
|   +-- platform/
|   |   +-- mod.rs
|   |   +-- fs.rs                # Platform filesystem helpers: `read_first_lines` (encoding-safe head-N, 128 KB buffer), `reveal_in_file_manager` (Explorer/Finder/xdg-open); free functions (no trait abstraction needed for a single-binary app)
|   |   +-- config.rs            # Platform-specific config/data paths + config.toml loading/validation (Rule 13: `load_config()` deserialises `RawConfig`, validates against named constants, returns `AppConfig` + warnings)
|   +-- util/
|       +-- mod.rs
|       +-- error.rs             # LogSleuthError enum, error chain helpers
|       +-- logging.rs           # tracing setup, debug mode activation; `init()` uses a two-layer tracing-subscriber registry: stderr layer always active; optional `Mutex<File>` layer added when `log_file` is Some (fail-open: file-open errors fall back to stderr-only); filter priority: RUST_LOG > --debug CLI > config level > default info
|       +-- constants.rs         # Named constants (limits, defaults, versions); includes MAX_CLIPBOARD_ENTRIES (clipboard export row cap); **MAX_TAIL_WATCH_FILES=100** (maximum simultaneously-watched files in Live Tail — gui.rs caps the list by mtime-desc sort before start_tail); **DIR_WATCH_POLL_INTERVAL_MS=2000**, **DIR_WATCH_CANCEL_CHECK_INTERVAL_MS=100**, **MIN_DIR_WATCH_POLL_INTERVAL_MS=1000**, **MAX_DIR_WATCH_POLL_INTERVAL_MS=60000**; **TAIL_POLL_INTERVAL_MS=500**, **TAIL_CANCEL_CHECK_INTERVAL_MS=100**, **MIN_TAIL_POLL_INTERVAL_MS=100**, **MAX_TAIL_POLL_INTERVAL_MS=10000**; **MAX_TOTAL_ENTRIES=1_000_000**, **MIN_MAX_TOTAL_ENTRIES=10_000**, **ABSOLUTE_MAX_TOTAL_ENTRIES=MAX_TOTAL_ENTRIES**; **MIN_MAX_FILES=1**, **DEFAULT_MAX_DEPTH=10**, **ABSOLUTE_MAX_DEPTH=50**; **MAX_SCAN_THREADS=64**
+-- profiles/
|   +-- veeam_vbr.toml           # Veeam Backup & Replication
|   +-- veeam_vbo365.toml        # Veeam Backup for M365
|   +-- iis_w3c.toml             # IIS W3C format
|   +-- sql_server_error.toml    # SQL Server ERRORLOG
|   +-- sql_server_agent.toml    # SQL Server Agent SQLAGENT.OUT
|   +-- apache_combined.toml     # Apache / nginx Combined Access log
|   +-- nginx_error.toml         # nginx error log
|   +-- windows_dhcp.toml        # Windows DHCP Server daily logs
|   +-- intune_ime.toml          # Microsoft Intune Management Extension (CMTrace format)
|   +-- windows_cluster.toml    # Windows Failover Cluster service log
|   +-- kubernetes_klog.toml    # Kubernetes klog format (control-plane components)
|   +-- exchange_tracking.toml  # Microsoft Exchange Server message tracking CSV
|   +-- postgresql_log.toml     # PostgreSQL server log
|   +-- tomcat_catalina.toml    # Apache Tomcat / Catalina log
|   +-- sccm_cmtrace.toml       # Microsoft SCCM / ConfigMgr (CMTrace format)
|   +-- windows_firewall.toml   # Windows Firewall log (pfirewall.log)
|   +-- syslog_rfc3164.toml     # BSD syslog
|   +-- syslog_rfc5424.toml      # IETF syslog
|   +-- json_lines.toml          # JSON Lines (generic)
|   +-- log4j_default.toml       # Log4j/Logback default
|   +-- generic_timestamp.toml   # Generic timestamp+message
|   +-- plain_text.toml          # Fallback (no structure)
+-- tests/
|   +-- e2e_discovery.rs         # E2E: discovery pipeline, auto-detect, parse, timestamp, severity
|   +-- fixtures/                # Sample log files per format for testing (veeam_vbr_sample.log, iis_w3c_sample.log, veeam_vbo365_sample.log)
|   +-- profiles/                # Test profile TOML files
+-- assets/
|   +-- app.manifest             # Windows UAC/DPI manifest
|   +-- icon.svg                 # Master icon source (512x512, regenerate PNGs/ICO from this)
|   +-- icon.ico                 # Multi-res Windows ICO (16/32/48/64/128/256 px), embedded by build.rs
|   +-- icon.png                 # Canonical 512x512 PNG, embedded in EXE at compile time via include_bytes!
|   +-- icon_32.png              # 32x32 PNG for egui fallback
|   +-- icon_48.png              # 48x48 PNG for taskbar/dock medium
|   +-- icon_256.png             # 256x256 PNG for installer/large display
|   +-- icon_512.png             # 512x512 PNG for high-DPI display
+-- examples/
|   +-- gen_icons.rs             # Dev tool: renders SVG -> PNG/ICO (cargo run --example gen_icons)
+-- installer/
|   +-- windows/
|   |   +-- logsleuth.nsi        # NSIS installer script
|   +-- macos/
|   |   +-- create-dmg.sh        # DMG builder
|   +-- linux/
|       +-- create-appimage.sh   # AppImage builder
+-- .github/
|   +-- workflows/
|       +-- ci.yml               # Build + test on all platforms
|       +-- release.yml          # Tag-triggered release pipeline
+-- build.rs                     # Embeds assets/icon.ico into the Windows EXE via winres (titlebar/taskbar/Alt+Tab)
+-- Cargo.toml                   # Dependencies and metadata
+-- Cargo.lock                   # Locked dependency versions
+-- config.example.toml          # Example configuration file
+-- scripts/
|   +-- New-LogSleuthProfile.ps1  # PowerShell profile generator: samples a log directory, infers timestamp/severity/line patterns, writes a .toml to %APPDATA%\LogSleuth\profiles\ for use as an external profile
+-- update-application.ps1       # Windows release script
+-- update-application.sh        # Unix release script (bash): semver validation, build/test/lint, git tag/push, old-tag pruning, dry-run/force/rollback
+-- LogSleuth-Specification.md   # Full specification document
+-- ATLAS.md                     # This file
+-- PROGRESS.md                  # Implementation progress tracker
+-- README.md                    # User-facing documentation
```

---

## 5. Entry Points, APIs, and Extension Points

### Entry Points

| Entry Point | Location | Description |
|------------|----------|-------------|
| GUI application | `src/main.rs` | Primary entry point; launches eframe window |
| CLI arguments | `src/main.rs` | `--debug`, `--profile-dir`, `--filter-level`, `[PATH]` |

### Internal APIs (Cross-Layer Boundaries)

| API | Location | Consumers |
|-----|----------|-----------|
| `ScanManager::start_scan(root, profiles, config)` | `app::scan` | UI layer (`gui.rs`) |
| `ScanManager::cancel_scan()` | `app::scan` | UI layer |
| `ScanManager::poll_progress() -> Vec<ScanProgress>` | `app::scan` | UI layer (called from `eframe::App::update`) |
| `TailManager::start_tail(files, entry_id_start)` | `app::tail` | UI layer (`gui.rs`) |
| `TailManager::stop_tail()` | `app::tail` | UI layer |
| `TailManager::is_active() -> bool` | `app::tail` | UI layer |
| `TailManager::poll_progress() -> Vec<TailProgress>` | `app::tail` | UI layer (called from `eframe::App::update`) |
| `DirWatcher::start_watch(root, known_paths, config)` | `app::dir_watcher` | UI layer (`gui.rs`) — called after scan ParsingCompleted on directory sessions |
| `DirWatcher::stop_watch()` | `app::dir_watcher` | UI layer — called on new-session, open-logs, and app exit |
| `DirWatcher::poll_progress() -> Vec<DirWatchProgress>` | `app::dir_watcher` | UI layer (called from `eframe::App::update`) |
| `discover_files(root, config, on_file_found) -> Result<(Vec<DiscoveredFile>, Vec<String>, usize)>` | `core::discovery` | `app::scan` background thread. Third element is raw file count before ingest limit. When count > limit, files are sorted by mtime descending and truncated. |
| `parse_content(content, path, profile, config, id_start) -> ParseResult` | `core::parser` | `app::scan` background thread |
| `profile::auto_detect(file_name, sample_lines, profiles) -> Option<DetectionResult>` | `core::profile` | `app::scan` background thread. Confidence = content_match ratio + `AUTO_DETECT_FILENAME_BONUS` (0.3) if filename glob matches; threshold is `AUTO_DETECT_MIN_CONFIDENCE` (0.3). A filename match alone is sufficient. |
| `apply_filters(entries, state) -> Vec<usize>` | `core::filter` | App layer |
| `apply_dedup(entries, indices, mode) -> (Vec<usize>, HashMap<usize, DedupInfo>)` | `core::filter` | App layer (post-filter step) |
| `load_all_profiles(user_dir) -> (Vec<FormatProfile>, Vec<ProfileError>)` | `app::profile_mgr` | `main.rs` at startup |
| `load_config(config_dir) -> (AppConfig, Vec<String>)` | `platform::config` | `main.rs` at startup — loads and validates `config.toml`, falls back to defaults for missing/invalid files |
| `export_csv(entries, path)` / `export_json(entries, path)` | `core::export` | App layer |

### Extension Points

| Extension | Mechanism | User action |
|-----------|-----------|-------------|
| Custom format profiles (external) | `.toml` file in `%APPDATA%\LogSleuth\profiles\` — auto-loaded at startup, overrides built-in profile with same `id` | Drop `.toml` file + **Options > Reload Profiles** (or restart) |
| Profile generator | `scripts/New-LogSleuthProfile.ps1` — samples a log directory, infers patterns, writes `.toml` | Run script, then Reload Profiles in Options |
| Configuration overrides | `config.toml` | Edit file, restart |

---

## 6. Build, Test, CI, and Release

### Build

```bash
# Debug build
cargo build

# Release build (all platforms)
cargo build --release

# Run with debug logging
RUST_LOG=debug cargo run

# Run with specific path
cargo run --release -- /path/to/logs
```

### Test

```bash
# Full test suite (unit + E2E)
cargo test

# E2E tests only
cargo test --test e2e_discovery

# With debug output
RUST_LOG=debug cargo test -- --nocapture
```

### CI (GitHub Actions)

- **ci.yml**: Triggered on push/PR to main, and `workflow_dispatch` (for manual verification per Part E). Runs on Windows, macOS, Linux.
  - `cargo build --release`
  - `cargo clippy -- -D warnings`
  - `cargo test`
  - `cargo fmt -- --check`

- **release.yml**: Triggered on `v*` tag push.
  - Builds release binaries on all platforms
  - Builds platform installers (NSIS, DMG, AppImage)
  - **`build-windows-portable`** job: compiles with `RUSTFLAGS="-C target-feature=+crt-static"` and produces `LogSleuth-{VERSION}-windows-portable.exe` — a fully self-contained EXE with the MSVC CRT statically linked; no installation or redistributable required
  - Creates GitHub Release with 4 Windows artifacts: installer + portable EXE; macOS DMG; Linux AppImage

### Release

- **Windows**: `.\update-application.ps1 [-Version x.y.z] [-Notes "..."] [-Force] [-DryRun]`
  - Reads/writes `[package]` version from `Cargo.toml`
  - Runs `cargo update` to refresh `Cargo.lock`
  - Runs `cargo build --release` (Windows/host binary only — validation step)
  - Runs `cargo fmt -- --check` and `cargo clippy -- -D warnings` (mirrors CI checks to catch failures before the tag is pushed)
  - Runs `cargo test` (rolls back all version changes on any failure)
  - Optionally runs `makensis installer/windows/logsleuth.nsi` if both file and tool exist
  - Commits version bump, creates annotated tag, pushes to origin
  - **macOS and Linux binaries are built by `release.yml` CI triggered by the pushed tag**
  - Prunes all older `v*.*.*` tags and GitHub releases (keeps only new tag)
  - DryRun mode prints the full plan without touching files, git, or the remote
- **Unix**: `./update-application.sh --version x.y.z` (not yet implemented)
- Both scripts follow DevWorkflow Part A Rule 18.

---

## 7. Configuration Schema

See `config.example.toml` and Specification Section 6.

**Loading**: `platform::config::load_config()` reads `config.toml` from `%APPDATA%\LogSleuth\config.toml` (or XDG equivalent). Missing file is silently accepted (first-run default). Unparseable file produces a warning and falls back to defaults (fail-fast per Rule 13).

**Validation**: All config values validated at startup with named-constant limits. Invalid values produce actionable error messages listing the invalid value, the expected range, and the default that will be used. All validation errors are accumulated before reporting (Rule 11).

**Versioning**: Config file includes no explicit version field; unknown keys are warned and ignored for forward compatibility.

---

## 8. Critical Invariants

These invariants MUST hold at all times. Violation is a defect.

| ID | Invariant |
|----|-----------|
| INV-01 | Core layer has zero dependencies on UI, platform, or I/O |
| INV-02 | No panics in library code; all errors propagated via `Result` |
| INV-03 | Source log files are never modified, deleted, or locked exclusively |
| INV-04 | UI thread never blocks on file I/O, parsing, or non-trivial sorting operations; background scan thread performs the initial chronological sort before streaming entries; append scans call sort_entries_chronologically() on the UI thread after all batches arrive (no I/O; sort only) |
| INV-05 | All cross-thread communication uses channels; no shared mutable state. Applies equally to scan and tail managers. |
| INV-06 | User-provided regex patterns are compiled with size/complexity limits |
| INV-07 | Memory usage is bounded: streaming parser, bounded collections with MAX_SIZE constants |
| INV-08 | All named constants (limits, defaults) are defined in `util::constants` |
| INV-09 | Every user-visible feature has E2E test coverage |
| INV-10 | Atlas, specification, code, and tests never contradict each other |

---

## 9. Runtime Dependencies

| Dependency | Required | Minimum Version | Rationale |
|-----------|----------|----------------|-----------|
| None (Rust static binary) | -- | -- | LogSleuth compiles to a static binary with no runtime dependencies |

### Build Dependencies

| Dependency | Minimum Version | Rationale |
|-----------|----------------|-----------|
| Rust toolchain | 1.75+ | Edition 2021, async traits stabilised |
| Windows 10 SDK | 10.0.19041+ | Windows builds only |
| Xcode Command Line Tools | 14+ | macOS builds only |
| `winres` (build-dep, Windows only) | 0.1 | Embeds ICO resource into the Windows EXE so the OS shows the icon in titlebar, taskbar, Alt+Tab, and Explorer |
| `resvg` (dev-dep, gen_icons tool) | 0.44 | SVG -> PNG rendering for icon asset regeneration |
| `ico` (dev-dep, gen_icons tool) | 0.3 | Builds multi-resolution ICO file from individual PNG layers |

---

## 10. Debug Mode

**Activation**:
- Environment variable: `RUST_LOG=debug` (or `RUST_LOG=trace` for maximum detail)
- CLI flag: `--debug` (equivalent to `RUST_LOG=debug`)
- Config file: `[logging] level = "debug"`

**Output location**: stderr by default. When `[logging] file` is configured, also writes to that file simultaneously using a second `tracing-subscriber` layer backed by `Mutex<File>`. Fail-open: if the file cannot be opened, a warning is printed to stderr and logging continues without the file layer (Rule 11).

**Content at debug level**: Function entry/exit for scan operations, profile auto-detection scoring, per-file parse progress, filter application timing, regex compilation results, config loading details.

**Content at trace level**: Individual line parse attempts, regex match details, chunk read operations, channel message counts.

**Safety**: Debug output never includes file content beyond the first 200 characters of any log line. No secrets, tokens, or PII are logged at any level.
