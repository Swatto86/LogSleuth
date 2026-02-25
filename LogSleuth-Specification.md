# LogSleuth Specification

## 1. Purpose

LogSleuth is a fast, cross-platform log file viewer and analyser built with Rust and egui. It discovers log files within a directory tree, identifies their format using extensible TOML-based profiles, parses them into a unified event model, and presents a filterable, colour-coded timeline for troubleshooting.

Where EventSleuth targets the Windows Event Log API for structured system events, LogSleuth targets the unstructured reality of application log files scattered across disk in dozens of vendor-specific formats.

### 1.1 Target Users

- IT administrators and support engineers troubleshooting application issues
- Systems administrators analysing service failures across multi-product environments
- Security teams reviewing application-level audit trails

### 1.2 Out of Scope (v1.0)

- Compressed/rotated log archives (`.gz`, `.zip`) -- deferred to future release
- Remote log collection (SSH, SMB, WinRM) -- local/mounted paths only
- Real-time log tailing/streaming -- initial release is snapshot-based
- Log ingestion pipelines or SIEM integration
- Modification or deletion of source log files

### 1.3 Implementation Status Convention

Requirements are tagged where their implementation status is notable:
- `[IMPL]` -- fully implemented and tested
- `[PARTIAL]` -- partially implemented; detail noted inline
- `[FUTURE]` -- planned but not yet implemented

---

## 2. Functional Requirements

### 2.1 Log Discovery

| ID | Requirement |
|----|-------------|
| DISC-01 | User points LogSleuth at a root directory path via the UI or CLI argument |
| DISC-02 | Recursive scan discovers all candidate log files, honouring configurable include/exclude glob patterns |
| DISC-03 | Default include patterns: `*.log`, `*.log.[0-9]*`, `*.txt` (when text content detected) |
| DISC-04 | Default exclude patterns: `*.gz`, `*.zip`, `*.bak`, `node_modules/`, `.git/` |
| DISC-05 | Discovery respects a configurable maximum depth limit (default: 10 levels) |
| DISC-06 | Discovery respects a configurable maximum file count limit (default: 500 files) |
| DISC-07 | Discovery runs in a background thread with progress reporting to the UI |
| DISC-08 | Files that cannot be read (permissions, locks) are recorded as warnings, not fatal errors |
| DISC-09 | Discovery results show: file path, size, detected format profile, last modified timestamp |

### 2.2 Format Profiles

