# LogSleuth

A fast, cross-platform log file viewer and analyser built with Rust and egui. Part of the Sleuth Tools collection alongside [EventSleuth](https://github.com/swatto86/EventSleuth) and DiskSleuth.

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

The entry count badge in the filter panel always reflects the current filtered vs. total count.

## Live Tail

After a scan completes, click **● Live Tail** in the sidebar to watch all loaded files for new content in real time. This is ideal for reproducing a product issue: scan the log directory first to establish baseline state, then activate Live Tail and re-trigger the problem to see the relevant log lines appear as they are written.

- Only lines written *after* Live Tail is activated are shown (the tool does not re-replay existing content).
- A green **● LIVE** badge appears in the status bar while tail is active.
- **↓ Auto** toggle (next to the stop button) pins the timeline to the bottom so new entries scroll into view automatically. Turn it off to scroll back through history, then back on to re-pin.
- File rotation and truncation are handled automatically: if a file is replaced or cleared, the offset resets to the beginning of the new file.
- Click **■ Stop Tail** to stop watching. The captured entries remain in the timeline for filtering and export.

> **Note**: Live tail decodes new bytes as UTF-8. UTF-16 log files (rare Windows system logs) are not supported for incremental tail; load them via a normal scan instead.

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

## Scan Summary

Use **File > Scan Summary** (or `Ctrl+S`) after a scan to see:
- Total entries, errors, and scan duration
- Per-file table: profile detected, entry count, error count, earliest and latest timestamps

## Cancel a Scan

A **Cancel** button appears in the status bar during an active scan. Cancellation is cooperative and completes any in-flight file cleanly.

## Built-in Format Profiles

| Profile | Product |
|---------|---------|
| Veeam VBR | Veeam Backup & Replication service and job logs |
| Veeam VBO365 | Veeam Backup for Microsoft 365 |
| IIS W3C | Microsoft IIS web server |
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
