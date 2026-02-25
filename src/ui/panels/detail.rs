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
            ui.label("Select a timeline entry to view details.");
        });
        return;
    };

    // Coloured severity badge as a heading row
    let sev_colour = theme::severity_colour(&entry.severity);
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
            ui.label("File:");
            ui.label(egui::RichText::new(entry.source_file.display().to_string()).monospace());
            ui.end_row();

            ui.label("Line:");
            ui.label(entry.line_number.to_string());
            ui.end_row();

            ui.label("Profile:");
            ui.label(&entry.profile_id);
            ui.end_row();

            if let Some(ref thread) = entry.thread {
                ui.label("Thread:");
                ui.label(egui::RichText::new(thread).monospace());
                ui.end_row();
            }

            if let Some(ref component) = entry.component {
                ui.label("Component:");
                ui.label(component);
                ui.end_row();
            }
        });

    ui.add_space(4.0);

    // Message area with copy-to-clipboard and open-in-folder buttons
    ui.horizontal(|ui| {
        ui.label("Message:");
        if ui.small_button("Copy").clicked() {
            ui.ctx().copy_text(entry.message.clone());
        }
        // Open the containing folder in Windows Explorer / macOS Finder / Linux file manager.
        if ui.small_button("Show in folder").clicked() {
            let folder = entry.source_file.parent().unwrap_or(&entry.source_file);
            #[cfg(target_os = "windows")]
            {
                // `explorer /select,<path>` must be ONE argument — no space between
                // the comma and the path.  Two separate .arg() calls would be parsed
                // as distinct argv entries, causing Explorer to ignore the path and
                // open the default folder without selecting the file.
                let _ = std::process::Command::new("explorer")
                    .arg(format!("/select,{}", entry.source_file.display()))
                    .spawn();
            }
            #[cfg(target_os = "macos")]
            {
                let _ = std::process::Command::new("open")
                    .arg("-R")
                    .arg(&entry.source_file)
                    .spawn();
            }
            #[cfg(not(any(target_os = "windows", target_os = "macos")))]
            {
                let _ = std::process::Command::new("xdg-open").arg(folder).spawn();
            }
            let _ = folder; // suppress unused-variable warning on all paths
        }
    });
    // Use most of the available panel height so multi-line messages are readable.
    // auto_shrink keeps it compact when the message is short.
    egui::ScrollArea::vertical()
        .id_salt("detail_message")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.label(egui::RichText::new(&entry.message).monospace().size(11.5));
        });
}
