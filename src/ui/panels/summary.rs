// LogSleuth - ui/panels/summary.rs
//
// Scan summary dialog.
// Implementation: next increment.

use crate::app::state::AppState;

/// Render the scan summary dialog (if open).
pub fn render(ctx: &egui::Context, state: &mut AppState) {
    if !state.show_summary {
        return;
    }

    egui::Window::new("Scan Summary")
        .collapsible(false)
        .resizable(true)
        .show(ctx, |ui| {
            if let Some(ref summary) = state.scan_summary {
                ui.label(format!("Files scanned: {}", summary.total_files_discovered));
                ui.label(format!("Files matched: {}", summary.files_matched));
                ui.label(format!("Files with errors: {}", summary.files_with_errors));
                ui.label(format!("Total entries: {}", summary.total_entries));
                ui.label(format!("Parse errors: {}", summary.total_parse_errors));
                ui.label(format!(
                    "Duration: {:.2}s",
                    summary.duration.as_secs_f64()
                ));
            } else {
                ui.label("No scan has been completed yet.");
            }

            ui.separator();
            if ui.button("Close").clicked() {
                state.show_summary = false;
            }
        });
}
