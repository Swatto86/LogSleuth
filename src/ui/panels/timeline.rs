// LogSleuth - ui/panels/timeline.rs
//
// Virtual-scrolling unified timeline view.
//
// Uses egui's `ScrollArea::show_rows` which renders only the rows currently
// visible in the viewport, giving O(1) rendering cost regardless of entry count.
// Rule 16 compliance: selection is always valid; row clicks update state directly.

use crate::app::state::AppState;
use crate::ui::theme;

/// Render the timeline panel (central area).
pub fn render(ui: &mut egui::Ui, state: &mut AppState) {
    let filtered = state.filtered_indices.len();

    if filtered == 0 {
        ui.centered_and_justified(|ui| {
            if state.entries.is_empty() {
                ui.label(
                    "No log entries loaded.\nOpen a directory via File \u{2192} Open Directory.",
                );
            } else {
                ui.label("No entries match the current filters.");
            }
        });
        return;
    }

    let row_height = theme::ROW_HEIGHT;

    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show_rows(ui, row_height, filtered, |ui, row_range| {
            for display_idx in row_range {
                let Some(&entry_idx) = state.filtered_indices.get(display_idx) else {
                    continue;
                };
                let Some(entry) = state.entries.get(entry_idx) else {
                    continue;
                };

                let is_selected = state.selected_index == Some(display_idx);
                let sev_colour = theme::severity_colour(&entry.severity);
                let file_colour = state.colour_for_file(&entry.source_file);

                // Format: [SEV ] HH:MM:SS | filename.log | first line of message
                let ts = entry
                    .timestamp
                    .map(|t| t.format("%H:%M:%S").to_string())
                    .unwrap_or_else(|| "--:--:--".to_string());

                let file_name = entry
                    .source_file
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("?");

                let first_line = entry.message.lines().next().unwrap_or(&entry.message);

                let row_text = egui::RichText::new(format!(
                    "[{:<4}] {} | {:>16} | {}",
                    entry.severity.short_label(),
                    ts,
                    truncate_filename(file_name, 16),
                    first_line,
                ))
                .color(sev_colour)
                .monospace()
                .size(12.0);

                // Each row: 4 px coloured file stripe | selectable label
                let response = ui
                    .horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        // Coloured left stripe — visual CMTrace-style file indicator.
                        let (bar_rect, _) = ui
                            .allocate_exact_size(egui::vec2(4.0, row_height), egui::Sense::hover());
                        ui.painter().rect_filled(bar_rect, 0.0, file_colour);
                        ui.selectable_label(is_selected, row_text)
                    })
                    .inner;

                if response.clicked() {
                    state.selected_index = Some(display_idx);
                }

                // Show full timestamp as tooltip for entries with date info
                if let Some(ts_full) = entry.timestamp {
                    response.on_hover_text(ts_full.format("%Y-%m-%d %H:%M:%S UTC").to_string());
                }
            }
        });
}

/// Return the last `max` characters of `s`, right-aligned.
fn truncate_filename(s: &str, max: usize) -> String {
    // Truncate from the LEFT so the extension is always visible
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max {
        format!("{:>width$}", s, width = max)
    } else {
        chars[chars.len() - max..].iter().collect()
    }
}
