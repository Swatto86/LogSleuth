// LogSleuth - ui/panels/log_summary.rs
//
// Log-entry summarisation panel.
//
// Enumerates the currently-filtered log entries and produces a structured
// summary of errors and warnings (or whichever severities are present).
// Colour coding mirrors the timeline's `theme::severity_colour` mapping.
//
// The window is modal-ish (floated, always on top) and is opened via
// View -> Log Summary or the "Summary" button in the Filters sidebar.
//
// Rule 16: the close button is always enabled.
// Rule 11: no unbounded allocation — at most MAX_PREVIEW_ROWS rows are shown
//          per severity group.

use crate::app::state::AppState;
use crate::core::model::Severity;
use crate::ui::theme;

/// Maximum number of message preview rows shown per severity group.
const MAX_PREVIEW_ROWS: usize = 50;

/// Render the log-entry summary window (if `state.show_log_summary` is true).
pub fn render(ctx: &egui::Context, state: &mut AppState) {
    if !state.show_log_summary {
        return;
    }

    // Collect statistics over currently-filtered entries.
    // We iterate once: build per-severity counts and collect up to
    // MAX_PREVIEW_ROWS representative messages for actionable severities.
    let total_filtered = state.filtered_indices.len();

    // Ordered severities used when building per-group previews.
    let display_order = [
        Severity::Critical,
        Severity::Error,
        Severity::Warning,
        Severity::Info,
        Severity::Debug,
        Severity::Unknown,
    ];

    // Per-severity entry count.
    let mut counts: std::collections::HashMap<Severity, usize> = std::collections::HashMap::new();

    // Per-severity preview rows: (timestamp_str, source_file, first_line_of_message).
    let mut previews: std::collections::HashMap<Severity, Vec<(String, String, String)>> =
        std::collections::HashMap::new();

    for &idx in &state.filtered_indices {
        let Some(entry) = state.entries.get(idx) else {
            continue;
        };
        let sev = entry.severity;
        *counts.entry(sev).or_insert(0) += 1;

        let rows = previews.entry(sev).or_default();
        if rows.len() < MAX_PREVIEW_ROWS {
            let ts = entry
                .timestamp
                .map(|t| t.format("%H:%M:%S").to_string())
                .unwrap_or_else(|| "--:--:--".to_string());
            let file = entry
                .source_file
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("?")
                .to_string();
            let msg = entry
                .message
                .lines()
                .next()
                .unwrap_or(&entry.message)
                .to_string();
            rows.push((ts, file, msg));
        }
    }

    egui::Window::new("Log Summary")
        .collapsible(false)
        .resizable(true)
        .min_width(560.0)
        .min_height(300.0)
        .default_width(720.0)
        .default_height(500.0)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            // ----------------------------------------------------------------
            // Header: overall counts line
            // ----------------------------------------------------------------
            ui.horizontal(|ui| {
                ui.strong(format!("Showing {total_filtered} filtered entries"));
                if total_filtered < state.entries.len() {
                    ui.label(
                        egui::RichText::new(format!(
                            "({} total, {} hidden by filters)",
                            state.entries.len(),
                            state.entries.len() - total_filtered
                        ))
                        .weak()
                        .small(),
                    );
                }
            });

            ui.add_space(4.0);

            // ----------------------------------------------------------------
            // Severity breakdown table
            // ----------------------------------------------------------------
            egui::Grid::new("log_summary_counts")
                .num_columns(3)
                .spacing([20.0, 3.0])
                .show(ui, |ui| {
                    ui.strong("Severity");
                    ui.strong("Count");
                    ui.strong("Share");
                    ui.end_row();

                    for sev in &display_order {
                        let count = counts.get(sev).copied().unwrap_or(0);
                        if count == 0 {
                            continue;
                        }
                        let colour = theme::severity_colour(sev);
                        let pct = if total_filtered > 0 {
                            (count as f64 / total_filtered as f64) * 100.0
                        } else {
                            0.0
                        };

                        ui.colored_label(colour, sev.label());
                        ui.colored_label(colour, count.to_string());
                        ui.label(egui::RichText::new(format!("{pct:.1}%")).weak().small());
                        ui.end_row();
                    }
                });

            ui.add_space(4.0);
            ui.separator();

            // ----------------------------------------------------------------
            // Per-severity message preview sections
            // Only show sections for severities that have entries.
            // Actionable severities (Critical, Error, Warning) are expanded
            // by default; Info/Debug/Unknown are collapsed.
            // ----------------------------------------------------------------
            egui::ScrollArea::vertical()
                .id_salt("log_summary_scroll")
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    for sev in &display_order {
                        let Some(rows) = previews.get(sev) else {
                            continue;
                        };
                        if rows.is_empty() {
                            continue;
                        }
                        let count = counts.get(sev).copied().unwrap_or(0);
                        let colour = theme::severity_colour(sev);

                        // Default-open for actionable severities.
                        let default_open = matches!(
                            sev,
                            Severity::Critical | Severity::Error | Severity::Warning
                        );

                        let header_text =
                            egui::RichText::new(format!("{} ({})", sev.label(), count))
                                .color(colour)
                                .strong();

                        egui::CollapsingHeader::new(header_text)
                            .id_salt(format!("log_summary_{}", sev.label()))
                            .default_open(default_open)
                            .show(ui, |ui| {
                                // Table: timestamp | file | message
                                egui::Grid::new(format!("log_summary_grid_{}", sev.label()))
                                    .num_columns(3)
                                    .spacing([8.0, 2.0])
                                    .striped(true)
                                    .show(ui, |ui| {
                                        for (ts, file, msg) in rows {
                                            ui.label(
                                                egui::RichText::new(ts)
                                                    .monospace()
                                                    .size(11.5)
                                                    .weak(),
                                            );
                                            ui.label(
                                                egui::RichText::new(file)
                                                    .monospace()
                                                    .size(11.5)
                                                    .weak(),
                                            );
                                            ui.label(
                                                egui::RichText::new(msg).color(colour).size(11.5),
                                            );
                                            ui.end_row();
                                        }
                                    });

                                if count > MAX_PREVIEW_ROWS {
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "... and {} more (export to CSV/JSON to see all)",
                                            count - MAX_PREVIEW_ROWS
                                        ))
                                        .weak()
                                        .small()
                                        .italics(),
                                    );
                                }
                            });
                    }

                    if total_filtered == 0 {
                        ui.centered_and_justified(|ui| {
                            ui.label("No entries match the current filters.");
                        });
                    }
                });

            ui.add_space(6.0);
            ui.separator();
            if ui.button("Close").clicked() {
                state.show_log_summary = false;
            }
        });
}
