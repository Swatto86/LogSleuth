// LogSleuth - ui/panels/timeline.rs
//
// Virtual-scrolling unified timeline view.
// Implementation: next increment.

use crate::app::state::AppState;
use crate::ui::theme;

/// Render the timeline panel (central area).
pub fn render(ui: &mut egui::Ui, state: &mut AppState) {
    if state.entries.is_empty() {
        ui.centered_and_justified(|ui| {
            ui.label("No log entries loaded.\nOpen a directory to begin scanning.");
        });
        return;
    }

    let total = state.entries.len();
    let filtered = state.filtered_indices.len();

    ui.label(format!("Showing {filtered} of {total} entries"));
    ui.separator();

    // TODO: Implement virtual scrolling in next increment.
    // For now, show a basic scrollable list.
    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            for (display_idx, &entry_idx) in state.filtered_indices.iter().enumerate() {
                if let Some(entry) = state.entries.get(entry_idx) {
                    let is_selected = state.selected_index == Some(display_idx);
                    let colour = theme::severity_colour(&entry.severity);

                    let response = ui.selectable_label(
                        is_selected,
                        egui::RichText::new(format!(
                            "{} [{}] {}",
                            entry.severity.short_label(),
                            entry
                                .source_file
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("?"),
                            truncate_message(&entry.message, 120),
                        ))
                        .color(colour)
                        .monospace(),
                    );

                    if response.clicked() {
                        state.selected_index = Some(display_idx);
                    }
                }
            }
        });
}

/// Truncate a message for display, preserving the start.
fn truncate_message(msg: &str, max_len: usize) -> String {
    let first_line = msg.lines().next().unwrap_or(msg);
    if first_line.len() > max_len {
        format!("{}...", &first_line[..max_len])
    } else {
        first_line.to_string()
    }
}
