// LogSleuth - ui/panels/options.rs
//
// Options dialog: runtime-configurable application settings.
// Shown when the user opens Edit > Options... from the menu bar.
//
// Currently exposes:
//   - Max files ingest limit (how many log files a single directory scan will load)
//
// Settings take effect on the *next* scan; they are not retroactively applied.
// All limits are validated against absolute bounds from util::constants to
// prevent accidental misconfiguration (Rule 13 + Rule 11 input validation).

use crate::app::state::AppState;
use crate::util::constants::{ABSOLUTE_MAX_FILES, DEFAULT_MAX_FILES};

/// Minimum sensible value for the max-files limit.
/// Set to 1 so the control is never completely disabled.
const MIN_MAX_FILES: usize = 1;

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
        .default_width(380.0)
        .show(ctx, |ui| {
            // ---- Ingest Limits ----
            ui.heading("Ingest Limits");
            ui.add_space(2.0);
            ui.label(
                egui::RichText::new(
                    "Controls how many log files are loaded when scanning a directory. \
                     When more files are found than the limit, only the most recently \
                     modified files are loaded.",
                )
                .small()
                .weak(),
            );
            ui.add_space(8.0);

            // Slider row.
            ui.horizontal(|ui| {
                ui.label("Max files per scan:");
                let mut v = state.max_files_limit as f64;
                let slider =
                    egui::Slider::new(&mut v, (MIN_MAX_FILES as f64)..=(ABSOLUTE_MAX_FILES as f64))
                        .integer()
                        .suffix(" files")
                        .logarithmic(true);
                if ui.add(slider).changed() {
                    state.max_files_limit = (v as usize).clamp(MIN_MAX_FILES, ABSOLUTE_MAX_FILES);
                }
            });

            // Status + optional reset on the same line.
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

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(6.0);

            // Footer: note line, then Close flush-right on the next line.
            ui.label(
                egui::RichText::new("Changes apply to the next scan.")
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
