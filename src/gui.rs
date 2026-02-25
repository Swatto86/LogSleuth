// LogSleuth - app.rs
//
// Top-level eframe::App implementation.
// Wires together all UI panels and manages the scan lifecycle.

use crate::app::scan::ScanManager;
use crate::app::state::AppState;
use crate::app::tail::TailManager;
use crate::core::discovery::DiscoveryConfig;
use crate::ui;

/// The LogSleuth application.
pub struct LogSleuthApp {
    pub state: AppState,
    pub scan_manager: ScanManager,
    pub tail_manager: TailManager,
}

impl LogSleuthApp {
    /// Create a new application instance with the given state.
    pub fn new(state: AppState) -> Self {
        Self {
            state,
            scan_manager: ScanManager::new(),
            tail_manager: TailManager::new(),
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
                    // Assign a palette colour to each newly discovered file.
                    for f in &files {
                        self.state.assign_file_colour(&f.path);
                    }
                    self.state.discovered_files = files;
                }
                crate::core::model::ScanProgress::AdditionalFilesDiscovered { files } => {
                    // Append mode: extend the file list and assign colours.
                    for f in &files {
                        self.state.assign_file_colour(&f.path);
                    }
                    self.state.discovered_files.extend(files);
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
                    // Entries were sorted chronologically by the background
                    // thread before streaming, so only filters need applying.
                    self.state.apply_filters();
                    // Persist the session so the next launch can restore this state.
                    self.state.save_session();
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

        // Poll live tail progress.
        let tail_messages = self.tail_manager.poll_progress();
        let had_tail = !tail_messages.is_empty();
        for msg in tail_messages {
            match msg {
                crate::core::model::TailProgress::Started { file_count } => {
                    tracing::info!(files = file_count, "Live tail active");
                }
                crate::core::model::TailProgress::NewEntries { entries } => {
                    self.state.entries.extend(entries);
                    self.state.apply_filters();
                    if self.state.tail_auto_scroll {
                        self.state.tail_scroll_to_bottom = true;
                    }
                }
                crate::core::model::TailProgress::FileError { path, message } => {
                    let msg = format!("Tail warning — {}: {}", path.display(), message);
                    tracing::warn!("{}", msg);
                    self.state.warnings.push(msg);
                }
                crate::core::model::TailProgress::Stopped => {
                    self.state.tail_active = false;
                    self.state.status_message = "Live tail stopped.".to_string();
                    tracing::info!("Live tail stopped");
                }
            }
        }
        // Keep repainting while tail is active so new entries appear promptly.
        if had_tail || self.state.tail_active {
            ctx.request_repaint_after(std::time::Duration::from_millis(
                crate::util::constants::TAIL_POLL_INTERVAL_MS,
            ));
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
        // initial_scan: set at startup when restoring a previous session.
        // Unlike pending_scan, does NOT call clear() so the restored
        // filter/colour/bookmark state is preserved during the re-scan.
        if let Some(path) = self.state.initial_scan.take() {
            self.scan_manager.start_scan(
                path,
                self.state.profiles.clone(),
                DiscoveryConfig::default(),
            );
        }
        // pending_single_files: user chose "Add File(s)" — append to session.
        if let Some(files) = self.state.pending_single_files.take() {
            self.state.scan_in_progress = true;
            self.state.status_message = format!("Adding {} file(s)...", files.len());
            self.scan_manager
                .start_scan_files(files, self.state.profiles.clone());
        }
        // request_cancel: a panel requested the current scan be cancelled.
        if self.state.request_cancel {
            self.state.request_cancel = false;
            self.scan_manager.cancel_scan();
        }

        // request_start_tail: a panel wants to activate live tail.
        if self.state.request_start_tail {
            self.state.request_start_tail = false;
            // Build TailFileInfo list from discovered files that have a resolved profile,
            // *respecting the current source-file filter* so Live Tail only watches the
            // files the user has selected.
            //
            // Source-file filter semantics (mirrors apply_filters / filters.rs):
            //   hide_all_sources = true  => nothing passes ("None" was pressed)
            //   source_files empty       => all files pass (no filter set)
            //   source_files non-empty   => only listed paths pass
            let hide_all = self.state.filter_state.hide_all_sources;
            let source_filter = self.state.filter_state.source_files.clone();
            let files: Vec<crate::app::tail::TailFileInfo> = self
                .state
                .discovered_files
                .iter()
                .filter(|f| {
                    if hide_all {
                        return false;
                    }
                    if !source_filter.is_empty() && !source_filter.contains(&f.path) {
                        return false;
                    }
                    true
                })
                .filter_map(|f| {
                    let profile_id = f.profile_id.as_ref()?;
                    let profile = self
                        .state
                        .profiles
                        .iter()
                        .find(|p| &p.id == profile_id)?
                        .clone();
                    Some(crate::app::tail::TailFileInfo {
                        path: f.path.clone(),
                        profile,
                    })
                })
                .collect();
            if files.is_empty() {
                self.state.status_message =
                    "No watchable files — run a scan first, or check your file filter.".to_string();
            } else {
                let watching = files.len();
                let start_id = self.state.next_entry_id();
                self.tail_manager.start_tail(files, start_id);
                self.state.tail_active = true;
                self.state.status_message =
                    format!("Live tail active — watching {watching} file(s).");
            }
        }

        // request_stop_tail: a panel wants to stop live tail.
        if self.state.request_stop_tail {
            self.state.request_stop_tail = false;
            self.tail_manager.stop_tail();
            self.state.tail_active = false;
            self.state.status_message = "Live tail stopped.".to_string();
        }

        // Top menu bar
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Open Directory\u{2026}").clicked() {
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
                    if ui.button("Add File(s)\u{2026}").clicked() {
                        if let Some(files) = rfd::FileDialog::new()
                            .add_filter("Log files", &["log", "txt", "log.1", "log.2", "log.3"])
                            .pick_files()
                        {
                            self.state.pending_single_files = Some(files);
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
                    let has_entries = !self.state.filtered_indices.is_empty();
                    ui.add_enabled_ui(has_entries, |ui| {
                        if ui.button("Log Summary").clicked() {
                            self.state.show_log_summary = true;
                            ui.close_menu();
                        }
                    });
                    ui.separator();
                    let has_bookmarks = self.state.bookmark_count() > 0;
                    ui.add_enabled_ui(has_bookmarks, |ui| {
                        let bm_label = format!(
                            "Copy Bookmark Report ({} entries)",
                            self.state.bookmark_count()
                        );
                        if ui.button(bm_label).clicked() {
                            let report = self.state.bookmarks_report();
                            ctx.copy_text(report);
                            self.state.status_message = format!(
                                "Copied bookmark report ({} entries) to clipboard.",
                                self.state.bookmark_count()
                            );
                            ui.close_menu();
                        }
                    });
                    // Copy all currently-filtered entries as a plain-text report.
                    // Disabled when no filtered entries exist (Rule 16).
                    ui.add_enabled_ui(has_entries, |ui| {
                        let n = self.state.filtered_indices.len();
                        let copy_label = format!("Copy Filtered Results ({n} entries)");
                        if ui.button(copy_label).clicked() {
                            let report = self.state.filtered_results_report();
                            ctx.copy_text(report);
                            self.state.status_message =
                                format!("Copied {n} filtered entries to clipboard.");
                            ui.close_menu();
                        }
                    });
                });
            });
        });

        // Status bar
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                // LIVE badge — shown while tail is active.
                if self.state.tail_active {
                    ui.label(
                        egui::RichText::new(" \u{25cf} LIVE ")
                            .strong()
                            .color(egui::Color32::from_rgb(34, 197, 94)) // Green 500
                            .background_color(egui::Color32::from_rgba_premultiplied(
                                34, 197, 94, 30,
                            )),
                    );
                    ui.separator();
                }
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

        // Left sidebar — two independent scroll areas so the file list and
        // filter controls each get proportional room and scroll independently.
        egui::SidePanel::left("sidebar")
            .default_width(ui::theme::SIDEBAR_WIDTH)
            .resizable(true)
            .show(ctx, |ui| {
                let available = ui.available_height();
                // Discovery section: top ~45 % of the sidebar.
                egui::ScrollArea::vertical()
                    .id_salt("sidebar_discovery")
                    .max_height(available * 0.45)
                    .show(ui, |ui| {
                        ui::panels::discovery::render(ui, &mut self.state);
                    });

                ui.separator();

                // Filters section: remaining space.
                egui::ScrollArea::vertical()
                    .id_salt("sidebar_filters")
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        ui::panels::filters::render(ui, &mut self.state);
                    });
            });

        // Central panel (timeline)
        egui::CentralPanel::default().show(ctx, |ui| {
            ui::panels::timeline::render(ui, &mut self.state);
        });

        // Summary dialogs (modal-ish)
        ui::panels::summary::render(ctx, &mut self.state);
        ui::panels::log_summary::render(ctx, &mut self.state);
    }

    /// Called by eframe when the application window is about to close.
    ///
    /// Saves the current session so the next launch can restore it.
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.state.save_session();
    }
}
