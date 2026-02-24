# LogSleuth -- Project Atlas

> **Status**: Pre-implementation (specification phase)
> **Last updated**: 2026-02-24

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
| **Unified Timeline** | Merged, chronologically sorted view of all parsed entries across all discovered files |
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
|   +-- main.rs                  # Entry point, CLI parsing, single-instance guard
|   +-- app.rs                   # App state, eframe::App impl
|   +-- app/
|   |   +-- mod.rs
|   |   +-- scan.rs              # Scan lifecycle, threading, progress
|   |   +-- state.rs             # Application state, filter state, selection
|   |   +-- profile_mgr.rs       # Profile loading (built-in + user), override logic
|   +-- core/
|   |   +-- mod.rs
|   |   +-- model.rs             # LogEntry, Severity, FormatProfile structs
|   |   +-- discovery.rs         # Directory traversal, glob matching, file enumeration
|   |   +-- profile.rs           # TOML profile parsing, validation, auto-detection scoring
|   |   +-- parser.rs            # Stream-oriented log parsing, multi-line handling
|   |   +-- filter.rs            # Composable filter engine
|   |   +-- export.rs            # CSV/JSON serialisation
|   +-- ui/
|   |   +-- mod.rs
|   |   +-- panels/
|   |   |   +-- mod.rs
|   |   |   +-- discovery.rs     # Directory picker, scan controls, file list
|   |   |   +-- timeline.rs      # Virtual-scrolling unified timeline
|   |   |   +-- detail.rs        # Entry detail pane
|   |   |   +-- summary.rs       # Scan summary dialog
|   |   |   +-- filters.rs       # Filter controls sidebar
|   |   +-- theme.rs             # Colours, severity mapping, layout constants
|   +-- platform/
|   |   +-- mod.rs
|   |   +-- fs.rs                # FileReader trait + OS implementations
|   |   +-- config.rs            # Platform-specific config/data paths
|   +-- util/
|       +-- mod.rs
|       +-- error.rs             # LogSleuthError enum, error chain helpers
|       +-- logging.rs           # tracing setup, debug mode activation
|       +-- constants.rs         # Named constants (limits, defaults, versions)
+-- profiles/
|   +-- veeam_vbr.toml           # Veeam Backup & Replication
|   +-- veeam_vbo365.toml        # Veeam Backup for M365
|   +-- iis_w3c.toml             # IIS W3C format
|   +-- syslog_rfc3164.toml      # BSD syslog
|   +-- syslog_rfc5424.toml      # IETF syslog
|   +-- json_lines.toml          # JSON Lines (generic)
|   +-- log4j_default.toml       # Log4j/Logback default
|   +-- generic_timestamp.toml   # Generic timestamp+message
|   +-- plain_text.toml          # Fallback (no structure)
+-- tests/
|   +-- e2e/                     # End-to-end tests (real files, real parsing)
|   +-- fixtures/                # Sample log files per format for testing
|   +-- profiles/                # Test profile TOML files
+-- assets/
|   +-- app.manifest             # Windows UAC/DPI manifest
|   +-- icon.ico                 # Application icon (Windows)
|   +-- icon.png                 # Application icon (Linux/macOS)
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
+-- build.rs                     # Icon embedding, profile embedding
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
| `ScanManager::start_scan(path, config)` | `app::scan` | UI layer |
| `ScanManager::cancel_scan()` | `app::scan` | UI layer |
| `FilterEngine::apply(entries, filters) -> Vec<usize>` | `core::filter` | App layer |
| `ProfileLoader::load_all(built_in, user_dir) -> Vec<FormatProfile>` | `core::profile` | App layer |
| `Parser::parse_file(reader, profile) -> Vec<LogEntry>` | `core::parser` | App scan thread |
| `Discovery::scan(root, config) -> Vec<DiscoveredFile>` | `core::discovery` | App scan thread |
| `Exporter::export_csv(entries, path)` | `core::export` | App layer |
| `Exporter::export_json(entries, path)` | `core::export` | App layer |

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
# Full test suite
cargo test

# E2E tests only
cargo test --test e2e

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
  - Creates GitHub Release with artifacts

### Release

- Windows: `.\update-application.ps1 -Version "x.y.z"`
- Unix: `./update-application.sh --version x.y.z`
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
| INV-04 | UI thread never blocks on file I/O or parsing operations |
| INV-05 | All cross-thread communication uses channels; no shared mutable state |
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
