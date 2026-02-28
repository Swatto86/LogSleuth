// LogSleuth - ui/panels/options.rs
//
// Options dialog: runtime-configurable application settings.
// Shown when the user opens Edit > Options... from the menu bar.
//
// Sections:
//   1. Ingest Limits  — max files per scan, max total entries, max scan depth
//   2. Live Tail      — tail poll interval
//   3. Directory Watch — watch poll interval
//
// Settings in sections 2 and 3 take effect when the *next* tail or watch
// session is started.  Ingest settings take effect on the *next* scan.
// All limits are validated against absolute bounds from util::constants to
// prevent accidental misconfiguration (Rule 13 + Rule 11 input validation).

use crate::app::state::AppState;
use crate::util::constants::{
    ABSOLUTE_MAX_DEPTH, ABSOLUTE_MAX_FILES, ABSOLUTE_MAX_TOTAL_ENTRIES, DEFAULT_FONT_SIZE,
    DEFAULT_MAX_DEPTH, DEFAULT_MAX_FILES, DIR_WATCH_POLL_INTERVAL_MS,
    MAX_DIR_WATCH_POLL_INTERVAL_MS, MAX_FONT_SIZE, MAX_TAIL_POLL_INTERVAL_MS, MAX_TOTAL_ENTRIES,
    MIN_DIR_WATCH_POLL_INTERVAL_MS, MIN_FONT_SIZE, MIN_MAX_FILES, MIN_MAX_TOTAL_ENTRIES,
    MIN_TAIL_POLL_INTERVAL_MS, TAIL_POLL_INTERVAL_MS,
};

