// LogSleuth - ui/panels/discovery.rs
//
// Scan controls and discovered-file list for the left sidebar.
//
// This panel sets `state.pending_scan` and `state.request_cancel` flags;
// gui.rs consumes those flags and calls the ScanManager. This keeps the
// panel within the UI layer (no direct access to ScanManager).

use crate::app::state::AppState;
use chrono::Local;

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

    // -------------------------------------------------------------------------
    // Date filter — limits the scan to files modified on or after a given date.
    // Shown BEFORE the Open Directory button so the user sets it first.
    // Persists across scans so the user can re-run with the same date in focus.
    // -------------------------------------------------------------------------
    ui.separator();
    ui.label(
        egui::RichText::new("File date filter (YYYY-MM-DD):")
            .small()
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Only scans files modified on or after this date. \
             Leave blank to scan all files.",
        )
        .small()
        .weak(),
    );
    ui.horizontal(|ui| {
        // Text input. Hint shows the expected format.
        let resp = ui.add(
            egui::TextEdit::singleline(&mut state.discovery_date_input)
                .hint_text("e.g. 2025-03-14")
                .desired_width(100.0),
        );

        // Validation feedback inline next to the input.
        let input_trimmed = state.discovery_date_input.trim().to_string();
        if !input_trimmed.is_empty() {
            let valid = state.discovery_modified_since().is_some();
            if valid {
                ui.colored_label(egui::Color32::from_rgb(74, 222, 128), "\u{2713}");
            } else {
                ui.colored_label(egui::Color32::from_rgb(248, 113, 113), "\u{2717}");
            }
        }

        let _ = resp; // response not otherwise used

        // "Today" quick-fill — populates the input with today's local date.
        if ui
            .small_button("Today")
            .on_hover_text("Set to today's date in local time")
            .clicked()
        {
            let today = Local::now().format("%Y-%m-%d").to_string();
            state.discovery_date_input = today;
        }

        // Clear button — only when a date has been entered.
        if !state.discovery_date_input.trim().is_empty()
            && ui
                .add(
                    egui::Button::new(
                        egui::RichText::new("\u{d7}")
                            .small()
                            .color(egui::Color32::from_rgb(156, 163, 175)),
                    )
                    .small()
                    .frame(false),
                )
                .on_hover_text("Clear date filter")
                .clicked()
        {
            state.discovery_date_input.clear();
        }
    });

    // Show the resolved start-of-day UTC value as feedback.
    if let Some(since) = state.discovery_modified_since() {
        ui.label(
            egui::RichText::new(format!(
                "Scanning files modified on or after {} UTC",
                since.format("%Y-%m-%d 00:00")
            ))
            .small()
            .color(egui::Color32::from_rgb(96, 165, 250)),
        );
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
    } else {
        // Two open buttons on the same row.
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    !state.scan_in_progress,
                    egui::Button::new("Open Directory\u{2026}"),
                )
                .on_hover_text("Scan a directory for log files")
                .clicked()
            {
                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                    state.pending_scan = Some(path);
                }
            }
            if ui
                .add_enabled(
                    !state.scan_in_progress,
                    egui::Button::new("Open Log(s)\u{2026}"),
                )
                .on_hover_text("Select individual log files to open as a new session")
                .clicked()
            {
                if let Some(files) = rfd::FileDialog::new()
                    .add_filter("Log files", &["log", "txt", "log.1", "log.2", "log.3"])
                    .pick_files()
                {
                    state.pending_replace_files = Some(files);
                }
            }
        });

        // Clear Session — resets everything including the selected directory.
        // Disabled while a scan is running.
        let has_session = state.scan_path.is_some() || !state.entries.is_empty();
        if has_session {
            ui.add_space(4.0);
            if ui
                .add_enabled(
                    !state.scan_in_progress,
                    egui::Button::new(
                        egui::RichText::new("Clear Session")
                            .small()
                            .color(egui::Color32::from_rgb(156, 163, 175)),
                    )
                    .frame(false),
                )
                .on_hover_text("Reset to a blank state with no directory or files selected")
                .clicked()
            {
                state.request_new_session = true;
            }
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
                    })
                    .response
                    .on_hover_text(file.path.display().to_string());

                    // Profile label — hovering shows the profile's known default
                    // log locations (pulled from log_locations in the TOML profile).
                    let profile_label = ui.label(
                        egui::RichText::new(&profile_text)
                            .small()
                            .color(profile_colour),
                    );
                    if let Some(pid) = &file.profile_id {
                        if let Some(prof) = state.profiles.iter().find(|p| &p.id == pid) {
                            if !prof.log_locations.is_empty() {
                                profile_label.on_hover_text(format!(
                                    "Default log locations:\n{}",
                                    prof.log_locations.join("\n")
                                ));
                            }
                        }
                    }
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
