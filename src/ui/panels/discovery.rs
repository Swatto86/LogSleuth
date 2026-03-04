// LogSleuth - ui/panels/discovery.rs
//
// Files tab for the left sidebar.
//
// Contains two logical sections:
//   1. Collapsible scan controls (date filter, Open Directory / Open Logs,
//      Clear Session).  Starts open so first-time users see everything;
//      collapses once a scan has been run to give the file list more space.
//   2. Unified discovered-file list with inline source-file filter checkboxes.
//      Replaces the duplicate file list that was previously shown in the
//      filters panel.
//
// This panel writes `state.pending_scan`, `state.request_cancel`, and
// `state.filter_state` flag fields; gui.rs consumes them each frame.
// No direct I/O or ScanManager access (Rule 1 boundary).

use crate::app::state::AppState;
use chrono::{DateTime, Datelike, Duration, Local, Utc};

/// Render the Files tab (scan controls + unified file list).
pub fn render(ui: &mut egui::Ui, state: &mut AppState) {
    // -------------------------------------------------------------------------
    // Section 1: Scan controls — collapsible so the file list dominates once a
    // scan has completed.  `default_open(true)` shows everything on first run.
    // -------------------------------------------------------------------------
    let scan_heading = if let Some(ref path) = state.scan_path {
        let dir = path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
        format!("Scan \u{2014} {dir}")
    } else {
        "Scan".to_string()
    };

    egui::CollapsingHeader::new(egui::RichText::new(scan_heading).strong())
        .default_open(true)
        .show(ui, |ui| {
            render_scan_controls(ui, state);
        });

    // -------------------------------------------------------------------------
    // Section 2: Unified file list — only rendered once files are available.
    // Combines discovery metadata (profile, size) with the source-file filter
    // checkboxes, eliminating the duplicate list that was in filters.rs.
    // -------------------------------------------------------------------------
    if !state.discovered_files.is_empty() {
        ui.add_space(6.0);

        // Build a sorted index list: most-recently-modified files first so that the
        // files most actively written to during a live session float to the top.
        // Files with no known mtime go to the end.
        let mut sorted_file_idxs: Vec<usize> = (0..state.discovered_files.len()).collect();
        sorted_file_idxs.sort_by(|&a, &b| {
            state.discovered_files[b]
                .modified
                .cmp(&state.discovered_files[a].modified)
        });

        // Activity window: hide files whose mtime is outside the rolling window.
        // Computed once here so the file list and counts are consistent.
        let activity_cutoff = state.activity_cutoff();
        if let Some(cutoff) = activity_cutoff {
            sorted_file_idxs.retain(|&idx| {
                let f = &state.discovered_files[idx];
                // Fail-open: include files with no known mtime.
                f.modified.map_or(true, |t| t >= cutoff)
            });
        }
        let total_all = state.discovered_files.len();
        // total reflects the (possibly activity-filtered) file count shown in the list.

        // Pre-collect (path, name, size, profile_text, profile_colour, mtime_text, parsing_skipped) once
        // so we can borrow state mutably for checkbox updates below.
        let file_entries: Vec<(
            std::path::PathBuf,
            String, // display name
            String, // size text
            String, // profile text
            egui::Color32,
            String, // mtime text (formatted for display)
            bool,   // parsing_skipped
        )> = sorted_file_idxs
            .iter()
            .map(|&idx| {
                let f = &state.discovered_files[idx];
                let name = f
                    .path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("?")
                    .to_string();
                let size = format_size(f.size);
                let (profile_text, profile_colour) = match &f.profile_id {
                    Some(id) if id == "plain-text" && f.detection_confidence == 0.0 => (
                        "plain-text (fallback)".to_string(),
                        egui::Color32::from_rgb(156, 163, 175),
                    ),
                    Some(id) => (
                        format!("{id} ({:.0}%)", f.detection_confidence * 100.0),
                        egui::Color32::from_rgb(74, 222, 128),
                    ),
                    None => (
                        "unmatched".to_string(),
                        egui::Color32::from_rgb(156, 163, 175),
                    ),
                };
                let mtime_text = format_mtime(f.modified);
                (
                    f.path.clone(),
                    name,
                    size,
                    profile_text,
                    profile_colour,
                    mtime_text,
                    f.parsing_skipped,
                )
            })
            .collect::<Vec<_>>();

        let total = file_entries.len();
        let active_count = state.filter_state.source_files.len();

        // Header row: count / active-filter indicator + All / global-reset.
        ui.horizontal(|ui| {
            // When an activity window is on, prefix the count with the window label.
            let file_count_text = if let Some(win_secs) = state.activity_window_secs {
                let label = render_window_label(win_secs);
                if total == total_all {
                    format!("{total} files - {label}")
                } else {
                    format!("{total}/{total_all} files - {label}")
                }
            } else if active_count == 0 && !state.filter_state.hide_all_sources {
                format!("{total} files")
            } else {
                let showing = if state.filter_state.hide_all_sources {
                    0usize
                } else if active_count == 0 {
                    total
                } else {
                    active_count
                };
                format!("{showing}/{total} files")
            };
            let count_colour = if state.activity_window_secs.is_some() {
                egui::Color32::from_rgb(251, 191, 36) // amber when activity window is on
            } else if active_count > 0 || state.filter_state.hide_all_sources {
                egui::Color32::from_rgb(96, 165, 250)
            } else {
                ui.style().visuals.text_color()
            };
            ui.label(
                egui::RichText::new(file_count_text)
                    .strong()
                    .color(count_colour),
            );
            if (active_count > 0 || state.filter_state.hide_all_sources)
                && ui
                    .small_button("All")
                    .on_hover_text("Show entries from all files (clear the file filter)")
                    .clicked()
            {
                state.filter_state.source_files.clear();
                state.filter_state.hide_all_sources = false;
                state.apply_filters();
            }
        });

        // "Parse skipped files" button — shown when the last scan excluded some
        // files due to an active parse-path filter and there is no scan running.
        let skipped_count = state
            .discovered_files
            .iter()
            .filter(|f| f.parsing_skipped)
            .count();
        if skipped_count > 0 && !state.scan_in_progress {
            ui.horizontal(|ui| {
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new(format!(
                                "\u{25b6} Parse skipped files ({skipped_count})"
                            ))
                            .small()
                            .color(egui::Color32::from_rgb(251, 191, 36)),
                        )
                        .small(),
                    )
                    .on_hover_text(
                        "These files have not been parsed yet - their entries are not in the timeline.\n\
                         This happens when a file was excluded by the initial scan filter, or when you\n\
                         unchecked a file (which removes its entries from memory).\n\
                         Click to parse all of them now and add their entries to the timeline.",
                    )
                    .clicked()
                {
                    state.pending_parse_skipped = true;
                }
            });
        }

        if !state.scan_in_progress && !state.entries.is_empty() {
            ui.horizontal(|ui| {
                if state.tail_active {
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
                } else if ui
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
            });
        }

        // Activity window — shown near Live Tail so related controls are grouped.
        render_activity_window(ui, state);

        ui.add_space(2.0);

        // Search box — always shown.  Supports comma-separated patterns, each
        // of which may contain `*` (any chars) or `?` (one char) wildcards.
        // Plain text without wildcards falls back to substring match.
        // Example: "svcVee*, *iis, svcBck*"
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut state.file_list_search)
                    .hint_text("\u{1f50d} search files\u{2026} e.g. \"svcVee*, *iis\"")
                    .desired_width(ui.available_width() - 22.0),
            )
            .on_hover_text("Filter the file list by name. Supports wildcards (* and ?) and comma-separated patterns.");
            // Clear button — visible only when the search box has text.
            if !state.file_list_search.is_empty()
                && ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new("\u{2715}")
                                .small()
                                .color(egui::Color32::from_rgb(156, 163, 175)),
                        )
                        .small()
                        .frame(false),
                    )
                    .on_hover_text("Clear search")
                    .clicked()
            {
                state.file_list_search.clear();
            }
        });

        // Build the visible subset filtered by the search box.
        let mut visible: Vec<usize> = file_entries
            .iter()
            .enumerate()
            .filter(|(_, (_, name, _, _, _, _, _))| {
                matches_file_search(&state.file_list_search, name)
            })
            .map(|(i, _)| i)
            .collect();
        // Sort checked files to the top so selected files are always visible
        // without needing to scroll.  Within each group (checked / unchecked)
        // preserve the original order (stable sort).
        {
            let hide_all = state.filter_state.hide_all_sources;
            let source_files = &state.filter_state.source_files;
            visible.sort_by_key(|&i| {
                let path = &file_entries[i].0;
                let checked = !hide_all && (source_files.is_empty() || source_files.contains(path));
                if checked {
                    0u8
                } else {
                    1u8
                }
            });
        }
        let visible_count = visible.len();

        // Select All / None for the visible subset.
        if visible_count > 1 {
            ui.horizontal(|ui| {
                if ui
                    .small_button("Select all")
                    .on_hover_text("Include all visible files in the filter")
                    .clicked()
                {
                    let prev_hide_all = state.filter_state.hide_all_sources;
                    let visible_paths: std::collections::HashSet<&std::path::PathBuf> =
                        visible.iter().map(|&i| &file_entries[i].0).collect();
                    let mut new_selected: std::collections::HashSet<std::path::PathBuf> =
                        visible.iter().map(|&i| file_entries[i].0.clone()).collect();
                    // Preserve selection state of ALL non-visible files, including
                    // files hidden by the activity window.  Bug fix: the previous
                    // loop only iterated file_entries (activity-filtered), so files
                    // outside the window that were explicitly selected were silently
                    // dropped, causing them to disappear from the filter when the
                    // activity window was later disabled.
                    let file_entry_paths: std::collections::HashSet<&std::path::PathBuf> =
                        file_entries.iter().map(|(p, _, _, _, _, _, _)| p).collect();
                    for f in &state.discovered_files {
                        if !visible_paths.contains(&f.path) && !file_entry_paths.contains(&f.path) {
                            let was_selected = !prev_hide_all
                                && (state.filter_state.source_files.is_empty()
                                    || state.filter_state.source_files.contains(&f.path));
                            if was_selected {
                                new_selected.insert(f.path.clone());
                            }
                        }
                    }
                    for (path, _, _, _, _, _, _) in &file_entries {
                        if !visible_paths.contains(path) {
                            let was_selected = !prev_hide_all
                                && (state.filter_state.source_files.is_empty()
                                    || state.filter_state.source_files.contains(path));
                            if was_selected {
                                new_selected.insert(path.clone());
                            }
                        }
                    }
                    state.filter_state.source_files = new_selected;
                    state.filter_state.hide_all_sources = false;
                    // Do NOT collapse source_files to empty when all files are
                    // selected.  Keeping the set explicitly enumerated ensures
                    // files added later by the directory watcher remain unchecked
                    // until the user explicitly ticks them (opt-in model).
                    state.apply_filters();
                }
                if ui
                    .small_button("None")
                    .on_hover_text("Exclude all visible files from the filter")
                    .clicked()
                {
                    let visible_paths: std::collections::HashSet<&std::path::PathBuf> =
                        visible.iter().map(|&i| &file_entries[i].0).collect();
                    let mut non_visible_selected: std::collections::HashSet<std::path::PathBuf> =
                        file_entries
                            .iter()
                            .filter(|(p, _, _, _, _, _, _)| !visible_paths.contains(p))
                            .filter(|(p, _, _, _, _, _, _)| {
                                !state.filter_state.hide_all_sources
                                    && (state.filter_state.source_files.is_empty()
                                        || state.filter_state.source_files.contains(p))
                            })
                            .map(|(p, _, _, _, _, _, _)| p.clone())
                            .collect();
                    // Bug fix: also preserve selection state of files outside the
                    // activity window.  The loop above only iterates file_entries
                    // (activity-filtered); without this, out-of-window files that
                    // were previously selected are lost when "None" is clicked.
                    let file_entry_paths: std::collections::HashSet<&std::path::PathBuf> =
                        file_entries.iter().map(|(p, _, _, _, _, _, _)| p).collect();
                    for f in &state.discovered_files {
                        if !file_entry_paths.contains(&f.path) {
                            let was_selected = !state.filter_state.hide_all_sources
                                && (state.filter_state.source_files.is_empty()
                                    || state.filter_state.source_files.contains(&f.path));
                            if was_selected {
                                non_visible_selected.insert(f.path.clone());
                            }
                        }
                    }
                    state.filter_state.source_files = non_visible_selected;
                    if state.filter_state.source_files.is_empty() {
                        state.filter_state.hide_all_sources = true;
                    }
                    state.apply_filters();
                }
                if !state.file_list_search.trim().is_empty() {
                    ui.label(
                        egui::RichText::new(format!("{visible_count}/{total}"))
                            .small()
                            .weak(),
                    );
                }
            });
        }

        // Virtual-scroll file list.
        // Each row: [checkbox] [dot] [filename]  [reveal-button]
        // Hover:    full path + size + profile
        // Row height scales with the user's font-size preference.
        if visible.is_empty() {
            ui.label(egui::RichText::new("No files match.").small().weak());
        } else {
            let row_height = crate::ui::theme::row_height(state.ui_font_size);
            egui::ScrollArea::vertical()
                .id_salt("discovery_file_list")
                .auto_shrink([false; 2])
                .show_rows(ui, row_height, visible.len(), |ui, row_range| {
                    for display_idx in row_range {
                        let entry_idx = visible[display_idx];
                        let (path, name, size_text, profile_text, profile_colour, mtime_text, parsing_skipped) =
                            &file_entries[entry_idx];

                        let mut checked = !state.filter_state.hide_all_sources
                            && (state.filter_state.source_files.is_empty()
                                || state.filter_state.source_files.contains(path));

                        ui.horizontal(|ui| {
                            // Coloured dot matching the file's timeline stripe colour.
                            let dot_colour = state.colour_for_file(path);
                            let (dot_rect, _) =
                                ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                            ui.painter()
                                .circle_filled(dot_rect.center(), 4.0, dot_colour);

                            // Checkbox + filename.
                            let skip_note = if *parsing_skipped { " \u{2298}" } else { "" }; // ⊘ = not loaded
                            let hover_detail = if mtime_text.is_empty() {
                                format!("{}\n{size_text}  \u{b7}  {profile_text}", path.display())
                            } else {
                                format!(
                                    "{}\n{size_text}  \u{b7}  {profile_text}\nModified: {mtime_text}",
                                    path.display()
                                )
                            };
                            let hover_detail = if *parsing_skipped {
                                format!("{hover_detail}\n\u{26a0} Not parsed \u{2014} tick the checkbox to load entries from this file,\nor use \u{201c}Parse skipped files\u{201d} to load all unparsed files at once.")
                            } else {
                                hover_detail
                            };
                            let name_label = egui::RichText::new(format!("{name}{skip_note}")).small();
                            let name_label = if *parsing_skipped {
                                name_label.color(egui::Color32::from_rgb(156, 163, 175))
                            } else {
                                name_label
                            };
                            let cb_resp = ui
                                .checkbox(&mut checked, name_label)
                                .on_hover_text(hover_detail);

                            if cb_resp.changed() {
                                if !checked {
                                    // Unchecking: seed the whitelist from all-pass then remove.
                                    // Bug fix: seed from ALL discovered files, not just
                                    // the activity-filtered file_entries.  When the activity
                                    // window is later disabled, files that were outside the
                                    // window must remain in the whitelist so they are not
                                    // silently excluded.
                                    if state.filter_state.source_files.is_empty()
                                        && !state.filter_state.hide_all_sources
                                    {
                                        for f in &state.discovered_files {
                                            state.filter_state.source_files.insert(f.path.clone());
                                        }
                                    }
                                    state.filter_state.source_files.remove(path);
                                    if state.filter_state.source_files.is_empty() {
                                        state.filter_state.hide_all_sources = true;
                                    }
                                    // Remove this file's entries from memory and mark it
                                    // as unparsed so re-ticking triggers a fresh parse.
                                    // apply_filters() is called below, which is sufficient;
                                    // remove_entries_for_file does NOT call it internally.
                                    state.remove_entries_for_file(path);
                                } else {
                                    state.filter_state.hide_all_sources = false;
                                    state.filter_state.source_files.insert((*path).clone());
                                    // Do NOT collapse source_files to empty when
                                    // all files are ticked.  Keeping the set
                                    // explicitly enumerated ensures new dir-watcher
                                    // files stay unchecked by default.
                                    // If this file has not yet been parsed (or had its
                                    // entries removed when it was unchecked), queue it
                                    // for an on-demand parse.  When no scan is in
                                    // progress, pending_single_files triggers an append
                                    // scan immediately next frame.  When a scan IS
                                    // running, push to queued_dir_watcher_files instead
                                    // so the parse fires once that scan completes
                                    // (drained by the ParsingCompleted handler in
                                    // gui.rs).  The previous guard `!scan_in_progress`
                                    // silently lost the request when scanning — the
                                    // comment claiming "re-parse will fire next frame"
                                    // was incorrect.
                                    if *parsing_skipped {
                                        if state.scan_in_progress {
                                            // A scan is already running — queue for
                                            // parse once it completes.  Use the
                                            // dedicated queued_parse_files list so
                                            // the ParsingCompleted handler drains it
                                            // with None (parse all), not the
                                            // profile-only filter used for
                                            // queued_dir_watcher_files.
                                            state
                                                .queued_parse_files
                                                .push(path.clone());
                                        } else if let Some(ref mut existing) =
                                            state.pending_single_files
                                        {
                                            existing.push(path.clone());
                                        } else {
                                            state.pending_single_files =
                                                Some(vec![path.clone()]);
                                        }
                                    }
                                }
                                state.apply_filters();
                            }

                            // Reveal-in-file-manager button — opens Explorer/Finder
                            // with this file pre-selected so the user can inspect it.
                            if ui
                                .add(
                                    egui::Button::new(
                                        egui::RichText::new("\u{1f4c2}")
                                            .small()
                                            .color(egui::Color32::from_rgb(107, 114, 128)),
                                    )
                                    .small()
                                    .frame(false),
                                )
                                .on_hover_text("Reveal in file manager")
                                .clicked()
                            {
                                crate::platform::fs::reveal_in_file_manager(path);
                            }

                            // Profile label and mtime — right-aligned, coloured by match
                            // quality.  In right-to-left order: profile is at the far
                            // right, mtime immediately to its left.
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(
                                        egui::RichText::new(profile_text.as_str())
                                            .small()
                                            .color(*profile_colour),
                                    );
                                    if !mtime_text.is_empty() {
                                        ui.label(
                                            egui::RichText::new(mtime_text.as_str())
                                                .small()
                                                .weak(),
                                        );
                                    }
                                },
                            );
                        });
                    }
                });
        }

        // Warnings badge.
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

        // Activity window toggle: only shown when files are loaded.
        // Moved above the file list, next to Live Tail.
    }
}

