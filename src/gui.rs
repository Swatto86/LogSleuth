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
    FILTER_DEBOUNCE_MS, MAX_DIR_WATCH_MESSAGES_PER_FRAME, MAX_QUEUED_DIR_WATCHER_FILES,
    MAX_SCAN_MESSAGES_PER_FRAME, MAX_TAIL_MESSAGES_PER_FRAME, MAX_TAIL_WATCH_FILES, MAX_WARNINGS,
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
                    self.state.discovery_truncated = total_found > total_files;
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
                    // Update-or-append mode: if a file is already in the list
                    // (e.g. because it was previously discovered with parsing_skipped=true
                    // and the user triggered "Parse skipped files"), update its profile
                    // info and propagate the skipped flag rather than creating a duplicate row.
                    // Truly new files are appended as before.
                    for f in files {
                        self.state.assign_file_colour(&f.path);
                        if let Some(existing) = self
                            .state
                            .discovered_files
                            .iter_mut()
                            .find(|e| e.path == f.path)
                        {
                            // Update profile with whatever was detected (filename-only
                            // detection is fine for skipped files; full detection for
                            // actually-parsed files).
                            if f.profile_id.is_some() {
                                existing.profile_id = f.profile_id;
                                existing.detection_confidence = f.detection_confidence;
                            }
                            // Propagate the actual parsed status from the pipeline:
                            //   false = file was fully parsed (entries in memory)
                            //   true  = file was profiled only (entries NOT in memory)
                            //
                            // Previously this was unconditionally `= false`, which
                            // cleared the □ indicator and broke the re-parse trigger
                            // for dir-watcher / session-restore discover-only scans.
                            existing.parsing_skipped = f.parsing_skipped;
                        } else {
                            self.state.discovered_files.push(f);
                        }
                    }
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
                        // Track max entry ID incrementally (Bug fix: O(1) instead
                        // of O(n) full scan in next_entry_id).
                        self.state.track_max_entry_id(&entries);
                        // Track entries without parsed timestamps so the mtime-update
                        // path in FileMtimeUpdates can be skipped when unneeded.
                        if entries.len() <= remaining {
                            self.state.track_notimestamp_entries(&entries);
                            self.state.entries.extend(entries);
                        } else {
                            let to_add = &entries[..remaining];
                            self.state.track_notimestamp_entries(to_add);
                            self.state
                                .entries
                                .extend(entries.into_iter().take(remaining));
                        }
                    }
                }
                crate::core::model::ScanProgress::ParsingCompleted { summary } => {
                    // Use the flag set by DiscoveryCompleted rather than
                    // comparing total_files_found vs summary.total_files_discovered.
                    // For append scans (start_scan_files) DiscoveryCompleted is never
                    // sent, so discovery_truncated remains false and we avoid a
                    // false-positive truncation message caused by a stale
                    // total_files_found from the initial directory scan.
                    let truncated = self.state.discovery_truncated;
                    // Profile-only scans (dir-watcher discovers, post-initial
                    // queued-file discovers) complete with files_matched == 0 and
                    // total_entries == 0 because the parse-path filter was
                    // Some(empty) — no file content was read.  Showing
                    // "0 entries from 0 files" in these cases is misleading; it
                    // overwrites the helpful "N files discovered — tick checkboxes"
                    // message and makes the user think checking boxes did nothing.
                    //
                    // When total_entries == 0 AND files_matched == 0 AND
                    // total_files_discovered > 0, all files were profiled-only.
                    // Show an actionable message instead.
                    //
                    // The fresh_scan_in_progress block below (for the initial
                    // directory scan) overrides this anyway, so the only case
                    // this fires is for subsequent profile-only appends
                    // (dir-watcher / queued discovers).
                    let all_profiled_only = summary.total_entries == 0
                        && summary.files_matched == 0
                        && summary.total_files_discovered > 0;
                    self.state.status_message = if all_profiled_only
                        && !self.state.fresh_scan_in_progress
                    {
                        let n = summary.total_files_discovered;
                        let word = if n == 1 { "file" } else { "files" };
                        format!(
                            "{n} {word} discovered \u{2014} tick checkboxes in the \
                             Files tab to load entries."
                        )
                    } else if truncated {
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
                    // Opt-in model: after an interactive fresh scan, default the
                    // file list to nothing-checked so the user explicitly selects
                    // which files to view.  Applied BEFORE sort_entries_chrono-
                    // logically() so apply_filters() (called inside it) sees the
                    // correct hide_all state immediately.
                    //
                    // For directory scans (pending_scan) all files are
                    // parsing_skipped=true at this point because parse_path_filter
                    // was an empty HashSet; entries is therefore empty and the
                    // generic "0 entries from 0 files" status is replaced with a
                    // helpful "tick files to load" prompt.
                    //
                    // For explicit file opens (pending_replace_files) entries are
                    // in memory but hidden; the user sees State 2 of the timeline
                    // empty-state hint.
                    if self.state.fresh_scan_in_progress {
                        self.state.fresh_scan_in_progress = false;
                        self.state.filter_state.hide_all_sources = true;
                        self.state.filter_state.source_files.clear();
                        if self.state.entries.is_empty() {
                            let n = self.state.discovered_files.len();
                            let word = if n == 1 { "file" } else { "files" };
                            self.state.status_message = format!(
                                "{n} {word} discovered \u{2014} tick files in the \
                                 Files tab to load their entries."
                            );
                        }
                    }
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
                                max_depth: self.state.max_scan_depth,
                                // Forward modified_since so the watcher applies the same
                                // date gate as the initial scan.  Without this, files that
                                // predate the filter are not in known_paths (the scan never
                                // included them) so the watcher sees every old file as
                                // "newly created" and floods the file list with them.
                                // This is the primary cause of out-of-filter files
                                // appearing in the Files panel after setting a date filter.
                                modified_since: self.state.discovery_modified_since(),
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

                    // Session restore: re-add any "Add File(s)..." files that were
                    // saved in the previous session but are not already in the
                    // discovered set (the directory scan may have found them too).
                    // Uses std::mem::take so extra_files_to_restore is cleared
                    // immediately and a second ParsingCompleted (from the append
                    // scan itself) does not re-trigger the same add.
                    if !self.state.extra_files_to_restore.is_empty() {
                        let known: std::collections::HashSet<std::path::PathBuf> = self
                            .state
                            .discovered_files
                            .iter()
                            .map(|f| f.path.clone())
                            .collect();
                        let extra: Vec<std::path::PathBuf> =
                            std::mem::take(&mut self.state.extra_files_to_restore)
                                .into_iter()
                                .filter(|p| !known.contains(p))
                                .collect();
                        if !extra.is_empty() {
                            tracing::info!(
                                count = extra.len(),
                                "Session restore: re-adding extra files from previous session"
                            );
                            let id_start = self.state.next_entry_id();
                            self.state.scan_in_progress = true;
                            self.state.status_message = format!(
                                "Restoring {} extra file(s) from previous session...",
                                extra.len()
                            );
                            self.scan_manager.start_scan_files(
                                extra,
                                self.state.profiles.clone(),
                                self.state.max_total_entries,
                                id_start,
                                None, // session-restore: parse the files
                            );
                        }
                    }

                    // Drain any user-requested parse files queued while a scan
                    // was in progress (checkbox ticked on a parsing_skipped file
                    // while a scan was already running).  These must be drained
                    // before the dir-watcher discover queue so user intent is
                    // served first, and with None filter so entries are loaded.
                    if !self.state.scan_in_progress && !self.state.queued_parse_files.is_empty() {
                        let queued = std::mem::take(&mut self.state.queued_parse_files);
                        let count = queued.len();
                        tracing::info!(
                            count,
                            "Parsing queued user-checkbox files after scan completed"
                        );
                        let id_start = self.state.next_entry_id();
                        self.state.scan_in_progress = true;
                        self.state.status_message = format!("Parsing {count} queued file(s)...");
                        self.scan_manager.start_scan_files(
                            queued,
                            self.state.profiles.clone(),
                            self.state.max_total_entries,
                            id_start,
                            None, // user explicitly checked — parse all entries
                        );
                    }

                    // Drain any dir-watcher files that were queued while this
                    // (or a previous) scan was running.  Only start when no
                    // other scan was just kicked off above (extra_files check
                    // or queued_parse_files drain may have set scan_in_progress).
                    if !self.state.scan_in_progress
                        && !self.state.queued_dir_watcher_files.is_empty()
                    {
                        let queued = std::mem::take(&mut self.state.queued_dir_watcher_files);
                        let count = queued.len();
                        tracing::info!(
                            count,
                            "Processing queued dir-watcher files after scan completed"
                        );
                        let id_start = self.state.next_entry_id();
                        self.state.scan_in_progress = true;
                        self.state.status_message = format!(
                            "Directory watcher: {count} new file(s) discovered — tick in Files tab to load entries."
                        );
                        // Opt-in model: newly-discovered files are profiled but not
                        // parsed.  The user ticks their checkbox to load entries.
                        // This matches the initial pending_scan behaviour and prevents
                        // silent RAM growth from auto-parsed files the user never opens.
                        self.scan_manager.start_scan_files(
                            queued,
                            self.state.profiles.clone(),
                            self.state.max_total_entries,
                            id_start,
                            Some(std::collections::HashSet::new()),
                        );
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
        // `entries_changed` tracks whether new entries were added this frame.
        // `entries_need_full_sort` is set when a full sort + filter rebuild is
        // required (eviction happened, or new entries arrived out of timestamp
        // order).  When only fast-path appends happened, we skip the O(n log n)
        // sort entirely — `extend_filtered_for_range` already updated the index.
        let mut entries_changed = false;
        let mut entries_need_full_sort = false;
        let tail_messages = self.tail_manager.poll_progress(MAX_TAIL_MESSAGES_PER_FRAME);
        let had_tail = !tail_messages.is_empty();
        for msg in tail_messages {
            match msg {
                crate::core::model::TailProgress::Started { file_count } => {
                    tracing::info!(files = file_count, "Live tail active");
                }
                crate::core::model::TailProgress::NewEntries { entries } => {
                    if entries.is_empty() {
                        continue;
                    }

                    // ---------------------------------------------------------
                    // Ring-buffer eviction (Fix A — RAM runaway prevention)
                    //
                    // Evict the oldest tail entries when adding `incoming`
                    // entries would push the tail section past
                    // `max_tail_buffer_entries`.  Entries from the initial scan
                    // (indices < tail_base_count) are NEVER evicted.
                    //
                    // `evict_tail_entries` drains the front of the tail section
                    // and recounts `notimestamp_entry_count` from scratch, so
                    // after eviction `filtered_indices` is stale and must be
                    // fully rebuilt via `sort_entries_chronologically`.
                    // ---------------------------------------------------------
                    let incoming = entries.len();
                    let tail_base = self.state.tail_base_count;
                    let tail_cap = self.state.max_tail_buffer_entries;
                    let current_tail_len = self.state.entries.len().saturating_sub(tail_base);
                    let mut post_eviction_rebuild = false;

                    if current_tail_len + incoming > tail_cap {
                        let to_evict = (current_tail_len + incoming).saturating_sub(tail_cap);
                        let evicted = self.state.evict_tail_entries(to_evict);
                        if evicted > 0 {
                            tracing::debug!(
                                evicted,
                                tail_cap,
                                "Tail: ring-buffer eviction — oldest tail entries removed"
                            );
                            post_eviction_rebuild = true;
                        }
                    }

                    // ---------------------------------------------------------
                    // Fast-path: sorted append (Fix B — avoid O(n log n) sort)
                    //
                    // When all incoming entries have timestamps >= the last
                    // existing entry's timestamp — the 99 % case for active log
                    // files — we can skip the full sort and update
                    // `filtered_indices` incrementally (Fix C — O(N) instead
                    // of O(M) filter rebuild).
                    //
                    // Conditions that force the slow path:
                    //   - Eviction happened (indices shifted → full rebuild).
                    //   - Either end has no parsed timestamp (cannot compare).
                    //   - First incoming entry is older than the last existing
                    //     entry (rare: multi-file tail with skewed clocks, or a
                    //     file that was rotated and resumed from offset 0).
                    // ---------------------------------------------------------
                    let last_ts = self.state.entries.last().and_then(|e| e.timestamp);
                    let first_new_ts = entries.first().and_then(|e| e.timestamp);
                    let can_fast_append = !post_eviction_rebuild
                        && matches!((last_ts, first_new_ts),
                            (Some(last), Some(first)) if first >= last);

                    let base_count = self.state.entries.len();
                    self.state.track_max_entry_id(&entries);
                    self.state.track_notimestamp_entries(&entries);
                    self.state.entries.extend(entries);

                    if can_fast_append {
                        // Fast path: extend filtered_indices for the new suffix
                        // only — O(N) where N = incoming entries.  The full
                        // sort_entries_chronologically (O(M log M)) is skipped.
                        self.state.extend_filtered_for_range(base_count);
                        // Do NOT set entries_need_full_sort; the list is already
                        // correct for this batch.
                    } else {
                        // Slow path: interleaved timestamps or post-eviction.
                        // sort_entries_chronologically rebuilds everything.
                        entries_need_full_sort = true;
                    }
                    entries_changed = true;

                    // In descending (newest-first) sort mode, new entries appear
                    // at display_idx 0 (the top of the viewport).  Request a
                    // scroll-to-top so the user sees them without scrolling up.
                    if self.state.sort_descending && self.state.tail_auto_scroll {
                        self.state.scroll_top_requested = true;
                    }
                }
                crate::core::model::TailProgress::FileError { path, message } => {
                    let msg = format!("Tail warning - {}: {}", path.display(), message);
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
        // Consolidate sort + filter after processing all tail messages this frame.
        //
        // `entries_need_full_sort` is set only when the slow path ran (eviction
        // or out-of-order timestamps).  For all-fast-path frames this block is a
        // no-op, saving the O(n log n) sort and O(n) filter rebuild that were
        // previously triggered on every single tail tick even when entries only
        // arrived in perfectly ascending time order.
        if entries_need_full_sort {
            self.state.sort_entries_chronologically();
            // sort_entries_chronologically calls apply_filters internally.
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
                    // Bug fix: if a scan is already in progress, queue the paths
                    // rather than starting a new scan.  `start_scan_files()` calls
                    // `cancel_scan()` internally, which would kill the in-flight
                    // scan and drop any `EntriesBatch` messages still in the old
                    // channel — silently losing entries from files that hadn't
                    // finished parsing yet.  The queue is drained in the next
                    // `ParsingCompleted` handler.
                    if self.state.scan_in_progress {
                        // Rule 11: bounded queue — cap to prevent unbounded growth
                        // in high-file-creation-rate directories.
                        let current = self.state.queued_dir_watcher_files.len();
                        let remaining = MAX_QUEUED_DIR_WATCHER_FILES.saturating_sub(current);
                        if remaining == 0 {
                            tracing::warn!(
                                dropped = count,
                                cap = MAX_QUEUED_DIR_WATCHER_FILES,
                                "Dir-watcher queue full; dropping new-file batch \
                                 (next walk cycle will re-discover missed files)"
                            );
                        } else {
                            let to_queue = paths.into_iter().take(remaining).collect::<Vec<_>>();
                            let queued_count = to_queue.len();
                            self.state.queued_dir_watcher_files.extend(to_queue);
                            tracing::info!(
                                queued_count,
                                queued_total = self.state.queued_dir_watcher_files.len(),
                                "Scan in progress; queueing new dir-watcher files"
                            );
                        }
                    } else {
                        tracing::info!(
                            count,
                            "Directory watcher: new file(s) discovered, profiling without parsing"
                        );
                        self.state.status_message = format!(
                            "Directory watcher: {count} new file(s) discovered — tick in Files tab to load entries."
                        );
                        self.state.scan_in_progress = true;
                        // Pass next_entry_id() so appended entries never reuse IDs
                        // already assigned during the initial scan (bookmarks /
                        // correlation use entry IDs as stable keys).
                        let id_start = self.state.next_entry_id();
                        // Opt-in model: newly-discovered files are profiled but not
                        // fully parsed.  Passing Some(empty) as parse_path_filter
                        // causes the scan pipeline to run filename-only profile
                        // detection for each file and set parsing_skipped=true so
                        // the file row shows the \u25a1 indicator.  Entries are loaded
                        // only when the user ticks the checkbox (pending_single_files).
                        // This matches the initial pending_scan behaviour and prevents
                        // unchecked files from consuming RAM.
                        self.scan_manager.start_scan_files(
                            paths,
                            self.state.profiles.clone(),
                            self.state.max_total_entries,
                            id_start,
                            Some(std::collections::HashSet::new()),
                        );
                    }
                }
                crate::core::model::DirWatchProgress::WalkStarted => {
                    tracing::debug!("Directory watcher: walk cycle started");
                    self.state.dir_watcher_scanning = true;
                }
                crate::core::model::DirWatchProgress::WalkComplete { new_count } => {
                    tracing::debug!(new_count, "Directory watcher: walk cycle complete");
                    self.state.dir_watcher_scanning = false;
                    if new_count == 0 {
                        let now = chrono::Local::now().format("%H:%M:%S");
                        self.state.status_message =
                            format!("Directory watcher: up to date (checked {now})");
                    }
                    // If new_count > 0 the NewFiles handler already set a more
                    // informative status message — don't overwrite it here.
                }
                crate::core::model::DirWatchProgress::WalkTimedOut => {
                    tracing::warn!("Directory watcher: walk timed out - directory tree may be slow or large. Retrying next cycle.");
                    self.state.dir_watcher_scanning = false;
                    let now = chrono::Local::now().format("%H:%M:%S");
                    self.state.status_message = format!(
                        "Directory watcher: walk timed out ({now}) - retrying. Try enabling --debug to diagnose."
                    );
                }
                crate::core::model::DirWatchProgress::FileMtimeUpdates(updates) => {
                    // Update the cached mtime on each DiscoveredFile so the
                    // file list in the discovery panel shows a live timestamp
                    // that refreshes whenever the file is written to.
                    //
                    // Also update LogEntry::file_modified for entries that have
                    // no parsed timestamp.  The time-range filter falls back to
                    // file_modified for plain-text / no-timestamp profiles; without
                    // this update those entries age out of "Last 1m" even though
                    // the file is still actively written to.
                    //
                    // Fix D — skip O(n) entry scan when not needed:
                    // When every entry in the session has a parsed timestamp
                    // (notimestamp_entry_count == 0), file_modified is never used
                    // as a time-range-filter fallback, so iterating all entries
                    // is pure waste.  This skips the scan for the common case of
                    // well-structured log files (Veeam, IIS, syslog, etc.).
                    //
                    // Bug fix: batch all path->mtime updates into a HashMap and
                    // iterate discovered_files and entries once each, reducing
                    // O(updates * files + entries) to O(updates + files + entries).
                    let mtime_map: std::collections::HashMap<
                        &std::path::PathBuf,
                        chrono::DateTime<chrono::Utc>,
                    > = updates.iter().map(|(p, t)| (p, *t)).collect();
                    // Update discovered_files using the HashMap (O(files) instead
                    // of O(updates * files) from the previous nested-find loop).
                    for f in self.state.discovered_files.iter_mut() {
                        if let Some(&mtime) = mtime_map.get(&f.path) {
                            f.modified = Some(mtime);
                        }
                    }
                    // Only iterate entries when at least one entry is missing a
                    // parsed timestamp (Fix D).  For large sessions with structured
                    // logs this skips an O(n) write loop that would run every 2s.
                    if self.state.notimestamp_entry_count > 0 {
                        for entry in self.state.entries.iter_mut() {
                            if entry.timestamp.is_none() {
                                if let Some(&mtime) = mtime_map.get(&entry.source_file) {
                                    entry.file_modified = Some(mtime);
                                }
                            }
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

        // If a relative time filter or activity window is active, refresh the
        // time-dependent filter state each frame and schedule a 1-second repaint
        // so the rolling boundary stays current as the clock advances.
        // Consolidated into a single check to avoid calling apply_filters() twice
        // per frame when both features are active simultaneously.
        //
        // `entries_changed = true` means one of the following already ran in this
        // frame and updated the filter state:
        //   (a) sort_entries_chronologically() → apply_filters() (slow path), or
        //   (b) extend_filtered_for_range() (fast path, new entries only).
        //
        // For case (a): skip, a full rebuild already happened.
        // For case (b): skip, old entries that aged out will be pruned on the
        //   NEXT 1-second timer frame (acceptable ~1 s visual lag).
        // For no-new-entries frames: call apply_filters() to age out entries
        //   that have crossed the rolling boundary since the last repaint.
        if self.state.filter_state.relative_time_secs.is_some()
            || self.state.activity_window_secs.is_some()
        {
            if !entries_changed {
                self.state.apply_filters();
            }
            ctx.request_repaint_after(std::time::Duration::from_secs(1));
        }

        // ---- Handle flags set by discovery panel ----
        // pending_scan: a panel requested a new scan via Open Directory button.
        if let Some(path) = self.state.pending_scan.take() {
            // Stop any current dir watcher before starting the new scan.
            // It will be restarted automatically when ParsingCompleted fires.
            self.dir_watcher.stop_watch();
            self.state.dir_watcher_active = false;
            // Bug fix: stop any running live tail before clear().  Without
            // this, the tail thread keeps sending NewEntries from the old
            // session's files, which contaminate the new session's entries.
            if self.state.tail_active {
                self.tail_manager.stop_tail();
            }
            // Capture the date filter BEFORE clear() — clear() does not reset
            // discovery_date_input intentionally (user preference, not scan state).
            let modified_since = self.state.discovery_modified_since();
            self.state.clear();
            self.state.scan_in_progress = true;
            self.state.fresh_scan_in_progress = true;
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
                // parse_path_filter = empty set: discover and profile all files
                // without reading any content.  Entries load only when the user
                // ticks a file in the Files tab (parse-on-demand, Rule 17).
                // Keeps startup fast and memory near-zero until explicit opt-in.
                Some(std::collections::HashSet::new()),
            );
        }
        // initial_scan: set at startup when restoring a previous session.
        // Unlike pending_scan, does NOT call clear() so the restored
        // filter/colour/bookmark state is preserved during the re-scan.
        if let Some(path) = self.state.initial_scan.take() {
            let modified_since = self.state.discovery_modified_since();
            // Opt-in parse model on session restore:
            //
            //   • explicit selection  (source_files non-empty, hide_all = false):
            //     parse only those files — restores exactly what the user had open.
            //
            //   • nothing checked     (hide_all_sources = true):
            //     parse nothing — use Some(empty) so the pipeline profiles files
            //     by filename only (fast) and sets parsing_skipped=true everywhere.
            //
            //   • all-pass / no filter (source_files empty, hide_all = false):
            //     treat the same as "nothing checked" — safe default that avoids
            //     auto-parsing potentially thousands of files the user may not
            //     want loaded.  The user re-selects files explicitly.
            //
            // NEVER pass None here — None means parse EVERYTHING which is the root
            // cause of the RAM blowup when the session was saved with nothing checked.
            let parse_path_filter = if !self.state.filter_state.source_files.is_empty()
                && !self.state.filter_state.hide_all_sources
            {
                tracing::debug!(
                    filter_count = self.state.filter_state.source_files.len(),
                    "Applying parse-path filter from restored session filter"
                );
                Some(self.state.filter_state.source_files.clone())
            } else {
                tracing::debug!(
                    hide_all = self.state.filter_state.hide_all_sources,
                    source_files_empty = self.state.filter_state.source_files.is_empty(),
                    "Session restore: no explicit file selection — using opt-in model (parse nothing)"
                );
                Some(std::collections::HashSet::new())
            };
            self.state.scan_in_progress = true;
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
                parse_path_filter,
            );
        }
        // pending_replace_files: user chose "Open Log(s)..." — clear and load selected files.
        if let Some(files) = self.state.pending_replace_files.take() {
            // Explicitly stop the directory watcher and clear scan_path so the watcher
            // is NOT restarted after the file-only scan completes (Rule 17 pre-flight).
            self.dir_watcher.stop_watch();
            self.state.dir_watcher_active = false;
            // Bug fix: stop any running live tail before clear() so stale
            // tail entries do not contaminate the new file-only session.
            if self.state.tail_active {
                self.tail_manager.stop_tail();
            }
            self.state.clear();
            // scan_path must be None for file-only sessions so the dir watcher is
            // not started in the ParsingCompleted handler.
            self.state.scan_path = None;
            self.state.scan_in_progress = true;
            self.state.fresh_scan_in_progress = true;
            self.state.status_message = format!("Opening {} file(s)...", files.len());
            // State was just cleared so next_entry_id() is 0; pass it explicitly
            // for clarity and future-proofing.
            self.scan_manager.start_scan_files(
                files,
                self.state.profiles.clone(),
                self.state.max_total_entries,
                0,
                None, // user chose these files explicitly — parse all
            );
        }
        // pending_single_files: user chose "Add File(s)" — append to session.
        if let Some(files) = self.state.pending_single_files.take() {
            // Bug fix: de-duplicate against files already loaded in this session
            // to prevent duplicate entries in the timeline and duplicate rows in
            // the file list.  A user may accidentally re-select the same files,
            // or the dir-watcher may have already picked them up.
            //
            // Exclude ONLY files that have been fully parsed (parsing_skipped == false).
            // Files with parsing_skipped == true were either excluded by the initial
            // parse-path filter or had their entries removed when the user unchecked
            // them; they must be allowed through so the on-demand re-parse works.
            let known: std::collections::HashSet<std::path::PathBuf> = self
                .state
                .discovered_files
                .iter()
                .filter(|f| !f.parsing_skipped)
                .map(|f| f.path.clone())
                .collect();
            let files: Vec<std::path::PathBuf> =
                files.into_iter().filter(|p| !known.contains(p)).collect();
            if files.is_empty() {
                self.state.status_message = "Selected file(s) already loaded.".to_string();
            } else {
                // Split into re-parses of already-discovered files (checkbox tick)
                // vs truly new files added via the Add File(s) dialog.
                //
                // Only new files (not already present in discovered_files at all)
                // should be recorded in manually_added_files.  Discovered-directory
                // files that the user checked are already tracked by the scan_path
                // scan and recording them as manually-added would cause a redundant
                // extra-files pass on the next session restore.
                let discovered_paths: std::collections::HashSet<std::path::PathBuf> = self
                    .state
                    .discovered_files
                    .iter()
                    .map(|f| f.path.clone())
                    .collect();
                let (reparse, new_files): (Vec<_>, Vec<_>) = files
                    .iter()
                    .cloned()
                    .partition(|p| discovered_paths.contains(p));
                // Only truly new files get recorded for session persistence.
                if !new_files.is_empty() {
                    self.state
                        .manually_added_files
                        .extend(new_files.iter().cloned());
                }
                let reparse_count = reparse.len();
                let new_count = new_files.len();
                self.state.scan_in_progress = true;
                self.state.status_message = match (reparse_count, new_count) {
                    (r, 0) => format!("Loading entries for {r} file(s)..."),
                    (0, n) => format!("Adding {n} file(s)..."),
                    (r, n) => format!("Loading {r} file(s), adding {n} new file(s)..."),
                };
                // Append scan: entry IDs must continue after the highest existing ID
                // so bookmarks and correlation are not confused by duplicate IDs.
                let id_start = self.state.next_entry_id();
                self.scan_manager.start_scan_files(
                    files,
                    self.state.profiles.clone(),
                    self.state.max_total_entries,
                    id_start,
                    None, // user explicitly checked or added these files — parse all
                );
            }
        }
        // pending_parse_skipped: the discovery panel requested a follow-up parse
        // for all files that were skipped by the parse-path filter on the last scan.
        // We clear the parsing_skipped flag on each file BEFORE calling start_scan_files
        // so that the AdditionalFilesDiscovered response updates the existing rows
        // in place rather than trying to create duplicates (which are de-duped away).
        if self.state.pending_parse_skipped {
            self.state.pending_parse_skipped = false;
            let skipped: Vec<std::path::PathBuf> = self
                .state
                .discovered_files
                .iter()
                .filter(|f| f.parsing_skipped)
                .map(|f| f.path.clone())
                .collect();
            if !skipped.is_empty() {
                // Pre-clear the flag so the update-or-append logic in
                // AdditionalFilesDiscovered can match these paths and update
                // them in-place when the pipeline result arrives.
                for f in &mut self.state.discovered_files {
                    if f.parsing_skipped {
                        f.parsing_skipped = false;
                    }
                }
                self.state.scan_in_progress = true;
                self.state.status_message =
                    format!("Parsing {} previously skipped file(s)...", skipped.len());
                let id_start = self.state.next_entry_id();
                self.scan_manager.start_scan_files(
                    skipped,
                    self.state.profiles.clone(),
                    self.state.max_total_entries,
                    id_start,
                    None, // user triggered "Parse skipped files" — parse all
                );
            }
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
            let mut files: Vec<crate::app::tail::TailFileInfo> = self
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
                        // Use the scan-time file size as the starting offset.
                        //
                        // Using `f.size` (set by the most recent completed scan)
                        // is safe because the background tail thread re-stats
                        // each file on its very first poll tick and reads any
                        // bytes that appeared after the scan completed.
                        // DiscoveredFile::size is refreshed after every append
                        // scan, so the "stale size on restart" window is at
                        // most a single scan duration.  All live stat work is
                        // confined to the background thread so it cannot freeze
                        // the UI (Rule 16).
                        initial_offset: Some(f.size),
                    })
                })
                .collect();
            // Safety cap: limit simultaneously-watched files to prevent
            // excessive file-handle and per-tick I/O overhead (Rule 11).
            // Sort by most-recently-modified so the most active logs always
            // fall within the cap; None-mtime files sort last.
            let files_total = files.len();
            if files_total > MAX_TAIL_WATCH_FILES {
                let mtimes: std::collections::HashMap<
                    std::path::PathBuf,
                    chrono::DateTime<chrono::Utc>,
                > = self
                    .state
                    .discovered_files
                    .iter()
                    .filter_map(|f| f.modified.map(|m| (f.path.clone(), m)))
                    .collect();
                files.sort_by(|a, b| {
                    let ta = mtimes.get(&a.path).copied();
                    let tb = mtimes.get(&b.path).copied();
                    tb.cmp(&ta) // newest mtime first; None sorts last
                });
                files.truncate(MAX_TAIL_WATCH_FILES);
                tracing::warn!(
                    total = files_total,
                    cap = MAX_TAIL_WATCH_FILES,
                    "Live tail: capped to {} most-recently-modified files",
                    MAX_TAIL_WATCH_FILES
                );
            }
            if files.is_empty() {
                self.state.status_message =
                    "No watchable files \u{2014} tick files in the Files tab first.".to_string();
            } else {
                let watching = files.len();
                let start_id = self.state.next_entry_id();
                // Record the current entry count as the ring-buffer baseline so
                // that entries from the initial scan are never subject to tail
                // eviction (Rule 11 — resource bounds).
                self.state.set_tail_base();
                self.tail_manager
                    .start_tail(files, start_id, self.state.tail_poll_interval_ms);
                self.state.tail_active = true;
                self.state.status_message = if files_total > MAX_TAIL_WATCH_FILES {
                    format!(
                        "Live tail active \u{2014} watching {watching} of {files_total} checked \
                         files (capped at {MAX_TAIL_WATCH_FILES}; uncheck some files to \
                         fit all within the cap)."
                    )
                } else {
                    format!("Live tail active \u{2014} watching {watching} file(s).")
                };
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
            // Discard any stale progress messages still queued in the channel.
            // cancel_scan() sets the flag but messages sent before the flag was
            // observed are still buffered.  Dropping the receiver ensures they
            // are discarded rather than applied to the freshly-cleared state on
            // the next frame (Bug fix: stale EntriesBatch after new_session).
            self.scan_manager.progress_rx = None;
            self.state.new_session();
            // Persist the blank state so the next launch starts fresh and does
            // not try to restore the old session directory.
            self.state.save_session();
        }

        // request_reload_profiles: re-scan the external profiles directory and
        // merge updated/new profiles with the built-ins. Preserves all current
        // scan, filter, and session state.
        if self.state.request_reload_profiles {
            self.state.request_reload_profiles = false;
            let dir = self.state.user_profiles_dir.clone();
            let (profiles, errors) = crate::app::profile_mgr::load_all_profiles(dir.as_deref());
            for err in &errors {
                tracing::warn!(error = %err, "Profile reload warning");
            }
            let total = profiles.len();
            let external = profiles.iter().filter(|p| !p.is_builtin).count();
            self.state.profiles = profiles;
            self.state.status_message =
                format!("Profiles reloaded - {total} total ({external} external).");
            tracing::info!(total, external, "Profiles reloaded via Options panel");
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
                            // Bug fix: stop any running live tail before clear()
                            // so stale tail entries do not contaminate the new session.
                            if self.state.tail_active {
                                self.tail_manager.stop_tail();
                            }
                            // Capture the date filter BEFORE clear() so the user's
                            // setting is not lost.  clear() intentionally preserves
                            // discovery_date_input, but modified_since must be read
                            // before we wipe any other transient state.
                            let modified_since = self.state.discovery_modified_since();
                            self.state.clear();
                            self.state.scan_in_progress = true;
                            self.state.fresh_scan_in_progress = true;
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
                                // Opt-in model: profile files by filename only;
                                // entries load only when the user ticks a checkbox.
                                Some(std::collections::HashSet::new()),
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
                    if ui.button("New Session")
                        .on_hover_text("Close the current session and start fresh with no files loaded")
                        .clicked()
                    {
                        self.state.request_new_session = true;
                        ui.close_menu();
                    }
                    ui.separator();
                    // Export sub-menu -- enabled only when there are filtered entries
                    let has_entries = !self.state.filtered_indices.is_empty();
                    ui.add_enabled_ui(has_entries, |ui| {
                        ui.menu_button("Export", |ui| {
                            if ui.button("Export CSV...")
                                .on_hover_text("Save the filtered entries as a comma-separated values file")
                                .clicked()
                            {
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
                            if ui.button("Export JSON...")
                                .on_hover_text("Save the filtered entries as a JSON array")
                                .clicked()
                            {
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
                    if ui.button("Exit")
                        .on_hover_text("Save the session and close LogSleuth")
                        .clicked()
                    {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.menu_button("Edit", |ui| {
                    if ui.button("Options\u{2026}")
                        .on_hover_text("Configure ingest limits, polling intervals, profiles, and appearance")
                        .clicked()
                    {
                        self.state.show_options = true;
                        ui.close_menu();
                    }
                });
                ui.menu_button("View", |ui| {
                    let has_summary = self.state.scan_summary.is_some();
                    ui.add_enabled_ui(has_summary, |ui| {
                        if ui.button("Scan Summary")
                            .on_hover_text("Show an overview of the last scan: file counts, entry totals, and per-file breakdown")
                            .clicked()
                        {
                            self.state.show_summary = true;
                            ui.close_menu();
                        }
                    });
                    let has_entries = !self.state.filtered_indices.is_empty();
                    ui.add_enabled_ui(has_entries, |ui| {
                        if ui.button("Log Summary")
                            .on_hover_text("Show a severity breakdown and message preview of the currently-filtered entries")
                            .clicked()
                        {
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
                        if ui.button(bm_label)
                            .on_hover_text("Copy a plain-text report of all bookmarked entries to the clipboard")
                            .clicked()
                        {
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
                        if ui.button(copy_label)
                            .on_hover_text("Copy all currently-filtered entries as a plain-text report to the clipboard")
                            .clicked()
                        {
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
                // LIVE badge -- shown while tail is active.
                if self.state.tail_active {
                    ui.label(
                        egui::RichText::new(" \u{25cf} LIVE ")
                            .strong()
                            .color(egui::Color32::from_rgb(34, 197, 94)) // Green 500
                            .background_color(egui::Color32::from_rgba_premultiplied(
                                34, 197, 94, 30,
                            )),
                    )
                    .on_hover_text("Live Tail is active -- new entries are being streamed in real time. Stop via the Files tab.");
                    ui.separator();
                }
                // WATCH badge — shown while the directory watcher is active, or
                // dimmed when paused but a directory session is loaded.
                // Clicking toggles the watcher on/off.
                let has_dir_session = self.state.scan_path.is_some()
                    && !self.state.discovered_files.is_empty();
                if self.state.dir_watcher_active {
                    let watch_tip = if self.state.dir_watcher_scanning {
                        "Directory watch active - scanning for new files... (click to pause)"
                    } else {
                        "Directory watch active - click to pause"
                    };
                    let watch_label = if self.state.dir_watcher_scanning {
                        egui::RichText::new(" \u{1f441} WATCH ... ")
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
                        .on_hover_text("Directory watch paused - click to resume")
                        .clicked()
                    {
                        watch_toggle = Some(true);
                    }
                    ui.separator();
                }
                ui.label(&self.state.status_message);
                // Cancel button visible only while a scan is running
                if self.state.scan_in_progress && ui.small_button("Cancel")
                    .on_hover_text("Stop the running scan. Files already parsed will be kept.")
                    .clicked()
                {
                    self.scan_manager.cancel_scan();
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let total = self.state.entries.len();
                    let filtered = self.state.filtered_indices.len();
                    let cap = self.state.max_total_entries;

                    // Entry-cap warning — shown when loaded entries are near or at the limit.
                    if total > 0 && cap > 0 {
                        let pct = (total as f64 / cap as f64 * 100.0).min(100.0);
                        if total >= cap {
                            // Hard cap reached — bright amber with exclamation.
                            ui.label(
                                egui::RichText::new(format!(
                                    "\u{26a0} ENTRY LIMIT ({cap})"
                                ))
                                .strong()
                                .color(egui::Color32::from_rgb(239, 68, 68)), // Red 500
                            )
                            .on_hover_text(format!(
                                "The entry limit of {cap} has been reached. \
                                 Some log files were skipped during the scan. \
                                 Filters reduce what is displayed but cannot recover skipped data. \
                                 Raise the limit in Edit > Options or narrow the scan with a date filter."
                            ));
                            ui.separator();
                        } else if pct >= 80.0 {
                            // Approaching limit — amber warning.
                            ui.label(
                                egui::RichText::new(format!(
                                    "\u{26a0} {pct:.0}% of entry limit"
                                ))
                                .color(egui::Color32::from_rgb(251, 191, 36)), // Amber 400
                            )
                            .on_hover_text(format!(
                                "{total} of {cap} entry limit used ({pct:.0}%). \
                                 If the limit is reached, remaining files will be skipped. \
                                 Filters reduce what is displayed but do not free loaded entries. \
                                 Raise the limit in Edit > Options or narrow the scan with a date filter."
                            ));
                            ui.separator();
                        }
                    }

                    if total > 0 {
                        ui.label(format!("{filtered}/{total} entries"))
                            .on_hover_text(format!(
                                "{filtered} entries match the current filters out of {total} total loaded"
                            ));
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
                            ui.label(format!("{loaded} files"))
                                .on_hover_text(format!("{loaded} log files loaded in this session"));
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
                            // Bug fix: forward user-configured depth limit so the
                            // watcher covers the same directory tree as the scan.
                            max_depth: self.state.max_scan_depth,
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

        // Debounced text-filter: fire apply_filters() once the text-search or
        // regex field has been unchanged for FILTER_DEBOUNCE_MS.
        //
        // Text fields set `filter_dirty_at = Some(Instant::now())` on each
        // keystroke instead of calling apply_filters() directly.  Here we check
        // whether enough time has elapsed and either fire the rebuild or schedule
        // a repaint for when it will be due.
        //
        // Button-driven filter changes (severity presets, time range, etc.) call
        // apply_filters() directly, which clears filter_dirty_at, so they are
        // never delayed by this path.
        if let Some(dirty_at) = self.state.filter_dirty_at {
            let debounce = std::time::Duration::from_millis(FILTER_DEBOUNCE_MS);
            if dirty_at.elapsed() >= debounce {
                // apply_filters() also clears filter_dirty_at.
                self.state.apply_filters();
            } else {
                ctx.request_repaint_after(debounce - dirty_at.elapsed());
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
                        .on_hover_text("Browse discovered files, manage the scan, and filter which files are included")
                        .clicked()
                    {
                        self.state.sidebar_tab = 0;
                    }
                    // Show filter-active indicator on Filters tab label.
                    let filter_active = !self.state.filter_state.source_files.is_empty()
                        || self.state.filter_state.hide_all_sources
                        || self.state.filter_state.relative_time_secs.is_some()
                        || !self.state.filter_state.text_search.is_empty()
                        // Bug fix: check the compiled regex (regex_search), not the
                        // raw pattern string.  An invalid pattern is non-empty but
                        // regex_search is None, so the filter is not actually applied.
                        || self.state.filter_state.regex_search.is_some()
                        || self.state.filter_state.bookmarks_only
                        // A severity filter is active only when the set is non-empty
                        // AND does not contain every variant (all-checked is equivalent
                        // to no filter).  An empty set also means no filter (all pass).
                        || (!self.state.filter_state.severity_levels.is_empty()
                            && self.state.filter_state.severity_levels.len()
                                < crate::core::model::Severity::all().len());
                    let filters_label = if filter_active {
                        "\u{25cf} Filters".to_string() // bullet dot = filter active
                    } else {
                        "Filters".to_string()
                    };
                    if ui
                        .selectable_label(self.state.sidebar_tab == 1, filters_label)
                        .on_hover_text("Filter the timeline by severity, text, regex, time range, or correlation")
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

        // Activity window + relative time auto-advance is handled by the
        // consolidated block earlier in update() to avoid calling
        // apply_filters() twice per frame when both features are active.
    }

    /// Called by eframe when the application window is about to close.
    ///
    /// Saves the current session so the next launch can restore it.
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // Stop background threads cleanly before the process exits.
        self.scan_manager.cancel_scan();
        self.dir_watcher.stop_watch();
        self.tail_manager.stop_tail();
        self.state.save_session();
    }
}
