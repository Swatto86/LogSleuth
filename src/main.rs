// LogSleuth - main.rs
//
// Application entry point. Handles:
// 1. CLI argument parsing
// 2. Logging initialisation (debug mode support)
// 3. Format profile loading (built-in + user-defined)
// 4. eframe GUI launch

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod gui;

// Re-export modules used across the crate.
// This structure mirrors the Atlas layer diagram.
pub mod app;
pub mod core;
pub mod platform;
pub mod ui;
pub mod util;

use clap::Parser;
use std::path::PathBuf;

/// LogSleuth - Cross-platform log file viewer and analyser.
///
/// Point LogSleuth at a directory to discover, parse, and analyse log files
/// from multiple products in a unified, filterable timeline.
#[derive(Parser, Debug)]
#[command(name = "LogSleuth", version, about)]
struct Cli {
    /// Directory to scan (opens file dialog if omitted).
    path: Option<PathBuf>,

    /// Additional directory containing user-defined format profiles.
    #[arg(short = 'p', long = "profile-dir")]
    profile_dir: Option<PathBuf>,

    /// Initial severity filter level.
    #[arg(short = 'f', long = "filter-level")]
    filter_level: Option<String>,

    /// Enable debug logging (equivalent to RUST_LOG=debug).
    #[arg(short = 'd', long = "debug")]
    debug: bool,
}

fn main() {
    let cli = Cli::parse();

    // Initialise logging subsystem
    util::logging::init(cli.debug, None, None);

    tracing::info!(
        version = util::constants::APP_VERSION,
        debug = cli.debug,
        "LogSleuth starting"
    );

    // Resolve platform paths
    let platform_paths = platform::config::PlatformPaths::resolve();

    // Determine profile directory: CLI override > platform default
    let user_profile_dir = cli
        .profile_dir
        .as_deref()
        .unwrap_or(&platform_paths.user_profiles_dir);

    // Load format profiles
    let (profiles, profile_errors) =
        app::profile_mgr::load_all_profiles(Some(user_profile_dir));

    if !profile_errors.is_empty() {
        for err in &profile_errors {
            tracing::warn!(error = %err, "Profile loading warning");
        }
    }

    tracing::info!(
        profiles = profiles.len(),
        "Ready to launch GUI"
    );

    // Create application state
    let mut state = app::state::AppState::new(profiles, cli.debug);

    // If a path was provided on the CLI, set it as the scan target
    if let Some(ref path) = cli.path {
        state.scan_path = Some(path.clone());
    }

    // Launch the GUI
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(format!(
                "{} v{}",
                util::constants::APP_NAME,
                util::constants::APP_VERSION
            ))
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([800.0, 500.0]),
        ..Default::default()
    };

    let result = eframe::run_native(
        util::constants::APP_NAME,
        native_options,
        Box::new(move |_cc| {
            Ok(Box::new(gui::LogSleuthApp::new(state)))
        }),
    );

    if let Err(e) = result {
        tracing::error!(error = %e, "Failed to launch GUI");
        eprintln!("Error: Failed to launch LogSleuth GUI: {e}");
        std::process::exit(1);
    }
}
