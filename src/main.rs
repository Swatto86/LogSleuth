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

/// Configure fonts for the egui context.
///
/// On Windows, loads Segoe UI, Segoe UI Emoji, and Segoe UI Symbol from the
/// system font directory and sets them as the primary proportional fonts.
/// These fonts have much broader Unicode coverage than the egui built-ins,
/// preventing square-glyph rendering for arrows, box-drawing, and other symbols.
/// The built-in egui fonts are kept as final fallbacks so no glyph is ever lost.
///
/// On non-Windows platforms the egui defaults are used unchanged.
fn configure_fonts(ctx: &egui::Context) {
    #[cfg(target_os = "windows")]
    {
        let mut fonts = egui::FontDefinitions::default();

        // Load Windows system fonts in priority order.
        // Segoe UI covers most Latin and common UI symbols.
        // Segoe UI Emoji adds Unicode emoji and many pictographic symbols.
        // Segoe UI Symbol covers Mathematical, Braille, and other specialist blocks.
        let candidates: &[(&str, &str)] = &[
            ("Segoe UI", r"C:\Windows\Fonts\segoeui.ttf"),
            ("Segoe UI Emoji", r"C:\Windows\Fonts\seguiemj.ttf"),
            ("Segoe UI Symbol", r"C:\Windows\Fonts\seguisym.ttf"),
        ];

        let mut loaded_names: Vec<&str> = Vec::new();
        for (name, path) in candidates {
            match std::fs::read(path) {
                Ok(data) => {
                    fonts
                        .font_data
                        .insert((*name).to_owned(), egui::FontData::from_owned(data).into());
                    loaded_names.push(name);
                    tracing::debug!(font = name, "Loaded Windows system font");
                }
                Err(e) => {
                    tracing::warn!(
                        font = name,
                        error = %e,
                        "Failed to load Windows system font; some symbols may render as squares"
                    );
                }
            }
        }

        if !loaded_names.is_empty() {
            // Proportional: place Windows fonts first so they take priority over
            // the egui default (NotoSans), while keeping it as a final fallback.
            if let Some(proportional) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
                for (i, name) in loaded_names.iter().enumerate() {
                    proportional.insert(i, (*name).to_owned());
                }
            }

            // Monospace: append Windows fonts as symbol fallbacks after the primary
            // monospace font (Hack) so log-line column alignment is preserved while
            // Unicode symbols outside the monospace range still render correctly.
            if let Some(monospace) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
                for name in &loaded_names {
                    monospace.push((*name).to_owned());
                }
            }

            ctx.set_fonts(fonts);
            tracing::info!(fonts = ?loaded_names, "Windows system fonts configured");
        }
    }

    // On non-Windows platforms the egui built-in fonts are used unchanged.
    #[cfg(not(target_os = "windows"))]
    let _ = ctx;
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
    let (profiles, profile_errors) = app::profile_mgr::load_all_profiles(Some(user_profile_dir));

    if !profile_errors.is_empty() {
        for err in &profile_errors {
            tracing::warn!(error = %err, "Profile loading warning");
        }
    }

    tracing::info!(profiles = profiles.len(), "Ready to launch GUI");

    // Create application state
    let mut state = app::state::AppState::new(profiles, cli.debug);

    // If a path was provided on the CLI, set it as the scan target
    if let Some(ref path) = cli.path {
        state.scan_path = Some(path.clone());
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
            configure_fonts(&cc.egui_ctx);
            Ok(Box::new(gui::LogSleuthApp::new(state)))
        }),
    );

    if let Err(e) = result {
        tracing::error!(error = %e, "Failed to launch GUI");
        eprintln!("Error: Failed to launch LogSleuth GUI: {e}");
        std::process::exit(1);
    }
}
