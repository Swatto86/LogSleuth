// LogSleuth - app.rs
//
// Top-level eframe::App implementation.
// Wires together all UI panels and manages the scan lifecycle.

use crate::app::scan::ScanManager;
use crate::app::state::AppState;
use crate::core::discovery::DiscoveryConfig;
use crate::ui;

/// The LogSleuth application.
pub struct LogSleuthApp {
    pub state: AppState,
    pub scan_manager: ScanManager,
}

impl LogSleuthApp {
    /// Create a new application instance with the given state.
    pub fn new(state: AppState) -> Self {
        Self {
            state,
            scan_manager: ScanManager::new(),
        }
    }
}

impl eframe::App for LogSleuthApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Poll for scan progress
        let messages = self.scan_manager.poll_progress();
        let had_messages = !messages.is_empty();
        for msg in messages {
            match msg {
                crate::core::model::ScanProgress::DiscoveryStarted => {
                    self.state.status_message = "Discovering files...".to_string();
                    self.state.scan_in_progress = true;
                }
                crate::core::model::ScanProgress::FileDiscovered { files_found, .. } => {
                    self.state.status_message =
                        format!("Discovering files... ({files_found} found)");
                }
                crate::core::model::ScanProgress::DiscoveryCompleted { total_files } => {
                    self.state.status_message =
                        format!("Discovery complete: {total_files} files found.");
                }
                crate::core::model::ScanProgress::FilesDiscovered { files } => {
                    self.state.discovered_files = files;
                }
                crate::core::model::ScanProgress::ParsingStarted { total_files } => {
                    self.state.status_message = format!("Parsing {total_files} files...");
                }
                crate::core::model::ScanProgress::FileParsed {
                    files_completed,
                    total_files,
                    ..
                } => {
                    self.state.status_message =
                        format!("Parsing files ({files_completed}/{total_files})...");
                }
                crate::core::model::ScanProgress::EntriesBatch { entries } => {
                    self.state.entries.extend(entries);
                }
                crate::core::model::ScanProgress::ParsingCompleted { summary } => {
                    self.state.status_message = format!(
                        "Scan complete: {} entries from {} files in {:.2}s",
                        summary.total_entries,
                        summary.files_matched,
                        summary.duration.as_secs_f64()
                    );
                    self.state.scan_summary = Some(summary);
                    self.state.scan_in_progress = false;
                    self.state.apply_filters();
                }
                crate::core::model::ScanProgress::Warning { message } => {
                    self.state.warnings.push(message);
                }
                crate::core::model::ScanProgress::Failed { error } => {
                    self.state.status_message = format!("Scan failed: {error}");
                    self.state.scan_in_progress = false;
                }
                crate::core::model::ScanProgress::Cancelled => {
                    self.state.status_message = "Scan cancelled.".to_string();
                    self.state.scan_in_progress = false;
                }
            }
        }
        // Repaint when scan is active so progress updates appear promptly.
        if had_messages || self.state.scan_in_progress {
            ctx.request_repaint();
        }

        // If a relative time filter is active, refresh the time window each frame
        // and schedule a 1-second repaint so the rolling boundary stays current
        // as the clock advances even when nothing else is happening.
        if self.state.filter_state.relative_time_secs.is_some() {
            self.state.apply_filters();
            ctx.request_repaint_after(std::time::Duration::from_secs(1));
        }

        // ---- Handle flags set by discovery panel ----
        // pending_scan: a panel requested a new scan via Open Directory button.
        if let Some(path) = self.state.pending_scan.take() {
            self.state.clear();
            self.state.scan_path = Some(path.clone());
            self.scan_manager.start_scan(
                path,
                self.state.profiles.clone(),
                DiscoveryConfig::default(),
            );
        }
        // request_cancel: a panel requested the current scan be cancelled.
        if self.state.request_cancel {
            self.state.request_cancel = false;
            self.scan_manager.cancel_scan();
        }

        // Top menu bar
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Open Directory...").clicked() {
                        if let Some(path) = rfd::FileDialog::new().pick_folder() {
                            self.state.clear();
                            self.state.scan_path = Some(path.clone());
                            self.scan_manager.start_scan(
                                path,
                                self.state.profiles.clone(),
                                DiscoveryConfig::default(),
                            );
                        }
                        ui.close_menu();
                    }
                    ui.separator();
                    // Export sub-menu -- enabled only when there are filtered entries
                    let has_entries = !self.state.filtered_indices.is_empty();
                    ui.add_enabled_ui(has_entries, |ui| {
                        ui.menu_button("Export", |ui| {
                            if ui.button("Export CSV...").clicked() {
                                if let Some(dest) = rfd::FileDialog::new()
                                    .add_filter("CSV", &["csv"])
                                    .set_file_name("export.csv")
                                    .save_file()
                                {
                                    let filtered_entries: Vec<_> = self
                                        .state
                                        .filtered_indices
                                        .iter()
                                        .filter_map(|&i| self.state.entries.get(i))
                                        .cloned()
                                        .collect();
                                    match std::fs::File::create(&dest) {
                                        Ok(f) => {
                                            match crate::core::export::export_csv(
                                                &filtered_entries,
                                                f,
                                                &dest,
                                            ) {
                                                Ok(n) => {
                                                    self.state.status_message =
                                                        format!("Exported {n} entries to CSV.");
                                                }
                                                Err(e) => {
                                                    self.state.status_message =
                                                        format!("CSV export failed: {e}");
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            self.state.status_message =
                                                format!("Cannot create file: {e}");
                                        }
                                    }
                                }
                                ui.close_menu();
                            }
                            if ui.button("Export JSON...").clicked() {
                                if let Some(dest) = rfd::FileDialog::new()
                                    .add_filter("JSON", &["json"])
                                    .set_file_name("export.json")
                                    .save_file()
                                {
                                    let filtered_entries: Vec<_> = self
                                        .state
                                        .filtered_indices
                                        .iter()
                                        .filter_map(|&i| self.state.entries.get(i))
                                        .cloned()
                                        .collect();
                                    match std::fs::File::create(&dest) {
                                        Ok(f) => {
                                            match crate::core::export::export_json(
                                                &filtered_entries,
                                                f,
                                                &dest,
                                            ) {
                                                Ok(n) => {
                                                    self.state.status_message =
                                                        format!("Exported {n} entries to JSON.");
                                                }
                                                Err(e) => {
                                                    self.state.status_message =
                                                        format!("JSON export failed: {e}");
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            self.state.status_message =
                                                format!("Cannot create file: {e}");
                                        }
                                    }
                                }
                                ui.close_menu();
                            }
                        });
                    });
                    ui.separator();
                    if ui.button("Exit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.menu_button("View", |ui| {
                    if ui.button("Scan Summary").clicked() {
                        self.state.show_summary = true;
                        ui.close_menu();
                    }
                });
            });
        });

        // Status bar
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(&self.state.status_message);
                // Cancel button visible only while a scan is running
                if self.state.scan_in_progress && ui.small_button("Cancel").clicked() {
                    self.scan_manager.cancel_scan();
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let total = self.state.entries.len();
                    let filtered = self.state.filtered_indices.len();
                    if total > 0 {
                        ui.label(format!("{filtered}/{total} entries"));
                    }
                });
            });
        });

        // Detail pane (bottom)
        egui::TopBottomPanel::bottom("detail_pane")
            .resizable(true)
            .default_height(ui::theme::DETAIL_PANE_HEIGHT)
            .show(ctx, |ui| {
                ui::panels::detail::render(ui, &self.state);
            });

        // Left sidebar (discovery + filters)
        egui::SidePanel::left("sidebar")
            .default_width(ui::theme::SIDEBAR_WIDTH)
            .resizable(true)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui::panels::discovery::render(ui, &mut self.state);
                    ui.add_space(16.0);
                    ui::panels::filters::render(ui, &mut self.state);
                });
            });

        // Central panel (timeline)
        egui::CentralPanel::default().show(ctx, |ui| {
            ui::panels::timeline::render(ui, &mut self.state);
        });

        // Summary dialog (modal-ish)
        ui::panels::summary::render(ctx, &mut self.state);
    }
}
