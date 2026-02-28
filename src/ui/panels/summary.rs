// LogSleuth - ui/panels/summary.rs
//
// Scan summary modal window.
// Shows overall statistics and a per-file breakdown table.
// Warnings from the scan are also listed.

use crate::app::state::AppState;

/// Render the scan summary dialog (if state.show_summary is true).
pub fn render(ctx: &egui::Context, state: &mut AppState) {
    if !state.show_summary {
        return;
    }

    let mut open = true;
    egui::Window::new("Scan Summary")
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .min_width(480.0)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            if let Some(ref summary) = state.scan_summary {
                // -----------------------------------------------------------------
                // Overall statistics
                // -----------------------------------------------------------------
                ui.strong("Overview");
                egui::Grid::new("summary_overview")
                    .num_columns(2)
                    .spacing([16.0, 4.0])
                    .show(ui, |ui| {
                        ui.label("Files discovered:");
                        ui.label(summary.total_files_discovered.to_string());
                        ui.end_row();

                        ui.label("Files matched:");
                        ui.label(summary.files_matched.to_string());
                        ui.end_row();

                        ui.label("Files with errors:");
                        let err_colour = if summary.files_with_errors > 0 {
                            egui::Color32::from_rgb(248, 113, 113)
                        } else {
                            ui.style().visuals.text_color()
                        };
                        ui.colored_label(err_colour, summary.files_with_errors.to_string());
                        ui.end_row();

                        ui.label("Total entries:");
                        ui.label(summary.total_entries.to_string());
                        ui.end_row();

                        ui.label("Parse errors:");
                        let pe_colour = if summary.total_parse_errors > 0 {
                            egui::Color32::from_rgb(253, 186, 116)
                        } else {
                            ui.style().visuals.text_color()
                        };
                        ui.colored_label(pe_colour, summary.total_parse_errors.to_string());
                        ui.end_row();

                        ui.label("Duration:");
                        ui.label(format!("{:.2}s", summary.duration.as_secs_f64()));
                        ui.end_row();
                    });

                // -----------------------------------------------------------------
                // Per-file breakdown table
                // -----------------------------------------------------------------
                if !summary.file_summaries.is_empty() {
                    ui.add_space(8.0);
                    ui.separator();
                    ui.strong("Per-file breakdown");

                    egui::ScrollArea::vertical()
                        .id_salt("summary_files")
                        .max_height(260.0)
                        .show(ui, |ui| {
                            egui::Grid::new("summary_file_table")
                                .num_columns(5)
                                .striped(true)
                                .spacing([12.0, 3.0])
                                .show(ui, |ui| {
                                    // Header row
                                    ui.strong("File");
                                    ui.strong("Profile");
                                    ui.strong("Entries");
                                    ui.strong("Errors");
                                    ui.strong("Time range");
                                    ui.end_row();

                                    for fs in &summary.file_summaries {
                                        let name = fs
                                            .path
                                            .file_name()
                                            .and_then(|n| n.to_str())
                                            .unwrap_or("?");
                                        ui.label(egui::RichText::new(name).monospace().size(11.5));
                                        ui.label(&fs.profile_id);
                                        ui.label(fs.entry_count.to_string());

                                        let err_colour = if fs.error_count > 0 {
                                            egui::Color32::from_rgb(248, 113, 113)
                                        } else {
                                            ui.style().visuals.text_color()
                                        };
                                        ui.colored_label(err_colour, fs.error_count.to_string());

                                        let time_range = match (fs.earliest, fs.latest) {
                                            (Some(e), Some(l)) if e == l => {
                                                e.format("%Y-%m-%d %H:%M:%S").to_string()
                                            }
                                            (Some(e), Some(l)) => {
                                                format!(
                                                    "{} \u{2013} {}",
                                                    e.format("%H:%M:%S"),
                                                    l.format("%H:%M:%S")
                                                )
                                            }
                                            _ => "--".to_string(),
                                        };
                                        ui.label(
                                            egui::RichText::new(time_range).monospace().size(11.5),
                                        );
                                        ui.end_row();
                                    }
                                });
                        });
                }

                // -----------------------------------------------------------------
                // Warnings
                // -----------------------------------------------------------------
                if !state.warnings.is_empty() {
                    ui.add_space(8.0);
                    ui.separator();
                    ui.strong(format!("Warnings ({})", state.warnings.len()));

                    egui::ScrollArea::vertical()
                        .id_salt("summary_warnings")
                        .max_height(120.0)
                        .show(ui, |ui| {
                            for warn in &state.warnings {
                                ui.label(
                                    egui::RichText::new(warn)
                                        .color(egui::Color32::from_rgb(253, 186, 116))
                                        .size(11.5),
                                );
                            }
                        });
                }
            } else {
                ui.label("No scan has been completed yet.");
            }

            ui.add_space(8.0);
            ui.separator();
            if ui.button("Close").clicked() {
                state.show_summary = false;
            }
        });

    if !open {
        state.show_summary = false;
    }
}
