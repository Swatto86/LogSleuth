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
// Output: stderr by default. Optionally also to a file.
// Never logs secrets, tokens, or PII at any level.

use tracing_subscriber::EnvFilter;

/// Initialise the logging subsystem.
///
/// `debug_flag` is true when the user passed --debug on the CLI.
/// `config_level` is the level from config.toml (if present).
/// `log_file` is the optional log file path from config.toml.
///
/// Priority: RUST_LOG env var > CLI --debug flag > config level > default "info".
pub fn init(debug_flag: bool, config_level: Option<&str>, _log_file: Option<&str>) {
    // Build the env filter with the correct priority
    let filter = if std::env::var("RUST_LOG").is_ok() {
        // RUST_LOG takes highest priority (already set)
        EnvFilter::from_default_env()
    } else if debug_flag {
        EnvFilter::new("debug")
    } else if let Some(level) = config_level {
        EnvFilter::new(level)
    } else {
        EnvFilter::new(super::constants::DEFAULT_LOG_LEVEL)
    };

    // TODO: When log_file support is added, use a layer that writes
    // to both stderr and the file simultaneously.
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .compact()
        .init();

    tracing::debug!(
        app = super::constants::APP_NAME,
        version = super::constants::APP_VERSION,
        "Logging initialised"
    );
}
