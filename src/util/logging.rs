// LogSleuth - util/logging.rs
//
// Structured logging with runtime-selectable debug mode.
// DevWorkflow Part A Rule 10: debug mode, structured timestamps,
// accessible channel, zero overhead when disabled.
//
// Activation:
//   - Environment variable: RUST_LOG=debug (or trace)
//   - CLI flag: --debug (sets RUST_LOG=debug)
//   - Config file: [logging] level = "debug"
//
// Output: stderr by default. When [logging] file is set in config.toml,
// also writes to that file simultaneously (fail-open: file errors fall back
// to stderr-only with a warning printed to stderr before logging starts).
//
// Never logs secrets, tokens, or PII at any level.

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Build the `EnvFilter` according to the priority hierarchy:
/// `RUST_LOG` env var > `--debug` CLI flag > config file level > default "info".
fn build_filter(debug_flag: bool, config_level: Option<&str>) -> EnvFilter {
    if std::env::var("RUST_LOG").is_ok() {
        // RUST_LOG takes highest priority (already set in the environment).
        EnvFilter::from_default_env()
    } else if debug_flag {
        EnvFilter::new("debug")
    } else if let Some(level) = config_level {
        EnvFilter::new(level)
    } else {
        EnvFilter::new(super::constants::DEFAULT_LOG_LEVEL)
    }
}

/// Initialise the logging subsystem.
///
/// `debug_flag` is true when the user passed --debug on the CLI.
/// `config_level` is the level string from config.toml (if present).
/// `log_file` is the optional log file path from config.toml.  When
/// `Some`, a second subscriber layer is added that appends structured
/// log output to the specified file in addition to stderr.  If the file
/// cannot be opened the function falls back to stderr-only and prints a
/// warning to stderr (fail-open; Rule 11).
///
/// Priority: RUST_LOG env var > CLI --debug flag > config level > default "info".
pub fn init(debug_flag: bool, config_level: Option<&str>, log_file: Option<&str>) {
    let filter = build_filter(debug_flag, config_level);

    // Attempt to open the optional log file for appending.
    // Fail-open: errors are printed to stderr and logging continues
    // without a file layer rather than crashing (Rule 11).
    let file_writer: Option<std::sync::Mutex<std::fs::File>> = log_file.and_then(|path| {
        match std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            Ok(f) => Some(std::sync::Mutex::new(f)),
            Err(e) => {
                eprintln!("[WARN] Cannot open log file '{path}': {e}; logging to stderr only");
                None
            }
        }
    });

    // Stderr layer is always active.
    let stderr_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_target(true)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .compact();

    // File layer is added only when a file was successfully opened.
    // Option<L> implements Layer<S> in tracing-subscriber, so this
    // composes cleanly with the registry without dynamic dispatch.
    let file_layer = file_writer.map(|w| {
        tracing_subscriber::fmt::layer()
            .with_writer(w)
            .with_target(true)
            .with_thread_ids(true)
            .with_file(true)
            .with_line_number(true)
            .compact()
    });

    tracing_subscriber::registry()
        .with(filter)
        .with(stderr_layer)
        .with(file_layer)
        .init();

    tracing::debug!(
        app = super::constants::APP_NAME,
        version = super::constants::APP_VERSION,
        "Logging initialised"
    );
}
