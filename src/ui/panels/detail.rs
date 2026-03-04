// LogSleuth - ui/panels/detail.rs
//
// Entry detail pane showing full message, metadata, and raw text.
// Severity label is coloured to match the timeline.

use crate::app::state::AppState;
use crate::ui::theme;

/// Render the detail pane (bottom panel).
pub fn render(ui: &mut egui::Ui, state: &AppState) {
    let Some(entry) = state.selected_entry() else {
        ui.centered_and_justified(|ui| {
            ui.label(
                egui::RichText::new(
                    "\u{2190} Click a row in the timeline to see its full details here",
                )
                .color(egui::Color32::from_rgb(107, 114, 128)),
            );
        });
        return;
    };

    // Resolve the global entry index for dedup info lookup.
    let global_entry_idx = state
        .selected_index
        .and_then(|si| state.filtered_indices.get(si).copied());

    // Coloured severity badge as a heading row
    let sev_colour = theme::severity_colour(&entry.severity, state.dark_mode);
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(entry.severity.label())
                .strong()
                .color(sev_colour),
        );
        ui.separator();
        ui.label(
            egui::RichText::new(
                entry
                    .source_file
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("?"),
            )
            .strong(),
        );
        if let Some(ts) = entry.timestamp {
            ui.label(egui::RichText::new(ts.format("  %Y-%m-%d %H:%M:%S UTC").to_string()).weak());
        }
    });

    ui.separator();

    // Metadata grid
    egui::Grid::new("detail_meta_grid")
        .num_columns(2)
        .spacing([8.0, 2.0])
        .show(ui, |ui| {
            ui.label("File:")
                .on_hover_text("Full path to the source log file");
            ui.label(egui::RichText::new(entry.source_file.display().to_string()).monospace());
            ui.end_row();

            ui.label("Line:")
                .on_hover_text("Line number in the source file where this entry starts");
            ui.label(entry.line_number.to_string());
            ui.end_row();

            ui.label("Profile:")
                .on_hover_text("The format profile used to parse this log entry");
            ui.label(&entry.profile_id);
            ui.end_row();

            if let Some(ref thread) = entry.thread {
                ui.label("Thread:")
                    .on_hover_text("Thread ID or name extracted from the log entry");
                ui.label(egui::RichText::new(thread).monospace());
                ui.end_row();
            }

            if let Some(ref component) = entry.component {
                ui.label("Component:")
                    .on_hover_text("Source component or module that emitted this log entry");
                ui.label(component);
                ui.end_row();
            }
        });

    ui.add_space(4.0);

    // -------------------------------------------------------------------------
    // Dedup occurrences section (only when the entry is part of a dedup group)
    // -------------------------------------------------------------------------
    if let Some(info) = global_entry_idx.and_then(|idx| state.dedup_info.get(&idx)) {
        if info.count > 1 {
            let header = format!("Occurrences ({})", info.count);
            egui::CollapsingHeader::new(
                egui::RichText::new(&header).color(egui::Color32::from_rgb(168, 85, 247)),
            )
            .default_open(false)
            .show(ui, |ui| {
                if let Some(first_ts) = info.first_timestamp {
                    ui.label(
                        egui::RichText::new(format!(
                            "First seen: {}",
                            first_ts.format("%Y-%m-%d %H:%M:%S UTC")
                        ))
                        .small()
                        .weak(),
                    );
                }
                egui::Grid::new("dedup_occurrences_grid")
                    .num_columns(3)
                    .spacing([12.0, 2.0])
                    .striped(true)
                    .show(ui, |ui| {
                        // Header row
                        ui.label(egui::RichText::new("Timestamp").small().strong());
                        ui.label(egui::RichText::new("File").small().strong());
                        ui.label(egui::RichText::new("Line").small().strong());
                        ui.end_row();

                        for &occ_idx in &info.all_indices {
                            if let Some(occ) = state.entries.get(occ_idx) {
                                let ts_str = occ
                                    .timestamp
                                    .map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string())
                                    .unwrap_or_else(|| "--".to_string());
                                let fname = occ
                                    .source_file
                                    .file_name()
                                    .and_then(|n| n.to_str())
                                    .unwrap_or("?");
                                ui.label(egui::RichText::new(ts_str).small().monospace());
                                ui.label(egui::RichText::new(fname).small());
                                ui.label(egui::RichText::new(occ.line_number.to_string()).small());
                                ui.end_row();
                            }
                        }
                    });
            });
            ui.add_space(4.0);
        }
    }

    // Message area with copy-to-clipboard and open-in-folder buttons
    ui.horizontal(|ui| {
        ui.label("Message:");
        if ui
            .small_button("\u{1f4cb} Copy")
            .on_hover_text("Copy the full message text to the clipboard")
            .clicked()
        {
            ui.ctx().copy_text(entry.message.clone());
        }
        // Open the containing folder in Windows Explorer / macOS Finder / Linux file manager.
        if ui
            .small_button("\u{1f4c2} Show in folder")
            .on_hover_text(
                "Open the file's folder in your system file manager with this file selected",
            )
            .clicked()
        {
            crate::platform::fs::reveal_in_file_manager(&entry.source_file);
        }
    });
    // Use most of the available panel height so multi-line messages are readable.
    // auto_shrink keeps it compact when the message is short.
    egui::ScrollArea::vertical()
        .id_salt("detail_message")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.label(egui::RichText::new(&entry.message).monospace());
        });
}
