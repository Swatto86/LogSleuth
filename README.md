# LogSleuth

A fast, cross-platform log file viewer and analyser built with Rust and egui. Part of the Sleuth Tools collection alongside [EventSleuth](https://github.com/swatto86/EventSleuth) and [DiskSleuth](https://github.com/swatto86/DiskSleuth).

## Download

Grab the latest release from the [Releases page](https://github.com/swatto86/LogSleuth/releases):

| File | Description |
|------|-------------|
| `LogSleuth-Setup-{version}.exe` | Windows installer — Start Menu shortcut, per-user or per-machine |
| `LogSleuth-{version}-windows-portable.exe` | Windows portable — single EXE, no installation required, no runtime dependencies |
| `LogSleuth-{version}.dmg` | macOS disk image |
| `LogSleuth-{version}.AppImage` | Linux AppImage (no installation required) |

> The portable Windows EXE has the MSVC CRT statically linked and runs on any Windows 10/11 machine without the Visual C++ Redistributable.

## What It Does

Point LogSleuth at a directory and it will:

1. **Discover** all log files recursively, regardless of vendor or format
2. **Auto-detect** the log format using extensible TOML-based profiles
3. **Parse** entries into a normalised model (timestamp, severity, message, source)
4. **Display** everything in a unified, colour-coded, virtual-scrolling timeline with per-file colour stripes
5. **Filter** by severity, text (exact or fuzzy), regex, time range, and source file
6. **Export** filtered results to CSV or JSON
7. **Summarise** each scan with a per-file breakdown (entries, errors, time range)
8. **Merge** multiple files or directories into one chronological timeline (CMTrace-style)
9. **Live tail** newly written log lines in real time as you reproduce an issue

## Filtering

The filter sidebar provides:

| Filter | Description |
|--------|-------------|
| Severity | Checkboxes for Critical / Error / Warning / Info / Debug / Unknown |
| Text search | Case-insensitive substring match across message + metadata |
| Fuzzy search | Toggle the **~** button next to the text input to enable fuzzy (subsequence) matching — e.g. `vcancl` matches `VssCancelAll` |
| Regex search | Full regex with live compile-error feedback |
| Relative time window | Quick-select **15 min / 1 h / 6 h / 24 h** buttons or type a custom number of minutes; LogSleuth automatically advances the window as the clock ticks |
| Source file | Per-file checklist with a coloured dot matching the file's timeline stripe. When more than 8 files are loaded a live search box appears. **Select All / None** operate on the currently visible (filtered) subset. **Solo** instantly isolates a single file. |

### Quick-Filter Presets

| Button | What it sets |
|--------|-------------|
| **Errors only** | Severity: Critical + Error |
| **Errors + Warn** | Severity: Critical + Error + Warning |
| **Err+Warn+15m** | Severity: Critical + Error + Warning, plus a 15-minute rolling time window. Ideal for immediate troubleshooting: shows only recent error-level activity. When Live Tail is running the window advances automatically so new entries flow in and old ones drop off. |
| **Clear** | Resets all filters |

### Copying Filtered Results

A **📋 Copy** button sits next to the entry-count footer at the bottom of the filter sidebar (also accessible via **View → Copy Filtered Results**). It copies all currently-filtered entries to the clipboard as a plain-text report including a filter summary header, timestamp, severity, source filename, and message for each entry. The copy is bounded at 10,000 entries; a truncation notice is appended if the limit is reached.

The entry count badge in the filter panel always reflects the current filtered vs. total count.

## Live Tail

After a scan completes, click **● Live Tail** in the sidebar to watch all loaded files for new content in real time. This is ideal for reproducing a product issue: scan the log directory first to establish baseline state, then activate Live Tail and re-trigger the problem to see the relevant log lines appear as they are written.

- Only lines written *after* Live Tail is activated are shown (the tool does not re-replay existing content).
- A green **● LIVE** badge appears in the status bar while tail is active.
- **↓ Auto** toggle (next to the stop button) pins the timeline to the bottom so new entries scroll into view automatically. Turn it off to scroll back through history, then back on to re-pin.
- File rotation and truncation are handled automatically: if a file is replaced or cleared, the offset resets to the beginning of the new file.
- Click **■ Stop Tail** to stop watching. The captured entries remain in the timeline for filtering and export.
- When a relative-time window is active during Live Tail, a green **● Rolling window (live)** indicator appears under the time-range control to confirm the window is continuously advancing.
- **Live Tail respects your source-file filter.** If you have narrowed the file list in the sidebar, only those selected files are watched. Selecting "None" stops all watching; the status bar shows how many files are actually being tailed.

> **Tip**: Click **Err+Warn+15m**, then **● Live Tail** to instantly monitor only recent errors and warnings across all loaded files in real time.

> **Note**: Live tail decodes new bytes as UTF-8. UTF-16 log files (rare Windows system logs) are not supported for incremental tail; load them via a normal scan instead.

## Bookmarks

Star any timeline entry with the **★/☆** button on the left of every row to bookmark it:

- Bookmarked rows are highlighted with a gold background tint.
- The **★ Bookmarks (N)** toggle in the filter sidebar shows only bookmarked entries.
- Use **× clear bm** to remove all bookmarks.
- Use **View → Copy Bookmark Report** to export all bookmarked entries to the clipboard as a structured report showing timestamp, severity, source file, and message for each bookmarked entry.

## Time Correlation

Select any timeline entry and enable the **◆ Correlation** overlay in the filter sidebar to highlight all entries across all loaded files whose timestamps fall within a configurable window (default ±30 seconds) of the selected entry:

- Correlated entries are highlighted with a teal background tint.
- The window size is configurable in the **Window: [ ] sec** input (1–3600 seconds).
- The overlay searches all entries, including those hidden by the current filter, so contextual events are never silently excluded.
- Useful for correlating failures across multiple components — e.g. select an application error and instantly see what was happening concurrently in the web server, database, and service logs.

## Session Persistence

LogSleuth automatically saves your session when the application closes and restores it at the next launch:

- **What is saved**: scan path, all active filter settings, per-file colour assignments, bookmarks, and the correlation window size.
- **What is not saved**: parsed log entries (files are always re-parsed on restore to reflect current content).
- Session data is stored in the platform data directory:
  - **Windows**: `%APPDATA%\LogSleuth\session.json`
  - **Linux**: `~/.local/share/logsleuth/session.json`
  - **macOS**: `~/Library/Application Support/LogSleuth/session.json`
- A corrupt or missing session file is silently ignored; the application starts fresh.

## Multi-File Merged Timeline

Use **File > Add File(s)…** to append individual log files to the current session without clearing existing entries. All files — whether from an initial directory scan or added one-by-one — are merged into a single chronological timeline.

Each source file is assigned a unique colour from a 24-entry palette:
- A **4 px coloured stripe** on the left edge of every timeline row identifies which file the entry came from.
- A matching **coloured dot** appears next to each file name in the discovery panel and the source-file filter list.
- The **Solo** button in the filter list isolates one file instantly.

> Entries are sorted chronologically on the background scan thread (not the UI thread), so opening hundreds of files does not freeze the interface.

## Detail Pane

Selecting any timeline entry shows it in the detail pane at the bottom. From there you can:
- **Copy** the full message to the clipboard.
- **Show in Folder** — opens the OS file manager with the source log file pre-selected.

## Exporting Results

Use **File > Export > CSV** or **File > Export > JSON** to save the currently filtered entry set. A native save dialog is presented. Files are written atomically (write to temp, then rename) to prevent partial output.

## About

Click the **ⓘ** icon in the top-right corner of the menu bar to open the About dialog, which shows the application version, a link to the GitHub repository, and licence information.

## Scan Summary

Use **File > Scan Summary** (or `Ctrl+S`) after a scan to see:
- Total entries, errors, and scan duration
- Per-file table: profile detected, entry count, error count, earliest and latest timestamps

## Cancel a Scan

A **Cancel** button appears in the status bar during an active scan. Cancellation is cooperative and completes any in-flight file cleanly.

## Built-in Format Profiles

| Profile | Product / Format |
|---------|------------------|
| Veeam VBR | Veeam Backup & Replication service and job logs (`Svc.*.log`, `Job.*.log`) |
| Veeam VBO365 | Veeam Backup for Microsoft 365 (`Veeam.Archiver.*.log`) |
| IIS W3C | Microsoft IIS web server W3C Extended format (`u_ex*.log`) |
| SQL Server Error Log | Microsoft SQL Server `ERRORLOG` / `ERRORLOG.N` |
| SQL Server Agent Log | SQL Server Agent `SQLAGENT.OUT` |
| Apache / nginx Combined Access | Apache httpd and nginx Combined Log Format (`access.log`, `access_log`) |
| nginx Error Log | nginx web server error log (`error.log`) |
| Windows DHCP Server Log | Windows Server DHCP daily activity logs (`DhcpSrvLog-*.log`) |
| Syslog (RFC 3164) | BSD syslog (rsyslog, syslog-ng) |
| Syslog (RFC 5424) | IETF structured syslog |
| JSON Lines | Newline-delimited JSON logs |
| Log4j / Logback | Standard Java logging output |
| Generic Timestamp | Fallback for ISO-timestamp + message |
| Plain Text | Fallback for unrecognised formats (full-text search only) |

## Custom Profiles

Create a `.toml` file in your profiles directory:

- **Windows**: `%APPDATA%\LogSleuth\profiles\`
- **Linux**: `~/.config/logsleuth/profiles/`
- **macOS**: `~/Library/Application Support/LogSleuth/profiles/`

See the [profiles/](profiles/) directory for examples.

## Building

```bash
# Debug build
cargo build

# Release build
cargo build --release

# Run
cargo run --release -- /path/to/logs

# Run with debug logging
cargo run --release -- --debug /path/to/logs
```

### Requirements

- Rust 1.75+ (install via [rustup](https://rustup.rs/))

## Usage

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

## Debug Mode

Activate with any of:
- Environment variable: `RUST_LOG=debug`
- CLI flag: `--debug`
- Config file: `[logging] level = "debug"`

Output goes to stderr. Never logs secrets, tokens, or PII.

## Project Structure

See [ATLAS.md](ATLAS.md) for the complete Project Atlas including architecture, module responsibilities, and invariants.

## Licence

MIT
