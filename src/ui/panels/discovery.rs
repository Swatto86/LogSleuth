// LogSleuth - ui/panels/discovery.rs
//
// Directory picker, scan controls, discovered file list.
// Implementation: next increment.

use crate::app::state::AppState;

/// Render the discovery panel (left sidebar section).
pub fn render(ui: &mut egui::Ui, state: &mut AppState) {
    ui.heading("Scan");
    ui.separator();

    if let Some(ref path) = state.scan_path {
        ui.label(format!("Path: {}", path.display()));
    } else {
        ui.label("No directory selected.");
    }

    if ui
        .add_enabled(!state.scan_in_progress, egui::Button::new("Open Directory..."))
        .clicked()
    {
        if let Some(path) = rfd::FileDialog::new().pick_folder() {
            state.scan_path = Some(path);
            // TODO: trigger scan
        }
    }

    if state.scan_in_progress {
        ui.spinner();
        ui.label("Scanning...");
    }
}
