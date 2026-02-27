// LogSleuth - gui.rs
//
// Top-level eframe::App implementation.
// Wires together all UI panels and manages the scan lifecycle.

use crate::app::dir_watcher::{DirWatchConfig, DirWatcher};
use crate::app::scan::ScanManager;
use crate::app::state::AppState;
use crate::app::tail::TailManager;
use crate::core::discovery::DiscoveryConfig;
use crate::ui;
use crate::util::constants::{
    MAX_DIR_WATCH_MESSAGES_PER_FRAME, MAX_SCAN_MESSAGES_PER_FRAME, MAX_TAIL_MESSAGES_PER_FRAME,
    MAX_WARNINGS,
};

/// The LogSleuth application.
pub struct LogSleuthApp {
    pub state: AppState,
    pub scan_manager: ScanManager,
    pub tail_manager: TailManager,
    /// Background thread that polls the scan directory for newly created log
    /// files and reports them so they can be appended to the live session.
    pub dir_watcher: DirWatcher,
}

impl LogSleuthApp {
    /// Create a new application instance with the given state.
    pub fn new(state: AppState) -> Self {
        Self {
            state,
            scan_manager: ScanManager::new(),
            tail_manager: TailManager::new(),
            dir_watcher: DirWatcher::new(),
        }
    }
}

