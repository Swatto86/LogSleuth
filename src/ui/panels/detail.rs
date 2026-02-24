// LogSleuth - ui/panels/detail.rs
//
// Entry detail pane showing full message, metadata, and raw text.
// Implementation: next increment.

use crate::app::state::AppState;

/// Render the detail pane (bottom panel).
pub fn render(ui: &mut egui::Ui, state: &AppState) {
    if let Some(entry) = state.selected_entry() {
        egui::Grid::new("detail_grid")
            .num_columns(2)
            .spacing([8.0, 4.0])
            .show(ui, |ui| {
                ui.label("Severity:");
                ui.label(entry.severity.label());
                ui.end_row();

                ui.label("File:");
                ui.label(entry.source_file.display().to_string());
                ui.end_row();

                ui.label("Line:");
                ui.label(entry.line_number.to_string());
                ui.end_row();

                if let Some(ref ts) = entry.timestamp {
                    ui.label("Timestamp:");
                    ui.label(ts.to_rfc3339());
                    ui.end_row();
                }

                if let Some(ref thread) = entry.thread {
                    ui.label("Thread:");
                    ui.label(thread);
                    ui.end_row();
                }

                if let Some(ref component) = entry.component {
                    ui.label("Component:");
                    ui.label(component);
                    ui.end_row();
                }

                ui.label("Profile:");
                ui.label(&entry.profile_id);
                ui.end_row();
            });

        ui.separator();
        ui.label("Message:");
        egui::ScrollArea::vertical()
            .max_height(100.0)
            .show(ui, |ui| {
                ui.label(egui::RichText::new(&entry.message).monospace());
            });
    } else {
        ui.centered_and_justified(|ui| {
            ui.label("Select an entry to view details.");
        });
    }
}
