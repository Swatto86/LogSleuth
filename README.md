# LogSleuth

A fast, cross-platform log file viewer and analyser built with Rust and egui. Part of the Sleuth Tools collection alongside [EventSleuth](https://github.com/swatto86/EventSleuth) and DiskSleuth.

## What It Does

Point LogSleuth at a directory and it will:

1. **Discover** all log files recursively, regardless of vendor or format
2. **Auto-detect** the log format using extensible TOML-based profiles
3. **Parse** entries into a normalised model (timestamp, severity, message, source)
4. **Display** everything in a unified, colour-coded, filterable timeline
5. **Correlate** events across multiple log files by timestamp

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