impl eframe::App for LogSleuthApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Apply the user's chosen theme every frame (cheap; egui diffs internally).
        if self.state.dark_mode {
            ctx.set_visuals(egui::Visuals::dark());
        } else {
            ctx.set_visuals(egui::Visuals::light());
        }

        // Apply the user-selected font size every frame.
        // All five standard TextStyles are scaled proportionally from the body
        // size so headings, buttons, small text, and monospace log entries all
        // update together.  egui diffs the Style internally so this is cheap
        // when the value has not changed.
        {
            let size = self.state.ui_font_size;
            let mut style = (*ctx.style()).clone();
            use egui::{FontFamily, FontId, TextStyle};
            style.text_styles = [
                (
                    TextStyle::Small,
                    FontId::new((size * 0.75).max(8.0), FontFamily::Proportional),
                ),
                (TextStyle::Body, FontId::new(size, FontFamily::Proportional)),
                (
                    TextStyle::Button,
                    FontId::new(size, FontFamily::Proportional),
                ),
                (
                    TextStyle::Heading,
                    FontId::new((size * 1.30).round(), FontFamily::Proportional),
                ),
                (
                    TextStyle::Monospace,
                    FontId::new(size, FontFamily::Monospace),
                ),
            ]
            .into();
            ctx.set_style(style);
        }

        // Poll for scan progress (capped at MAX_SCAN_MESSAGES_PER_FRAME so a
        // burst of queued messages cannot stall the render loop — Rule 11).
        let messages = self.scan_manager.poll_progress(MAX_SCAN_MESSAGES_PER_FRAME);
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
                crate::core::model::ScanProgress::DiscoveryCompleted {
                    total_files,
                    total_found,
                } => {
                    self.state.total_files_found = total_found;
                    self.state.status_message = if total_found > total_files {
                        format!(
                            "Discovery complete: {total_files} of {total_found} files loaded (most recently modified)."
                        )
                    } else {
                        format!("Discovery complete: {total_files} files found.")
                    };
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
                    // Cap total entries at the configured limit on the UI thread as
                    // well as on the background thread.  This matters for append
                    // scans where the UI already holds entries from a previous scan
                    // and the background thread cannot know the prior count.
                    let cap = self.state.max_total_entries;
                    let current = self.state.entries.len();
                    let remaining = cap.saturating_sub(current);
                    if remaining > 0 {
                        if entries.len() <= remaining {
                            self.state.entries.extend(entries);
                        } else {
                            self.state
                                .entries
                                .extend(entries.into_iter().take(remaining));
                        }
                    }
                }
                crate::core::model::ScanProgress::ParsingCompleted { summary } => {
                    let truncated = self.state.total_files_found > summary.total_files_discovered;
                    self.state.status_message = if truncated {
                        format!(
                            "Scan complete: {} entries from {} files in {:.2}s  \u{2014}  {}/{} files loaded (raise limit in Options to load more)",
                            summary.total_entries,
                            summary.files_matched,
                            summary.duration.as_secs_f64(),
                            summary.total_files_discovered,
                            self.state.total_files_found,
                        )
                    } else {
                        format!(
                            "Scan complete: {} entries from {} files in {:.2}s",
                            summary.total_entries,
                            summary.files_matched,
                            summary.duration.as_secs_f64()
                        )
                    };
                    self.state.scan_summary = Some(summary);
                    self.state.scan_in_progress = false;
                    // Sort chronologically before applying filters.
                    // For a fresh scan the entries are already sorted by the background
                    // thread (cheap timsort no-op).  For append scans the new entries
                    // are pre-sorted among themselves but must be interleaved with the
                    // existing sorted entries — sort_entries_chronologically handles both
                    // cases correctly and calls apply_filters() when done.
                    self.state.sort_entries_chronologically();
                    // Persist the session so the next launch can restore this state.
                    self.state.save_session();

                    // (Re-)start the recursive directory watcher whenever a scan over
                    // a known directory completes.  This covers both the initial scan
                    // and any append scan triggered by the watcher itself, keeping the
                    // watcher's known-paths set in sync with the current session.
                    //
                    // Only started for directory sessions (scan_path is Some).
                    // File-only sessions (pending_replace_files) clear scan_path first,
                    // so they correctly skip this branch.
                    if let Some(ref dir) = self.state.scan_path.clone() {
                        let known: std::collections::HashSet<std::path::PathBuf> = self
                            .state
                            .discovered_files
                            .iter()
                            .map(|f| f.path.clone())
                            .collect();
                        self.dir_watcher.start_watch(
                            dir.clone(),
                            known,
                            DirWatchConfig {
                                poll_interval_ms: self.state.dir_watch_poll_interval_ms,
                                // Do NOT forward modified_since to the watcher.  The date
                                // filter governs which existing files are loaded on the
                                // initial scan.  For live watching the rule is simpler: any
                                // file not yet in known_paths is genuinely new and must be
                                // added.  Applying the mtime gate here causes silent misses
                                // when the remote server's clock is behind the filter cutoff,
                                // or when a pre-existing file starts receiving new writes
                                // after the scan ran.
                                modified_since: None,
                                ..DirWatchConfig::default()
                            },
                        );
                        self.state.dir_watcher_active = true;
                        tracing::info!(dir = %dir.display(), "Directory watcher (re)started after scan");
                    }

                    // If Live Tail was active before the append scan completed,
                    // restart it so the newly added files are watched too.
                    if self.state.tail_active {
                        self.tail_manager.stop_tail();
                        self.state.request_start_tail = true;
                    }
                }
                crate::core::model::ScanProgress::Warning { message } => {
                    // Bounded push: prevent the warnings Vec from growing beyond
                    // MAX_WARNINGS (Rule 11 — resource bounds on growing collections).
                    if self.state.warnings.len() < MAX_WARNINGS {
                        self.state.warnings.push(message);
                    } else {
                        tracing::warn!("MAX_WARNINGS reached; suppressing further scan warnings");
                    }
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

        // Poll live tail progress (capped at MAX_TAIL_MESSAGES_PER_FRAME — Rule 11).
        let tail_messages = self.tail_manager.poll_progress(MAX_TAIL_MESSAGES_PER_FRAME);
        let had_tail = !tail_messages.is_empty();
        for msg in tail_messages {
            match msg {
                crate::core::model::TailProgress::Started { file_count } => {
                    tracing::info!(files = file_count, "Live tail active");
                }
                crate::core::model::TailProgress::NewEntries { entries } => {
                    // Cap total entries so live tail cannot grow state.entries past
                    // the configured limit (Rule 11 — OOM guard for unbounded tail).
                    let cap = self.state.max_total_entries;
                    let current = self.state.entries.len();
                    let remaining = cap.saturating_sub(current);
                    if remaining == 0 {
                        tracing::warn!(
                            count = entries.len(),
                            "Tail entry cap reached; new entries discarded"
                        );
                    } else if entries.len() <= remaining {
                        self.state.entries.extend(entries);
                    } else {
                        self.state
                            .entries
                            .extend(entries.into_iter().take(remaining));
                        tracing::warn!(
                            "Tail entry cap reached; batch truncated to fit remaining capacity"
                        );
                    }
                    self.state.apply_filters();
                    // In descending (newest-first) sort mode, new entries appear at
                    // display_idx 0 (the top of the viewport).  Request a scroll-to-top
                    // so the user sees them without needing to scroll up manually.
                    if self.state.sort_descending && self.state.tail_auto_scroll {
                        self.state.scroll_top_requested = true;
                    }
                }
                crate::core::model::TailProgress::FileError { path, message } => {
                    let msg = format!("Tail warning — {}: {}", path.display(), message);
                    tracing::warn!("{}", msg);
                    if self.state.warnings.len() < MAX_WARNINGS {
                        self.state.warnings.push(msg);
                    }
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
                self.state.tail_poll_interval_ms,
            ));
        }

        // Poll directory watcher for newly-created files (capped at
        // MAX_DIR_WATCH_MESSAGES_PER_FRAME — Rule 11).
        let dir_watch_messages = self
            .dir_watcher
            .poll_progress(MAX_DIR_WATCH_MESSAGES_PER_FRAME);
        for msg in dir_watch_messages {
            match msg {
                crate::core::model::DirWatchProgress::NewFiles(paths) => {
                    let count = paths.len();
                    tracing::info!(count, "Directory watcher: adding new files to session");
                    self.state.status_message =
                        format!("Directory watcher: {count} new file(s) detected, adding...");
                    self.state.scan_in_progress = true;
                    // Pass next_entry_id() so appended entries never reuse IDs
                    // already assigned during the initial scan (bookmarks /
                    // correlation use entry IDs as stable keys).
                    let id_start = self.state.next_entry_id();
                    self.scan_manager.start_scan_files(
                        paths,
                        self.state.profiles.clone(),
                        self.state.max_total_entries,
                        id_start,
                    );
                }
                crate::core::model::DirWatchProgress::WalkStarted => {
                    tracing::debug!("Directory watcher: walk cycle started");
                    self.state.dir_watcher_scanning = true;
                }
                crate::core::model::DirWatchProgress::WalkComplete { new_count } => {
                    tracing::debug!(new_count, "Directory watcher: walk cycle complete");
                    self.state.dir_watcher_scanning = false;
                    if new_count == 0 {
                        // No new files: update status so it's clear the watcher
                        // is alive and the directory is up-to-date.
                        let now = chrono::Local::now().format("%H:%M:%S");
                        self.state.status_message =
                            format!("Directory watcher: up to date (checked {now})");
                    }
                    // If new_count > 0 the NewFiles handler already set a more
                    // informative status message — don't overwrite it here.
                }
                crate::core::model::DirWatchProgress::FileMtimeUpdates(updates) => {
                    // Update the cached mtime on each DiscoveredFile so the
                    // file list in the discovery panel shows a live timestamp
                    // that refreshes whenever the file is written to.
                    for (path, mtime) in updates {
                        if let Some(f) = self
                            .state
                            .discovered_files
                            .iter_mut()
                            .find(|f| f.path == path)
                        {
                            f.modified = Some(mtime);
                        }
                    }
                }
            }
        }
        // Keep repainting while the dir watcher is active so new files appear
        // promptly (poll at the same cadence as the watcher thread).
        if self.state.dir_watcher_active {
            ctx.request_repaint_after(std::time::Duration::from_millis(
                self.state.dir_watch_poll_interval_ms,
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
            // Stop any current dir watcher before starting the new scan.
            // It will be restarted automatically when ParsingCompleted fires.
            self.dir_watcher.stop_watch();
            self.state.dir_watcher_active = false;
            // Capture the date filter BEFORE clear() — clear() does not reset
            // discovery_date_input intentionally (user preference, not scan state).
            let modified_since = self.state.discovery_modified_since();
            self.state.clear();
            self.state.scan_path = Some(path.clone());
            self.scan_manager.start_scan(
                path,
                self.state.profiles.clone(),
                DiscoveryConfig {
                    max_files: self.state.max_files_limit,
                    max_depth: self.state.max_scan_depth,
                    max_total_entries: self.state.max_total_entries,
                    modified_since,
                    ..DiscoveryConfig::default()
                },
            );
        }
        // initial_scan: set at startup when restoring a previous session.
        // Unlike pending_scan, does NOT call clear() so the restored
        // filter/colour/bookmark state is preserved during the re-scan.
        if let Some(path) = self.state.initial_scan.take() {
            let modified_since = self.state.discovery_modified_since();
            self.scan_manager.start_scan(
                path,
                self.state.profiles.clone(),
                DiscoveryConfig {
                    max_files: self.state.max_files_limit,
                    max_depth: self.state.max_scan_depth,
                    max_total_entries: self.state.max_total_entries,
                    modified_since,
                    ..DiscoveryConfig::default()
                },
            );
        }
        // pending_replace_files: user chose "Open Log(s)..." — clear and load selected files.
        if let Some(files) = self.state.pending_replace_files.take() {
            // Explicitly stop the directory watcher and clear scan_path so the watcher
            // is NOT restarted after the file-only scan completes (Rule 17 pre-flight).
            self.dir_watcher.stop_watch();
            self.state.dir_watcher_active = false;
            self.state.clear();
            // scan_path must be None for file-only sessions so the dir watcher is
            // not started in the ParsingCompleted handler.
            self.state.scan_path = None;
            self.state.scan_in_progress = true;
            self.state.status_message = format!("Opening {} file(s)...", files.len());
            // State was just cleared so next_entry_id() is 0; pass it explicitly
            // for clarity and future-proofing.
            self.scan_manager.start_scan_files(
                files,
                self.state.profiles.clone(),
                self.state.max_total_entries,
                0,
            );
        }
        // pending_single_files: user chose "Add File(s)" — append to session.
        if let Some(files) = self.state.pending_single_files.take() {
            self.state.scan_in_progress = true;
            self.state.status_message = format!("Adding {} file(s)...", files.len());
            // Append scan: entry IDs must continue after the highest existing ID
            // so bookmarks and correlation are not confused by duplicate IDs.
            let id_start = self.state.next_entry_id();
            self.scan_manager.start_scan_files(
                files,
                self.state.profiles.clone(),
                self.state.max_total_entries,
                id_start,
            );
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
                        // Seed from file size at scan time so the first poll
                        // tick catches any entries appended after the scan
                        // completed but before tail was activated (the gap).
                        initial_offset: Some(f.size),
                    })
                })
                .collect();
            if files.is_empty() {
                self.state.status_message =
                    "No watchable files — run a scan first, or check your file filter.".to_string();
            } else {
                let watching = files.len();
                let start_id = self.state.next_entry_id();
                self.tail_manager
                    .start_tail(files, start_id, self.state.tail_poll_interval_ms);
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

        // request_new_session: reset everything and return to the blank initial state.
        if self.state.request_new_session {
            self.state.request_new_session = false;
            // Stop any in-progress tail watcher first.
            if self.state.tail_active {
                self.tail_manager.stop_tail();
            }
            // Stop the directory watcher.
            self.dir_watcher.stop_watch();
            self.state.dir_watcher_active = false;
            // Cancel any in-flight scan so the background thread stops sending
            // EntriesBatch / ParsingCompleted messages that would immediately
            // re-populate the state we are about to clear.
            if self.state.scan_in_progress {
                self.scan_manager.cancel_scan();
            }
            self.state.new_session();
            // Persist the blank state so the next launch starts fresh and does
            // not try to restore the old session directory.
            self.state.save_session();
        }

        // Top menu bar
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    let scanning = self.state.scan_in_progress;
                    let dir_btn =
                        ui.add_enabled(!scanning, egui::Button::new("Open Directory\u{2026}"));
                    if dir_btn
                        .on_hover_text(if scanning {
                            "Cannot open a directory while a scan is in progress"
                        } else {
                            "Scan a directory for log files"
                        })
                        .clicked()
                    {
                        if let Some(path) = rfd::FileDialog::new().pick_folder() {
                            self.dir_watcher.stop_watch();
                            self.state.dir_watcher_active = false;
                            // Capture the date filter BEFORE clear() so the user's
                            // setting is not lost.  clear() intentionally preserves
                            // discovery_date_input, but modified_since must be read
                            // before we wipe any other transient state.
                            let modified_since = self.state.discovery_modified_since();
                            self.state.clear();
                            self.state.scan_path = Some(path.clone());
                            self.scan_manager.start_scan(
                                path,
                                self.state.profiles.clone(),
                                DiscoveryConfig {
                                    max_files: self.state.max_files_limit,
                                    max_depth: self.state.max_scan_depth,
                                    max_total_entries: self.state.max_total_entries,
                                    modified_since,
                                    ..DiscoveryConfig::default()
                                },
                            );
                        }
                        ui.close_menu();
                    }
                    let open_logs_btn =
                        ui.add_enabled(!scanning, egui::Button::new("Open Log(s)\u{2026}"));
                    if open_logs_btn
                        .on_hover_text(if scanning {
                            "Cannot open files while a scan is in progress"
                        } else {
                            "Select individual log files to open as a new session"
                        })
                        .clicked()
                    {
                        if let Some(files) = rfd::FileDialog::new()
                            .add_filter("Log files", &["log", "txt", "log.1", "log.2", "log.3"])
                            .pick_files()
                        {
                            self.state.pending_replace_files = Some(files);
                        }
                        ui.close_menu();
                    }
                    let add_files_btn =
                        ui.add_enabled(!scanning, egui::Button::new("Add File(s)\u{2026}"));
                    if add_files_btn
                        .on_hover_text(if scanning {
                            "Cannot add files while a scan is in progress"
                        } else {
                            "Append individual log files to the current session"
                        })
                        .clicked()
                    {
                        if let Some(files) = rfd::FileDialog::new()
                            .add_filter("Log files", &["log", "txt", "log.1", "log.2", "log.3"])
                            .pick_files()
                        {
                            self.state.pending_single_files = Some(files);
                        }
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("New Session").clicked() {
                        self.state.request_new_session = true;
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
                ui.menu_button("Edit", |ui| {
                    if ui.button("Options\u{2026}").clicked() {
                        self.state.show_options = true;
                        ui.close_menu();
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
                // Right-aligned theme + ⓘ About buttons. Must come AFTER the
                // left-anchored menus; placing with_layout(right_to_left) first
                // would consume all available space and leave no room for File /
                // View / Edit to render.
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let about_btn = ui.add(
                        egui::Button::new(
                            egui::RichText::new(" \u{24d8} ")
                                .strong()
                                .color(egui::Color32::from_rgb(156, 163, 175)),
                        )
                        .frame(false),
                    );
                    if about_btn.on_hover_text("About LogSleuth").clicked() {
                        self.state.show_about = true;
                    }

                    // Theme toggle: show the icon for the mode you will switch TO.
                    let (icon, hint) = if self.state.dark_mode {
                        ("\u{2600}", "Switch to light mode") // ☀
                    } else {
                        ("\u{263d}", "Switch to dark mode") // ☽
                    };
                    let theme_btn = ui.add(
                        egui::Button::new(
                            egui::RichText::new(format!(" {icon} "))
                                .color(egui::Color32::from_rgb(156, 163, 175)),
                        )
                        .frame(false),
                    );
                    if theme_btn.on_hover_text(hint).clicked() {
                        self.state.dark_mode = !self.state.dark_mode;
                    }
                });
            });
        });

        // Local flag for WATCH badge toggle; consumed after the status bar closure
        // so that `&mut self` is available to start/stop the watcher.
        // Some(true) = resume, Some(false) = pause, None = no action.
        let mut watch_toggle: Option<bool> = None;

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
                // WATCH badge — shown while the directory watcher is active, or
                // dimmed when paused but a directory session is loaded.
                // Clicking toggles the watcher on/off.
                let has_dir_session = self.state.scan_path.is_some()
                    && !self.state.discovered_files.is_empty();
                if self.state.dir_watcher_active {
                    let watch_tip = if self.state.dir_watcher_scanning {
                        "Directory watch active — scanning for new files… (click to pause)"
                    } else {
                        "Directory watch active — click to pause"
                    };
                    let watch_label = if self.state.dir_watcher_scanning {
                        egui::RichText::new(" \u{1f441} WATCH ⋯ ")
                    } else {
                        egui::RichText::new(" \u{1f441} WATCH ")
                    };
                    if ui
                        .add(
                            egui::Button::new(
                                watch_label
                                    .strong()
                                    .color(egui::Color32::from_rgb(96, 165, 250)) // Blue 400
                                    .background_color(egui::Color32::from_rgba_premultiplied(
                                        96, 165, 250, 28,
                                    )),
                            )
                            .frame(false),
                        )
                        .on_hover_text(watch_tip)
                        .clicked()
                    {
                        watch_toggle = Some(false);
                    }
                    ui.separator();
                } else if has_dir_session {
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new(" \u{1f441} WATCH ")
                                    .strong()
                                    .color(egui::Color32::from_rgba_premultiplied(
                                        96, 165, 250, 110,
                                    )),
                            )
                            .frame(false),
                        )
                        .on_hover_text("Directory watch paused — click to resume")
                        .clicked()
                    {
                        watch_toggle = Some(true);
                    }
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
                        ui.separator();
                    }
                    // File count — amber when the ingest limit was applied.
                    let loaded = self.state.discovered_files.len();
                    if loaded > 0 {
                        let found = self.state.total_files_found;
                        if found > loaded {
                            ui.label(
                                egui::RichText::new(format!("{loaded}/{found} files"))
                                    .color(egui::Color32::from_rgb(251, 191, 36)),
                            )
                            .on_hover_text(format!(
                                "{found} files discovered; showing the {loaded} most recently modified. \
                                 Raise the limit in Edit > Options."
                            ));
                        } else {
                            ui.label(format!("{loaded} files"));
                        }
                        ui.separator();
                    }
                });
            });
        });

        // Handle WATCH badge toggle action (deferred so that &mut self is available
        // without conflicting with the status-bar panel closure borrow).
        if let Some(start) = watch_toggle {
            if start {
                if let Some(dir) = self.state.scan_path.clone() {
                    let known = self
                        .state
                        .discovered_files
                        .iter()
                        .map(|f| f.path.clone())
                        .collect();
                    self.dir_watcher.start_watch(
                        dir,
                        known,
                        DirWatchConfig {
                            poll_interval_ms: self.state.dir_watch_poll_interval_ms,
                            modified_since: None,
                            ..DirWatchConfig::default()
                        },
                    );
                    self.state.dir_watcher_active = true;
                    self.state.status_message = "Directory watch resumed.".to_string();
                    tracing::info!("Directory watch resumed by user");
                }
            } else {
                self.dir_watcher.stop_watch();
                self.state.dir_watcher_active = false;
                self.state.status_message = "Directory watch paused.".to_string();
                tracing::info!("Directory watch paused by user");
            }
        }

        // Detail pane (bottom)
        egui::TopBottomPanel::bottom("detail_pane")
            .resizable(true)
            .default_height(ui::theme::DETAIL_PANE_HEIGHT)
            .show(ctx, |ui| {
                ui::panels::detail::render(ui, &self.state);
            });

        // Left sidebar — tab-based, resizable.
        // 'Files' tab: collapsible scan controls + unified file list with
        //              inline source-file filter checkboxes and solo buttons.
        // 'Filters' tab: severity, text, regex, time, correlation controls.
        // Resizable so users can widen it when file names are long.
        egui::SidePanel::left("sidebar")
            .default_width(ui::theme::SIDEBAR_WIDTH)
            .min_width(300.0)
            .max_width(800.0)
            .resizable(true)
            .show(ctx, |ui| {
                // Tab strip — Files tab shows the file count as a badge.
                ui.horizontal(|ui| {
                    let files_label = if self.state.discovered_files.is_empty() {
                        "Files".to_string()
                    } else {
                        format!("Files ({})", self.state.discovered_files.len())
                    };
                    if ui
                        .selectable_label(self.state.sidebar_tab == 0, files_label)
                        .clicked()
                    {
                        self.state.sidebar_tab = 0;
                    }
                    // Show filter-active indicator on Filters tab label.
                    let filter_active = !self.state.filter_state.source_files.is_empty()
                        || self.state.filter_state.hide_all_sources
                        || self.state.filter_state.relative_time_secs.is_some()
                        || !self.state.filter_state.text_search.is_empty()
                        || !self.state.filter_state.regex_pattern.is_empty()
                        || self.state.filter_state.bookmarks_only
                        || self.state.filter_state.severity_levels.len()
                            != crate::core::model::Severity::all().len();
                    let filters_label = if filter_active {
                        "\u{25cf} Filters".to_string() // bullet dot = filter active
                    } else {
                        "Filters".to_string()
                    };
                    if ui
                        .selectable_label(self.state.sidebar_tab == 1, filters_label)
                        .clicked()
                    {
                        self.state.sidebar_tab = 1;
                    }
                });
                ui.separator();

                // Each tab gets the full remaining height in a single scroll area —
                // no cramped 45/55 split, no dual scrollbars.
                egui::ScrollArea::vertical()
                    .id_salt("sidebar_main")
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        if self.state.sidebar_tab == 0 {
                            ui::panels::discovery::render(ui, &mut self.state);
                        } else {
                            ui::panels::filters::render(ui, &mut self.state);
                        }
                    });
            });

        // Central panel (timeline)
        egui::CentralPanel::default().show(ctx, |ui| {
            ui::panels::timeline::render(ui, &mut self.state);
        });

        // Summary dialogs (modal-ish)
        ui::panels::summary::render(ctx, &mut self.state);
        ui::panels::log_summary::render(ctx, &mut self.state);
        ui::panels::about::render(ctx, &mut self.state);
        ui::panels::options::render(ctx, &mut self.state);

        // Activity window auto-advance: re-apply filters every second so the
        // rolling cutoff stays current and stale files/entries age out without
        // any user interaction.  The repaint_after keeps the UI responsive
        // without busy-looping at the full frame rate.
        if self.state.activity_window_secs.is_some() {
            self.state.apply_filters();
            ctx.request_repaint_after(std::time::Duration::from_secs(1));
        }
    }

    /// Called by eframe when the application window is about to close.
    ///
    /// Saves the current session so the next launch can restore it.
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // Stop background threads cleanly before the process exits.
        self.dir_watcher.stop_watch();
        self.tail_manager.stop_tail();
        self.state.save_session();
    }
}
