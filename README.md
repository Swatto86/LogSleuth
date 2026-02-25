# LogSleuth

A fast, cross-platform log file viewer and analyser built with Rust and egui. Part of the Sleuth Tools collection alongside [EventSleuth](https://github.com/swatto86/EventSleuth) and DiskSleuth.

## What It Does

Point LogSleuth at a directory and it will:

1. **Discover** all log files recursively, regardless of vendor or format
2. **Auto-detect** the log format using extensible TOML-based profiles
3. **Parse** entries into a normalised model (timestamp, severity, message, source)
4. **Display** everything in a unified, colour-coded, virtual-scrolling timeline
5. **Filter** by severity, text, regex, time range, and source file
6. **Export** filtered results to CSV or JSON
7. **Summarise** each scan with a per-file breakdown (entries, errors, time range)

## Filtering

The filter sidebar provides:

| Filter | Description |
|--------|-------------|
| Severity | Checkboxes for Critical / Error / Warning / Info / Debug / Unknown |
| Text search | Case-insensitive substring match across message + metadata |
| Regex search | Full regex with live compile-error feedback |
| Relative time window | Quick-select **15 min / 1 h / 6 h / 24 h** buttons or type a custom number of minutes; LogSleuth automatically advances the window as the clock ticks |
| Source file | Per-file checklist; when more than 8 files are loaded a live search box appears. **Select All / None** operate on the currently visible (filtered) subset |

The entry count badge in the filter panel always reflects the current filtered vs. total count.

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