| ID | Requirement |
|----|-------------|
| PROF-01 | Format profiles are defined in TOML files, one per format |
| PROF-02 | Built-in profiles are embedded in the binary and loaded at startup |
| PROF-03 | User-defined profiles can be loaded from a configurable directory (default: `~/.config/logsleuth/profiles/` on Linux/macOS, `%APPDATA%\LogSleuth\profiles\` on Windows) |
| PROF-04 | User-defined profiles override built-in profiles with the same `id` |
| PROF-05 | Profile validation occurs at startup; invalid profiles produce actionable error messages and are skipped |
| PROF-06 | Auto-detection: LogSleuth reads the first N lines (default: 20) of each file and attempts to match against all loaded profiles by regex hit rate, selecting the highest-confidence match |
| PROF-07 | Users can manually override the detected profile for any file via the UI |
| PROF-08 | Files that match no profile are assigned a fallback "plain text" profile (no timestamp/level extraction, full-text only) |

#### 2.2.1 Profile Schema

```toml
[profile]
id = "veeam-vbr"
name = "Veeam Backup & Replication"
version = "1.0"
description = "VBR service and job logs"

[detection]
# Glob patterns for filename matching (optional, aids auto-detection)
file_patterns = ["Svc.Veeam*.log", "Job.*.log", "Agent.*.log"]
# Regex applied to first N lines to confirm format (required)
content_match = '^\[\d{2}\.\d{2}\.\d{4}\s\d{2}:\d{2}:\d{2}\]\s<\d+>'

[parsing]
# Named capture groups: timestamp, level, source, thread, message
# Groups not present in the regex are treated as absent for that format
line_pattern = '^\[(?P<timestamp>\d{2}\.\d{2}\.\d{4}\s\d{2}:\d{2}:\d{2})\]\s<(?P<thread>\d+)>\s(?P<level>\w+)\s+(?P<message>.+)$'
timestamp_format = "%d.%m.%Y %H:%M:%S"

# How to handle lines that do not match line_pattern
# "continuation" = append to previous entry's message (stack traces, multi-line)
# "skip" = ignore the line
# "raw" = treat as a standalone entry with no parsed fields
multiline_mode = "continuation"

[severity_mapping]
# Maps extracted level strings to normalised severity enum
# Case-insensitive matching
error = ["Error", "Failed", "Exception"]
warning = ["Warning", "Warn"]
info = ["Info", "Information"]
debug = ["Debug", "Verbose"]
critical = ["Critical", "Fatal"]
```

#### 2.2.2 Built-in Profiles (v1.0)

| Profile ID | Product | Line Format Example |
|------------|---------|-------------------|
| `veeam-vbr` | Veeam Backup & Replication | `[23.11.2016 23:07:16] <01> Info  Message` |
| `veeam-vbo365` | Veeam Backup for M365 | `3/27/2019 9:21:14 AM 1 (2384) Message` |
| `iis-w3c` | IIS Web Server | `2024-01-15 14:30:22 W3SVC1 GET /api/health ...` |
| `syslog-rfc3164` | Syslog (BSD) | `Jan 15 14:30:22 hostname sshd[1234]: Message` |
| `syslog-rfc5424` | Syslog (IETF) | `<34>1 2024-01-15T14:30:22Z host app 1234 - Message` |
| `json-lines` | JSON Lines (generic) | `{"timestamp":"...","level":"error","message":"..."}` |
| `log4j-default` | Log4j/Logback | `2024-01-15 14:30:22,123 ERROR [main] Class - Message` |
| `generic-timestamp` | Generic fallback | `2024-01-15 14:30:22 Message` |
| `plain-text` | No structure (fallback) | Any text, no parsing |

### 2.3 Parsing Engine

| ID | Requirement |
|----|-------------|
| PARS-01 | Parsing is stream-oriented: files are read in chunks (configurable, default 64 KB), not loaded entirely into memory |
| PARS-02 | Parsing runs in background thread(s), reporting progress to the UI |
| PARS-03 | Each parsed log entry produces a `LogEntry` struct with normalised fields (see 3.1) |
| PARS-04 | Multi-line entries (stack traces, continuation lines) are concatenated into a single `LogEntry` |
| PARS-05 | Parse errors on individual lines are counted and reported in the scan summary, not treated as fatal |
| PARS-06 | Maximum entry size limit (default: 64 KB) prevents unbounded memory from malformed files |
| PARS-07 | For files exceeding a configurable threshold (default: 100 MB), a "large file" indicator is shown and the user is warned about potential scan duration |

### 2.4 Unified Timeline View

| ID | Requirement |
|----|-------------|
| VIEW-01 | All parsed entries from all discovered files are merged into a single chronological timeline |
| VIEW-02 | Virtual scrolling handles 1M+ entries without performance degradation |
| VIEW-03 | Each row displays: timestamp, severity icon/colour, source file (abbreviated), message preview |
| VIEW-04 | Selecting a row shows the full entry detail in a detail pane (full message, file path, line number, thread ID, raw text) |
| VIEW-05 | Severity colour coding: Critical=red, Error=dark red, Warning=amber, Info=default, Debug=grey |
| VIEW-06 | Entries with no parsed timestamp are sorted to the end with a visual indicator |

### 2.5 Filtering

| ID | Requirement |
|----|-------------|
| FILT-01 | Filter by severity level (multi-select: Critical, Error, Warning, Info, Debug) |
| FILT-02 | Filter by source file (multi-select from discovered files). When more than 8 files are loaded, a real-time search box appears above the list. **Select All** / **Select None** buttons operate on the currently visible (search-filtered) subset. An `N / total` counter shows how many files are visible. |
| FILT-03 | Filter by time range. Two modes: (a) **relative rolling window** -- select a preset (15 min, 1 h, 6 h, 24 h) or type a custom number of minutes; the window advances automatically as the clock ticks, re-evaluated every second; (b) **absolute bounds** -- explicit start/end `DateTime<Utc>` stored in `FilterState.time_start` / `time_end` and evaluated in `core::filter`. The rolling window is computed in `app::state::apply_filters()` so `core` remains clock-free. |
| FILT-04 | Filter by text search (substring match across message field, case-insensitive) |
| FILT-05 | Filter by regex search (user-provided regex against message field) |
| FILT-06 | Filters are composable: all active filters are AND-combined |
| FILT-07 | Filter changes apply immediately (no "Apply" button). `[FUTURE]` Debounce on text input (300ms) is not yet implemented; text filters apply on every keystroke. |
| FILT-08 | Matched/total entry count displayed as a badge in the filter panel. `[PARTIAL]` Active filter count label (e.g. "3 filters active") is not yet shown. |
| FILT-09 | "Errors only" quick-filter button for the most common troubleshooting workflow |
| FILT-10 | `[IMPL]` Relative time window quick-buttons (15 min / 1 h / 6 h / 24 h) and a custom "Last ___ min" text input with Enter-to-commit and a clear (✕) button. A live feedback label shows the computed absolute "After HH:MM:SS" boundary. |

### 2.6 Cross-Log Correlation

> **[FUTURE -- Deferred to post-v1.0]** The requirements in this section are planned but not yet implemented.

| ID | Requirement |
|----|-------------|
| CORR-01 | Selecting a time window in one log file highlights all entries across all files within that window |
| CORR-02 | "Show context" action on any entry: display entries from all files within a configurable time window (default: +/- 30 seconds) |
| CORR-03 | Correlation view is a filtered subset of the unified timeline, not a separate view |

### 2.7 Export

| ID | Requirement |
|----|-------------|
| EXP-01 | Export filtered results to CSV (timestamp, severity, source file, message) |
| EXP-02 | Export filtered results to JSON (structured, one object per entry) |
| EXP-03 | Export includes a metadata header: scan path, filter criteria, export timestamp, entry count |
| EXP-04 | Export runs in a background thread with progress indicator |
| EXP-05 | Large exports (>100k entries) warn the user before proceeding |

### 2.8 Scan Summary

| ID | Requirement |
|----|-------------|
| SUMM-01 | `[IMPL]` After scanning completes, display a summary: total files scanned, total entries parsed, entries by severity, files with errors, parse error count, scan duration |
| SUMM-02 | `[IMPL]` Summary includes per-file breakdown: file path, format profile, entry count, error count, time range (earliest/latest timestamp) |
| SUMM-03 | `[IMPL]` Summary is accessible at any time via **File > Scan Summary** (`Ctrl+S`) after a scan completes |

---

## 3. Data Model

### 3.1 LogEntry (Normalised)

```
LogEntry {
    id: u64,                          // Unique ID (monotonic within session)
    timestamp: Option<DateTime<Utc>>,  // Parsed timestamp (None if unparseable)
    severity: Severity,                // Normalised severity enum
    source_file: PathBuf,              // Originating file path
    line_number: u64,                  // Starting line number in source file
    thread: Option<String>,            // Thread/process ID if available
    component: Option<String>,         // Source/component if available
    message: String,                   // Full message text (including continuation lines)
    raw_text: String,                  // Original unparsed text
    profile_id: String,                // ID of the format profile used to parse this entry
}
```

### 3.2 Severity Enum

```
Severity {
    Critical,   // System/service down, data loss
    Error,      // Operation failed
    Warning,    // Potential issue, degraded operation
    Info,       // Normal operational messages
    Debug,      // Diagnostic detail
    Unknown,    // Could not determine severity
}
```

### 3.3 FormatProfile (Runtime)

```
FormatProfile {
    id: String,
    name: String,
    version: String,
    description: String,
    file_patterns: Vec<GlobPattern>,
    content_match: Regex,
    line_pattern: Regex,
    timestamp_format: String,
    multiline_mode: MultilineMode,     // Continuation | Skip | Raw
    severity_mapping: HashMap<Severity, Vec<String>>,
}
```

---

## 4. Architecture

### 4.1 High-Level Architecture

```
+-------------------------------------------------------------+
|                        UI Layer (egui)                       |
|  +----------+  +-----------+  +--------+  +---------------+ |
|  | Discovery |  | Timeline  |  | Detail |  | Scan Summary  | |
|  | Panel     |  | View      |  | Pane   |  | Dialog        | |
|  +----------+  +-----------+  +--------+  +---------------+ |
+-------------------------------------------------------------+
         |               |             |
+-------------------------------------------------------------+
|                    Application Layer                         |
|  +-------------+  +----------+  +---------+  +------------+ |
|  | ScanManager |  | FilterEng|  | Exporter|  | ProfileMgr | |
|  +-------------+  +----------+  +---------+  +------------+ |
+-------------------------------------------------------------+
         |               |             |
+-------------------------------------------------------------+
|                      Core Layer                              |
|  +-------------+  +----------+  +---------+  +------------+ |
|  | Discovery   |  | Parser   |  | Profile |  | LogEntry   | |
|  | Engine      |  | Engine   |  | Loader  |  | Model      | |
|  +-------------+  +----------+  +---------+  +------------+ |
+-------------------------------------------------------------+
         |
+-------------------------------------------------------------+
|                   Platform Layer                             |
|  +------------+  +------------+  +-----------+              |
|  | FileSystem |  | Config     |  | Logging   |              |
|  | Adapter    |  | Adapter    |  | Adapter   |              |
|  +------------+  +------------+  +-----------+              |
+-------------------------------------------------------------+
```

### 4.2 Layer Responsibilities

**UI Layer** -- Presentation only. No business logic. Communicates with the Application Layer via channels/messages.

**Application Layer** -- Orchestrates operations: manages scan lifecycle, applies filters, coordinates export, loads profiles. Owns application state.

**Core Layer** -- Pure business logic. No I/O, no UI, no platform dependencies. All file discovery patterns, parsing logic, profile matching, and data model definitions live here. Fully testable without a GUI or filesystem.

**Platform Layer** -- Abstracts OS-specific behaviour behind trait interfaces. File reading, configuration paths, logging output. Enables cross-platform support.

### 4.3 Threading Model

```
+------------------+
|    UI Thread     | <-- Rendering, user input, state display
+--------+---------+
         |  channels (mpsc)
         v
+------------------+
|   Scan Thread    | <-- Discovery + parsing (one per scan operation)
+--------+---------+
         |  spawns per-file
         v
+------------------+
| Parse Workers    | <-- Parallel file parsing (thread pool, default: num_cpus)
+------------------+
         |
         v
+------------------+
| Export Thread     | <-- Background CSV/JSON export
+------------------+
```

All cross-thread communication uses `std::sync::mpsc` channels. No shared mutable state. The UI thread receives `ScanProgress`, `ParseResult`, and `ExportProgress` messages.

### 4.4 Module Breakdown

| Module | Responsibility | Dependencies |
|--------|---------------|-------------|
| `core::model` | `LogEntry`, `Severity`, `FormatProfile` structs | None |
| `core::discovery` | Recursive file discovery with glob matching | `core::model` |
| `core::profile` | Profile TOML parsing, validation, auto-detection | `core::model` |
| `core::parser` | Stream-oriented log parsing using profiles | `core::model`, `core::profile` |
| `core::filter` | Composable filter engine: severity, text, regex, absolute time bounds, source file whitelist; `FilterState` holds `relative_time_secs` (rolling window) and `relative_time_input` (UI buffer) | `core::model` |
| `core::export` | CSV/JSON serialisation | `core::model` |
| `app::scan` | Scan lifecycle management, threading, progress | `core::*` |
| `app::state` | Application state (`pending_scan`, `request_cancel`, `file_list_search`); `apply_filters()` computes absolute time bound from `relative_time_secs` before delegating to `core::filter` | `core::model` |
| `app::profile_mgr` | Load built-in + user profiles, handle overrides | `core::profile` |
| `ui::panels` | UI panels (discovery, timeline, detail, summary, filters); filters panel includes relative time quick-buttons and source file checklist with search | `app::*` |
| `ui::theme` | Colour scheme, severity colours, layout constants | None |
| `platform::fs` | File reading trait + OS implementations | None |
| `platform::config` | Config paths per OS | None |
| `util::error` | Typed error hierarchy | None |
| `util::logging` | Structured logging with debug mode | None |

---

## 5. Non-Functional Requirements

### 5.1 Performance

| ID | Requirement |
|----|-------------|
| PERF-01 | UI remains responsive (60fps) during scanning and parsing |
| PERF-02 | Virtual scrolling renders only visible rows (same pattern as EventSleuth) |
| PERF-03 | 100k entries: filter application < 100ms |
| PERF-04 | 1M entries: filter application < 500ms |
| PERF-05 | Memory usage bounded: streaming parser, entries stored in contiguous Vec with indices |
| PERF-06 | Large file scanning (>1 GB) uses memory-mapped I/O where available |

### 5.2 Reliability

| ID | Requirement |
|----|-------------|
| REL-01 | No panics in library code; all errors propagated via Result types |
| REL-02 | File read errors (permissions, locks, encoding) are non-fatal per file |
| REL-03 | Malformed log lines are non-fatal per line |
| REL-04 | Profile parse errors are non-fatal per profile |
| REL-05 | Application state is never corrupted by background thread failures |

### 5.3 Cross-Platform

| ID | Requirement |
|----|-------------|
| PLAT-01 | Windows 10+, macOS 12+, Linux (X11/Wayland with glibc 2.31+) |
| PLAT-02 | No platform-specific code in core or app layers |
| PLAT-03 | CI builds and tests on all three platforms |
| PLAT-04 | Path handling uses `std::path::PathBuf` throughout, no hardcoded separators |

### 5.4 Security

| ID | Requirement |
|----|-------------|
| SEC-01 | LogSleuth operates read-only on all source files; no writes to scan directories |
| SEC-02 | User-defined profile regexes are compiled with size/complexity limits to prevent ReDoS |
| SEC-03 | No secrets, tokens, or PII are logged by LogSleuth itself |
| SEC-04 | Export files are written atomically (write to temp, rename) to prevent partial output |

---

## 6. Configuration

Configuration is stored in TOML format at the platform-appropriate location:
- Linux/macOS: `~/.config/logsleuth/config.toml`
- Windows: `%APPDATA%\LogSleuth\config.toml`

```toml
[discovery]
max_depth = 10
max_files = 500
include_patterns = ["*.log", "*.log.[0-9]*", "*.txt"]
exclude_patterns = ["*.gz", "*.zip", "*.bak", "node_modules/", ".git/"]

[parsing]
chunk_size_bytes = 65536
max_entry_size_bytes = 65536
large_file_threshold_bytes = 104857600  # 100 MB
worker_threads = 0                       # 0 = auto (num_cpus)
content_detection_lines = 20

[ui]
theme = "dark"                           # "dark" or "light"
timestamp_display = "local"              # "local" or "utc"
correlation_window_seconds = 30
filter_debounce_ms = 300

[export]
large_export_warning_threshold = 100000

[profiles]
user_profile_directory = ""              # Empty = platform default

[logging]
level = "info"                           # "error", "warn", "info", "debug", "trace"
file = ""                                # Empty = stderr only
```

Configuration is validated at startup. Invalid values produce actionable error messages and fall back to defaults. Unknown keys are warned and ignored (forward compatibility).

---

## 7. Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| Ctrl+O | Open directory (file dialog) |
| Ctrl+F | Focus text search filter |
| Ctrl+Shift+F | Focus regex search filter |
| Ctrl+E | Toggle export dialog |
| Ctrl+R | Rescan current directory |
| F5 | Rescan current directory |
| Ctrl+1 | Quick filter: Errors only |
| Ctrl+2 | Quick filter: Errors + Warnings |
| Escape | Clear all filters |
| Ctrl+S | Open scan summary |
| Up/Down | Navigate entries |
| Enter | Open selected entry in detail pane |

---

## 8. CLI Interface

```
LogSleuth [OPTIONS] [PATH]

Arguments:
  [PATH]  Directory to scan (opens file dialog if omitted)

Options:
  -p, --profile-dir <DIR>   Additional profile directory
  -f, --filter-level <LVL>  Initial severity filter (error, warning, info, debug)
  -d, --debug               Enable debug logging
  -v, --version             Print version
  -h, --help                Print help
```

---

## 9. Dependencies (Planned)

| Crate | Purpose | Justification |
|-------|---------|--------------|
| `eframe`/`egui` | GUI framework | Cross-platform, immediate mode, proven in EventSleuth |
| `regex` | Log line parsing | Industry-standard Rust regex engine |
| `chrono` | Timestamp parsing and normalisation | Full timezone and format support |
| `toml` | Profile and config parsing | Native TOML support |
| `serde`/`serde_json` | Serialisation (JSON export, config) | De facto standard |
| `csv` | CSV export | Robust CSV writing |
| `glob` | File pattern matching | Standard glob semantics |
| `walkdir` | Recursive directory traversal | Efficient, cross-platform |
| `memmap2` | Memory-mapped file I/O for large files | Zero-copy large file reading |
| `rayon` | Parallel parsing work pool | Work-stealing thread pool |
| `tracing`/`tracing-subscriber` | Structured logging with debug mode | Levelled, structured, filterable |
| `rfd` | Native file dialogs | Cross-platform directory picker |
| `directories` | Platform config/data paths | XDG/AppData/Library resolution |
| `image` | Application icon | Icon embedding |

---

## 10. Release & Distribution

### 10.1 Platforms

| Platform | Artifact | Installer |
|----------|----------|-----------|
| Windows | `LogSleuth-Setup-x.y.z.exe` | NSIS installer (Start Menu, uninstaller) |
| macOS | `LogSleuth-x.y.z.dmg` | DMG with .app bundle |
| Linux | `LogSleuth-x.y.z.AppImage` | AppImage (portable) |

### 10.2 CI/CD

GitHub Actions workflow triggered by version tag push:
1. Build release binaries on all three platforms
2. Run full test suite on all three platforms
3. Build platform installers
4. Create GitHub Release with artifacts and release notes

### 10.3 Automated Release Script

`update-application.ps1` (Windows) / `update-application.sh` (Unix) following the DevWorkflow Part A Rule 18 contract.

---

## 11. Future Considerations (Post v1.0)

- Compressed log support (`.gz`, `.zip`)
- Real-time log tailing with configurable poll interval
- Remote log sources (SSH, SMB)
- Bookmarks and annotations on log entries
- Saved filter presets
- Log diff: compare two time windows or two scan sessions
- Plugin system for custom analysers
- Additional built-in profiles: SCCM/CMTrace, Exchange, Apache, nginx, PostgreSQL, SQL Server
