// LogSleuth - main.rs
//
// Application entry point. Handles:
// 1. CLI argument parsing
// 2. Logging initialisation (debug mode support)
// 3. Format profile loading (built-in + user-defined)
// 4. eframe GUI launch

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod gui;

// Re-export modules from the library crate so that `gui.rs` and other
// binary-side code can still use `crate::app::...`, `crate::core::...` etc.
pub use logsleuth::app;

pub use logsleuth::core;
pub use logsleuth::platform;
pub use logsleuth::ui;
pub use logsleuth::util;

use clap::Parser;
use std::path::PathBuf;

/// Compile-time-embedded icon PNG bytes (512x512 RGBA).
///
/// Using `include_bytes!` ensures the asset is baked into the binary so the
/// icon is always available regardless of the working directory at runtime.
static ICON_PNG: &[u8] = include_bytes!("../assets/icon.png");

/// Decode the embedded PNG and return an `eframe`-compatible `IconData`.
///
/// Falls back to a transparent 1x1 placeholder if decoding fails so the
/// application always launches rather than panicking on a missing asset.
fn load_icon() -> egui::IconData {
    use image::ImageDecoder;

    match image::codecs::png::PngDecoder::new(std::io::Cursor::new(ICON_PNG)) {
        Ok(decoder) => {
            let (w, h) = decoder.dimensions();
            // Convert to RGBA8 regardless of the source colour space.
            match image::DynamicImage::from_decoder(decoder) {
                Ok(img) => {
                    let rgba = img.into_rgba8();
                    egui::IconData {
                        rgba: rgba.into_raw(),
                        width: w,
                        height: h,
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to decode icon PNG; using placeholder");
                    placeholder_icon()
                }
            }
        }
        Err(e) => {
            tracing::warn!(error = %e, "Failed to open icon PNG decoder; using placeholder");
            placeholder_icon()
        }
    }
}

/// 1x1 transparent RGBA icon used when the real icon cannot be loaded.
fn placeholder_icon() -> egui::IconData {
    egui::IconData {
        rgba: vec![0u8; 4],
        width: 1,
        height: 1,
    }
}

/// Pre-load system font definitions **before** `eframe::run_native` is called.
///
/// Doing all file I/O here (rather than inside the creator closure) satisfies
/// DevWorkflow Rule 16: "All expensive initialisation MUST complete *before*
/// calling `eframe::run_native()`" so the OS window never shows a white
/// background while fonts are being read from disk.
///
/// On Windows, loads:
///   - Consolas as the primary monospace font (excellent log-file readability,
///     ships with every Windows install since Vista).
///   - Segoe UI as the primary proportional font (native Windows UI feel).
///   - Segoe UI Symbol and Segoe UI Emoji as Unicode-coverage fallbacks for
///     both families, preventing square-glyph rendering for box-drawing,
///     arrows, mathematical, and emoji code points.
///
/// The built-in egui fonts (Hack, NotoSans) are retained as final fallbacks
/// so no glyph is ever permanently lost regardless of what is installed.
///
/// On non-Windows platforms the egui defaults are returned unchanged.
fn build_font_definitions() -> egui::FontDefinitions {
    #[cfg(target_os = "windows")]
    {
        let mut fonts = egui::FontDefinitions::default();

        // Candidate fonts: (family-tag, path-on-disk).
        // Loaded in order; missing files are skipped with a warning.
        let candidates: &[(&str, &str)] = &[
            ("Segoe UI", r"C:\Windows\Fonts\segoeui.ttf"),
            ("Segoe UI Symbol", r"C:\Windows\Fonts\seguisym.ttf"),
            ("Segoe UI Emoji", r"C:\Windows\Fonts\seguiemj.ttf"),
            ("Consolas", r"C:\Windows\Fonts\consola.ttf"),
        ];

        let mut loaded: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for (name, path) in candidates {
            match std::fs::read(path) {
                Ok(data) => {
                    fonts
                        .font_data
                        .insert((*name).to_owned(), egui::FontData::from_owned(data).into());
                    loaded.insert(name);
                    tracing::debug!(font = name, "Loaded Windows system font");
                }
                Err(e) => {
                    tracing::warn!(
                        font = name,
                        error = %e,
                        "Failed to load Windows system font; some glyphs may render as squares"
                    );
                }
            }
        }

        // ----------------------------------------------------------------
        // Proportional family
        // Place Windows fonts at the front so Segoe UI is chosen for
        // regular UI text, then keep the egui built-ins as last resorts.
        // ----------------------------------------------------------------
        if let Some(proportional) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
            let mut insert_at = 0usize;
            for name in &["Segoe UI", "Segoe UI Symbol", "Segoe UI Emoji"] {
                if loaded.contains(name) {
                    proportional.insert(insert_at, (*name).to_owned());
                    insert_at += 1;
                }
            }
        }

        // ----------------------------------------------------------------
        // Monospace family
        // Segoe UI first — consistent look across the whole application.
        // Segoe UI Symbol second — Unicode box-drawing, arrows, etc.
        // Segoe UI Emoji third — catchall emoji/symbol fallback.
        // Consolas fourth — ASCII-domain monospace when explicitly requested
        //   by code that calls FontFamily::Monospace for column-aligned text.
        // The egui built-in (Hack) and NotoSans are kept at the end.
        // ----------------------------------------------------------------
        if let Some(monospace) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
            let mut insert_at = 0usize;
            for name in &["Segoe UI", "Segoe UI Symbol", "Segoe UI Emoji", "Consolas"] {
                if loaded.contains(name) {
                    monospace.insert(insert_at, (*name).to_owned());
                    insert_at += 1;
                }
            }
        }

        tracing::info!(
            loaded = ?loaded.iter().collect::<Vec<_>>(),
            "Windows system fonts pre-loaded"
        );

        fonts
    }

    // On non-Windows platforms the egui built-in fonts are used unchanged.
    #[cfg(not(target_os = "windows"))]
    egui::FontDefinitions::default()
}

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

    // Resolve platform paths first so config.toml can be loaded before
    // logging is initialised (the config may specify a log level).
    let platform_paths = platform::config::PlatformPaths::resolve();

    // Load and validate config.toml (Rule 13: external configuration,
    // validated at startup, misconfiguration warns and falls back to defaults).
    // Loaded before logging init so the [logging] level/file settings take effect.
    let (app_config, config_warnings) = platform::config::load_config(&platform_paths.config_dir);

    // Initialise logging subsystem with priority: RUST_LOG > CLI > config > default.
    util::logging::init(
        cli.debug,
        app_config.log_level.as_deref(),
        app_config.log_file.as_deref(),
    );

    tracing::info!(
        version = util::constants::APP_VERSION,
        debug = cli.debug,
        "LogSleuth starting"
    );

    // Report any config validation warnings now that logging is ready.
    for warning in &config_warnings {
        tracing::warn!("{}", warning);
    }

    // Determine profile directory: CLI override > platform default
    let user_profile_dir = cli
        .profile_dir
        .as_deref()
        .unwrap_or(&platform_paths.user_profiles_dir);

    // Load format profiles
    let (profiles, profile_errors) = app::profile_mgr::load_all_profiles(Some(user_profile_dir));

    if !profile_errors.is_empty() {
        for err in &profile_errors {
            tracing::warn!(error = %err, "Profile loading warning");
        }
    }

    tracing::info!(profiles = profiles.len(), "Ready to launch GUI");

    // Create application state
    let mut state = app::state::AppState::new(profiles, cli.debug);

    // Apply config.toml values where they override defaults (Rule 13).
    state.max_files_limit = app_config.max_files;
    state.max_scan_depth = app_config.max_depth;
    state.dark_mode = app_config.dark_mode;
    state.correlation_window_secs = app_config.correlation_window_secs;
    state.correlation_window_input = app_config.correlation_window_secs.to_string();
    state.ui_font_size = app_config.font_size;

    // Expose the external profiles directory to the UI so Options can show it
    // and trigger reloads without a restart.
    state.user_profiles_dir = Some(user_profile_dir.to_path_buf());

    // Create the profiles directory on first launch so users can immediately
    // find it after opening Options > External Profiles > Open Folder.
    if let Err(e) = std::fs::create_dir_all(user_profile_dir) {
        tracing::warn!(
            dir = %user_profile_dir.display(),
            error = %e,
            "Could not create user profiles directory"
        );
    }

    // Set the persistent session file path so save/restore can locate it.
    let session_file = app::session::session_path(&platform_paths.data_dir);
    state.session_path = Some(session_file.clone());

    // Try restoring the previous session.  All errors are silently ignored:
    // a missing or corrupt file simply starts the app in a clean state.
    if let Some(session) = app::session::load(&session_file) {
        tracing::info!(path = %session_file.display(), "Restoring previous session");
        let has_scan = session.scan_path.is_some();
        state.restore_from_session(session);
        // Queue the re-scan via initial_scan (not pending_scan) so the
        // restored filter/colour/bookmark state is NOT cleared before parsing.
        if has_scan {
            state.initial_scan = state.scan_path.clone();
            state.status_message = "Rescanning previous session\u{2026}".to_string();
        }
    }

    // A path supplied on the CLI always overrides the session scan path.
    if let Some(ref path) = cli.path {
        state.scan_path = Some(path.clone());
        state.initial_scan = Some(path.clone());
    }

    // A severity level supplied on the CLI overrides the session-restored filter.
    // `--filter-level warning` shows Warning + Error + Critical (i.e. the given
    // level and all more-severe levels).  Matching is case-insensitive.
    if let Some(ref level_str) = cli.filter_level {
        use crate::core::model::Severity;
        let level_lower = level_str.to_lowercase();
        let matched = Severity::all()
            .iter()
            .copied()
            .find(|s| s.label().to_lowercase() == level_lower);
        if let Some(sev) = matched {
            // Collect all variants that are at least as severe as the requested level.
            // Severity derives Ord; smaller discriminant == more severe (Critical < Error < …).
            state.filter_state.severity_levels = Severity::all()
                .iter()
                .copied()
                .filter(|s| *s <= sev)
                .collect();
            tracing::info!(level = %level_str, "Applied CLI --filter-level");
        } else {
            tracing::warn!(
                level = %level_str,
                "Unknown --filter-level value, ignoring. \
                 Valid values: critical, error, warning, info, debug"
            );
        }
    }

    // Launch the GUI
    //
    // The icon is applied at two levels:
    //   1. OS-level (Windows EXE resource) — embedded by build.rs via winres.
    //      This covers the taskbar, Alt+Tab, title bar, and Explorer.
    //   2. Runtime (eframe viewport) — loaded here from the PNG asset.
    //      This covers the eframe-managed window icon on all platforms and
    //      acts as the canonical source on Linux/macOS.
    let icon_data = load_icon();

    // Pre-load font definitions before opening the OS window (Rule 16: no I/O
    // inside the creator closure passed to run_native).
    let font_defs = build_font_definitions();

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(format!(
                "{} v{}",
                util::constants::APP_NAME,
                util::constants::APP_VERSION
            ))
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([800.0, 500.0])
            .with_icon(icon_data),
        ..Default::default()
    };

    let result = eframe::run_native(
        util::constants::APP_NAME,
        native_options,
        Box::new(move |cc| {
            // Apply pre-loaded font definitions (file I/O completed before run_native).
            cc.egui_ctx.set_fonts(font_defs);
            Ok(Box::new(gui::LogSleuthApp::new(state)))
        }),
    );

    if let Err(e) = result {
        tracing::error!(error = %e, "Failed to launch GUI");
        eprintln!("Error: Failed to launch LogSleuth GUI: {e}");
        std::process::exit(1);
    }
}
