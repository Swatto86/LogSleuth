# LogSleuth -- Project Atlas

> **Status**: Increment 17 complete — 14 built-in profiles, About dialog, sidebar layout fixes
> **Last updated**: 2026-02-25

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
| **Format Profile** | A TOML definition describing how to detect and parse a specific log format (regex patterns, timestamp format, severity mappings) |
| **Discovery** | Recursive scan of a directory tree to find candidate log files using glob patterns |
| **Auto-detection** | Matching discovered files to format profiles by sampling the first N lines against each profile's content regex |
| **Severity** | Normalised enum: Critical, Error, Warning, Info, Debug, Unknown |
| **Unified Timeline** | Merged, chronologically sorted view of all parsed entries across all discovered files. Sort is performed on the background scan thread before entries are streamed to the UI. |
| **File Colour** | Each source file is assigned a unique colour from a 24-entry palette; a coloured stripe on every timeline row and a coloured dot in the file list identify the origin file visually (CMTrace-style) |
| **Correlation** | Viewing entries across multiple log files within a time window centred on a selected entry |
| **Scan** | A complete discovery + parse operation on a target directory |

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
|   +-- main.rs                  # Entry point, CLI parsing, logging init, GUI launch; configure_fonts() loads Consolas (primary monospace), Segoe UI (primary proportional), Segoe UI Symbol + Emoji (Unicode fallbacks) on Windows
|   +-- lib.rs                   # Library crate entry point (exposes modules for integration tests)
|   +-- gui.rs                   # eframe::App implementation, scan progress routing, panel wiring; sidebar split into two independent ScrollAreas (discovery top ~45%, filters bottom)
|   +-- app/
|   |   +-- mod.rs
|   |   +-- profile_mgr.rs       # Profile loading (built-in + user), override logic
|   |   +-- scan.rs              # Scan lifecycle: background thread, cancel (AtomicBool), retry backoff, UTF-16 BOM detection, plain-text fallback, background chronological sort before streaming batches
|   |   +-- session.rs           # Session persistence: SessionData + PersistedFilter structs (serde JSON); session_path(), save() (atomic write via .json.tmp rename), load() (returns None on missing/corrupt/version-mismatch — never errors to user); SESSION_VERSION const for forward-compat
|   |   +-- state.rs             # Application state; tail flags; show_log_summary; show_about; bookmarks: HashMap<u64,String>; correlation_active, correlation_window_secs, correlated_ids: HashSet<u64>; session_path: Option<PathBuf> (never cleared); initial_scan: Option<PathBuf> (startup re-scan without clear()); toggle_bookmark(), is_bookmarked(), bookmark_count(), clear_bookmarks(), bookmarks_report(), filtered_results_report() (bounded to MAX_CLIPBOARD_ENTRIES), update_correlation(), next_entry_id(), save_session(), restore_from_session()
|   |   +-- tail.rs              # Live tail: TailManager + run_tail_watcher poll loop (500 ms), per-file byte-offset tracking, partial-line buffer, rotation/truncation detection, TailFileInfo; file-selection filter applied before start (respects hide_all_sources + source_files whitelist)
|   +-- core/
|   |   +-- mod.rs
|   |   +-- model.rs             # LogEntry, Severity, FormatProfile structs; FormatProfile includes severity_override: HashMap<Severity,Vec<Regex>> + apply_severity_override() method
|   |   +-- discovery.rs         # Recursive traversal (walkdir), glob include/exclude, filter_entry dir exclusion, metadata
|   |   +-- export.rs            # CSV/JSON serialisation
|   |   +-- filter.rs            # Composable filter engine: severity, text (exact or fuzzy subsequence), regex, absolute/relative time window, source file whitelist (hide_all_sources flag for explicit "none" state); bookmark filter (bookmarks_only + bookmarked_ids populated by app layer)
|   |   +-- profile.rs           # TOML profile parsing, validation, auto-detection scoring; SeverityOverrideDef TOML struct; override patterns compiled via compile_regex in validate_and_compile
|   |   +-- parser.rs            # Stream-oriented log parsing, multi-line handling, chrono timestamp parsing
|   +-- ui/
|   |   +-- mod.rs
|   |   +-- panels/
|   |   |   +-- mod.rs
|   |   |   +-- about.rs         # About dialog: centred modal window (version from CARGO_PKG_VERSION, GitHub link, MIT licence); show_about flag on AppState; ⓘ button right-aligned in menu bar
|   |   |   +-- discovery.rs     # Directory picker, scan controls, file list (max_height 360 px)
|   |   |   +-- timeline.rs      # Virtual-scrolling unified timeline; 4 px coloured left stripe per row; amber star button (★/☆) per row for bookmarking; gold tint on bookmarked rows; bookmark toggle applied after ScrollArea to avoid borrow conflict
|   |   |   +-- detail.rs        # Entry detail pane (no height cap); Show in Folder button (Windows: explorer /select,; macOS: open -R; Linux: xdg-open)
|   |   |   +-- summary.rs       # Scan summary dialog (overall statistics + per-file breakdown)
|   |   |   +-- log_summary.rs   # Log-entry summary panel: severity breakdown table + collapsible message preview lists (max 50 rows/severity), colour-coded; opened via View menu or Filters "Summary" button
|   |   |   +-- filters.rs       # Filter controls sidebar: two button rows (Row 1: severity presets — Errors only/Errors+Warn/Err+Warn+15m/Clear; Row 2: Summary/Bookmarks/clear bm); severity checkboxes; text/regex inputs; fuzzy ~ toggle; relative time quick-buttons (15m/1h/6h/24h) + custom input + rolling-window live indicator; source file checklist with coloured dot + Solo button + real-time search box (shown >8 files); Select All/None on visible subset; "● Rolling window (live)" indicator; "Copy" clipboard button at entry-count footer (disabled when empty)
|   |   +-- theme.rs             # Colours, severity mapping, layout constants; 24-entry FILE_COLOUR_PALETTE for per-file stripes; SIDEBAR_WIDTH=320 (min_width enforced on SidePanel)
|   +-- platform/
|   |   +-- mod.rs
|   |   +-- fs.rs                # FileReader trait + OS implementations
|   |   +-- config.rs            # Platform-specific config/data paths
|   +-- util/
|       +-- mod.rs
|       +-- error.rs             # LogSleuthError enum, error chain helpers
|       +-- logging.rs           # tracing setup, debug mode activation
|       +-- constants.rs         # Named constants (limits, defaults, versions); includes MAX_CLIPBOARD_ENTRIES (clipboard export row cap)
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
|   +-- syslog_rfc3164.toml      # BSD syslog
|   +-- syslog_rfc5424.toml      # IETF syslog
|   +-- json_lines.toml          # JSON Lines (generic)
|   +-- log4j_default.toml       # Log4j/Logback default
|   +-- generic_timestamp.toml   # Generic timestamp+message
|   +-- plain_text.toml          # Fallback (no structure)
+-- tests/
|   +-- e2e_discovery.rs         # E2E: discovery pipeline, auto-detect, parse, timestamp, severity
|   +-- fixtures/                # Sample log files per format for testing (veeam_vbr_sample.log, iis_w3c_sample.log)
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
+-- update-application.ps1       # Windows release script
+-- update-application.sh        # Unix release script
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
| `discover_files(root, config, on_file_found) -> Result<(Vec<DiscoveredFile>, Vec<String>)>` | `core::discovery` | `app::scan` background thread |
| `parse_content(content, path, profile, config, id_start) -> ParseResult` | `core::parser` | `app::scan` background thread |
| `profile::auto_detect(file_name, sample_lines, profiles) -> Option<DetectionResult>` | `core::profile` | `app::scan` background thread |
| `apply_filters(entries, state) -> Vec<usize>` | `core::filter` | App layer |
| `load_all_profiles(user_dir) -> (Vec<FormatProfile>, Vec<ProfileError>)` | `app::profile_mgr` | `main.rs` at startup |
| `export_csv(entries, path)` / `export_json(entries, path)` | `core::export` | App layer |