/// Render the collapsible scan-controls body: date filter, Open buttons, Clear.
fn render_scan_controls(ui: &mut egui::Ui, state: &mut AppState) {
    // Current scan path (small, weak — for reference when the header is collapsed).
    if let Some(ref path) = state.scan_path {
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
    // Persists across scans so the user can re-run with the same date.
    // -------------------------------------------------------------------------
    ui.label(
        egui::RichText::new("File date/time filter:")
            .small()
            .strong(),
    )
    .on_hover_text("Restrict scanning to files whose OS last-modified time is on or after a given date. Useful for large directories where you only care about recent logs.");
    ui.label(
        egui::RichText::new(
            "Only scan files modified on or after this date/time. \
             Leave blank to scan all.",
        )
        .small()
        .weak(),
    );

    // Input row: text field + tick/cross + clear.
    ui.horizontal(|ui| {
        let resp = ui.add(
            egui::TextEdit::singleline(&mut state.discovery_date_input)
                .hint_text("YYYY-MM-DD HH:MM:SS")
                .desired_width(138.0),
        );
        if !state.discovery_date_input.trim().is_empty() {
            if state.discovery_modified_since().is_some() {
                ui.colored_label(egui::Color32::from_rgb(74, 222, 128), "\u{2713}");
            } else {
                ui.colored_label(egui::Color32::from_rgb(248, 113, 113), "\u{2717}");
            }
        }
        let _ = resp;
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
                .on_hover_text("Clear date/time filter - rescans with no date restriction")
                .clicked()
        {
            state.discovery_date_input.clear();
            // Clear means "scan all files": trigger a rescan immediately when a
            // path is already configured so the user doesn't have to press Open.
            // Guard against interrupting an active scan (Bug fix).
            if !state.scan_in_progress {
                state.pending_scan = state.scan_path.clone();
            }
        }
    });

    // Quick-fill row.  Each button updates the date field AND triggers an
    // immediate rescan if a scan path is already configured (Rule 16: controls
    // apply their action immediately rather than requiring a secondary click).
    let mut did_update_date = false;
    ui.horizontal_wrapped(|ui| {
        if ui
            .small_button("Today")
            .on_hover_text("Set to today's date (00:00:00 local) and rescan")
            .clicked()
        {
            state.discovery_date_input = Local::now().format("%Y-%m-%d 00:00:00").to_string();
            did_update_date = true;
        }
        if ui
            .small_button("-1h")
            .on_hover_text("1 hour ago - rescan")
            .clicked()
        {
            let t = Local::now() - Duration::hours(1);
            state.discovery_date_input = t.format("%Y-%m-%d %H:%M:%S").to_string();
            did_update_date = true;
        }
        if ui
            .small_button("-6h")
            .on_hover_text("6 hours ago - rescan")
            .clicked()
        {
            let t = Local::now() - Duration::hours(6);
            state.discovery_date_input = t.format("%Y-%m-%d %H:%M:%S").to_string();
            did_update_date = true;
        }
        if ui
            .small_button("-24h")
            .on_hover_text("24 hours ago - rescan")
            .clicked()
        {
            let t = Local::now() - Duration::hours(24);
            state.discovery_date_input = t.format("%Y-%m-%d %H:%M:%S").to_string();
            did_update_date = true;
        }
        if ui
            .small_button("Now")
            .on_hover_text("Current local date and time - rescan")
            .clicked()
        {
            state.discovery_date_input = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
            did_update_date = true;
        }
    });
    if did_update_date && !state.scan_in_progress {
        state.pending_scan = state.scan_path.clone();
    }

    if !state.discovery_date_input.trim().is_empty() && state.discovery_modified_since().is_some() {
        // Display the filter in local wall-clock terms — `discovery_date_input`
        // already holds a local-time string so we show it directly rather than
        // converting back from the UTC `DateTime` (which would re-introduce an
        // offset shift for non-UTC users).
        ui.label(
            egui::RichText::new(format!(
                "Files modified on/after {} (local time)",
                state.discovery_date_input.trim()
            ))
            .small()
            .color(egui::Color32::from_rgb(96, 165, 250)),
        );
    }
    ui.add_space(4.0);

    // Scan / cancel controls.
    if state.scan_in_progress {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label("Scanning\u{2026}");
        });
        if ui
            .button("Cancel")
            .on_hover_text("Stop the scan in progress. Files already parsed will be kept.")
            .clicked()
        {
            state.request_cancel = true;
        }
    } else {
        ui.add_space(4.0);

        // Open buttons — no free-text path input.  Local paths only; network
        // paths (UNC shares, mapped drives to remote servers) are blocked here
        // and also validated inside the pickers to prevent hangs on slow
        // network enumeration.
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    !state.scan_in_progress,
                    egui::Button::new("Open Directory\u{2026}"),
                )
                .on_hover_text("Browse for a local directory to scan")
                .clicked()
            {
                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                    if is_network_path(&path) {
                        state.status_message =
                            "\u{26a0} Network paths (UNC shares / mapped drives to remote \
                             servers) are not supported. Copy the logs to a local drive first."
                                .to_string();
                    } else {
                        state.pending_scan = Some(path);
                    }
                }
            }
            if ui
                .add_enabled(
                    !state.scan_in_progress,
                    egui::Button::new("Open Log(s)\u{2026}"),
                )
                .on_hover_text("Select individual local log files to open as a new session")
                .clicked()
            {
                if let Some(files) = rfd::FileDialog::new()
                    .add_filter("Log files", &["log", "txt", "log.1", "log.2", "log.3"])
                    .pick_files()
                {
                    if files.iter().any(|f| is_network_path(f)) {
                        state.status_message =
                            "\u{26a0} Network paths (UNC shares / mapped drives to remote \
                             servers) are not supported. Copy the logs to a local drive first."
                                .to_string();
                    } else {
                        state.pending_replace_files = Some(files);
                    }
                }
            }
        });

        let has_session = state.scan_path.is_some() || !state.entries.is_empty();
        if has_session {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !state.scan_in_progress,
                        egui::Button::new(
                            egui::RichText::new("\u{2717} Clear Session")
                                .small()
                                .color(egui::Color32::from_rgb(248, 113, 113)),
                        ),
                    )
                    .on_hover_text("Close the current session and return to the start screen. All loaded data will be discarded.")
                    .clicked()
                {
                    state.request_new_session = true;
                }

                // Rescan shortcut -- re-runs the scan on the same directory with the
                // current date filter and ingest settings.  Handy after changing
                // options or when the directory contents have changed.
                if state.scan_path.is_some()
                    && ui
                        .add_enabled(
                            !state.scan_in_progress,
                            egui::Button::new(
                                egui::RichText::new("\u{1f504} Rescan")
                                    .small()
                                    .color(egui::Color32::from_rgb(96, 165, 250)),
                            )
                            .frame(false),
                        )
                        .on_hover_text("Re-scan the current directory with the active date filter and ingest settings")
                        .clicked()
                    {
                        state.pending_scan = state.scan_path.clone();
                    }
            });
        }
    }
}

