// LogSleuth - ui/panels/discovery.rs
//
// Scan controls and discovered-file list for the left sidebar.
//
// This panel sets `state.pending_scan` and `state.request_cancel` flags;
// gui.rs consumes those flags and calls the ScanManager. This keeps the
// panel within the UI layer (no direct access to ScanManager).

use crate::app::state::AppState;

/// Render the scan controls and file list (left sidebar top section).
pub fn render(ui: &mut egui::Ui, state: &mut AppState) {
    ui.heading("Scan");
    ui.separator();

    // Current scan path (truncated to fit sidebar)
    if let Some(ref path) = state.scan_path.clone() {
        ui.label(
            egui::RichText::new(path.display().to_string())
                .small()
                .weak(),
        );
    } else {
        ui.label(egui::RichText::new("No directory selected.").small().weak());
    }

    ui.add_space(4.0);

    // Scan / cancel controls
    if state.scan_in_progress {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label("Scanning\u{2026}");
        });
        if ui.button("Cancel").clicked() {
            state.request_cancel = true;
        }
    } else if ui
        .add_enabled(
            !state.scan_in_progress,
            egui::Button::new("Open Directory\u{2026}"),
        )
        .clicked()
    {
        if let Some(path) = rfd::FileDialog::new().pick_folder() {
            state.pending_scan = Some(path);
        }
    }

    // Discovered file list (shown after scan completes or while scanning)
    if !state.discovered_files.is_empty() {
        ui.add_space(6.0);
        ui.separator();
        ui.label(
            egui::RichText::new(format!("{} files discovered", state.discovered_files.len()))
                .small()
                .strong(),
        );

        // Live Tail controls — available once files are loaded and no scan running.
        if !state.scan_in_progress && !state.entries.is_empty() {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if state.tail_active {
                    // Active: red stop button.
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new("\u{25a0} Stop Tail")
                                    .color(egui::Color32::from_rgb(239, 68, 68)),
                            )
                            .small(),
                        )
                        .on_hover_text("Stop watching files for new log lines")
                        .clicked()
                    {
                        state.request_stop_tail = true;
                    }
                    // Auto-scroll toggle.
                    let scroll_colour = if state.tail_auto_scroll {
                        egui::Color32::from_rgb(34, 197, 94)
                    } else {
                        egui::Color32::from_rgb(107, 114, 128)
                    };
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new("\u{2193} Auto")
                                    .small()
                                    .color(scroll_colour),
                            )
                            .small()
                            .frame(false),
                        )
                        .on_hover_text("Toggle auto-scroll to newest entry")
                        .clicked()
                    {
                        state.tail_auto_scroll = !state.tail_auto_scroll;
                    }
                } else {
                    // Inactive: green start button.
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new("\u{25cf} Live Tail")
                                    .color(egui::Color32::from_rgb(34, 197, 94)),
                            )
                            .small(),
                        )
                        .on_hover_text("Watch loaded files for new log lines written in real time")
                        .clicked()
                    {
                        state.request_start_tail = true;
                    }
                }
            });
        }

        egui::ScrollArea::vertical()
            .id_salt("discovery_files")
            .max_height(360.0)
            .show(ui, |ui| {
                for file in &state.discovered_files {
                    let name = file
                        .path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("?");

                    let (profile_text, profile_colour) = match &file.profile_id {
                        Some(id) if id == "plain-text" && file.detection_confidence == 0.0 => (
                            // Fallback assignment: readable but no structured format detected.
                            "plain-text (fallback)".to_string(),
                            egui::Color32::from_rgb(156, 163, 175), // gray
                        ),
                        Some(id) => (
                            format!("{id} ({:.0}%)", file.detection_confidence * 100.0),
                            egui::Color32::from_rgb(74, 222, 128), // green
                        ),
                        None => (
                            "unmatched".to_string(),
                            egui::Color32::from_rgb(156, 163, 175), // gray
                        ),
                    };

                    let size_text = format_size(file.size);

                    ui.horizontal(|ui| {
                        // Coloured dot matching the file's timeline stripe colour.
                        let colour = state.colour_for_file(&file.path);
                        let (dot_rect, _) =
                            ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                        ui.painter().circle_filled(dot_rect.center(), 4.0, colour);
                        ui.label(egui::RichText::new(name).small().strong());
                        ui.label(egui::RichText::new(size_text).small().weak());
                    });
                    ui.label(
                        egui::RichText::new(profile_text)
                            .small()
                            .color(profile_colour),
                    );
                    ui.add_space(2.0);
                }
            });
    }

    // Warnings summary
    if !state.warnings.is_empty() {
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(format!(
                "{} warning{}",
                state.warnings.len(),
                if state.warnings.len() == 1 { "" } else { "s" }
            ))
            .small()
            .color(egui::Color32::from_rgb(217, 119, 6)),
        );
    }
}

/// Human-readable byte size.
fn format_size(bytes: u64) -> String {
    if bytes >= 1_073_741_824 {
        format!("{:.1} GB", bytes as f64 / 1_073_741_824.0)
    } else if bytes >= 1_048_576 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1_024 {
        format!("{:.1} KB", bytes as f64 / 1_024.0)
    } else {
        format!("{bytes} B")
    }
}