### Extension Points

| Extension | Mechanism | User action |
|-----------|-----------|-------------|
| Custom format profiles | TOML file in user profile directory | Drop `.toml` file, restart or rescan |
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

- **ci.yml**: Triggered on push/PR to main. Runs on Windows, macOS, Linux.
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

**Validation**: All config values validated at startup with named-constant limits. Invalid values produce actionable error messages listing the invalid value, the expected range, and the default that will be used.

**Versioning**: Config file includes no explicit version field; unknown keys are warned and ignored for forward compatibility.

---

## 8. Critical Invariants

These invariants MUST hold at all times. Violation is a defect.

| ID | Invariant |
|----|-----------|
| INV-01 | Core layer has zero dependencies on UI, platform, or I/O |
| INV-02 | No panics in library code; all errors propagated via `Result` |
| INV-03 | Source log files are never modified, deleted, or locked exclusively |
| INV-04 | UI thread never blocks on file I/O, parsing, or sorting operations; chronological sort runs on the background scan thread before entries are streamed |
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

**Output location**: stderr by default. If `[logging] file` is set, also writes to that file.

**Content at debug level**: Function entry/exit for scan operations, profile auto-detection scoring, per-file parse progress, filter application timing, regex compilation results, config loading details.

**Content at trace level**: Individual line parse attempts, regex match details, chunk read operations, channel message counts.

**Safety**: Debug output never includes file content beyond the first 200 characters of any log line. No secrets, tokens, or PII are logged at any level.