/// Format a UTC modification time for compact display in a file-list row.
///
/// Returns:
/// - `"HH:MM:SS"`       when the file was modified today (local time)
/// - `"D Mon HH:MM"`    (e.g. `"26 Feb 14:30"`) when modified this calendar year
/// - `"YYYY-MM-DD"`     for modifications in a prior year
/// - `""`               when `modified` is `None`
fn format_mtime(modified: Option<DateTime<Utc>>) -> String {
    let Some(mtime) = modified else {
        return String::new();
    };
    let local = mtime.with_timezone(&Local);
    let now = Local::now();
    if local.date_naive() == now.date_naive() {
        local.format("%H:%M:%S").to_string()
    } else if local.year() == now.year() {
        // %e = space-padded day (" 6" or "26"); trims leading space via format
        // so single-digit days look like "6 Feb" rather than " 6 Feb".
        local
            .format("%e %b %H:%M")
            .to_string()
            .trim_start()
            .to_string()
    } else {
        local.format("%Y-%m-%d").to_string()
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

/// Returns `true` when `path` resolves to a network location that LogSleuth
/// refuses to scan.  Network paths can cause multi-second hangs during
/// directory enumeration (slow links, VPN, offline DFS targets) and are
/// therefore blocked at the UI layer.
///
/// Blocked on Windows:
///   - UNC paths             (`\\\\server\\share\\...`)
///   - Verbatim UNC paths    (`\\\\?\\UNC\\server\\share\\...`)
///   - Device-namespace paths (`\\\\.\\ ...`)
///
/// On non-Windows the function always returns `false` — network path
/// conventions differ and are enforced at the OS level instead.
fn is_network_path(path: &std::path::Path) -> bool {
    #[cfg(windows)]
    {
        use std::path::{Component, Prefix};
        if let Some(Component::Prefix(p)) = path.components().next() {
            return matches!(
                p.kind(),
                Prefix::UNC(_, _) | Prefix::VerbatimUNC(_, _) | Prefix::DeviceNS(_)
            );
        }
    }
    let _ = path;
    false
}

/// Returns `true` if `name` matches any of the comma-separated patterns in
/// `query` (case-insensitive).
///
/// Each token may contain:
/// - `*` — matches any sequence of characters (including empty)
/// - `?` — matches exactly one character
///
/// A token with no metacharacters performs a substring match so plain typed
/// text continues to work as before.  An empty query always matches everything.
///
/// Examples:
/// - `"svcVee*"` matches `svcVeeam.log`
/// - `"*iis*"` matches `u_ex26020708.log` if the name contains "iis"
/// - `"svcVee*, *iis, svcBck*"` matches any file satisfying at least one token
fn matches_file_search(query: &str, name: &str) -> bool {
    if query.trim().is_empty() {
        return true;
    }
    let name_lower = name.to_lowercase();
    for raw in query.split(',') {
        let pat = raw.trim();
        if pat.is_empty() {
            continue;
        }
        let pat_lower = pat.to_lowercase();
        if pat_lower.contains('*') || pat_lower.contains('?') {
            if glob_match(&pat_lower, &name_lower) {
                return true;
            }
        } else if name_lower.contains(pat_lower.as_str()) {
            // Plain text — substring match for backwards compatibility.
            return true;
        }
    }
    false
}

/// Iterative glob matcher (case-insensitive inputs expected).
///
/// `*` matches 0..N characters; `?` matches exactly one Unicode character.
/// Uses a two-pointer approach that runs in O(n*m) worst case without
/// exponential backtracking, making it safe for any user-typed pattern.
///
/// Bug fix: previous implementation operated on raw bytes (`&[u8]`), which
/// caused `?` to match a single byte instead of a single Unicode character.
/// For file names containing multi-byte UTF-8 characters (e.g. accented
/// letters), `?` would match only the first byte of the character, causing
/// the overall match to fail. Now operates on `char` slices for correctness.
fn glob_match(pat: &str, txt: &str) -> bool {
    let pat_chars: Vec<char> = pat.chars().collect();
    let txt_chars: Vec<char> = txt.chars().collect();
    glob_match_chars(&pat_chars, &txt_chars)
}

fn glob_match_chars(pat: &[char], txt: &[char]) -> bool {
    let (mut pi, mut ti) = (0usize, 0usize);
    // Saved positions for the last `*` we encountered.
    let mut star_pi: Option<usize> = None;
    let mut star_ti: usize = 0;

    while ti < txt.len() {
        if pi < pat.len() && pat[pi] == '*' {
            // Skip consecutive stars.
            while pi < pat.len() && pat[pi] == '*' {
                pi += 1;
            }
            // If `*` is at the end of the pattern, it matches everything remaining.
            if pi == pat.len() {
                return true;
            }
            star_pi = Some(pi);
            star_ti = ti;
            continue;
        }

        if pi < pat.len() && (pat[pi] == '?' || pat[pi] == txt[ti]) {
            pi += 1;
            ti += 1;
            continue;
        }

        // Mismatch: backtrack to last `*` if available and consume one more
        // character of txt.
        if let Some(sp) = star_pi {
            star_ti += 1;
            ti = star_ti;
            pi = sp;
            continue;
        }

        return false;
    }

    // Text exhausted: skip any trailing `*` in the pattern (they match empty).
    while pi < pat.len() && pat[pi] == '*' {
        pi += 1;
    }

    pi == pat.len()
}

// =============================================================================
// Activity window UI
// =============================================================================

/// Human-readable label for a window duration.
fn format_window_duration(secs: u64) -> String {
    if secs < 60 {
        format!("last {secs}s")
    } else if secs % 3600 == 0 {
        let h = secs / 3600;
        format!("last {h}h")
    } else {
        let m = secs / 60;
        format!("last {m}m")
    }
}

/// Alias used by the file-list header.
fn render_window_label(secs: u64) -> String {
    format_window_duration(secs)
}

/// Render the activity window toggle + preset row at the bottom of the Files panel.
///
/// The activity window hides files (and all their entries) whose OS
/// last-modified time is older than `now - window`.  The cutoff auto-advances
/// with the clock every second so stale files age out automatically.
fn render_activity_window(ui: &mut egui::Ui, state: &mut crate::app::state::AppState) {
    ui.add_space(4.0);
    ui.separator();

    let amber = egui::Color32::from_rgb(251, 191, 36);
    let dim = egui::Color32::from_rgb(107, 114, 128);
    let is_active = state.activity_window_secs.is_some();

    // Header row: label + Off button when active.
    ui.horizontal(|ui| {
        let label_colour = if is_active { amber } else { dim };
        ui.label(
            egui::RichText::new("\u{23f1} Activity window")
                .small()
                .strong()
                .color(label_colour),
        )
        .on_hover_text(
            "Hides files (and their log entries) that haven't been written to \
             recently.\nUseful during a live incident: set a window like \"5m\" \
             or \"1h\" to focus only on files that are actively changing \
             right now.\nStale files age out automatically as the clock advances.",
        );
        if is_active {
            if ui
                .add(
                    egui::Button::new(
                        egui::RichText::new("\u{d7} Off")
                            .small()
                            .color(egui::Color32::from_rgb(156, 163, 175)),
                    )
                    .small()
                    .frame(false),
                )
                .on_hover_text("Disable activity window - show all loaded files and entries")
                .clicked()
            {
                state.activity_window_secs = None;
                state.activity_window_input.clear();
                state.apply_filters();
            }
        } else {
            ui.label(egui::RichText::new("off").small().color(dim));
        }
    });

    // Preset buttons + custom input (always shown when files are loaded).
    const PRESETS: &[(&str, u64)] = &[("30s", 30), ("5m", 300), ("15m", 900), ("1h", 3600)];
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        for &(label, secs) in PRESETS {
            let preset_active = state.activity_window_secs == Some(secs);
            let colour = if preset_active {
                amber
            } else {
                ui.style().visuals.text_color()
            };
            let resp = ui
                .add(egui::Button::new(egui::RichText::new(label).small().color(colour)).small())
                .on_hover_text(if preset_active {
                    "Click to turn off activity window"
                } else {
                    "Show only files modified within this window"
                });
            if resp.clicked() {
                if preset_active {
                    state.activity_window_secs = None;
                    state.activity_window_input.clear();
                } else {
                    state.activity_window_secs = Some(secs);
                    state.activity_window_input.clear();
                }
                changed = true;
            }
        }

        // Custom input: a number of minutes.
        let input_resp = ui.add(
            egui::TextEdit::singleline(&mut state.activity_window_input)
                .hint_text("min")
                .desired_width(34.0),
        );
        let set_clicked = ui
            .add_enabled(
                !state.activity_window_input.trim().is_empty(),
                egui::Button::new(egui::RichText::new("Set").small()).small(),
            )
            .on_hover_text("Set custom activity window (minutes)")
            .clicked();
        // Commit on Set click OR when focus leaves the field (consistent
        // with the relative-time custom input in the Filters tab).
        let committed = set_clicked
            || (input_resp.lost_focus() && !state.activity_window_input.trim().is_empty());
        if committed {
            if let Ok(mins) = state.activity_window_input.trim().parse::<u64>() {
                // Bug fix: use checked_mul to prevent u64 overflow (silent
                // wrapping in release / panic in debug) for very large inputs.
                // Bug fix: cap to MAX_CUSTOM_TIME_MINUTES so users cannot set
                // meaningless multi-million-year windows that bypass the filter.
                if mins > 0 && mins <= crate::util::constants::MAX_CUSTOM_TIME_MINUTES {
                    if let Some(secs) = mins.checked_mul(60) {
                        state.activity_window_secs = Some(secs);
                        changed = true;
                    }
                } else {
                    // Out-of-range (0 or exceeds cap): reset the field to the
                    // last applied window so it doesn't display stale text that
                    // no longer matches the active filter.
                    state.activity_window_input = state
                        .activity_window_secs
                        .map(|s| (s / 60).to_string())
                        .unwrap_or_default();
                }
            } else {
                // Non-numeric text: reset to the last applied window.
                state.activity_window_input = state
                    .activity_window_secs
                    .map(|s| (s / 60).to_string())
                    .unwrap_or_default();
            }
        }
    });

    if changed {
        state.apply_filters();
    }

    // Hint when active.
    if let Some(secs) = state.activity_window_secs {
        ui.label(
            egui::RichText::new(format!(
                "Showing files written in the {}. \
                 Ages out automatically.",
                format_window_duration(secs)
            ))
            .small()
            .weak(),
        );
    }
}