/// Render the Options dialog (if `state.show_options` is true).
pub fn render(ctx: &egui::Context, state: &mut AppState) {
    if !state.show_options {
        return;
    }

    let mut open = true;
    egui::Window::new("Options")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(420.0)
        .show(ctx, |ui| {
            // =========================================================
            // Section 0 — Appearance
            // =========================================================
            ui.heading("Appearance");
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("Font size:");
                let mut v = state.ui_font_size as f64;
                if ui
                    .add(
                        egui::Slider::new(
                            &mut v,
                            (MIN_FONT_SIZE as f64)..=(MAX_FONT_SIZE as f64),
                        )
                        .step_by(0.5)
                        .suffix(" pt"),
                    )
                    .changed()
                {
                    state.ui_font_size = (v as f32).clamp(MIN_FONT_SIZE, MAX_FONT_SIZE);
                }
                if (state.ui_font_size - DEFAULT_FONT_SIZE).abs() > 0.1
                    && ui
                        .small_button("Reset")
                        .on_hover_text("Reset to the built-in default (14 pt)")
                        .clicked()
                {
                    state.ui_font_size = DEFAULT_FONT_SIZE;
                }
            });
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(
                    "Scales all text in the application. Takes effect immediately.",
                )
                .small()
                .weak(),
            );

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(6.0);

            // =========================================================
            // Section 1 — Ingest Limits
            // =========================================================
            ui.heading("Ingest Limits");
            ui.add_space(2.0);
            ui.label(
                egui::RichText::new(
                    "Controls how much data is loaded when scanning a directory. \
                     Changes take effect on the next scan.",
                )
                .small()
                .weak(),
            );
            ui.add_space(8.0);

            // Max files per scan.
            ui.horizontal(|ui| {
                ui.label("Max files per scan:");
                let mut v = state.max_files_limit as f64;
                if ui
                    .add(
                        egui::Slider::new(
                            &mut v,
                            (MIN_MAX_FILES as f64)..=(ABSOLUTE_MAX_FILES as f64),
                        )
                        .integer()
                        .suffix(" files")
                        .logarithmic(true),
                    )
                    .changed()
                {
                    state.max_files_limit =
                        (v as usize).clamp(MIN_MAX_FILES, ABSOLUTE_MAX_FILES);
                }
            });
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "Default: {DEFAULT_MAX_FILES}  |  Max: {ABSOLUTE_MAX_FILES}"
                    ))
                    .small()
                    .weak(),
                );
                if state.max_files_limit != DEFAULT_MAX_FILES
                    && ui
                        .small_button("Reset")
                        .on_hover_text("Reset to the built-in default")
                        .clicked()
                {
                    state.max_files_limit = DEFAULT_MAX_FILES;
                }
            });

            ui.add_space(8.0);

            // Max total entries.
            ui.horizontal(|ui| {
                ui.label("Max total entries:");
                let mut v = state.max_total_entries as f64;
                if ui
                    .add(
                        egui::Slider::new(
                            &mut v,
                            (MIN_MAX_TOTAL_ENTRIES as f64)
                                ..=(ABSOLUTE_MAX_TOTAL_ENTRIES as f64),
                        )
                        .integer()
                        .suffix(" entries")
                        .logarithmic(true),
                    )
                    .changed()
                {
                    state.max_total_entries = (v as usize)
                        .clamp(MIN_MAX_TOTAL_ENTRIES, ABSOLUTE_MAX_TOTAL_ENTRIES);
                }
            });
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "Default: {}  |  Max: {}",
                        MAX_TOTAL_ENTRIES,
                        ABSOLUTE_MAX_TOTAL_ENTRIES
                    ))
                    .small()
                    .weak(),
                );
                if state.max_total_entries != MAX_TOTAL_ENTRIES
                    && ui
                        .small_button("Reset")
                        .on_hover_text("Reset to the built-in default")
                        .clicked()
                {
                    state.max_total_entries = MAX_TOTAL_ENTRIES;
                }
            });

            ui.add_space(8.0);

            // Max scan depth.
            ui.horizontal(|ui| {
                ui.label("Max scan depth:");
                let mut v = state.max_scan_depth as f64;
                if ui
                    .add(
                        egui::Slider::new(
                            &mut v,
                            1.0..=(ABSOLUTE_MAX_DEPTH as f64),
                        )
                        .integer()
                        .suffix(" levels"),
                    )
                    .changed()
                {
                    state.max_scan_depth =
                        (v as usize).clamp(1, ABSOLUTE_MAX_DEPTH);
                }
            });
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "Default: {DEFAULT_MAX_DEPTH}  |  Max: {ABSOLUTE_MAX_DEPTH}"
                    ))
                    .small()
                    .weak(),
                );
                if state.max_scan_depth != DEFAULT_MAX_DEPTH
                    && ui
                        .small_button("Reset")
                        .on_hover_text("Reset to the built-in default")
                        .clicked()
                {
                    state.max_scan_depth = DEFAULT_MAX_DEPTH;
                }
            });

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(6.0);

            // =========================================================
            // Section 2 — Live Tail
            // =========================================================
            ui.heading("Live Tail");
            ui.add_space(2.0);
            ui.label(
                egui::RichText::new(
                    "How often the background thread checks watched files for new content. \
                     Lower values give faster updates; higher values reduce CPU/disk use. \
                     Applied when the next tail session is started.",
                )
                .small()
                .weak(),
            );
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                ui.label("Poll interval:");
                let mut v = state.tail_poll_interval_ms as f64;
                if ui
                    .add(
                        egui::Slider::new(
                            &mut v,
                            (MIN_TAIL_POLL_INTERVAL_MS as f64)
                                ..=(MAX_TAIL_POLL_INTERVAL_MS as f64),
                        )
                        .integer()
                        .suffix(" ms")
                        .logarithmic(true),
                    )
                    .changed()
                {
                    state.tail_poll_interval_ms = (v as u64)
                        .clamp(MIN_TAIL_POLL_INTERVAL_MS, MAX_TAIL_POLL_INTERVAL_MS);
                }
            });
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "Default: {} ms  |  Range: {} \u{2013} {} ms",
                        TAIL_POLL_INTERVAL_MS,
                        MIN_TAIL_POLL_INTERVAL_MS,
                        MAX_TAIL_POLL_INTERVAL_MS
                    ))
                    .small()
                    .weak(),
                );
                if state.tail_poll_interval_ms != TAIL_POLL_INTERVAL_MS
                    && ui
                        .small_button("Reset")
                        .on_hover_text("Reset to the built-in default")
                        .clicked()
                {
                    state.tail_poll_interval_ms = TAIL_POLL_INTERVAL_MS;
                }
            });

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(6.0);

            // =========================================================
            // Section 3 — Directory Watch
            // =========================================================
            ui.heading("Directory Watch");
            ui.add_space(2.0);
            ui.label(
                egui::RichText::new(
                    "How often the directory watcher polls for new log files created after \
                     the initial scan. Lower values detect new files sooner at the cost of \
                     more frequent directory walks. Applied when the next watch session is started.",
                )
                .small()
                .weak(),
            );
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                ui.label("Poll interval:");
                let mut v = state.dir_watch_poll_interval_ms as f64;
                if ui
                    .add(
                        egui::Slider::new(
                            &mut v,
                            (MIN_DIR_WATCH_POLL_INTERVAL_MS as f64)
                                ..=(MAX_DIR_WATCH_POLL_INTERVAL_MS as f64),
                        )
                        .integer()
                        .suffix(" ms")
                        .logarithmic(true),
                    )
                    .changed()
                {
                    state.dir_watch_poll_interval_ms = (v as u64).clamp(
                        MIN_DIR_WATCH_POLL_INTERVAL_MS,
                        MAX_DIR_WATCH_POLL_INTERVAL_MS,
                    );
                }
            });
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "Default: {} ms  |  Range: {} \u{2013} {} ms",
                        DIR_WATCH_POLL_INTERVAL_MS,
                        MIN_DIR_WATCH_POLL_INTERVAL_MS,
                        MAX_DIR_WATCH_POLL_INTERVAL_MS
                    ))
                    .small()
                    .weak(),
                );
                if state.dir_watch_poll_interval_ms != DIR_WATCH_POLL_INTERVAL_MS
                    && ui
                        .small_button("Reset")
                        .on_hover_text("Reset to the built-in default")
                        .clicked()
                {
                    state.dir_watch_poll_interval_ms = DIR_WATCH_POLL_INTERVAL_MS;
                }
            });

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(6.0);

            // =========================================================
            // Section 4 — External Profiles
            // =========================================================
            ui.heading("External Profiles");
            ui.add_space(2.0);
            ui.label(
                egui::RichText::new(
                    "Place custom .toml profile files here to add site-specific or \
                     product-specific log formats. Use the generator script \
                     (scripts/New-LogSleuthProfile.ps1) to create profiles \
                     from any log directory.",
                )
                .small()
                .weak(),
            );
            ui.add_space(8.0);

            // Profile directory path.
            ui.horizontal(|ui| {
                ui.label("Profile folder:");
                if let Some(ref dir) = state.user_profiles_dir {
                    ui.monospace(dir.display().to_string())
                        .on_hover_text("LogSleuth scans this directory for .toml profiles on startup and on Reload");
                } else {
                    ui.label(egui::RichText::new("(not configured)").weak());
                }
            });
            ui.add_space(4.0);

            // Loaded profile counts.
            let total = state.profiles.len();
            let builtin_count = state.profiles.iter().filter(|p| p.is_builtin).count();
            let external_count = total.saturating_sub(builtin_count);
            ui.label(
                egui::RichText::new(format!(
                    "{total} profiles loaded  \u{2014}  {builtin_count} built-in,  {external_count} external"
                ))
                .small()
                .weak(),
            );
            ui.add_space(8.0);

            // Action buttons.
            ui.horizontal(|ui| {
                let has_dir = state.user_profiles_dir.is_some();
                if ui
                    .add_enabled(has_dir, egui::Button::new("Open Folder"))
                    .on_hover_text("Open the external profiles folder in your file manager")
                    .clicked()
                {
                    if let Some(ref dir) = state.user_profiles_dir {
                        // Ensure the directory exists before opening it.
                        if let Err(e) = std::fs::create_dir_all(dir) {
                            tracing::warn!(
                                dir = %dir.display(),
                                error = %e,
                                "Failed to create profiles directory"
                            );
                            state.status_message =
                                format!("Cannot create profiles folder: {e}");
                        } else {
                            // Open the directory itself (not /select), so the
                            // user lands inside the folder ready to drop .toml files.
                            #[cfg(target_os = "windows")]
                            if let Err(e) = std::process::Command::new("explorer.exe").arg(dir).spawn() {
                                tracing::warn!(dir = %dir.display(), error = %e, "Failed to open profiles folder");
                                state.status_message = format!("Cannot open folder: {e}");
                            }
                            #[cfg(target_os = "macos")]
                            if let Err(e) = std::process::Command::new("open").arg(dir).spawn() {
                                tracing::warn!(dir = %dir.display(), error = %e, "Failed to open profiles folder");
                                state.status_message = format!("Cannot open folder: {e}");
                            }
                            #[cfg(target_os = "linux")]
                            if let Err(e) = std::process::Command::new("xdg-open").arg(dir).spawn() {
                                tracing::warn!(dir = %dir.display(), error = %e, "Failed to open profiles folder");
                                state.status_message = format!("Cannot open folder: {e}");
                            }
                        }
                    }
                }
                ui.add_space(4.0);
                if ui
                    .button("Reload Profiles")
                    .on_hover_text(
                        "Re-scan the external profiles folder and merge any new or updated \
                         profiles with the built-in set. Takes effect immediately.",
                    )
                    .clicked()
                {
                    state.request_reload_profiles = true;
                }
            });

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(6.0);

            // =========================================================
            // Footer
            // =========================================================
            ui.label(
                egui::RichText::new(
                    "Ingest settings apply to the next scan. \
                     Tail/watch settings apply when the next session is started. \
                     Profile changes take effect immediately.",
                )
                .small()
                .italics()
                .weak(),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Close").clicked() {
                    state.show_options = false;
                }
            });
        });

    if !open {
        state.show_options = false;
    }
}
