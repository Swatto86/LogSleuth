// LogSleuth - app.rs
//
// Top-level eframe::App implementation.
// Wires together all UI panels and manages the scan lifecycle.

use crate::app::scan::ScanManager;
use crate::app::state::AppState;
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
        for msg in self.scan_manager.poll_progress() {
            match msg {
                crate::core::model::ScanProgress::DiscoveryStarted => {
                    self.state.status_message = "Discovering files...".to_string();
                    self.state.scan_in_progress = true;
                }
                crate::core::model::ScanProgress::DiscoveryCompleted { total_files } => {
                    self.state.status_message =
                        format!("Discovery complete: {total_files} files found.");
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
                _ => {}
            }
        }

        // Top menu bar
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Open Directory...").clicked() {
                        if let Some(path) = rfd::FileDialog::new().pick_folder() {
                            self.state.scan_path = Some(path.clone());
                            self.state.clear();
                            self.scan_manager.start_scan(path);
                        }
                        ui.close_menu();
                    }
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
